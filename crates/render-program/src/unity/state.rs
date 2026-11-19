//! Fixed-function render state from a pass's `ShaderState`, converted to the neutral
//! [`RenderState`](crate::state::RenderState).
//!
//! The enum values follow the managed engine's own definitions in `Disunity/Enums/Rendering.cs`
//! (`CullMode`, `CompareFunction`, `BlendMode`, `BlendOp`, `StencilOp` and `ColorWriteMask`).
//! `BlendOp` values above `Max` (the logical and advanced blend operations) have no wgpu
//! equivalent and are rejected as unknown values.

use worldrunner_unity_serialized::shader_state::{
    ShaderState, StateValue, Stencil, StencilOps, TargetBlend,
};

use crate::properties::{PropertySet, PropertyValue, lookup};
use crate::state::{ColorState, DepthState, RasterState, StencilState};

/// State a pass resolves to, for render target 0.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderState {
    pub cull: Option<wgpu::Face>,
    pub depth_compare: wgpu::CompareFunction,
    pub depth_write: bool,
    pub color_write: wgpu::ColorWrites,
    pub blend: Option<wgpu::BlendState>,
    /// The stencil test, operations and masks, and the reference this draw writes and compares
    /// with. `None` when the pass leaves the stencil buffer alone: a test that always passes and
    /// operations that keep.
    pub stencil: Option<StencilState>,
}

impl RenderState {
    /// The neutral state of a draw into a pass whose depth attachment has no stencil aspect,
    /// that treats `front_face` as the front winding: one colour target, depth tested and
    /// written as resolved, and no stencil, depth bias or alpha to coverage, which
    /// `resolve_state` has already refused. A target with no stencil aspect has nothing to test or
    /// write, so this engine drops the resolved stencil.
    #[must_use]
    pub fn neutral(&self, front_face: wgpu::FrontFace) -> crate::state::RenderState {
        self.neutral_with_stencil(front_face, false)
    }

    /// Like [`neutral`](Self::neutral), keeping the resolved stencil when the pass's depth
    /// attachment `has_stencil`.
    #[must_use]
    pub fn neutral_with_stencil(
        &self,
        front_face: wgpu::FrontFace,
        has_stencil: bool,
    ) -> crate::state::RenderState {
        crate::state::RenderState {
            raster: RasterState {
                cull: self.cull,
                front_face,
                alpha_to_coverage: false,
            },
            depth: Some(DepthState {
                test: self.depth_compare,
                write: self.depth_write,
                bias: wgpu::DepthBiasState::default(),
            }),
            colors: vec![Some(ColorState {
                blend: self.blend,
                write_mask: self.color_write,
            })],
            stencil: self.stencil.clone().filter(|_| has_stencil),
        }
    }
}

/// A resolved state and the properties that were missing, so it used their defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedState {
    pub state: RenderState,
    pub defaulted: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum StateError {
    #[error("{field} has no meaning for the value {value}")]
    UnknownValue { field: &'static str, value: f32 },
    #[error("{field} is set to {value}, which this renderer does not draw")]
    Unsupported { field: &'static str, value: f32 },
    #[error("state property {property} is not a float")]
    NotAFloat { property: String },
    #[error("state property {property} is the integer {value}, beyond any state value")]
    IntegerOutOfRange { property: String, value: i32 },
    #[error("the pass has no blend state for render target 0")]
    NoBlendTarget,
}

/// Reads state values, remembering the properties that fell back to their defaults.
struct Resolver<'a> {
    layers: &'a [&'a PropertySet],
    defaulted: Vec<String>,
}

