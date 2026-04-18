//! 3-way merge (M11) — per-hunk picker UI shown when the file on disk
//! diverges from a dirty buffer.
//!
//! The simplest meaningful design: take a 2-way diff between `mine` (the
//! local buffer) and `theirs` (the new disk content), split it into hunks
//! with surrounding context, and let the user decide per hunk which side to
//! keep. Decisions are applied by walking the `similar` change stream a
//! second time and emitting the chosen side's lines for each change group.
//!
//! This isn't a full 3-way merge with a common ancestor (DESIGN.md §4.6 long
//! form) — we don't retain the last-saved content. For the practical case
//! where two processes race on the same file, picking per hunk solves the UX
//! problem at a fraction of the complexity.

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// No choice yet. Treated as `Mine` on apply so the user's in-flight
    /// work isn't silently overwritten by a disk change they haven't looked at.
    Pending,
    Mine,
    Theirs,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    /// Context lines shown above the change block.
    pub context_before: Vec<String>,
    pub mine_lines: Vec<String>,
    pub theirs_lines: Vec<String>,
    pub context_after: Vec<String>,
    pub decision: Decision,
}

#[derive(Debug, Clone)]
pub struct MergeState {
    pub mine: String,
    pub theirs: String,
    pub hunks: Vec<Hunk>,
    pub current: usize,
    /// SHA-256 of `theirs` — stored here so the caller can update the
    /// document's `last_save_hash` after applying.
    pub theirs_hash: [u8; 32],
}

impl MergeState {
    /// Build a merge state by diffing `mine` vs `theirs`. Returns `None` if
    /// the two strings are identical (caller should silently reload instead).
    pub fn new(mine: String, theirs: String, theirs_hash: [u8; 32]) -> Option<Self> {
        if mine == theirs {
            return None;
        }
        let hunks = build_hunks(&mine, &theirs);
        if hunks.is_empty() {
            return None;
        }
        Some(Self {
            mine,
            theirs,
            hunks,
            current: 0,
            theirs_hash,
        })
    }

    pub fn next_hunk(&mut self) {
        if !self.hunks.is_empty() {
            self.current = (self.current + 1) % self.hunks.len();
        }
    }

    pub fn prev_hunk(&mut self) {
        if !self.hunks.is_empty() {
            self.current = if self.current == 0 {
                self.hunks.len() - 1
            } else {
                self.current - 1
            };
        }
    }

    pub fn set_current(&mut self, decision: Decision) {
        if let Some(h) = self.hunks.get_mut(self.current) {
            h.decision = decision;
        }
    }

    pub fn set_all(&mut self, decision: Decision) {
        for h in &mut self.hunks {
            h.decision = decision;
        }
    }

    pub fn pending_count(&self) -> usize {
        self.hunks
            .iter()
            .filter(|h| h.decision == Decision::Pending)
            .count()
    }

    /// Apply current decisions, producing the merged text. Pending hunks are
    /// treated as `Mine` (preserve local work by default).
    pub fn apply(&self) -> String {
        apply_decisions(&self.mine, &self.theirs, &self.hunks)
    }
}

fn build_hunks(mine: &str, theirs: &str) -> Vec<Hunk> {
    let diff = TextDiff::from_lines(mine, theirs);
    let mut hunks = Vec::new();
    for group in diff.grouped_ops(3) {
        let mut context_before = Vec::new();
        let mut mine_lines = Vec::new();
        let mut theirs_lines = Vec::new();
        let mut context_after = Vec::new();
        let mut seen_change = false;

        for op in &group {
            for change in diff.iter_changes(op) {
                let text = strip_trailing_newline(change.value()).to_string();
                match change.tag() {
                    ChangeTag::Equal => {
                        if seen_change {
                            context_after.push(text);
                        } else {
                            context_before.push(text);
                        }
                    }
                    ChangeTag::Delete => {
                        seen_change = true;
                        mine_lines.push(text);
                    }
                    ChangeTag::Insert => {
                        seen_change = true;
                        theirs_lines.push(text);
                    }
                }
            }
        }

        if seen_change {
            hunks.push(Hunk {
                context_before,
                mine_lines,
                theirs_lines,
                context_after,
                decision: Decision::Pending,
            });
        }
    }
    hunks
}

fn apply_decisions(mine: &str, theirs: &str, hunks: &[Hunk]) -> String {
    let diff = TextDiff::from_lines(mine, theirs);
    let mut out = String::new();
    let mut hunk_idx = 0;
    let mut in_change_run = false;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                if in_change_run {
                    in_change_run = false;
                    hunk_idx += 1;
                }
                out.push_str(change.value());
            }
            ChangeTag::Delete | ChangeTag::Insert => {
                in_change_run = true;
                let decision = hunks
                    .get(hunk_idx)
                    .map(|h| h.decision)
                    .unwrap_or(Decision::Pending);
                let take_mine = matches!(decision, Decision::Mine | Decision::Pending);
                let take_theirs = matches!(decision, Decision::Theirs);
                match change.tag() {
                    ChangeTag::Delete if take_mine => out.push_str(change.value()),
                    ChangeTag::Insert if take_theirs => out.push_str(change.value()),
                    _ => {}
                }
            }
        }
    }
    out
}

fn strip_trailing_newline(s: &str) -> &str {
    s.strip_suffix('\n')
        .map(|s| s.strip_suffix('\r').unwrap_or(s))
        .unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_yields_none() {
        let s = "one\ntwo\nthree\n".to_string();
        assert!(MergeState::new(s.clone(), s, [0; 32]).is_none());
    }

    #[test]
    fn single_hunk_produced() {
        let mine = "one\ntwo\nthree\n".to_string();
        let theirs = "one\ntwoX\nthree\n".to_string();
        let state = MergeState::new(mine, theirs, [0; 32]).unwrap();
        assert_eq!(state.hunks.len(), 1);
        assert_eq!(state.hunks[0].mine_lines, vec!["two"]);
        assert_eq!(state.hunks[0].theirs_lines, vec!["twoX"]);
    }

    #[test]
    fn apply_all_mine_yields_mine() {
        let mine = "one\ntwo\nthree\n".to_string();
        let theirs = "one\ntwoX\nthree\n".to_string();
        let mut state = MergeState::new(mine.clone(), theirs, [0; 32]).unwrap();
        state.set_all(Decision::Mine);
        assert_eq!(state.apply(), mine);
    }

    #[test]
    fn apply_all_theirs_yields_theirs() {
        let mine = "one\ntwo\nthree\n".to_string();
        let theirs = "one\ntwoX\nthree\n".to_string();
        let mut state = MergeState::new(mine, theirs.clone(), [0; 32]).unwrap();
        state.set_all(Decision::Theirs);
        assert_eq!(state.apply(), theirs);
    }

    #[test]
    fn per_hunk_picking_mixes_sides() {
        // Changes far enough apart that similar groups them as two hunks
        // (default context radius is 3).
        let mine = (1..=30)
            .map(|n| format!("line{n}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let theirs = mine
            .replace("line5", "line5_mod")
            .replace("line25", "line25_mod");
        let mut state = MergeState::new(mine, theirs, [0; 32]).unwrap();
        assert_eq!(state.hunks.len(), 2);
        state.hunks[0].decision = Decision::Mine;
        state.hunks[1].decision = Decision::Theirs;
        let out = state.apply();
        assert!(out.contains("line5\n"));
        assert!(out.contains("line25_mod\n"));
        assert!(!out.contains("line5_mod"));
        assert!(!out.contains("line25\n"));
    }
}
