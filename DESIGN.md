# markross-down — Design

## 1. Requirements

1. **Dual mode** — Raw source view and Preview (rendered) view, toggleable globally.
2. **Inline mode switching in Preview** — when the cursor enters a block, that block falls back to raw text so the user can edit it; when the cursor leaves, it re-renders.
3. **Graphic rendering** of headers (as enlarged bitmaps), tables (as rasterized grids), and images.
4. **Plugin system** for custom block renderers (e.g. a Mermaid plugin that turns ```` ```mermaid ```` fences into rendered diagrams).
5. **External-edit reconciliation** — detect when another process modifies the open file, and merge/reload without losing local work.
6. **Text selection, copy/paste, mouse interaction** — app-level, consistent across both modes.

## 2. Architecture overview

```
        ┌──────────────────────────────────────────────────┐
        │                     App loop                      │
        │  (tokio multi-thread + single-threaded render)    │
        └──────────────────────────────────────────────────┘
                │             │            │            │
         keyboard/mouse    file watcher  plugin IPC   autosave
                │             │            │
                ▼             ▼            ▼
        ┌──────────────────────────────────────────────────┐
        │                 Editor state                      │
        │   Document (Rope) + AST + Cursor + Selection      │
        └──────────────────────────────────────────────────┘
                            │
                            ▼
        ┌──────────────────────────────────────────────────┐
        │                  Render pipeline                  │
        │  Layout → per-block state (raw|rendered) →        │
        │  Dirty regions → Frame                            │
        └──────────────────────────────────────────────────┘
                │                          │
                ▼                          ▼
        ratatui cells          ratatui-image placements
        (text, styles)         (Kitty / Sixel / iTerm2 / halfblock)
```

## 3. Module layout

```
src/
├── main.rs                   # Entry, arg parsing, signal handlers
├── app.rs                    # Top-level app state + event loop
├── config.rs                 # Settings, theme, keybinds
├── clipboard.rs              # OSC 52 + arboard fallback
│
├── document/
│   ├── mod.rs                # Document wrapper
│   ├── buffer.rs             # Rope-backed text buffer
│   ├── parser.rs             # Incremental Markdown parsing
│   └── ast.rs                # Block/inline AST with stable IDs
│
├── editor/
│   ├── mod.rs                # Editor state
│   ├── cursor.rs             # Movement, word boundaries
│   ├── selection.rs          # Anchor/head model, span ops
│   ├── input.rs              # Keyboard + mouse event handling
│   └── history.rs            # Undo/redo (chunked)
│
├── render/
│   ├── mod.rs                # Orchestration
│   ├── raw.rs                # Raw-mode text renderer
│   ├── preview.rs            # Preview-mode renderer (styled text + graphics)
│   ├── hybrid.rs             # Per-block raw/rendered toggle
│   ├── graphic/
│   │   ├── mod.rs            # Bitmap rendering dispatch
│   │   ├── header.rs         # Header-as-image rasterizer
│   │   ├── table.rs          # Table layout + grid rasterizer
│   │   └── cache.rs          # content_hash → bitmap cache
│   └── dirty.rs              # Dirty-region tracker (block ID based)
│
├── plugin/
│   ├── mod.rs                # Plugin registry
│   ├── manifest.rs           # plugin.toml parser
│   ├── host.rs               # Subprocess plugin host
│   ├── cache.rs              # Plugin output cache
│   └── builtin/
│       └── mermaid.rs        # Reference subprocess plugin (mmdc)
│
└── watcher/
    ├── mod.rs                # notify integration (debounced)
    └── merge.rs              # 3-way merge UX + diff rendering
```

## 4. Core design decisions

### 4.1 Text buffer — `ropey`
Rope gives O(log n) edits at arbitrary positions and cheap line indexing. Standard for modern editors (helix, zed's early versions). Each buffer tracks a monotonic revision counter.

### 4.2 Parsing — block-granular incremental
Markdown is fundamentally block-oriented. On every edit:

1. Identify which top-level block(s) the edit touches (via rope line offsets).
2. Re-parse only those blocks with `pulldown-cmark`.
3. Assign stable block IDs: hash of `(kind, content)` for pure blocks, positional ID for duplicates.

Full re-parse stays as a fallback for correctness checks. For very large files (> 1 MB) a future upgrade can swap in `tree-sitter-markdown` for true incremental trees.

### 4.3 Render pipeline

Each block is in exactly one of three states:

| State | When | How it's drawn |
|---|---|---|
| `Raw` | Global mode is Raw, **or** cursor/selection overlaps this block in Preview | Plain text cells with syntax highlight on the Markdown source |
| `Styled` | Preview mode, no cursor/selection overlap, block has no graphic rendering (paragraphs, lists, inline code, blockquote) | ratatui `Paragraph` with styles |
| `Graphic` | Preview mode, no overlap, block is H1/H2, table, image, or plugin-handled | Bitmap drawn into an offscreen buffer → `ratatui-image` placement |

A block's state is recomputed per frame but actual work only happens when it **changes**. Dirty tracker keys by block ID; bitmaps are cached by `(block_id, content_hash, font_scale, theme_rev)`.

### 4.4 Inline mode switching (preview ⇄ raw per block)

```
on cursor/selection change:
    for each block B:
        prev = B.render_state
        next = compute_state(B, cursor, selection, global_mode)
        if prev != next:
            mark B dirty
    repaint dirty blocks
```

`compute_state`:
- Raw mode → `Raw`
- Preview mode:
  - If cursor line ∈ B.line_range → `Raw`
  - If selection intersects B.line_range → `Raw`
  - Else if B has graphic renderer → `Graphic`
  - Else → `Styled`

Transitions are at the block boundary, which prevents visual tearing. When a block flips Graphic → Raw, the placement is removed via Kitty `delete` command; when Raw → Graphic, the bitmap is re-rasterized (cached if unchanged).

### 4.5 Plugin system

**Chosen strategy: subprocess-based plugins** with a TOML manifest. Rationale: mermaid, graphviz, plantuml all ship as CLI tools; subprocess covers 80 % of realistic plugins with zero language lock-in. WASM (extism/wasmtime) is a future extension for finer-grained, sandboxed plugins.

Plugin manifest (`~/.config/markross-down/plugins/<name>/plugin.toml`):

```toml
name = "mermaid"
version = "0.1.0"
triggers = ["fenced_code:mermaid"]         # what AST nodes to intercept

[renderer]
type = "subprocess"
command = "mmdc"
args = ["-i", "-", "-o", "{output}", "-b", "transparent"]
stdin = "content"                            # "content" | "none"
output = "png"                               # file format the plugin produces

[cache]
key = "content"                              # cache by content hash
max_entries = 256
```

Host contract:
1. During render, for each block whose `trigger` matches a loaded plugin, the host invokes the plugin (cached by key).
2. Plugin returns a PNG path; host hands it to `ratatui-image`.
3. Plugins run async; while pending, the block renders as `Raw` with a status hint.

Failure modes: plugin missing → render as Raw + inline warning. Plugin timeout (default 5 s) → same.

### 4.6 External-edit reconciliation

- `notify` + `notify-debouncer-full` watches the open file (and parent dir, to catch atomic saves / rename-replace).
- Every save by the app records a `(mtime, content_hash, revision)` fingerprint.
- On watch event:
  - If content hash matches the last saved fingerprint → self-trigger, ignore.
  - If local buffer is clean (no unsaved edits since last save) → silent reload, preserve cursor line if possible.
  - If local buffer is dirty → enter **Merge mode**:
    - Compute 3-way diff with `similar` using (common ancestor = last saved disk content) × (their = new disk content) × (mine = local buffer).
    - Render a split view: mine | theirs, with conflict markers for overlapping hunks.
    - Actions: `accept mine`, `accept theirs`, `edit merged`, `abort`.

Revision counter ensures editor actions taken during merge mode apply to the merged state, not the pre-merge buffer.

### 4.7 Graphic rendering — headers & tables

Generic pipeline: AST node → layout → `cosmic-text` rasterization → `tiny-skia` composition → PNG bytes → `ratatui-image::StatefulImage`.

**Header**: measure terminal cell pixel size (ratatui-image query), pick a font scale from config (`h1 = 3.0x`, `h2 = 2.0x`, etc.), rasterize at that scale. Cache key includes `(scale, color, weight)`.

**Table**: measure column widths by content, compute row heights, rasterize each cell with `cosmic-text`, composite with grid lines in `tiny-skia`. Max width = viewport width in pixels; overflow uses horizontal scroll (arrow keys or mouse wheel).

**Font**: ship [Noto Sans / Noto Sans Mono] as embedded default; allow override via config.

### 4.8 Selection, clipboard, mouse

- Enable `\x1b[?1000h \x1b[?1002h \x1b[?1006h \x1b[?2004h` at startup; restore on exit (including panic/SIGINT handler).
- Selection model is app-level (anchor + head, byte offsets into rope).
- Copy → OSC 52 `\x1b]52;c;<base64>\x07`; when OSC 52 is unavailable (detected via capability query or user config), fall back to `arboard`.
- Paste → bracketed paste (`\x1b[200~` … `\x1b[201~`) distinguishes pasted blocks from typed input; treated as a single undo unit.
- Mouse down/drag/up → update selection in the document model; selection that crosses a Graphic block forces that block to `Raw` for visual consistency.
- `Shift+drag` is intentionally left to the terminal (native selection escape hatch); documented in the status bar on first launch.

### 4.9 Event loop

```rust
tokio::select! {
    Some(ev) = input_stream.next()      => handle_input(ev),
    Some(ev) = watcher_rx.recv()        => handle_watch(ev),
    Some(ev) = plugin_rx.recv()         => handle_plugin_done(ev),
    _       = autosave_ticker.tick()    => maybe_autosave(),
}
```

Rendering runs synchronously on the main thread after any state change, driven by the dirty tracker. Plugin subprocesses, file watching, and autosave live on the tokio runtime; plugins post `PluginDone { block_id, bitmap }` messages back.

## 5. Milestones

| # | Scope | Deliverable |
|---|---|---|
| M1 | Skeleton + raw editor | Cargo project, `crossterm` + `ratatui` setup, rope buffer, movement, file open/save |
| M2 | Selection + clipboard + mouse | App-level selection, OSC 52 copy, bracketed paste, mouse drag |
| M3 | Preview mode (text-only) | `pulldown-cmark` AST, block IDs, Styled state rendering (no graphics yet) |
| M4 | Inline mode switching | Per-block Raw/Styled toggle driven by cursor & selection |
| M5 | File watcher + merge | `notify` integration, 3-way merge UI |
| M6 | Graphic renderer | `ratatui-image`, `cosmic-text`+`tiny-skia` pipeline, headers as images, bitmap cache |
| M7 | Tables as graphics | Table layout engine, cell rasterization, grid composition |
| M8 | Plugin system | Manifest format, subprocess host, plugin cache, Mermaid reference plugin |
| M9 | Polish | Config, themes, keybinds, panic-safe terminal restore, status line, docs |

## 6. Risks and open questions

| Risk | Mitigation |
|---|---|
| Terminal graphics protocol fragmentation | `ratatui-image` abstracts; halfblock fallback remains usable on unsupported terminals |
| Per-keystroke bitmap rasterization cost | Raster only on idle/debounce; active typing keeps the block in Raw state anyway |
| IME / CJK input (Korean, Japanese, Chinese) | **Open**: terminal IME support varies; plan a preedit overlay using ratatui-image or a small raw-text inline region |
| Plugin security | Subprocess plugins run with user permissions; manifest must be explicitly installed; sandbox (bubblewrap/firejail) in future WASM path |
| Unicode width edge cases in Raw mode | Lean on `unicode-width`; test with emoji, CJK, ZWJ sequences |
| tmux graphics passthrough | Detect tmux via `$TERM`/`$TMUX`; require `allow-passthrough on`; document setup |
| Large file (> 10 MB) editing | Defer; rope handles it, but incremental parser and graphic cache sizing need attention |

## 7. Non-goals (for v0.x)

- WYSIWYG inline formatting (click-to-bold, etc.) — edits happen in raw Markdown.
- Multi-cursor.
- LSP integration (linters, spellcheck).
- Collaborative editing.
- Mobile / non-terminal UI.
