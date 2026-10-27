use super::{LayeredError, LayeredGpuSource};
use worldrunner_render_plan::{
    SceneNormalEncoding, SceneResources, SceneTerrain, TerrainColorizationChannels,
    TerrainSplatFilter,
};

#[expect(
    clippy::cast_precision_loss,
    reason = "authored parameters use the CPU reference shader precision"
)]
pub(super) fn encode(
    terrain: &SceneTerrain,
    resources: &SceneResources,
) -> Result<LayeredGpuSource, LayeredError> {
    let source = terrain.layered_source().ok_or(LayeredError::Source)?;
    let mut result = LayeredGpuSource {
        parameters: Vec::new(),
        grid: Vec::new(),
        arrays: Vec::new(),
        resident: std::sync::Mutex::default(),
        images: Vec::new(),
        bounds: [
            terrain.origin_east_millimeters() as f64 * 0.001,
            terrain.origin_north_millimeters() as f64 * 0.001,
            f64::from(terrain.columns() - 1)
                * f64::from(terrain.sample_spacing_millimeters())
                * 0.001,
            f64::from(terrain.rows() - 1) * f64::from(terrain.sample_spacing_millimeters()) * 0.001,
        ],
    };
    // Preserve resource handle indexing, including textures unused by this source.
    let mut used = std::collections::BTreeSet::new();
    for surface in terrain.surfaces() {
        let layer = surface.appearance().layered.ok_or(LayeredError::Source)?;
        used.extend([
            layer.albedo_metallic.raw(),
            layer.normal_roughness.raw(),
            layer.height.raw(),
        ]);
    }
    if let Some(map) = source.colorization {
        used.insert(map.texture.raw());
    }
    for (index, texture) in resources.textures().iter().enumerate() {
        if used.contains(&u32::try_from(index).map_err(|_| LayeredError::Texture)?) {
            super::textures::image(&mut result, texture)?;
        } else {
            words(&mut result.images, [1, 1, 0, 0]);
        }
    }
    grid(&mut result, source)?;
    colorization(&mut result.parameters, source.colorization);
    for surface in terrain.surfaces() {
        surface_parameters(&mut result.parameters, surface, source, resources)?;
    }
    Ok(result)
}
#[expect(
    clippy::cast_precision_loss,
    reason = "matches CPU reference f32 parameters"
)]
fn grid(
    result: &mut LayeredGpuSource,
    source: &worldrunner_render_plan::TerrainLayeredSource,
) -> Result<(), LayeredError> {
    let grid_offset = u32::try_from(result.grid.len() / 4).map_err(|_| LayeredError::Texture)?;
    for index in source.type_grid.indices() {
        result.grid.extend(u32::from(*index).to_le_bytes());
    }
    words(
        &mut result.parameters,
        [
            source.type_grid.dimensions()[0],
            source.type_grid.dimensions()[1],
            grid_offset,
            u32::from(source.filter == TerrainSplatFilter::Spiral14),
        ],
    );
    floats(
        &mut result.parameters,
        [
            source.type_grid.origin_millimeters()[0] as f32 * 0.001,
            source.type_grid.origin_millimeters()[1] as f32 * 0.001,
            source.type_grid.spacing_millimeters() as f32 * 0.001,
            0.0,
        ],
    );
    Ok(())
}
#[expect(
    clippy::cast_precision_loss,
    reason = "matches CPU reference f32 parameters"
)]
fn colorization(
    parameters: &mut Vec<u8>,
    map: Option<worldrunner_render_plan::TerrainColorization>,
) {
    if let Some(map) = map {
        let channels = match map.channels {
            TerrainColorizationChannels::Rgb => 0,
            TerrainColorizationChannels::Grayscale => 1,
            TerrainColorizationChannels::BgrRoughness => 2,
        };
        words(parameters, [1, map.texture.raw(), channels, 0]);
        floats(
            parameters,
            [
                map.origin_millimeters[0] as f32 * 0.001,
                map.origin_millimeters[1] as f32 * 0.001,
                map.extent_millimeters[0] as f32 * 0.001,
                map.extent_millimeters[1] as f32 * 0.001,
            ],
        );
    } else {
        words(parameters, [0; 4]);
        floats(parameters, [0.0; 4]);
    }
}
#[expect(
    clippy::cast_precision_loss,
    reason = "matches CPU reference f32 parameters"
)]
fn surface_parameters(
    parameters: &mut Vec<u8>,
    surface: &worldrunner_render_plan::SceneTerrainSurface,
    source: &worldrunner_render_plan::TerrainLayeredSource,
    resources: &SceneResources,
) -> Result<(), LayeredError> {
    let appearance = surface.appearance();
    let layer = appearance.layered.ok_or(LayeredError::Source)?;
    let map = source
        .channels(layer, resources.textures())
        .map_err(|_| LayeredError::Texture)?;
    let packed = channel_words(map);
    words(
        parameters,
        [
            layer.albedo_metallic.raw(),
            layer.normal_roughness.raw(),
            layer.height.raw(),
            packed[0],
        ],
    );
    words(
        parameters,
        [
            (layer.offset_millimeters[0] as f32 * 0.001).to_bits(),
            (layer.offset_millimeters[1] as f32 * 0.001).to_bits(),
            (layer.repeat_millimeters as f32 * 0.001).to_bits(),
            packed[1],
        ],
    );
    floats(
        parameters,
        [
            f32::from(appearance.tint_percent[0]) / 100.0,
            f32::from(appearance.tint_percent[1]) / 100.0,
            f32::from(appearance.tint_percent[2]) / 100.0,
            appearance.blur_radius_millimeters as f32 * 0.001,
        ],
    );
    let angle = appearance.rotation_arcminutes as f32 * std::f32::consts::PI / 10_800.0;
    floats(
        parameters,
        [
            angle.cos(),
            angle.sin(),
            layer.roughness_metallic_offset_micro[0] as f32 * 0.000_001,
            layer.roughness_metallic_offset_micro[1] as f32 * 0.000_001,
        ],
    );
    Ok(())
}
pub(super) fn words(bytes: &mut Vec<u8>, values: [u32; 4]) {
    bytes.extend(values.into_iter().flat_map(u32::to_le_bytes));
}
fn floats(bytes: &mut Vec<u8>, values: [f32; 4]) {
    bytes.extend(values.into_iter().flat_map(f32::to_le_bytes));
}

// Nine four-bit slot/component selectors fit beside normal and roughness flags.
fn channel_words(map: worldrunner_render_plan::TerrainLayerChannelMap) -> [u32; 2] {
    let channels = [
        map.albedo[0],
        map.albedo[1],
        map.albedo[2],
        map.normal[0],
        map.normal[1],
        map.normal[2],
        map.metallic,
        map.height,
        map.roughness,
    ];
    let mut words = [
        u32::from(map.normal_encoding == SceneNormalEncoding::TwoChannelXy)
            | (u32::from(map.invert_roughness) << 1),
        0,
    ];
    for (index, channel) in channels.into_iter().enumerate() {
        let selector = u32::try_from(channel.texture.index()).expect("three slots") * 4
            + u32::from(channel.channel);
        if index < 7 {
            words[0] |= selector << (4 + index * 4);
        } else {
            words[1] |= selector << ((index - 7) * 4);
        }
    }
    words
}
