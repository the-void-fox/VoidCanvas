use crate::document::{Changes, FrameData, Layer};

#[derive(Clone, Debug)]
pub enum Command {
    Draw {
        layer_idx: usize,
        /// The animation frame drawn on; undo goes back to it.
        frame: usize,
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
    /// Bakes the layer at `upper` into the layer below it (in every frame) and removes it.
    LayerMergeDown {
        upper: usize,
        layer: Layer,
        below: Layer,
        merged: Layer,
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
    /// Also used for "Duplicate Frame".
    FrameAdd {
        index: usize,
        data: FrameData,
    },
    FrameRemove {
        index: usize,
        data: FrameData,
    },
    FrameMove {
        from: usize,
        to: usize,
    },
    FrameTiming {
        old: Vec<u32>,
        new: Vec<u32>,
    },
}

impl Command {
    /// Name in the History panel when the caller gives none.
    pub fn label(&self) -> &'static str {
        match self {
            Command::Draw { .. }           => "Edit",
            Command::LayerAdd { .. }       => "New Layer",
            Command::LayerRemove { .. }    => "Delete Layer",
            Command::LayerMove { .. }      => "Move Layer",
            Command::LayerMergeDown { .. } => "Merge Down",
            Command::LayerRename { .. }    => "Rename Layer",
            Command::Resize { .. }         => "Resize Canvas",
            Command::FrameAdd { .. }       => "New Frame",
            Command::FrameRemove { .. }    => "Delete Frame",
            Command::FrameMove { .. }      => "Move Frame",
            Command::FrameTiming { .. }    => "Frame Timing",
        }
    }
}
