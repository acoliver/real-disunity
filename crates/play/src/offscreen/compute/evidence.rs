//! What the player pairs render textures with: the `GraphicsFormat` code of each render texture a
//! dispatch names and what the kernel declares at that binding, for the report.

use worldrunner_world_simulation::graphics::{ComputeBindings, GraphicsSnapshot, TextureRef};

use super::Kernel;

/// The name of the kind of value a texture binding samples.
fn sample_kind(sample: wgpu::TextureSampleType) -> &'static str {
    match sample {
        wgpu::TextureSampleType::Float { .. } => "Float",
        wgpu::TextureSampleType::Depth => "Depth",
        wgpu::TextureSampleType::Sint => "Sint",
        wgpu::TextureSampleType::Uint => "Uint",
    }
}

/// What the kernel declares at each binding a render texture of the dispatch is named for, with
/// the render texture's `GraphicsFormat` code. A storage image gives the format the kernel
/// stores and a sampled texture the kind of value it reads.
pub(super) fn pairings(
    kernel: &Kernel,
    snapshot: &GraphicsSnapshot,
    bindings: &ComputeBindings,
) -> Vec<String> {
    let render_format = |name: &str| {
        let (_, slot) = bindings
            .properties
            .textures
            .iter()
            .find(|(property, _)| property == name)?;
        let TextureRef::Render(id) = &slot.texture else {
            return None;
        };
        snapshot
            .render_textures
            .get(id)
            .map(|texture| texture.format)
    };
    let declared = |set, binding| {
        kernel
            .program
            .layout()
            .slot(set, binding)
            .map(|slot| slot.ty)
    };
    let mut found = Vec::new();
    for target in &kernel.images {
        let Ok(name) = target.name.resolve(&[]) else {
            continue;
        };
        if let (
            Some(code),
            Some(wgpu::BindingType::StorageTexture {
                format,
                view_dimension,
                ..
            }),
        ) = (render_format(name), declared(target.set, target.binding))
        {
            found.push(format!(
                "render texture format {code} bound as storage image {format:?} {view_dimension:?}"
            ));
        }
    }
    for target in &kernel.textures {
        let Ok(name) = target.name.resolve(&[]) else {
            continue;
        };
        if let (
            Some(code),
            Some(wgpu::BindingType::Texture {
                sample_type,
                view_dimension,
                ..
            }),
        ) = (render_format(name), declared(target.set, target.binding))
        {
            found.push(format!(
                "render texture format {code} sampled as {} {view_dimension:?}",
                sample_kind(sample_type)
            ));
        }
    }
    found
}
