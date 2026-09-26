use anyhow::{bail, Context, Result};
use chrono::Local;
use colored::Colorize;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::config::{self, Config};
use crate::db::Db;
use crate::display;
use crate::editor::open_editor;
use crate::entry::{Entry, Mood};
use crate::git;

fn open_db(journal_path: &std::path::Path) -> Result<Db> {
    Db::open(journal_path)
}

pub fn cmd_setup(path: PathBuf) -> Result<()> {
    let path = path.canonicalize()
        .with_context(|| format!("Path '{}' does not exist", path.display()))?;

    // Validate it's inside a git repo
    git::find_repo(&path)
        .with_context(|| format!(
            "'{}' is not inside a git repository",
            path.display()
        ))?;

    // Create the directory if needed
    std::fs::create_dir_all(&path)?;

    let mut cfg = config::load_config()?;
    cfg.journal_path = Some(path.clone());
    config::save_config(&cfg)?;

    println!("{} Journal path set to {}", "Done.".green().bold(), path.display());
    println!("  Entries will be stored in {}", path.join("journal.db").display());
    Ok(())
}

pub fn cmd_new(
    config: &Config,
    title: Option<String>,
    tags_str: Option<String>,
    mood_str: Option<String>,
    body_inline: Option<String>,
) -> Result<()> {
    let journal_path = config.require_journal_path()?;

    let tags: Vec<String> = tags_str
        .as_deref()
        .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
        .unwrap_or_default();

    let mood: Option<Mood> = mood_str.as_deref().map(|m| m.parse()).transpose()?;

    let mut entry = Entry::new(title, tags, mood);

    entry.body = match body_inline {
        Some(b) => b,
        None => {
            let edited = open_editor("")?;
            edited.trim().to_string()
        }
    };

    if entry.body.is_empty() {
        bail!("Aborting: empty entry.");
    }

    let db = open_db(journal_path)?;
    db.upsert(&entry)?;

    let title_display = entry.title.as_deref().unwrap_or("(untitled)");
    let msg = format!(
        "journal: add {} \"{}\"",
        entry.created_at.format("%Y-%m-%dT%H:%M:%S"),
        title_display
    );
    git::commit_db(journal_path, &msg)?;

    println!("{} {}", "Created:".green().bold(), title_display);
    println!("  id: {}", &entry.id[..8].dimmed());
    Ok(())
}

pub fn cmd_list(
    config: &Config,
    tag: Option<String>,
    from_str: Option<String>,
    to_str: Option<String>,
    limit: usize,
) -> Result<()> {
    let journal_path = config.require_journal_path()?;
    let db = open_db(journal_path)?;

    let from = from_str.as_deref().map(parse_date_start).transpose()?;
    let to = to_str.as_deref().map(parse_date_end).transpose()?;

    let rows = db.list(tag.as_deref(), from, to, limit)?;
    display::print_entry_list(&rows, config.date_fmt());
    Ok(())
}

pub fn cmd_show(config: &Config, id_prefix: &str) -> Result<()> {
    let journal_path = config.require_journal_path()?;
    let db = open_db(journal_path)?;
    let row = db
        .get_by_id(id_prefix)?
        .with_context(|| format!("No entry found with id starting '{}'", id_prefix))?;
    display::print_entry(&row);
    Ok(())
}

pub fn cmd_edit(config: &Config, id_prefix: &str) -> Result<()> {
    let journal_path = config.require_journal_path()?;
    let db = open_db(journal_path)?;

    let row = db
        .get_by_id(id_prefix)?
        .with_context(|| format!("No entry found with id starting '{}'", id_prefix))?;

    let original_body = row.body.clone().unwrap_or_default();
    let edited_body = open_editor(&original_body)?;
    let edited_body = edited_body.trim().to_string();

    if edited_body == original_body {
        println!("{}", "No changes made.".dimmed());
        return Ok(());
    }

    if edited_body.is_empty() {
        bail!("Aborting: empty body.");
    }

    // Reconstruct entry from row and apply changes
    let updated = Entry {
        id: row.id.clone(),
        title: row.title.clone(),
        created_at: row.created_at,
        updated_at: Local::now(),
        tags: row.tags.clone(),
        mood: row.mood.clone(),
        body: edited_body,
    };

    db.upsert(&updated)?;

    let title_display = updated.title.as_deref().unwrap_or("(untitled)");
    let msg = format!(
        "journal: edit {} \"{}\"",
        updated.updated_at.format("%Y-%m-%dT%H:%M:%S"),
        title_display
    );
    git::commit_db(journal_path, &msg)?;

    println!("{} \"{}\"", "Updated:".green().bold(), title_display);
    Ok(())
}

