//! What each slot's level choice reads that does not depend on the eye, filled with its bounds.
use worldrunner_render_plan::SceneFadeReference;

use crate::instances::fade::Anchor;

/// The per-slot level inputs of one batch, laid out by `Shape`: one squared distance ceiling
/// per level of detail, already scaled by the placement's LOD distance scale; its own fade
/// band and origin in metres; and one fade anchor per reference in `Shape::references`.
#[derive(Debug, Default)]
pub(crate) struct Inputs {
    shape: Shape,
    ceilings: Vec<u128>,
    fades: Vec<SlotFade>,
    anchors: Vec<Anchor>,
}

/// What a batch's mesh and materials make each slot store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Shape {
    /// Levels of detail of the batch's mesh.
    pub levels: usize,
    /// Distinct authored fade references of the batch's materials that can fade out.
    pub references: Vec<SceneFadeReference>,
}

/// A placement's own fade band in metres, as `fade_band_meters` converts it, and its origin.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SlotFade {
    pub band: Option<[f32; 2]>,
    pub origin: [f32; 3],
}

/// One slot's inputs, read during selection.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Slot<'a> {
    pub ceilings: &'a [u128],
    pub fade: SlotFade,
    pub references: &'a [SceneFadeReference],
    pub anchors: &'a [Anchor],
}

/// One slot's inputs, written when its bounds are filled.
pub(crate) struct SlotMut<'a> {
    pub ceilings: &'a mut [u128],
    pub fade: &'a mut SlotFade,
    pub anchors: &'a mut [Anchor],
}

impl Inputs {
    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Discards every slot's inputs and lays them out for `shape`.
    pub fn reset(&mut self, shape: &Shape) {
        self.shape = shape.clone();
        self.ceilings.clear();
        self.fades.clear();
        self.anchors.clear();
    }

    /// Makes room for `slots` slots.
    pub fn grow(&mut self, slots: usize) {
        if self.fades.len() < slots {
            self.fades.resize(slots, SlotFade::default());
            self.ceilings.resize(slots * self.shape.levels, 0);
            self.anchors
                .resize(slots * self.shape.references.len(), Anchor::default());
        }
    }

    pub fn slot_mut(&mut self, position: usize) -> SlotMut<'_> {
        let (levels, references) = (self.shape.levels, self.shape.references.len());
        SlotMut {
            ceilings: &mut self.ceilings[position * levels..(position + 1) * levels],
            fade: &mut self.fades[position],
            anchors: &mut self.anchors[position * references..(position + 1) * references],
        }
    }

    pub fn slot(&self, position: usize) -> Slot<'_> {
        let (levels, references) = (self.shape.levels, self.shape.references.len());
        Slot {
            ceilings: &self.ceilings[position * levels..(position + 1) * levels],
            fade: self.fades[position],
            references: &self.shape.references,
            anchors: &self.anchors[position * references..(position + 1) * references],
        }
    }
}
