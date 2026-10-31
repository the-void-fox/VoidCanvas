pub mod cell;
pub mod layer;
pub mod document;

use std::collections::HashMap;

pub use cell::{Cell, Color};
pub use layer::Layer;
pub use document::Document;

/// Cell edits keyed by position: (before, after).
pub type Changes = HashMap<(usize, usize), (Cell, Cell)>;
