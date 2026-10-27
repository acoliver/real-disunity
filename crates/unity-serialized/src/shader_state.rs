//! A shader pass's fixed-function render state: blending, depth, stencil and fog.

use crate::{SerializedError, object::ObjectReader};

/// Fewest bytes of a serialized string: its 32-bit length.
const STRING_LEN: usize = 4;

/// A state value. When `property` is not empty, the value comes from that material property
/// at draw time and `value` is its default.
#[derive(Clone, Debug, PartialEq)]
pub struct StateValue {
    pub value: f32,
    pub property: String,
}

impl StateValue {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            value: r.f32()?,
            property: r.string()?,
        })
    }
}

/// Blending for one render target. Values are Unity's `BlendMode`, `BlendOp` and
/// `ColorWriteMask` enums.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetBlend {
    pub src: StateValue,
    pub dst: StateValue,
    pub src_alpha: StateValue,
    pub dst_alpha: StateValue,
    pub op: StateValue,
    pub op_alpha: StateValue,
    pub color_mask: StateValue,
}

impl TargetBlend {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            src: StateValue::read(r)?,
            dst: StateValue::read(r)?,
            src_alpha: StateValue::read(r)?,
            dst_alpha: StateValue::read(r)?,
            op: StateValue::read(r)?,
            op_alpha: StateValue::read(r)?,
            color_mask: StateValue::read(r)?,
        })
    }
}

/// Stencil operations, as Unity's `StencilOp` and `CompareFunction` values.
#[derive(Clone, Debug, PartialEq)]
pub struct StencilOps {
    pub pass: StateValue,
    pub fail: StateValue,
    pub z_fail: StateValue,
    pub compare: StateValue,
}

impl StencilOps {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            pass: StateValue::read(r)?,
            fail: StateValue::read(r)?,
            z_fail: StateValue::read(r)?,
            compare: StateValue::read(r)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stencil {
    /// Operations for both faces, then for front and back faces.
    pub both: StencilOps,
    pub front: StencilOps,
    pub back: StencilOps,
    pub read_mask: StateValue,
    pub write_mask: StateValue,
    pub reference: StateValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Depth {
    pub clip: StateValue,
    /// Unity's `CompareFunction`.
    pub test: StateValue,
    pub write: StateValue,
    /// Unity's `CullMode`.
    pub cull: StateValue,
    pub conservative: StateValue,
    pub offset_factor: StateValue,
    pub offset_units: StateValue,
    pub alpha_to_mask: StateValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fog {
    pub start: StateValue,
    pub end: StateValue,
    pub density: StateValue,
    /// Color as x, y, z and w values, and the color property it comes from.
    pub color: [StateValue; 4],
    pub color_property: String,
    pub mode: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShaderState {
    pub name: String,
    /// Blending for render targets 0 through 7.
    pub blend: Vec<TargetBlend>,
    /// Whether each target uses its own blending instead of target 0's.
    pub separate_blend: bool,
    pub depth: Depth,
    pub stencil: Stencil,
    pub fog: Fog,
    pub gpu_program_id: i32,
    pub tags: Vec<(String, String)>,
    pub lod: i32,
    pub lighting: bool,
}

/// Render targets with their own blend state.
const BLEND_TARGETS: usize = 8;

impl ShaderState {
    pub(crate) fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let blend = (0..BLEND_TARGETS)
            .map(|_| TargetBlend::read(r))
            .collect::<Result<_, _>>()?;
        let separate_blend = r.bool()?;
        r.align()?;
        let depth = Depth {
            clip: StateValue::read(r)?,
            test: StateValue::read(r)?,
            write: StateValue::read(r)?,
            cull: StateValue::read(r)?,
            conservative: StateValue::read(r)?,
            offset_factor: StateValue::read(r)?,
            offset_units: StateValue::read(r)?,
            alpha_to_mask: StateValue::read(r)?,
        };
        let stencil = Stencil {
            both: StencilOps::read(r)?,
            front: StencilOps::read(r)?,
            back: StencilOps::read(r)?,
            read_mask: StateValue::read(r)?,
            write_mask: StateValue::read(r)?,
            reference: StateValue::read(r)?,
        };
        let fog = read_fog(r)?;
        let gpu_program_id = r.i32()?;
        let tags = string_pairs(r, "pass state tag")?;
        let lod = r.i32()?;
        let lighting = r.bool()?;
        r.align()?;
        Ok(Self {
            name,
            blend,
            separate_blend,
            depth,
            stencil,
            fog,
            gpu_program_id,
            tags,
            lod,
            lighting,
        })
    }
}

fn read_fog(r: &mut ObjectReader<'_>) -> Result<Fog, SerializedError> {
    let [start, end, density] = [
        StateValue::read(r)?,
        StateValue::read(r)?,
        StateValue::read(r)?,
    ];
    let color = [
        StateValue::read(r)?,
        StateValue::read(r)?,
        StateValue::read(r)?,
        StateValue::read(r)?,
    ];
    Ok(Fog {
        start,
        end,
        density,
        color,
        color_property: r.string()?,
        mode: r.i32()?,
    })
}

/// A string-to-string map, such as a pass's tags.
pub(crate) fn string_pairs(
    r: &mut ObjectReader<'_>,
    what: &'static str,
) -> Result<Vec<(String, String)>, SerializedError> {
    r.array(what, 2 * STRING_LEN, |r| Ok((r.string()?, r.string()?)))
}
