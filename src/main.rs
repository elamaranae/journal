mod cli;
mod commands;
mod config;
mod db;
mod display;
mod editor;
mod entry;
mod git;
mod tui;

use clap::Parser;
use cli::{Cli, Commands};
use config::load_config;

fn main() {
    let cli = Cli::parse();

    // Setup doesn't need an existing config
    if let Commands::Setup { path } = cli.command {
        if let Err(e) = commands::cmd_setup(path) {
            eprintln!("Error: {:#}", e);
            std::process::exit(1);
        }
        return;
    }

    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            std::process::exit(1);
        }
    };

    let result = match cli.command {
        Commands::Setup { .. } => unreachable!(),
        Commands::New { title, tags, mood, body } => {
            commands::cmd_new(&config, title, tags, mood, body)
        }
        Commands::List { tag, from, to, limit } => {
            commands::cmd_list(&config, tag, from, to, limit)
        }
        Commands::Show { id } => commands::cmd_show(&config, &id),
        Commands::Edit { id } => commands::cmd_edit(&config, &id),
        Commands::Delete { id, yes } => commands::cmd_delete(&config, &id, yes),
        Commands::Search { query } => commands::cmd_search(&config, &query),
        Commands::View => {
            let journal_path = match config.require_journal_path() {
                Ok(p) => p.clone(),
                Err(e) => { eprintln!("Error: {:#}", e); std::process::exit(1); }
            };
            let db = match crate::db::Db::open(&journal_path) {
                Ok(d) => d,
                Err(e) => { eprintln!("Error: {:#}", e); std::process::exit(1); }
            };
            tui::run(db, journal_path)
        }
        Commands::Stats => commands::cmd_stats(&config),
        Commands::Push { remote, branch } => commands::cmd_push(&config, &remote, &branch),
        Commands::Pull { remote, branch } => commands::cmd_pull(&config, &remote, &branch),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}
