//! Fixed-function render state from a pass's `ShaderState`.
//!
//! The enum values follow the managed engine's own definitions in `Disunity/Enums/Rendering.cs`
//! (`CullMode`, `CompareFunction`, `BlendMode`, `BlendOp` and `ColorWriteMask`). `BlendOp`
//! values above `Max` (the logical and advanced blend operations) have no wgpu equivalent and
//! are rejected as unknown values.

use worldrunner_unity_serialized::shader_state::{
    ShaderState, StateValue, StencilOps, TargetBlend,
};

use crate::properties::{PropertySet, PropertyValue, lookup};

/// State a pipeline needs, for render target 0.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderState {
    pub cull: Option<wgpu::Face>,
    pub depth_compare: wgpu::CompareFunction,
    pub depth_write: bool,
    pub color_write: wgpu::ColorWrites,
    pub blend: Option<wgpu::BlendState>,
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
        (0..=max)
            .find(|candidate| same(f32::from(*candidate), raw))
            .ok_or(StateError::UnknownValue { field, value: raw })
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
    Ok(ResolvedState {
        state: RenderState {
            cull: [None, Some(wgpu::Face::Front), Some(wgpu::Face::Back)][usize::from(cull)],
            depth_compare: COMPARE[usize::from(compare)],
            depth_write: write == 1,
            color_write,
            blend,
        },
        defaulted: reader.defaulted,
    })
}

/// Unity's `CompareFunction` in order; `Disabled` is no test, which is `Always`.
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
    /// Rejects state the renderer does not draw: stencil, depth bias, alpha to coverage,
    /// conservative rasterisation and unclipped depth.
    fn unsupported(&mut self, state: &ShaderState) -> Result<(), StateError> {
        let depth = &state.depth;
        self.require("depth clip", &depth.clip, 1.0)?;
        self.require("depth offset", &depth.offset_factor, 0.0)?;
        self.require("depth offset", &depth.offset_units, 0.0)?;
        self.require("alpha to coverage", &depth.alpha_to_mask, 0.0)?;
        self.require("conservative rasterisation", &depth.conservative, 0.0)?;
        for ops in [
            &state.stencil.both,
            &state.stencil.front,
            &state.stencil.back,
        ] {
            self.stencil_off(ops)?;
        }
        Ok(())
    }

    /// Stencil ops that keep the buffer and always pass: the same as no stencil.
    fn stencil_off(&mut self, ops: &StencilOps) -> Result<(), StateError> {
        for operation in [&ops.pass, &ops.fail, &ops.z_fail] {
            self.require("stencil", operation, 0.0)?;
        }
        let compare = self.value(&ops.compare)?;
        if same(compare, 0.0) || same(compare, 8.0) {
            Ok(())
        } else {
            Err(StateError::Unsupported {
                field: "stencil",
                value: compare,
            })
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
