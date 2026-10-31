pub mod commands;
pub use commands::Command;

use std::collections::VecDeque;
use crate::document::{Changes, Document, merge_changes};

/// A recorded command with the name shown in the History panel.
struct Entry {
    cmd: Command,
    label: String,
}

pub struct History {
    pub max_size: usize,
    undo_stack: VecDeque<Entry>,
    redo_stack: Vec<Entry>,
    /// Group of the newest command, if it may still absorb more changes.
    open_group: Option<u64>,
}

impl History {
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            open_group: None,
        }
    }

    /// Records a command whose effect is already applied to the document.
    #[cfg(test)]
    pub fn push(&mut self, cmd: Command) {
        let label = cmd.label();
        self.push_as(cmd, label);
    }

    /// Like [`Self::push`], with a name for the History panel.
    pub fn push_as(&mut self, cmd: Command, label: impl Into<String>) {
        self.undo_stack.push_back(Entry { cmd, label: label.into() });
        if self.undo_stack.len() > self.max_size {
            self.undo_stack.pop_front();
        }
        self.redo_stack.clear();
        self.open_group = None;
    }

    /// Records drawn changes; consecutive pushes with the same `group`
    /// (e.g. one typing session) become a single undo step.
    pub fn push_grouped(&mut self, layer_idx: usize, changes: Changes, group: u64, label: &str) {
        if self.open_group == Some(group)
            && let Some(Entry { cmd: Command::Draw { layer_idx: top_layer, changes: top }, .. }) = self.undo_stack.back_mut()
            && *top_layer == layer_idx
        {
            merge_changes(top, changes);
            self.redo_stack.clear();
            return;
        }
        self.push_as(Command::Draw { layer_idx, changes }, label);
        self.open_group = Some(group);
    }

    /// Applies a command to the document and records it.
    pub fn execute(&mut self, cmd: Command, doc: &mut Document) {
        let label = cmd.label();
        self.execute_as(cmd, label, doc);
    }

    pub fn execute_as(&mut self, cmd: Command, label: impl Into<String>, doc: &mut Document) {
        apply_command(&cmd, doc);
        self.push_as(cmd, label);
    }

    pub fn undo(&mut self, doc: &mut Document) -> bool {
        self.open_group = None;
        if let Some(entry) = self.undo_stack.pop_back() {
            apply_inverse(&entry.cmd, doc);
            self.redo_stack.push(entry);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        self.open_group = None;
        if let Some(entry) = self.redo_stack.pop() {
            apply_command(&entry.cmd, doc);
            self.undo_stack.push_back(entry);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool { !self.undo_stack.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo_stack.is_empty() }

    /// Names of the applied steps (oldest first) and of the undone ones (next redo first).
    pub fn labels(&self) -> (Vec<&str>, Vec<&str>) {
        (
            self.undo_stack.iter().map(|e| e.label.as_str()).collect(),
            self.redo_stack.iter().rev().map(|e| e.label.as_str()).collect(),
        )
    }

    /// Undoes or redoes until exactly `applied` steps are applied.
    pub fn jump_to(&mut self, applied: usize, doc: &mut Document) {
        while self.undo_stack.len() > applied && self.undo(doc) {}
        while self.undo_stack.len() < applied && self.redo(doc) {}
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.open_group = None;
    }
}

// Undo / redo write through `Layer::put`, so they work on locked layers too.
fn write_cells(doc: &mut Document, layer_idx: usize, changes: &Changes, new: bool) {
    if let Some(layer) = doc.layers.get_mut(layer_idx) {
        for (&(x, y), (old_cell, new_cell)) in changes {
            layer.put(x, y, if new { new_cell.clone() } else { old_cell.clone() });
        }
    }
}

fn apply_command(cmd: &Command, doc: &mut Document) {
    match cmd {
        Command::Draw { layer_idx, changes } => {
            write_cells(doc, *layer_idx, changes, true);
        }
        Command::LayerAdd { index, layer } => {
            doc.insert_layer(*index, layer.clone());
        }
        Command::LayerRemove { index, .. } => {
            doc.take_layer(*index);
        }
        Command::LayerMove { from, to } => {
            doc.move_layer(*from, *to);
        }
        Command::LayerMergeDown { upper, changes, .. } => {
            write_cells(doc, upper - 1, changes, true);
            doc.take_layer(*upper);
            doc.active_layer = upper - 1;
        }
        Command::LayerRename { index, new, .. } => {
            if let Some(layer) = doc.layers.get_mut(*index) {
                layer.name = new.clone();
            }
        }
        Command::Resize { new_size, .. } => {
            doc.resize(new_size.0, new_size.1);
        }
    }
    doc.modified = true;
}

fn apply_inverse(cmd: &Command, doc: &mut Document) {
    match cmd {
        Command::Draw { layer_idx, changes } => {
            write_cells(doc, *layer_idx, changes, false);
        }
        Command::LayerAdd { index, .. } => {
            doc.take_layer(*index);
            doc.active_layer = index.saturating_sub(1).min(doc.layers.len() - 1);
        }
        Command::LayerRemove { index, layer } => {
            doc.insert_layer(*index, layer.clone());
        }
        Command::LayerMove { from, to } => {
            doc.move_layer(*to, *from);
        }
        Command::LayerMergeDown { upper, layer, changes } => {
            write_cells(doc, upper - 1, changes, false);
            doc.insert_layer(*upper, layer.clone());
        }
        Command::LayerRename { index, old, .. } => {
            if let Some(layer) = doc.layers.get_mut(*index) {
                layer.name = old.clone();
            }
        }
        Command::Resize { old_size, old_layers, .. } => {
            doc.layers = old_layers.clone();
            doc.width = old_size.0;
            doc.height = old_size.1;
            doc.active_layer = doc.active_layer.min(doc.layers.len() - 1);
        }
    }
    doc.modified = true;
}
