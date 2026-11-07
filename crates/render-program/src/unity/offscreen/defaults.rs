//! Constants for the vertex inputs a mesh has no channel for.
//!
//! A mesh stores only the attribute channels it has: the built-in primitives have no colour, and
//! many meshes of the Old World player have none either. A pass that reads such a channel is not
//! left out; the input reads one constant at every vertex instead, from a small shared buffer
//! bound with a stride of zero.
//!
//! The rule:
//! - Only the optional attribute channels are defaulted: normal (1), tangent (2), colour (3) and
//!   the texture coordinate sets (4 to 11). A mesh without positions (0) is not a mesh, and the
//!   blend weights and indices (12 and 13) are not stored by this renderer, so a pass that reads
//!   them is left out rather than drawn with weights that were never there.
//! - Colour defaults to (1, 1, 1, 1), the identity of the multiplication the input takes part in.
//!   Every other channel defaults to zeros.
//! - The components and type are the ones the vertex stage declares for the input. A colour input
//!   that is not a float has no multiplicative identity to read, so it is not defaulted.

use std::collections::HashMap;

use naga::{Binding, ScalarKind, TypeInner};

use crate::resources::BufferHandle;
use crate::runtime::ProgramRuntime;
use crate::unity::module::StageProgram;

use super::error::OffscreenError;

/// The channel number of the vertex colour in a mesh's table of channels, which the shaders'
/// vertex input tables use too: 0 position, 1 normal, 2 tangent, 3 colour, 4 to 11 the texture
/// coordinate sets 0 to 7.
pub(super) const COLOUR: i8 = 3;
/// The optional attribute channels: normal up to the last texture coordinate set.
const OPTIONAL: std::ops::RangeInclusive<i8> = 1..=11;

/// The bytes of the widest input, four components of four bytes.
const WIDEST: usize = 16;

/// The constant an input reads when its mesh lacks the channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fill {
    Zeros,
    Ones,
}

/// What a vertex stage declares for one input: the format that supplies its components and
/// whether they are floats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Declared {
    pub(super) format: wgpu::VertexFormat,
    float: bool,
}

/// The inputs the vertex stage of a program declares, by location. An input the stage declares
/// with a type no vertex format supplies is listed without a declaration.
#[derive(Default)]
pub(super) struct DeclaredInputs(HashMap<u32, Option<Declared>>);

/// What to bind for an input and how to read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Constant {
    pub(super) fill: Fill,
    pub(super) format: wgpu::VertexFormat,
}

/// The constant for the input at `location` that source channel `channel` feeds, or `None` when
/// the pass cannot be drawn without the channel.
pub(super) fn default_for(channel: i8, location: u32, inputs: &DeclaredInputs) -> Option<Constant> {
    if !OPTIONAL.contains(&channel) {
        return None;
    }
    // A location the stage does not declare is not read, so any format will do.
    let declared = match inputs.0.get(&location) {
        Some(declared) => (*declared)?,
        None => Declared {
            format: wgpu::VertexFormat::Float32x4,
            float: true,
        },
    };
    let fill = match (channel == COLOUR, declared.float) {
        (true, true) => Fill::Ones,
        (true, false) => return None,
        (false, _) => Fill::Zeros,
    };
    Some(Constant {
        fill,
        format: declared.format,
    })
}

/// The inputs `stage` reads, by location, with the vertex format that supplies each.
pub(super) fn declared_inputs(stage: &StageProgram) -> DeclaredInputs {
    let module = stage.naga_module();
    let mut inputs = DeclaredInputs::default();
    let Some(entry) = module
        .entry_points
        .iter()
        .find(|entry| entry.name == stage.module().entry_point)
    else {
        return inputs;
    };
    for argument in &entry.function.arguments {
        match (&argument.binding, &module.types[argument.ty].inner) {
            (Some(binding), inner) => note(&mut inputs, binding, inner),
            (None, TypeInner::Struct { members, .. }) => {
                for member in members {
                    if let Some(binding) = &member.binding {
                        note(&mut inputs, binding, &module.types[member.ty].inner);
                    }
                }
            }
            (None, _) => {}
        }
    }
    inputs
}

fn note(inputs: &mut DeclaredInputs, binding: &Binding, inner: &TypeInner) {
    if let Binding::Location { location, .. } = binding {
        inputs.0.insert(*location, declared(inner));
    }
}

fn declared(inner: &TypeInner) -> Option<Declared> {
    use wgpu::VertexFormat as F;
    let (scalar, count) = match inner {
        TypeInner::Scalar(scalar) => (scalar, 1),
        TypeInner::Vector { size, scalar } => (scalar, *size as u8),
        _ => return None,
    };
    if scalar.width != 4 {
        return None;
    }
    let (format, float) = match (scalar.kind, count) {
        (ScalarKind::Float, 1) => (F::Float32, true),
        (ScalarKind::Float, 2) => (F::Float32x2, true),
        (ScalarKind::Float, 3) => (F::Float32x3, true),
        (ScalarKind::Float, _) => (F::Float32x4, true),
        (ScalarKind::Uint, 1) => (F::Uint32, false),
        (ScalarKind::Uint, 2) => (F::Uint32x2, false),
        (ScalarKind::Uint, 3) => (F::Uint32x3, false),
        (ScalarKind::Uint, _) => (F::Uint32x4, false),
        (ScalarKind::Sint, 1) => (F::Sint32, false),
        (ScalarKind::Sint, 2) => (F::Sint32x2, false),
        (ScalarKind::Sint, 3) => (F::Sint32x3, false),
        (ScalarKind::Sint, _) => (F::Sint32x4, false),
        _ => return None,
    };
    Some(Declared { format, float })
}

/// The two constant buffers every default is read from, which the state keeps for good.
#[derive(Clone, Copy)]
pub(super) struct DefaultBuffers {
    zeros: BufferHandle,
    ones: BufferHandle,
}

impl DefaultBuffers {
    pub(super) fn create(runtime: &mut ProgramRuntime) -> Result<Self, OffscreenError> {
        let ones: Vec<u8> = (0..WIDEST / 4)
            .flat_map(|_| 1.0_f32.to_le_bytes())
            .collect();
        Ok(Self {
            zeros: runtime.create_buffer("offscreen default zeros", &[0; WIDEST])?,
            ones: runtime.create_buffer("offscreen default ones", &ones)?,
        })
    }

    pub(super) fn buffer(self, fill: Fill) -> BufferHandle {
        match fill {
            Fill::Zeros => self.zeros,
            Fill::Ones => self.ones,
        }
    }

    /// The bytes of each buffer.
    pub(super) const LEN: u64 = WIDEST as u64;
}
