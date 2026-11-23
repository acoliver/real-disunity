//! Test-only accessors of this module's types.

use super::*;

impl PreparedObjects {
    /// One bind group for each material, made from what the material lends. The scene's passes
    /// bind the prepared sets instead; only the tests that record draws themselves bind these.
    ///
    /// The groups use a layout from `upload::material_bind_group_layout`, which is the function
    /// that also makes the layout the tests' pipelines take as their material group
    /// (`Pipelines::material_layout`). wgpu treats identical layouts as compatible, so a group made
    /// here binds to those pipelines.
    pub(crate) fn material_groups(&self, device: &wgpu::Device) -> Vec<wgpu::BindGroup> {
        use worldrunner_render_program::runtime::{ImportedResource, ImportedTexture};

        let layout = crate::upload::material_bind_group_layout(device);
        self.tables
            .iter()
            .map(|table| {
                let lent = |name: &str| {
                    table
                        .get(&named(name))
                        .unwrap_or_else(|| panic!("a material lends {name}"))
                };
                let buffer = |name: &str| match lent(name) {
                    ImportedResource::Buffer(buffer) => buffer,
                    other => panic!("{name} is lent as {other:?}, not a buffer"),
                };
                let ImportedResource::Sampler(sampler) = lent(semantic::SAMPLER) else {
                    panic!("the sampler is lent as a sampler");
                };
                let views = MAP_SEMANTICS.map(|name| match lent(name) {
                    ImportedResource::Texture(ImportedTexture { view, .. }) => view,
                    other => panic!("{name} is lent as {other:?}, not a texture view"),
                });
                crate::upload::extended_material_group(
                    device,
                    &layout,
                    buffer(semantic::MATERIAL),
                    crate::upload::MaterialTextureBindings {
                        maps: [views[0], views[1], views[2], views[3], views[4]],
                        extra: [views[5], views[6], views[7]],
                    },
                    sampler,
                    buffer(semantic::CONTACT_HEIGHTS),
                )
            })
            .collect()
    }

    /// How many pipelines were asked for, whether found or made.
    pub(crate) fn lookups(&self) -> usize {
        self.lookups.get()
    }
}
