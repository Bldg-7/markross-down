use anyhow::{Context, Result};
use ropey::Rope;
use std::fs;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

pub struct Document {
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

impl Document {
    pub fn empty() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            dirty: false,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let rope = if path.exists() {
            let file = fs::File::open(path)
                .with_context(|| format!("opening {}", path.display()))?;
            Rope::from_reader(BufReader::new(file))
                .with_context(|| format!("reading {}", path.display()))?
        } else {
            Rope::new()
        };
        Ok(Self {
            rope,
            path: Some(path.to_path_buf()),
            dirty: false,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .context("document has no associated path")?;
        let mut writer = BufWriter::new(fs::File::create(path)?);
        self.rope.write_to(&mut writer)?;
        writer.flush()?;
        self.dirty = false;
        Ok(())
    }

    pub fn insert_char(&mut self, char_idx: usize, ch: char) {
        self.rope.insert_char(char_idx, ch);
        self.dirty = true;
    }

    pub fn remove(&mut self, char_range: std::ops::Range<usize>) {
        self.rope.remove(char_range);
        self.dirty = true;
    }

    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "[scratch]".to_string())
    }
}

/// Number of characters in a line, excluding any trailing `\n` or `\r\n`.
pub fn line_len_no_newline(rope: &Rope, line: usize) -> usize {
    if line >= rope.len_lines() {
        return 0;
    }
    let slice = rope.line(line);
    let mut len = slice.len_chars();
    if len > 0 && slice.char(len - 1) == '\n' {
        len -= 1;
        if len > 0 && slice.char(len - 1) == '\r' {
            len -= 1;
        }
    }
    len
}
