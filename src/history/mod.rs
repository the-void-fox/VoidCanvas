pub mod commands;
pub use commands::Command;

use crate::document::Document;

pub struct History {
    pub max_size: usize,
    undo_stack: Vec<Command>,
    redo_stack: Vec<Command>,
}

impl History {
    pub fn new(max_size: usize) -> Self {
        Self {
            max_size,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn push(&mut self, cmd: Command) {
        self.undo_stack.push(cmd);
        if self.undo_stack.len() > self.max_size {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, doc: &mut Document) -> bool {
        if let Some(cmd) = self.undo_stack.pop() {
            apply_inverse(&cmd, doc);
            self.redo_stack.push(cmd);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, doc: &mut Document) -> bool {
        if let Some(cmd) = self.redo_stack.pop() {
            apply_command(&cmd, doc);
            self.undo_stack.push(cmd);
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
    }
}

fn apply_command(cmd: &Command, doc: &mut Document) {
    match cmd {
        Command::Draw { layer_idx, changes } => {
            if let Some(layer) = doc.layers.get_mut(*layer_idx) {
                for (&(x, y), (_, new)) in changes {
                    layer.set(x, y, new.clone());
                }
            }
        }
        Command::LayerAdd { index } => {
            doc.active_layer = *index;
        }
        Command::LayerRemove { index, .. } => {
            doc.layers.remove(*index);
            if doc.active_layer >= doc.layers.len() {
                doc.active_layer = doc.layers.len().saturating_sub(1);
            }
        }
        Command::LayerMove { from, to } => {
            doc.move_layer(*from, *to);
        }
    }
}

fn apply_inverse(cmd: &Command, doc: &mut Document) {
    match cmd {
        Command::Draw { layer_idx, changes } => {
            if let Some(layer) = doc.layers.get_mut(*layer_idx) {
                for (&(x, y), (old, _)) in changes {
                    layer.set(x, y, old.clone());
                }
            }
        }
        Command::LayerAdd { index } => {
            doc.layers.remove(*index);
            if doc.active_layer >= doc.layers.len() {
                doc.active_layer = doc.layers.len().saturating_sub(1);
            }
        }
        Command::LayerRemove { index, layer_snapshot } => {
            doc.layers.insert(*index, layer_snapshot.clone());
            doc.active_layer = *index;
        }
        Command::LayerMove { from, to } => {
            doc.move_layer(*to, *from);
        }
    }
}