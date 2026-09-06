use std::path::PathBuf;

use directories::ProjectDirs;

use crate::error::{CoreError, Result};

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("dev", "wordglow", "wordglow").ok_or(CoreError::NoDataDir)
}

/// Root data directory, e.g. `~/.local/share/wordglow` on Linux.
pub fn data_dir() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    let dir = dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Path to the single sqlite database file.
pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("wordglow.db"))
}

/// Directory holding cached lesson audio files.
pub fn audio_dir() -> Result<PathBuf> {
    let dir = data_dir()?.join("audio");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Path to the cached audio file for a given lesson id, with the given
/// extension (no leading dot), e.g. `"mp3"` or `"webm"`.
pub fn audio_path(lesson_id: &str, ext: &str) -> Result<PathBuf> {
    Ok(audio_dir()?.join(format!("{lesson_id}.{ext}")))
}
