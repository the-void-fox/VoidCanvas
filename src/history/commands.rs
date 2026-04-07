use std::collections::HashMap;
use crate::document::{Cell, Layer};

pub type CellChange = HashMap<(usize, usize), (Cell, Cell)>;

#[derive(Clone, Debug)]
pub enum Command {
    Draw {
        layer_idx: usize,
        changes: CellChange,
    },
    LayerAdd {
        index: usize,
    },
    LayerRemove {
        index: usize,
        layer_snapshot: Layer,
    },
    LayerMove {
        from: usize,
        to: usize,
    },
}