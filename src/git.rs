use anyhow::{Context, Result};
use git2::{Repository, Signature};
use std::path::Path;

/// Walk up from `start` to find the git repository root.
pub fn find_repo(start: &Path) -> Result<Repository> {
    Repository::discover(start)
        .with_context(|| format!(
            "'{}' is not inside a git repository. \
             Initialize one first with: git init <path>",
            start.display()
        ))
}

/// Stage `journal.db` (relative to repo workdir) and commit.
pub fn commit_db(journal_path: &Path, message: &str) -> Result<()> {
    let repo = find_repo(journal_path)?;
    let workdir = repo.workdir()
        .with_context(|| "Bare repositories are not supported")?;

    let db_abs = journal_path.join("journal.db");
    let db_rel = db_abs.strip_prefix(workdir)
        .with_context(|| format!(
            "journal.db at '{}' is outside the git workdir '{}'",
            db_abs.display(), workdir.display()
        ))?;

    let mut index = repo.index()?;
    index.add_path(db_rel)?;
    index.write()?;

    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let sig = get_signature(&repo)?;

    let parent_commit = match repo.head() {
        Ok(head) => Some(head.peel_to_commit()?),
        Err(_) => None,
    };
    let parents: Vec<&git2::Commit<'_>> = parent_commit.iter().collect();

    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)?;
    Ok(())
}

fn get_signature(repo: &Repository) -> Result<Signature<'_>> {
    let config = repo.config().unwrap_or_else(|_| {
        git2::Config::open_default().expect("no git config")
    });
    let name = config.get_string("user.name").unwrap_or_else(|_| "Journal User".to_string());
    let email = config.get_string("user.email").unwrap_or_else(|_| "journal@local".to_string());
    Signature::now(&name, &email).map_err(Into::into)
}
