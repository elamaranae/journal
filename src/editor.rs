use anyhow::{Context, Result};
use std::process::Command;
use tempfile::NamedTempFile;

/// Open $EDITOR with optional initial content, return the edited text.
pub fn open_editor(initial_content: &str) -> Result<String> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());

    let mut tmpfile = NamedTempFile::new().with_context(|| "Failed to create temp file")?;
    use std::io::Write;
    write!(tmpfile, "{}", initial_content)?;

    let path = tmpfile.path().to_owned();

    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("Failed to launch editor '{}'", editor))?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| "Failed to read editor output")?;

    Ok(content)
}
