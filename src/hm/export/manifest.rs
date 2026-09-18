use std::path::Path;

use anyhow::Result;
use serde::Serialize;

#[derive(Serialize)]
pub struct Row {
    pub name: String,
    pub game: String,
    pub category: String,
    pub package: String,
    pub source: String,
    pub output: String,
    pub format: String,
    pub rate: u32,
    pub channels: u8,
    pub seconds: f32,
    pub at: String,
}

pub fn write(path: &Path, rows: &[Row]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(rows)?;
    std::fs::write(path, text)?;
    Ok(())
}
