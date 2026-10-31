pub mod commands;
pub use commands::Command;

use std::collections::VecDeque;
use crate::document::{Changes, Document, merge_changes};

pub struct History {
    pub max_size: usize,
    undo_stack: VecDeque<Command>,
    redo_stack: Vec<Command>,
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
    pub fn push(&mut self, cmd: Command) {
        self.undo_stack.push_back(cmd);
        if self.undo_stack.len() > self.max_size {
            self.undo_stack.pop_front();
        }
        self.redo_stack.clear();
        self.open_group = None;
    }

    /// Records drawn changes; consecutive pushes with the same `group`
    /// (e.g. one typing session) become a single undo step.
    pub fn push_grouped(&mut self, layer_idx: usize, changes: Changes, group: u64) {
        if self.open_group == Some(group)
            && let Some(Command::Draw { layer_idx: top_layer, changes: top }) = self.undo_stack.back_mut()
            && *top_layer == layer_idx
        {
            merge_changes(top, changes);
            self.redo_stack.clear();
            return;
        }
        self.push(Command::Draw { layer_idx, changes });
        self.open_group = Some(group);
    }

    /// Applies a command to the document and records it.
    pub fn execute(&mut self, cmd: Command, doc: &mut Document) {
        apply_command(&cmd, doc);
        self.push(cmd);
    }

    pub fn undo(&mut self, doc: &mut Document) -> bool {
        self.open_group = None;
        if let Some(cmd) = self.undo_stack.pop_back() {
            apply_inverse(&cmd, doc);
            self.redo_stack.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        self.open_group = None;
        if let Some(cmd) = self.redo_stack.pop() {
            apply_command(&cmd, doc);
            self.undo_stack.push_back(cmd);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool { !self.undo_stack.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo_stack.is_empty() }

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
