//! Undo and redo for every edit.
//!
//! Projects are cheap to clone (layer pixels are shared copy-on-write), so history is a stack of
//! whole-project snapshots. Each frame the app keeps the project as it was when the frame began;
//! if the frame edited the document, that snapshot becomes an undo step. Continuous edits (one
//! brush stroke, one drag, typing in one field) are coalesced into a single step.

use boardcraft_core::{Id, Project};

/// Most undo steps kept. Each step shares unchanged pixels with its neighbours; a step after a
/// stroke keeps the stroked layer's previous pixels alive.
pub const LIMIT: usize = 60;

pub struct Snapshot {
    pub project: Project,
    pub selected: Option<Id>,
    pub dirty: bool,
}

/// What groups edits into one step: the pointer press, or the focused text field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Press(u64),
    Focus(egui::Id),
    Other,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    frame_start: Option<(Snapshot, u64)>,
    last: Option<Gesture>,
}

impl History {
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.frame_start = None;
        self.last = None;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    #[cfg(test)]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Remember the document as the frame begins.
    pub fn begin_frame(&mut self, project: &Project, selected: Option<Id>, dirty: bool, edits: u64) {
        self.frame_start = Some((Snapshot { project: project.clone(), selected, dirty }, edits));
    }

    /// After the frame: record a step if it edited the document.
    pub fn end_frame(&mut self, edits: u64, gesture: Gesture) {
        let Some((before, start_edits)) = self.frame_start.take() else { return };
        if edits == start_edits {
            if !matches!(gesture, Gesture::Press(_)) {
                // A press or focus that ended without edits closes the gesture.
                if self.last.is_some_and(|g| g != gesture) {
                    self.last = None;
                }
            }
            return;
        }
        let continuing = gesture != Gesture::Other && self.last == Some(gesture);
        self.last = Some(gesture);
        if continuing {
            return;
        }
        self.push(before);
    }

    /// Record a step explicitly (edits made outside a frame).
    pub fn push(&mut self, snapshot: Snapshot) {
        self.undo.push(snapshot);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Step back: returns the snapshot to restore; `current` goes on the redo stack.
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let s = self.undo.pop()?;
        self.redo.push(current);
        self.last = None;
        Some(s)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let s = self.redo.pop()?;
        self.undo.push(current);
        self.last = None;
        Some(s)
    }
}
