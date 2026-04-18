use anyhow::{Context, Result};
use ropey::Rope;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

pub struct Document {
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// SHA-256 of the bytes last read from or written to disk. Used to
    /// distinguish self-triggered writes from genuine external edits.
    pub last_save_hash: Option<[u8; 32]>,
}

impl Document {
    pub fn empty() -> Self {
        Self {
            rope: Rope::new(),
            path: None,
            dirty: false,
            last_save_hash: None,
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let (rope, hash) = if path.exists() {
            let mut file = fs::File::open(path)
                .with_context(|| format!("opening {}", path.display()))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .with_context(|| format!("reading {}", path.display()))?;
            let rope = Rope::from_reader(BufReader::new(&bytes[..]))
                .with_context(|| format!("parsing {}", path.display()))?;
            let hash: [u8; 32] = Sha256::digest(&bytes).into();
            (rope, Some(hash))
        } else {
            (Rope::new(), None)
        };
        Ok(Self {
            rope,
            path: Some(path.to_path_buf()),
            dirty: false,
            last_save_hash: hash,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .context("document has no associated path")?;
        let bytes = rope_to_bytes(&self.rope);
        let mut writer = BufWriter::new(fs::File::create(path)?);
        writer.write_all(&bytes)?;
        writer.flush()?;
        self.dirty = false;
        self.last_save_hash = Some(Sha256::digest(&bytes).into());
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

fn rope_to_bytes(rope: &Rope) -> Vec<u8> {
    let mut buf = Vec::with_capacity(rope.len_bytes());
    for chunk in rope.chunks() {
        buf.extend_from_slice(chunk.as_bytes());
    }
    buf
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
