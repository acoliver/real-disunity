//! A shader pass's fixed-function render state: blending, depth, stencil and fog.

use crate::{SerializedError, object::ObjectReader};

/// Fewest bytes of a serialized string: its 32-bit length.
const STRING_LEN: usize = 4;

/// The name Unity stores for a state value that no material property supplies.
const FIXED: &str = "<noninit>";

/// A state value: fixed in the shader, or read from a material property at draw time, in
/// which case `value` is the default.
#[derive(Clone, Debug, PartialEq)]
pub struct StateValue {
    pub value: f32,
    /// The material property, such as `_ZTest`, or `None` for a fixed value.
    pub property: Option<String>,
}

impl StateValue {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            value: r.f32()?,
            property: property_name(r)?,
        })
    }
}

/// A state property name, or `None` for Unity's name for a fixed value.
fn property_name(r: &mut ObjectReader<'_>) -> Result<Option<String>, SerializedError> {
    let name = r.string()?;
    Ok((name != FIXED).then_some(name))
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
    /// Operations a pass sets for both faces at once.
    pub both: StencilOps,
    /// Operations for front faces only.
    pub front: StencilOps,
    /// Operations for back faces only.
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
    /// Color as x, y, z and w values.
    pub color: [StateValue; 4],
    /// The material property the whole color comes from, or `None` for a fixed color.
    pub color_property: Option<String>,
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
    /// Pass tags, such as `LIGHTMODE` and `RenderPipeline`. Unity stores some keys in upper
    /// case.
    pub tags: Vec<(String, String)>,
    pub lod: i32,
    pub lighting: bool,
}

/// Render targets with their own blend state.
const BLEND_TARGETS: usize = 8;

/// State values: 7 per blend target, 8 depth values, 3 stencil operation sets of 4, 3
/// stencil values, 3 fog values and 4 fog color values.
const STATE_VALUES: usize = 7 * BLEND_TARGETS + 8 + 3 * 4 + 3 + 3 + 4;

/// Fewest bytes of a pass's state: its name, every value with an empty property name, the
/// padded blend flag, the fog color name, fog mode, program ID, tag count, LOD and the
/// padded lighting flag.
pub(crate) const STATE_LEN: usize =
    STRING_LEN + STATE_VALUES * (4 + STRING_LEN) + 4 + STRING_LEN + 4 * 4 + 4;

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
        color_property: property_name(r)?,
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
