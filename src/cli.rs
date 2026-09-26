use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "journal", about = "A git-tracked personal journal", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new journal entry
    New {
        /// Entry title
        #[arg(short, long)]
        title: Option<String>,

        /// Comma-separated tags
        #[arg(short = 'T', long)]
        tags: Option<String>,

        /// Mood: great, good, okay, bad, awful
        #[arg(short, long)]
        mood: Option<String>,

        /// Write entry inline (skips opening editor)
        #[arg(short, long)]
        body: Option<String>,
    },

    /// List journal entries
    List {
        /// Filter by tag
        #[arg(short, long)]
        tag: Option<String>,

        /// From date (YYYY-MM-DD)
        #[arg(long)]
        from: Option<String>,

        /// To date (YYYY-MM-DD)
        #[arg(long)]
        to: Option<String>,

        /// Max entries to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Show a journal entry
    Show {
        /// Entry ID prefix (min 4 chars)
        id: String,
    },

    /// Edit an existing entry
    Edit {
        /// Entry ID prefix
        id: String,
    },

    /// Delete an entry
    Delete {
        /// Entry ID prefix
        id: String,

        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },

    /// Full-text search across entries
    Search {
        /// Search query
        query: String,
    },

    /// One-time setup: point journal at a folder inside an existing git repo
    Setup {
        /// Path to the journal folder (must be inside a git repo)
        path: std::path::PathBuf,
    },

    /// Open full-screen TUI browser
    View,

    /// Show journal statistics
    Stats,

    /// Push to git remote
    Push {
        /// Remote name (default: origin)
        #[arg(default_value = "origin")]
        remote: String,

        /// Branch name (default: main)
        #[arg(default_value = "main")]
        branch: String,
    },

    /// Pull from git remote
    Pull {
        /// Remote name (default: origin)
        #[arg(default_value = "origin")]
        remote: String,

        /// Branch name (default: main)
        #[arg(default_value = "main")]
        branch: String,
    },
}
