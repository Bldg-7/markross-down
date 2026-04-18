//! Visual snapshot tests: render an `Editor` into a ratatui `TestBackend` and
//! rasterise the resulting cell buffer to a PNG via `fontdue` + `image`. The
//! PNGs land under `target/visual/` and can be opened by any image viewer.
//!
//! Run only when this module is compiled for tests.

#![cfg(test)]

use std::path::{Path, PathBuf};

use fontdue::{Font, FontSettings};
use image::{Rgba, RgbaImage};
use ratatui::backend::TestBackend;
use ratatui::buffer::{Buffer, Cell};
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;
use ropey::Rope;

use crate::document::Document;
use crate::editor::{Editor, RenderMode};
use crate::merge::{Decision, MergeState};
use crate::plugin::PluginHost;
use crate::render;
use crate::theme::Theme;

const FONT_SIZE: f32 = 16.0;
const CELL_W: u32 = 10;
const CELL_H: u32 = 20;

// Prefer fonts with broad box-drawing and quadrant-block coverage, since
// tui-big-text and table rendering rely on those ranges.
const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoMono-Regular.ttf",
    "/Library/Fonts/Menlo.ttc",
    "/System/Library/Fonts/Menlo.ttc",
];

fn load_font() -> Font {
    for p in FONT_CANDIDATES {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(font) = Font::from_bytes(bytes, FontSettings::default()) {
                return font;
            }
        }
    }
    panic!(
        "no usable monospace font on this machine; tried {:?}",
        FONT_CANDIDATES
    );
}

fn snapshot_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("target/visual");
    std::fs::create_dir_all(&p).expect("create target/visual");
    p.push(format!("{name}.png"));
    p
}

fn make_editor(markdown: &str) -> Editor {
    let mut doc = Document::empty();
    doc.rope = Rope::from_str(markdown);
    Editor::new(doc, PluginHost::new(Vec::new()))
}

fn render_editor_to_png(editor: &mut Editor, w: u16, h: u16, path: &Path) {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).expect("init TestBackend terminal");
    terminal
        .draw(|f| render::draw(editor, f))
        .expect("render frame");
    let buffer = terminal.backend().buffer().clone();
    buffer_to_png(&buffer, path);
}

fn buffer_to_png(buffer: &Buffer, path: &Path) {
    let font = load_font();
    let line_metrics = font
        .horizontal_line_metrics(FONT_SIZE)
        .expect("font has horizontal metrics");
    let ascent = line_metrics.ascent;

    let area = buffer.area;
    let img_w = area.width as u32 * CELL_W;
    let img_h = area.height as u32 * CELL_H;
    let mut img = RgbaImage::from_pixel(img_w, img_h, Rgba([0, 0, 0, 255]));

    for y in 0..area.height {
        for x in 0..area.width {
            let Some(cell) = buffer.cell((x, y)) else {
                continue;
            };
            if cell.skip {
                continue;
            }
            let (fg, bg) = resolve_cell_colors(cell);
            let cx = x as u32 * CELL_W;
            let cy = y as u32 * CELL_H;
            fill_rect(&mut img, cx, cy, CELL_W, CELL_H, bg);
            for ch in cell.symbol().chars() {
                draw_glyph(&mut img, &font, ascent, ch, cx, cy, fg);
                // Wide chars in ratatui span two cells; the second cell has
                // `skip = true` and is handled by the outer loop.
            }
            if cell.modifier.contains(Modifier::UNDERLINED) {
                let py = cy + CELL_H - 2;
                fill_rect(&mut img, cx, py, CELL_W, 1, fg);
            }
        }
    }
    img.save(path).expect("write png");
}

fn resolve_cell_colors(cell: &Cell) -> (Rgba<u8>, Rgba<u8>) {
    let mut fg = ansi_to_rgba(cell.fg, Rgba([220, 220, 220, 255]));
    let mut bg = ansi_to_rgba(cell.bg, Rgba([0, 0, 0, 255]));
    if cell.modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    (fg, bg)
}

fn ansi_to_rgba(c: Color, default: Rgba<u8>) -> Rgba<u8> {
    match c {
        Color::Reset => default,
        Color::Black => Rgba([0, 0, 0, 255]),
        Color::Red => Rgba([205, 49, 49, 255]),
        Color::Green => Rgba([13, 188, 121, 255]),
        Color::Yellow => Rgba([229, 229, 16, 255]),
        Color::Blue => Rgba([36, 114, 200, 255]),
        Color::Magenta => Rgba([188, 63, 188, 255]),
        Color::Cyan => Rgba([17, 168, 205, 255]),
        Color::Gray => Rgba([180, 180, 180, 255]),
        Color::DarkGray => Rgba([102, 102, 102, 255]),
        Color::LightRed => Rgba([241, 76, 76, 255]),
        Color::LightGreen => Rgba([35, 209, 139, 255]),
        Color::LightYellow => Rgba([245, 245, 67, 255]),
        Color::LightBlue => Rgba([59, 142, 234, 255]),
        Color::LightMagenta => Rgba([214, 112, 214, 255]),
        Color::LightCyan => Rgba([41, 184, 219, 255]),
        Color::White => Rgba([229, 229, 229, 255]),
        Color::Rgb(r, g, b) => Rgba([r, g, b, 255]),
        Color::Indexed(_) => default,
    }
}

