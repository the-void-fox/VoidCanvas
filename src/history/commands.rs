use crate::document::{Changes, Layer};

#[derive(Clone, Debug)]
pub enum Command {
    Draw {
        layer_idx: usize,
        changes: Changes,
    },
    /// Also used for "Duplicate layer".
    LayerAdd {
        index: usize,
        layer: Layer,
    },
    LayerRemove {
        index: usize,
        layer: Layer,
    },
    LayerMove {
        from: usize,
        to: usize,
    },
    /// Bakes the layer at `upper` into the layer below it and removes it.
    LayerMergeDown {
        upper: usize,
        layer: Layer,
        changes: Changes,
    },
    LayerRename {
        index: usize,
        old: String,
        new: String,
    },
    Resize {
        old_size: (usize, usize),
        new_size: (usize, usize),
        old_layers: Vec<Layer>,
    },
}