impl Resolver<'_> {
    fn value(&mut self, value: &StateValue) -> Result<f32, StateError> {
        let Some(property) = &value.property else {
            return Ok(value.value);
        };
        match lookup(self.layers, property) {
            Some(PropertyValue::Float(found)) => Ok(*found),
            // State values are small enums and masks; i16 converts to f32 exactly.
            Some(PropertyValue::Int(found)) => {
                i16::try_from(*found)
                    .map(f32::from)
                    .map_err(|_| StateError::IntegerOutOfRange {
                        property: property.clone(),
                        value: *found,
                    })
            }
            Some(_) => Err(StateError::NotAFloat {
                property: property.clone(),
            }),
            None => {
                self.defaulted.push(property.clone());
                Ok(value.value)
            }
        }
    }

    /// A value that must be a whole number from 0 to `max`.
    fn choice(
        &mut self,
        field: &'static str,
        value: &StateValue,
        max: u8,
    ) -> Result<u8, StateError> {
        let raw = self.value(value)?;
        whole(field, raw, max)
    }

    /// A value that must equal `expected`, for state the renderer does not draw.
    fn require(
        &mut self,
        field: &'static str,
        value: &StateValue,
        expected: f32,
    ) -> Result<(), StateError> {
        let raw = self.value(value)?;
        if same(raw, expected) {
            Ok(())
        } else {
            Err(StateError::Unsupported { field, value: raw })
        }
    }
}

/// `raw` as a whole number from 0 to `max`.
fn whole(field: &'static str, raw: f32, max: u8) -> Result<u8, StateError> {
    (0..=max)
        .find(|candidate| same(f32::from(*candidate), raw))
        .ok_or(StateError::UnknownValue { field, value: raw })
}

/// Whether two floats are the same value, taking -0 for 0.
fn same(a: f32, b: f32) -> bool {
    (a + 0.0).to_bits() == (b + 0.0).to_bits()
}

/// Resolves a pass's state, reading each property-controlled value from the first of `layers`
/// that has it.
///
/// # Errors
/// Fails on a value with no meaning for its field, a property that is not a float, or state
/// this renderer does not draw.
pub fn resolve_state(
    state: &ShaderState,
    layers: &[&PropertySet],
) -> Result<ResolvedState, StateError> {
    let mut reader = Resolver {
        layers,
        defaulted: Vec::new(),
    };
    reader.unsupported(state)?;
    let depth = &state.depth;
    let cull = reader.choice("cull", &depth.cull, 2)?;
    let compare = reader.choice("ZTest", &depth.test, 8)?;
    let write = reader.choice("ZWrite", &depth.write, 1)?;
    let target = state.blend.first().ok_or(StateError::NoBlendTarget)?;
    let (color_write, blend) = reader.target(target)?;
    let stencil = reader.stencil(&state.stencil)?;
    Ok(ResolvedState {
        state: RenderState {
            cull: [None, Some(wgpu::Face::Front), Some(wgpu::Face::Back)][usize::from(cull)],
            depth_compare: COMPARE[usize::from(compare)],
            depth_write: write == 1,
            color_write,
            blend,
            stencil,
        },
        defaulted: reader.defaulted,
    })
}

/// The values the serialized state carries for a comparison, in order, as wgpu's; `Disabled` is
/// no test, which is `Always`.
const COMPARE: [wgpu::CompareFunction; 9] = [
    wgpu::CompareFunction::Always,
    wgpu::CompareFunction::Never,
    wgpu::CompareFunction::Less,
    wgpu::CompareFunction::Equal,
    wgpu::CompareFunction::LessEqual,
    wgpu::CompareFunction::Greater,
    wgpu::CompareFunction::NotEqual,
    wgpu::CompareFunction::GreaterEqual,
    wgpu::CompareFunction::Always,
];

/// Unity's `BlendMode` in order.
const FACTORS: [wgpu::BlendFactor; 11] = [
    wgpu::BlendFactor::Zero,
    wgpu::BlendFactor::One,
    wgpu::BlendFactor::Dst,
    wgpu::BlendFactor::Src,
    wgpu::BlendFactor::OneMinusDst,
    wgpu::BlendFactor::SrcAlpha,
    wgpu::BlendFactor::OneMinusSrc,
    wgpu::BlendFactor::DstAlpha,
    wgpu::BlendFactor::OneMinusDstAlpha,
    wgpu::BlendFactor::SrcAlphaSaturated,
    wgpu::BlendFactor::OneMinusSrcAlpha,
];

