use crate::document::{Blend, Changes, FrameData, Layer, Scene3D};

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
        layer: Box<Layer>,
        below: Box<Layer>,
        merged: Box<Layer>,
    },
    LayerRename {
        index: usize,
        old: String,
        new: String,
    },
    /// Blend mode and opacity.
    LayerStyle {
        index: usize,
        old: (Blend, f32),
        new: (Blend, f32),
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
    /// A 3D layer's scene; `None` is an ordinary layer (rasterized).
    Scene {
        index: usize,
        old: Option<Box<Scene3D>>,
        new: Option<Box<Scene3D>>,
    },
    /// Several commands as one step (e.g. a script); undone in reverse order.
    Group(Vec<Command>),
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
            Command::LayerStyle { .. }     => "Layer Style",
            Command::Resize { .. }         => "Resize Canvas",
            Command::FrameAdd { .. }       => "New Frame",
            Command::FrameRemove { .. }    => "Delete Frame",
            Command::FrameMove { .. }      => "Move Frame",
            Command::FrameTiming { .. }    => "Frame Timing",
            Command::Group(_)              => "Script",
            Command::Scene { .. }          => "3D",
        }
    }
}