fn fill_rect(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    for py in y..(y + h).min(img.height()) {
        for px in x..(x + w).min(img.width()) {
            img.put_pixel(px, py, color);
        }
    }
}

fn draw_glyph(
    img: &mut RgbaImage,
    font: &Font,
    ascent: f32,
    ch: char,
    cx: u32,
    cy: u32,
    color: Rgba<u8>,
) {
    let (metrics, bitmap) = font.rasterize(ch, FONT_SIZE);
    if metrics.width == 0 || metrics.height == 0 {
        return;
    }
    let baseline = cy as i32 + ascent as i32;
    let gx = cx as i32 + metrics.xmin;
    let gy = baseline - metrics.ymin - metrics.height as i32;
    for row in 0..metrics.height {
        for col in 0..metrics.width {
            let alpha = bitmap[row * metrics.width + col];
            if alpha == 0 {
                continue;
            }
            let px = gx + col as i32;
            let py = gy + row as i32;
            if px < 0 || py < 0 {
                continue;
            }
            let (px, py) = (px as u32, py as u32);
            if px >= img.width() || py >= img.height() {
                continue;
            }
            let existing = *img.get_pixel(px, py);
            let blended = blend(existing, color, alpha);
            img.put_pixel(px, py, blended);
        }
    }
}

fn blend(bg: Rgba<u8>, fg: Rgba<u8>, alpha: u8) -> Rgba<u8> {
    let a = alpha as f32 / 255.0;
    Rgba([
        (fg[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8,
        (fg[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8,
        (fg[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8,
        255,
    ])
}

// ---------- tests ----------

const SAMPLE_MD: &str = "\
# Hello, World!

This is **bold** and *italic* and `inline code`.

- first item
- second item

> quoted text

```rust
fn main() {
    println!(\"hi\");
}
```
";

#[test]
fn snapshot_raw_editor() {
    let mut editor = make_editor(SAMPLE_MD);
    render_editor_to_png(&mut editor, 60, 18, &snapshot_path("raw_editor"));
}

#[test]
fn snapshot_preview_mode() {
    let mut editor = make_editor(SAMPLE_MD);
    editor.mode = RenderMode::Preview;
    // Move cursor out of every block so nothing falls back to raw.
    editor.cursor.line = 999;
    render_editor_to_png(&mut editor, 60, 18, &snapshot_path("preview_mode"));
}

#[test]
fn snapshot_inline_mode_switch() {
    let mut editor = make_editor(SAMPLE_MD);
    editor.mode = RenderMode::Preview;
    // Cursor on the heading — it should display as raw while the rest stay styled.
    editor.cursor.line = 0;
    render_editor_to_png(&mut editor, 60, 18, &snapshot_path("inline_mode_switch"));
}

#[test]
fn snapshot_big_heading() {
    let md = "# Big Heading\n\nBody text follows.\n";
    let mut editor = make_editor(md);
    editor.mode = RenderMode::Preview;
    editor.cursor.line = 999;
    render_editor_to_png(&mut editor, 80, 10, &snapshot_path("big_heading"));
}

#[test]
fn snapshot_table() {
    let md = "\
Here is a table:

| Name   | Age | Role  |
|--------|----:|:-----:|
| Alice  |  30 | Admin |
| Bob    |  25 | User  |
| Carol  | 100 | Owner |
";
    let mut editor = make_editor(md);
    editor.mode = RenderMode::Preview;
    editor.cursor.line = 999;
    render_editor_to_png(&mut editor, 60, 15, &snapshot_path("table"));
}

#[test]
fn snapshot_selection() {
    let md = "Select this text with the mouse or shift+arrow.\nAnother line below.\n";
    let mut editor = make_editor(md);
    editor.selection_anchor = Some(7);
    editor.cursor.line = 0;
    editor.cursor.col = 21;
    render_editor_to_png(&mut editor, 60, 6, &snapshot_path("selection"));
}

#[test]
fn snapshot_merge_view() {
    let mine = "\
# Project status

- [x] scaffolding
- [ ] preview mode
- [ ] plugins

Notes: ongoing, TBD details.
";
    let theirs = "\
# Project status

- [x] scaffolding
- [x] preview mode
- [ ] plugins
- [ ] theme

Notes: see DESIGN.md.
";
    let state = MergeState::new(mine.to_string(), theirs.to_string(), [0; 32]).unwrap();
    let mut editor = make_editor(mine);
    editor.merge = Some(state);
    // Mark one hunk as decided to show the styling difference.
    editor.merge.as_mut().unwrap().hunks[0].decision = Decision::Theirs;
    render_editor_to_png(&mut editor, 80, 22, &snapshot_path("merge_view"));
}

#[test]
fn snapshot_custom_theme() {
    let theme = Theme {
        heading1: "#00ff88".into(),
        inline_code: "#ff88cc".into(),
        border: "rgb(120, 120, 180)".into(),
        status_bg: "#202040".into(),
        status_fg: "lightcyan".into(),
        ..Theme::default()
    };

    let mut doc = Document::empty();
    doc.rope = Rope::from_str(SAMPLE_MD);
    let mut editor = Editor::with_theme(doc, PluginHost::new(Vec::new()), theme);
    editor.mode = RenderMode::Preview;
    editor.cursor.line = 999;
    render_editor_to_png(&mut editor, 60, 18, &snapshot_path("custom_theme"));
}
