//! User-configurable keybinds. Actions are referenced by stable string names
//! (`"toggle_preview"`, `"save"`, etc.), keys by descriptors like
//! `"Ctrl+Shift+P"` or `"F2"`. Plain letter keys (without a modifier) always
//! fall through to text insertion, so the user can't lock themselves out by
//! mapping every letter.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

use crate::editor::input::Action;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KeybindEntry {
    pub key: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyDescriptor {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, Default)]
pub struct KeybindTable {
    table: HashMap<KeyDescriptor, Action>,
}

impl KeybindTable {
    pub fn from_config(entries: &[KeybindEntry]) -> (Self, Vec<String>) {
        let mut table = HashMap::new();
        let mut errors = Vec::new();
        for e in entries {
            let Some(desc) = parse_key(&e.key) else {
                errors.push(format!("unrecognised key: {:?}", e.key));
                continue;
            };
            let Some(action) = action_from_name(&e.action) else {
                errors.push(format!("unrecognised action: {:?}", e.action));
                continue;
            };
            table.insert(desc, action);
        }
        (Self { table }, errors)
    }

    pub fn lookup(&self, key: &KeyEvent) -> Option<Action> {
        // Use a "matchable" modifier set — crossterm reports shifted letters
        // both as uppercase Char + SHIFT and, on some terminals, just the
        // uppercase Char with no SHIFT. Normalise before lookup.
        let modifiers = normalise_modifiers(key);
        let desc = KeyDescriptor {
            code: key.code,
            modifiers,
        };
        self.table.get(&desc).cloned()
    }
}

fn normalise_modifiers(key: &KeyEvent) -> KeyModifiers {
    // Drop SHIFT when the code is an uppercase Char — otherwise
    // "Shift+A" and "A" wouldn't match the same entry.
    let mut m = key.modifiers;
    if let KeyCode::Char(c) = key.code {
        if c.is_ascii_uppercase() {
            m.remove(KeyModifiers::SHIFT);
        }
    }
    m
}

pub fn parse_key(s: &str) -> Option<KeyDescriptor> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }
    let mut modifiers = KeyModifiers::empty();
    for part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "alt" | "meta" | "option" => modifiers |= KeyModifiers::ALT,
            _ => return None,
        }
    }
    let last = parts.last()?.to_lowercase();
    let code = match last.as_str() {
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" | "bksp" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        s if s.starts_with('f') && s.len() <= 3 => {
            let n: u8 = s[1..].parse().ok()?;
            if !(1..=24).contains(&n) {
                return None;
            }
            KeyCode::F(n)
        }
        s if s.chars().count() == 1 => {
            let c = s.chars().next()?;
            // Store Ctrl/Alt-letter combos as the lowercase char so a config
            // `"Ctrl+S"` and a runtime Ctrl+s lookup both match.
            if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                KeyCode::Char(c.to_ascii_lowercase())
            } else {
                KeyCode::Char(c)
            }
        }
        _ => return None,
    };
    Some(KeyDescriptor { code, modifiers })
}

pub fn action_from_name(s: &str) -> Option<Action> {
    match s {
        "save" => Some(Action::Save),
        "quit" => Some(Action::Quit),
        "copy" => Some(Action::Copy),
        "cut" => Some(Action::Cut),
        "paste_hint" => Some(Action::PasteHint),
        "select_all" => Some(Action::SelectAll),
        "toggle_preview" => Some(Action::TogglePreview),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_keys() {
        assert_eq!(
            parse_key("Enter"),
            Some(KeyDescriptor {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::empty(),
            })
        );
        assert_eq!(
            parse_key("F2"),
            Some(KeyDescriptor {
                code: KeyCode::F(2),
                modifiers: KeyModifiers::empty(),
            })
        );
    }

    #[test]
    fn parse_modifier_combos() {
        assert_eq!(
            parse_key("Ctrl+S"),
            Some(KeyDescriptor {
                code: KeyCode::Char('s'),
                modifiers: KeyModifiers::CONTROL,
            })
        );
        assert_eq!(
            parse_key("Ctrl+Shift+P"),
            Some(KeyDescriptor {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            })
        );
    }

    #[test]
    fn parse_rejects_garbage() {
        assert!(parse_key("Ctrl+").is_none());
        assert!(parse_key("SuperKey").is_none());
        assert!(parse_key("Ctrl+Shift+SuperLong").is_none());
    }

    #[test]
    fn table_lookup_finds_user_binding() {
        let entries = vec![
            KeybindEntry {
                key: "Ctrl+E".into(),
                action: "toggle_preview".into(),
            },
            KeybindEntry {
                key: "F5".into(),
                action: "save".into(),
            },
        ];
        let (table, errors) = KeybindTable::from_config(&entries);
        assert!(errors.is_empty());
        let ev = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
        assert!(matches!(table.lookup(&ev), Some(Action::TogglePreview)));
        let f5 = KeyEvent::new(KeyCode::F(5), KeyModifiers::empty());
        assert!(matches!(table.lookup(&f5), Some(Action::Save)));
    }

    #[test]
    fn table_reports_errors_on_bad_entries() {
        let entries = vec![
            KeybindEntry {
                key: "Ctrl+!".into(),
                action: "save".into(),
            },
            KeybindEntry {
                key: "Ctrl+Q".into(),
                action: "nuke".into(),
            },
        ];
        let (_table, errors) = KeybindTable::from_config(&entries);
        assert_eq!(errors.len(), 1); // "Ctrl+!" parses as valid (char = '!'); only "nuke" fails.
        assert!(errors[0].contains("nuke"));
    }
}
