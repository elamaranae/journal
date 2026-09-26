use colored::Colorize;
use comfy_table::{presets::UTF8_FULL_CONDENSED, Attribute, Cell, Color, ContentArrangement, Table};

use crate::db::EntryRow;
use crate::entry::Mood;

pub fn print_entry_list(rows: &[EntryRow], date_format: &str) {
    if rows.is_empty() {
        println!("{}", "No entries found.".dimmed());
        return;
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("ID").add_attribute(Attribute::Bold),
            Cell::new("Date").add_attribute(Attribute::Bold),
            Cell::new("Title").add_attribute(Attribute::Bold),
            Cell::new("Tags").add_attribute(Attribute::Bold),
            Cell::new("Mood").add_attribute(Attribute::Bold),
            Cell::new("Words").add_attribute(Attribute::Bold),
        ]);

    for row in rows {
        let short_id = &row.id[..8];
        let date = row.created_at.format(date_format).to_string();
        let title = row.title.as_deref().unwrap_or("(untitled)");
        let tags = if row.tags.is_empty() {
            String::new()
        } else {
            row.tags.join(", ")
        };
        let mood = row.mood.as_ref().map(|m| mood_colored(m)).unwrap_or_default();

        table.add_row(vec![
            Cell::new(short_id).fg(Color::DarkGrey),
            Cell::new(date).fg(Color::Cyan),
            Cell::new(title),
            Cell::new(tags).fg(Color::Yellow),
            Cell::new(mood),
            Cell::new(row.word_count.to_string()),
        ]);
    }

    println!("{table}");
}

pub fn print_entry(row: &EntryRow) {
    let title = row.title.as_deref().unwrap_or("(untitled)");
    let date = row.created_at.format("%Y-%m-%d %H:%M").to_string();

    println!();
    println!("{}", title.bold().bright_white());
    println!("{}", date.dimmed());

    if !row.tags.is_empty() {
        let tag_str = row.tags.iter().map(|t| format!("#{}", t)).collect::<Vec<_>>().join(" ");
        println!("{}", tag_str.yellow());
    }
    if let Some(mood) = &row.mood {
        println!("mood: {}", mood_colored(mood));
    }

    println!("{}", "─".repeat(60).dimmed());
    println!();

    let body = row.body.as_deref().unwrap_or("");
    println!("{}", body);

    println!();
    println!(
        "{}",
        format!("{} words · id: {}", row.word_count, row.id).dimmed()
    );
}

pub fn print_stats(total: i64, words: i64, _date_format: &str, oldest: Option<&str>, newest: Option<&str>) {
    println!();
    println!("{}", "Journal Stats".bold().bright_white());
    println!("{}", "─".repeat(30).dimmed());
    println!("  {}: {}", "Total entries".bold(), total.to_string().cyan());
    println!("  {}: {}", "Total words".bold(), words.to_string().cyan());
    if let (Some(o), Some(n)) = (oldest, newest) {
        println!("  {}: {}", "Oldest entry".bold(), o.dimmed());
        println!("  {}: {}", "Newest entry".bold(), n.dimmed());
    }
    println!();
}

fn mood_colored(mood: &Mood) -> String {
    match mood {
        Mood::Great => "great".bright_green().to_string(),
        Mood::Good => "good".green().to_string(),
        Mood::Okay => "okay".yellow().to_string(),
        Mood::Bad => "bad".red().to_string(),
        Mood::Awful => "awful".bright_red().to_string(),
    }
}
