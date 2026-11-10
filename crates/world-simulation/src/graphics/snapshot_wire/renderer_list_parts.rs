//! The encodings of the renderer list records.
use super::WireError;
use super::codec::{Reader, Wire, Writer};
use crate::graphics::{MAX_CULL_PLANES, MAX_LIST_PASS_TAGS, OffscreenWork, RendererListDesc};

const WORK_CREATE_RENDERER_LIST: u8 = 13;
const WORK_DRAW_RENDERER_LIST: u8 = 14;

const LIST_DRAW: u8 = 0;
const LIST_SHADOW: u8 = 1;

/// The most entries a decoder accepts in a list's tag or plane run.
fn limit(most: usize) -> u32 {
    u32::try_from(most).unwrap_or(u32::MAX)
}

impl Wire for RendererListDesc {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        match self {
            Self::Draw {
                cull,
                pass_tags,
                queue_min,
                queue_max,
                layer_mask,
                rendering_layer_mask,
                sorting,
                per_object,
                override_material,
                override_pass,
            } => {
                LIST_DRAW.write(out)?;
                cull.write(out)?;
                out.count(pass_tags.len(), limit(MAX_LIST_PASS_TAGS))?;
                for tag in pass_tags {
                    tag.write(out)?;
                }
                [*queue_min, *queue_max].write(out)?;
                [*layer_mask, *rendering_layer_mask, *sorting, *per_object].write(out)?;
                override_material.write(out)?;
                override_pass.write(out)
            }
            Self::Shadow {
                cull,
                light_index,
                planes,
                sphere,
                objects_filter,
                projection,
                use_rendering_layer_mask_test,
            } => {
                LIST_SHADOW.write(out)?;
                cull.write(out)?;
                light_index.write(out)?;
                out.count(planes.len(), limit(MAX_CULL_PLANES))?;
                for plane in planes {
                    plane.write(out)?;
                }
                sphere.write(out)?;
                [*objects_filter, *projection].write(out)?;
                use_rendering_layer_mask_test.write(out)
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        match u8::read(input)? {
            LIST_DRAW => read_draw(input),
            LIST_SHADOW => read_shadow(input),
            other => Err(WireError::UnknownTag(u32::from(other))),
        }
    }
}

fn read_draw(input: &mut Reader<'_>) -> Result<RendererListDesc, WireError> {
    let cull = Wire::read(input)?;
    let count = input.count(limit(MAX_LIST_PASS_TAGS))?;
    let mut pass_tags = Vec::with_capacity(count);
    for _ in 0..count {
        pass_tags.push(String::read(input)?);
    }
    let [queue_min, queue_max] = <[i32; 2]>::read(input)?;
    let [layer_mask, rendering_layer_mask, sorting, per_object] = <[u32; 4]>::read(input)?;
    Ok(RendererListDesc::Draw {
        cull,
        pass_tags,
        queue_min,
        queue_max,
        layer_mask,
        rendering_layer_mask,
        sorting,
        per_object,
        override_material: Wire::read(input)?,
        override_pass: Wire::read(input)?,
    })
}

fn read_shadow(input: &mut Reader<'_>) -> Result<RendererListDesc, WireError> {
    let cull = Wire::read(input)?;
    let light_index = Wire::read(input)?;
    let count = input.count(limit(MAX_CULL_PLANES))?;
    let mut planes = Vec::with_capacity(count);
    for _ in 0..count {
        planes.push(<[f32; 4]>::read(input)?);
    }
    let sphere = Wire::read(input)?;
    let [objects_filter, projection] = <[u32; 2]>::read(input)?;
    let use_rendering_layer_mask_test = Wire::read(input)?;
    Ok(RendererListDesc::Shadow {
        cull,
        light_index,
        planes,
        sphere,
        objects_filter,
        projection,
        use_rendering_layer_mask_test,
    })
}

/// Writes the renderer list variants of [`OffscreenWork`]; the other variants have their own
/// writers.
pub(super) fn write_renderer_list(work: &OffscreenWork, out: &mut Writer) -> Result<(), WireError> {
    match work {
        OffscreenWork::CreateRendererList { id, desc } => {
            WORK_CREATE_RENDERER_LIST.write(out)?;
            id.write(out)?;
            desc.write(out)
        }
        OffscreenWork::DrawRendererList { list } => {
            WORK_DRAW_RENDERER_LIST.write(out)?;
            list.write(out)
        }
        _ => Err(WireError::UnknownTag(u32::MAX)),
    }
}

/// Reads the renderer list variant that starts with `kind`, or refuses a kind no encoder writes.
pub(super) fn read_renderer_list(
    kind: u8,
    input: &mut Reader<'_>,
) -> Result<OffscreenWork, WireError> {
    match kind {
        WORK_CREATE_RENDERER_LIST => Ok(OffscreenWork::CreateRendererList {
            id: Wire::read(input)?,
            desc: Wire::read(input)?,
        }),
        WORK_DRAW_RENDERER_LIST => Ok(OffscreenWork::DrawRendererList {
            list: Wire::read(input)?,
        }),
        other => Err(WireError::UnknownTag(u32::from(other))),
    }
}
