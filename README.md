# journal

A minimal, git-tracked personal journal CLI built in Rust.

Entries live as a SQLite database inside any existing git repo — commit alongside your notes, dotfiles, or whatever you already version-control.

## Setup

```bash
cargo install --path .

# Point journal at a folder inside an existing git repo
journal setup ~/notes/journal
```

## Usage

```bash
journal new                              # open $EDITOR
journal new --title "Today" --tags "work,ideas" --mood good
journal new --body "quick inline note"

journal list                             # table output
journal view                             # full-screen TUI

journal show <id>                        # show entry by id prefix
journal edit <id>                        # edit in $EDITOR
journal delete <id>

journal search "query"                   # full-text + fuzzy title search

journal stats
journal push [remote] [branch]
journal pull [remote] [branch]
```

## TUI keybindings (`journal view`)

| Key | Action |
|-----|--------|
| `↑↓` / `j k` | navigate |
| `/` | live search |
| `Esc` | clear search / go back |
| `↵` | open entry |
| `p` | toggle preview pane |
| `o` | edit in `$EDITOR` (show mode) |
| `j k` | scroll body (show mode) |
| `q` | quit |
| `:q` | quit (vim-style) |

## How it works

- **SQLite** (`journal.db`) is the single source of truth — tracked by git
- Every write auto-commits `journal.db` to the containing repo
- Clone the repo on another machine and all entries are immediately accessible
- Full-text search via SQLite FTS5 with case-insensitive fuzzy title matching

## Moods

`great` · `good` · `okay` · `bad` · `awful`
