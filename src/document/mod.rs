pub mod cell;
pub mod layer;
#[allow(clippy::module_inception)]
pub mod document;

use std::collections::HashMap;

pub use cell::{Cell, Color};
pub use layer::Layer;
pub use document::Document;

/// Cell edits keyed by position: (before, after).
pub type Changes = HashMap<(usize, usize), (Cell, Cell)>;

/// Folds `more` into `into`, keeping the earliest "before" and the latest "after" of each cell.
pub fn merge_changes(into: &mut Changes, more: Changes) {
    for (pos, (old, new)) in more {
        into.entry(pos)
            .and_modify(|entry| entry.1 = new.clone())
            .or_insert((old, new));
    }
}
