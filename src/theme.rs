use ratatui::style::Color;
use serde::{Deserialize, Serialize};

/// User-overridable colours. Each field is a string so the TOML form is
/// ergonomic (`heading1 = "magenta"` or `heading1 = "#bc3fbc"`). Unknown
/// names fall back to `Color::Reset` which the terminal renders with its
/// default foreground.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    #[serde(default = "d_h1")]
    pub heading1: String,
    #[serde(default = "d_h2")]
    pub heading2: String,
    #[serde(default = "d_h3")]
    pub heading3: String,
    #[serde(default = "d_h4")]
    pub heading4: String,
    #[serde(default = "d_h5")]
    pub heading5: String,
    #[serde(default = "d_h6")]
    pub heading6: String,

    #[serde(default = "d_inline_code")]
    pub inline_code: String,
    #[serde(default = "d_code_block")]
    pub code_block: String,
    #[serde(default = "d_link")]
    pub link: String,
    #[serde(default = "d_image")]
    pub image: String,
    #[serde(default = "d_border")]
    pub border: String,
    #[serde(default = "d_status_bg")]
    pub status_bg: String,
    #[serde(default = "d_status_fg")]
    pub status_fg: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            heading1: d_h1(),
            heading2: d_h2(),
            heading3: d_h3(),
            heading4: d_h4(),
            heading5: d_h5(),
            heading6: d_h6(),
            inline_code: d_inline_code(),
            code_block: d_code_block(),
            link: d_link(),
            image: d_image(),
            border: d_border(),
            status_bg: d_status_bg(),
            status_fg: d_status_fg(),
        }
    }
}

impl Theme {
    pub fn heading(&self, level: u8) -> Color {
        match level {
            1 => parse_color(&self.heading1),
            2 => parse_color(&self.heading2),
            3 => parse_color(&self.heading3),
            4 => parse_color(&self.heading4),
            5 => parse_color(&self.heading5),
            _ => parse_color(&self.heading6),
        }
    }
    pub fn inline_code(&self) -> Color {
        parse_color(&self.inline_code)
    }
    pub fn code_block(&self) -> Color {
        parse_color(&self.code_block)
    }
    pub fn link(&self) -> Color {
        parse_color(&self.link)
    }
    pub fn image(&self) -> Color {
        parse_color(&self.image)
    }
    pub fn border(&self) -> Color {
        parse_color(&self.border)
    }
    pub fn status_bg(&self) -> Color {
        parse_color(&self.status_bg)
    }
    pub fn status_fg(&self) -> Color {
        parse_color(&self.status_fg)
    }
}

/// Accepts named ANSI colours (`"red"`, `"darkgray"`, …), hex (`"#rrggbb"`),
/// and `"rgb(r,g,b)"`. Case-insensitive. Unknown input → `Color::Reset`.
pub fn parse_color(s: &str) -> Color {
    let t = s.trim().to_lowercase();
    match t.as_str() {
        "reset" | "default" | "" => Color::Reset,
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" | "purple" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Color::DarkGray,
        "lightred" | "light_red" => Color::LightRed,
        "lightgreen" | "light_green" => Color::LightGreen,
        "lightyellow" | "light_yellow" => Color::LightYellow,
        "lightblue" | "light_blue" => Color::LightBlue,
        "lightmagenta" | "light_magenta" => Color::LightMagenta,
        "lightcyan" | "light_cyan" => Color::LightCyan,
        "white" => Color::White,
        _ => parse_numeric_color(&t).unwrap_or(Color::Reset),
    }
}

fn parse_numeric_color(s: &str) -> Option<Color> {
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            let r: u8 = parts[0].parse().ok()?;
            let g: u8 = parts[1].parse().ok()?;
            let b: u8 = parts[2].parse().ok()?;
            return Some(Color::Rgb(r, g, b));
        }
    }
    None
}

fn d_h1() -> String {
    "magenta".into()
}
fn d_h2() -> String {
    "blue".into()
}
fn d_h3() -> String {
    "cyan".into()
}
fn d_h4() -> String {
    "green".into()
}
fn d_h5() -> String {
    "yellow".into()
}
fn d_h6() -> String {
    "red".into()
}
fn d_inline_code() -> String {
    "lightyellow".into()
}
fn d_code_block() -> String {
    "lightyellow".into()
}
fn d_link() -> String {
    "cyan".into()
}
fn d_image() -> String {
    "magenta".into()
}
fn d_border() -> String {
    "darkgray".into()
}
fn d_status_bg() -> String {
    "darkgray".into()
}
fn d_status_fg() -> String {
    "white".into()
}
