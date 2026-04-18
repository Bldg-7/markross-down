# markross-down

A terminal-native Markdown editor with hybrid text/graphics rendering.

> Status: early design. See [`DESIGN.md`](DESIGN.md) for the full architecture plan.

## Goals

- Edit Markdown in the terminal with graphic-quality rendering — big headers, real tables, embedded images
- **Raw** and **Preview** modes, with per-block **inline mode switching** during editing
- **Live external sync**: detect and reconcile edits from other processes
- **Plugin system** for custom block renderers (Mermaid, Graphviz, PlantUML, ...)
- First-class text selection, system clipboard, and mouse interaction

## Stack

| Concern | Crate |
|---|---|
| TUI framework | [`ratatui`](https://github.com/ratatui-org/ratatui) |
| Terminal I/O | [`crossterm`](https://github.com/crossterm-rs/crossterm) |
| Graphics protocols (Kitty / Sixel / iTerm2 / halfblock) | [`ratatui-image`](https://github.com/benjajaja/ratatui-image) |
| Markdown parser | [`pulldown-cmark`](https://github.com/raphlinus/pulldown-cmark) |
| Text buffer | [`ropey`](https://github.com/cessen/ropey) |
| File watcher | [`notify`](https://github.com/notify-rs/notify) |
| Font shaping & rasterization | [`cosmic-text`](https://github.com/pop-os/cosmic-text) + [`tiny-skia`](https://github.com/RazrFalcon/tiny-skia) |

## Target terminals

Graphics-rich mode: **Kitty**, **WezTerm**, **Ghostty**, **iTerm2**.
Fallback (halfblocks + plain text): any VT100-compatible terminal.

## Build

```bash
cargo run -- <path/to/file.md>
```

## License

TBD.
