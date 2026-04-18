use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub enum WatchEvent {
    Changed,
    Removed,
    Error(String),
}

pub struct FileWatcher {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    pub rx: Receiver<WatchEvent>,
}

impl FileWatcher {
    pub fn new(path: &Path) -> Result<Self> {
        let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let parent = target
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let (tx, rx) = channel::<WatchEvent>();
        let target_cb = target.clone();
        let tx_cb = tx.clone();
        let mut debouncer = new_debouncer(
            Duration::from_millis(300),
            None,
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    for ev in events {
                        if !ev.event.paths.iter().any(|p| paths_match(p, &target_cb)) {
                            continue;
                        }
                        use notify::EventKind;
                        let kind = match ev.event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) => WatchEvent::Changed,
                            EventKind::Remove(_) => WatchEvent::Removed,
                            _ => continue,
                        };
                        let _ = tx_cb.send(kind);
                    }
                }
                Err(errors) => {
                    for e in errors {
                        let _ = tx_cb.send(WatchEvent::Error(e.to_string()));
                    }
                }
            },
        )
        .context("spawning file watcher")?;
        debouncer
            .watch(&parent, RecursiveMode::NonRecursive)
            .with_context(|| format!("watching {}", parent.display()))?;
        Ok(Self {
            _debouncer: debouncer,
            rx,
        })
    }
}

fn paths_match(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}