/// The values the serialized state carries for a stencil operation, 0 to 7, as wgpu's.
const STENCIL_OPERATIONS: [wgpu::StencilOperation; 8] = [
    wgpu::StencilOperation::Keep,
    wgpu::StencilOperation::Zero,
    wgpu::StencilOperation::Replace,
    wgpu::StencilOperation::IncrementClamp,
    wgpu::StencilOperation::DecrementClamp,
    wgpu::StencilOperation::Invert,
    wgpu::StencilOperation::IncrementWrap,
    wgpu::StencilOperation::DecrementWrap,
];

/// The names a stencil error gives the pass, fail and depth-fail operations and the comparison
/// of one face.
type StencilNames = [&'static str; 4];
const STENCIL_BOTH: StencilNames = [
    "stencil pass operation",
    "stencil fail operation",
    "stencil depth-fail operation",
    "stencil comparison",
];
const STENCIL_FRONT: StencilNames = [
    "front stencil pass operation",
    "front stencil fail operation",
    "front stencil depth-fail operation",
    "front stencil comparison",
];
const STENCIL_BACK: StencilNames = [
    "back stencil pass operation",
    "back stencil fail operation",
    "back stencil depth-fail operation",
    "back stencil comparison",
];

/// Unity's `BlendOp` in order.
const OPERATIONS: [wgpu::BlendOperation; 5] = [
    wgpu::BlendOperation::Add,
    wgpu::BlendOperation::Subtract,
    wgpu::BlendOperation::ReverseSubtract,
    wgpu::BlendOperation::Min,
    wgpu::BlendOperation::Max,
];

/// Unity's `ColorWriteMask` bits (alpha 1, blue 2, green 4, red 8) as wgpu's.
const MASK_BITS: [(u8, wgpu::ColorWrites); 4] = [
    (8, wgpu::ColorWrites::RED),
    (4, wgpu::ColorWrites::GREEN),
    (2, wgpu::ColorWrites::BLUE),
    (1, wgpu::ColorWrites::ALPHA),
];

impl Resolver<'_> {
    /// Rejects state the renderer does not draw: depth bias, alpha to coverage, conservative
    /// rasterisation and unclipped depth.
    fn unsupported(&mut self, state: &ShaderState) -> Result<(), StateError> {
        let depth = &state.depth;
        self.require("depth clip", &depth.clip, 1.0)?;
        self.require("depth offset", &depth.offset_factor, 0.0)?;
        self.require("depth offset", &depth.offset_units, 0.0)?;
        self.require("alpha to coverage", &depth.alpha_to_mask, 0.0)?;
        self.require("conservative rasterisation", &depth.conservative, 0.0)?;
        Ok(())
    }

    /// The stencil of a pass, or `None` when it keeps the buffer and always passes.
    ///
    /// A face takes each of its own values unless it holds the unset one (keep for an
    /// operation, `Disabled` or `Always` for the comparison), and then the value set for both
    /// faces. The masks and the reference are 0 to 255 and may name properties, so each draw
    /// has its own reference.
    fn stencil(&mut self, stencil: &Stencil) -> Result<Option<StencilState>, StateError> {
        let faces = [
            (&stencil.front, STENCIL_FRONT),
            (&stencil.back, STENCIL_BACK),
        ]
        .map(|(own, names)| self.stencil_face(&stencil.both, own, names));
        let [front, back] = faces;
        let (front, back) = (front?, back?);
        let read_mask = self.choice("stencil read mask", &stencil.read_mask, 255)?;
        let write_mask = self.choice("stencil write mask", &stencil.write_mask, 255)?;
        let reference = self.choice("stencil reference", &stencil.reference, 255)?;
        let inert = wgpu::StencilFaceState::IGNORE;
        if front == inert && back == inert {
            return Ok(None);
        }
        Ok(Some(StencilState {
            ops: wgpu::StencilState {
                front,
                back,
                read_mask: u32::from(read_mask),
                write_mask: u32::from(write_mask),
            },
            reference: u32::from(reference),
        }))
    }

    fn stencil_face(
        &mut self,
        both: &StencilOps,
        own: &StencilOps,
        names: StencilNames,
    ) -> Result<wgpu::StencilFaceState, StateError> {
        let unset_operation = [0.0];
        let unset_compare = [0.0, 8.0];
        let operation = |reader: &mut Self, pair: [&StateValue; 2], unset: &[f32], slot: usize| {
            let (raw, name) = reader.set_value(pair, unset, [STENCIL_BOTH[slot], names[slot]])?;
            whole(name, raw, 7).map(|op| STENCIL_OPERATIONS[usize::from(op)])
        };
        let pass_op = operation(self, [&both.pass, &own.pass], &unset_operation, 0)?;
        let fail_op = operation(self, [&both.fail, &own.fail], &unset_operation, 1)?;
        let depth_fail_op = operation(self, [&both.z_fail, &own.z_fail], &unset_operation, 2)?;
        let (raw, name) = self.set_value(
            [&both.compare, &own.compare],
            &unset_compare,
            [STENCIL_BOTH[3], names[3]],
        )?;
        Ok(wgpu::StencilFaceState {
            compare: COMPARE[usize::from(whole(name, raw, 8)?)],
            pass_op,
            fail_op,
            depth_fail_op,
        })
    }

    /// The face's own value of a stencil setting, or the value for both faces when the face holds
    /// one of the `unset` values, with the name of the field it came from.
    fn set_value(
        &mut self,
        [both, own]: [&StateValue; 2],
        unset: &[f32],
        [both_name, own_name]: [&'static str; 2],
    ) -> Result<(f32, &'static str), StateError> {
        let raw = self.value(own)?;
        if unset.iter().any(|value| same(raw, *value)) {
            Ok((self.value(both)?, both_name))
        } else {
            Ok((raw, own_name))
        }
    }

    fn target(
        &mut self,
        target: &TargetBlend,
    ) -> Result<(wgpu::ColorWrites, Option<wgpu::BlendState>), StateError> {
        let mask = self.choice("color mask", &target.color_mask, 15)?;
        let color_write = MASK_BITS
            .iter()
            .filter(|(bit, _)| mask & bit != 0)
            .fold(wgpu::ColorWrites::empty(), |all, (_, write)| all | *write);
        let color = self.component([&target.src, &target.dst, &target.op])?;
        let alpha = self.component([&target.src_alpha, &target.dst_alpha, &target.op_alpha])?;
        let off = wgpu::BlendComponent::REPLACE;
        let blend = (color != off || alpha != off).then_some(wgpu::BlendState { color, alpha });
        Ok((color_write, blend))
    }

    /// One blend equation. Min and max ignore factors in Unity, but wgpu requires `One`.
    fn component(
        &mut self,
        [src, dst, op]: [&StateValue; 3],
    ) -> Result<wgpu::BlendComponent, StateError> {
        let src_factor = FACTORS[usize::from(self.choice("blend factor", src, 10)?)];
        let dst_factor = FACTORS[usize::from(self.choice("blend factor", dst, 10)?)];
        let operation = OPERATIONS[usize::from(self.choice("blend operation", op, 4)?)];
        if matches!(
            operation,
            wgpu::BlendOperation::Min | wgpu::BlendOperation::Max
        ) {
            return Ok(wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation,
            });
        }
        Ok(wgpu::BlendComponent {
            src_factor,
            dst_factor,
            operation,
        })
    }
}
