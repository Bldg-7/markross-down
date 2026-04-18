use std::io::{self, Write};

use base64::{engine::general_purpose::STANDARD, Engine};

/// Copy text to the system clipboard via the OSC 52 sequence.
///
/// Supported by Kitty, WezTerm, Ghostty, iTerm2, recent xterm, and by tmux
/// when `allow-passthrough on` and `set -as terminal-features ',*:clipboard'`
/// are configured. On terminals without OSC 52 support this is a silent no-op
/// for the user; the write still reaches the PTY so we report success.
pub fn copy(text: &str) -> io::Result<()> {
    let encoded = STANDARD.encode(text.as_bytes());
    let mut stdout = io::stdout().lock();
    write!(stdout, "\x1b]52;c;{}\x1b\\", encoded)?;
    stdout.flush()
}