pub fn cmd_delete(config: &Config, id_prefix: &str, yes: bool) -> Result<()> {
    let journal_path = config.require_journal_path()?;
    let db = open_db(journal_path)?;

    let row = db
        .get_by_id(id_prefix)?
        .with_context(|| format!("No entry found with id starting '{}'", id_prefix))?;

    let title_display = row.title.as_deref().unwrap_or("(untitled)");

    if !yes {
        print!("Delete \"{}\" ({})? [y/N] ", title_display, &row.id[..8]);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("{}", "Aborted.".dimmed());
            return Ok(());
        }
    }

    db.delete(&row.id)?;

    let msg = format!(
        "journal: delete {} \"{}\"",
        row.created_at.format("%Y-%m-%dT%H:%M:%S"),
        title_display
    );
    git::commit_db(journal_path, &msg)?;

    println!("{} \"{}\"", "Deleted:".red().bold(), title_display);
    Ok(())
}

pub fn cmd_search(config: &Config, query: &str) -> Result<()> {
    let journal_path = config.require_journal_path()?;
    let db = open_db(journal_path)?;
    let rows = db.search(query)?;
    if rows.is_empty() {
        println!("{}", "No results.".dimmed());
    } else {
        println!("{} result(s) for '{}'", rows.len(), query.bold());
        display::print_entry_list(&rows, config.date_fmt());
    }
    Ok(())
}

pub fn cmd_stats(config: &Config) -> Result<()> {
    let journal_path = config.require_journal_path()?;
    let db = open_db(journal_path)?;
    let total = db.count()?;
    let words = db.total_words()?;

    let rows = db.list(None, None, None, 1)?;
    let newest = rows.first().map(|r| r.created_at.format("%Y-%m-%d").to_string());

    let mut all = db.list(None, None, None, usize::MAX)?;
    all.reverse();
    let oldest = all.first().map(|r| r.created_at.format("%Y-%m-%d").to_string());

    display::print_stats(total, words, config.date_fmt(), oldest.as_deref(), newest.as_deref());
    Ok(())
}

pub fn cmd_push(config: &Config, remote: &str, branch: &str) -> Result<()> {
    use git2::{PushOptions, RemoteCallbacks};
    let journal_path = config.require_journal_path()?;
    let repo = git::find_repo(journal_path)?;

    let mut remote = repo.find_remote(remote)
        .with_context(|| format!("Remote '{}' not found", remote))?;

    let refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);
    let callbacks = RemoteCallbacks::new();
    let mut opts = PushOptions::new();
    opts.remote_callbacks(callbacks);

    remote.push(&[refspec.as_str()], Some(&mut opts))?;
    println!("{} Pushed to {}/{}", "Done.".green().bold(), remote.name().unwrap_or("?"), branch);
    Ok(())
}

pub fn cmd_pull(config: &Config, remote_name: &str, branch: &str) -> Result<()> {
    use git2::{FetchOptions, MergeAnalysis, RemoteCallbacks};
    let journal_path = config.require_journal_path()?;
    let repo = git::find_repo(journal_path)?;

    let mut remote = repo.find_remote(remote_name)
        .with_context(|| format!("Remote '{}' not found", remote_name))?;

    let callbacks = RemoteCallbacks::new();
    let mut fetch_opts = FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);
    remote.fetch(&[branch], Some(&mut fetch_opts), None)?;

    let fetch_head = repo.find_reference("FETCH_HEAD")?;
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;
    let (analysis, _) = repo.merge_analysis(&[&fetch_commit])?;

    if analysis.contains(MergeAnalysis::ANALYSIS_FASTFORWARD) {
        let refname = format!("refs/heads/{}", branch);
        let mut reference = repo.find_reference(&refname)?;
        reference.set_target(fetch_commit.id(), "Fast-forward")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
        println!("{} Fast-forwarded {}", "Done.".green().bold(), branch);
    } else if analysis.contains(MergeAnalysis::ANALYSIS_UP_TO_DATE) {
        println!("{}", "Already up to date.".dimmed());
    } else {
        bail!("Cannot fast-forward. Please resolve manually with git.");
    }

    Ok(())
}

fn parse_date_start(s: &str) -> Result<chrono::DateTime<Local>> {
    use chrono::NaiveDate;
    let nd = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("Invalid date '{}'. Use YYYY-MM-DD", s))?;
    Ok(nd.and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).unwrap())
}

fn parse_date_end(s: &str) -> Result<chrono::DateTime<Local>> {
    use chrono::NaiveDate;
    let nd = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("Invalid date '{}'. Use YYYY-MM-DD", s))?;
    Ok(nd.and_hms_opt(23, 59, 59).unwrap().and_local_timezone(Local).unwrap())
}
