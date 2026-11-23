//! One shadowed frame captured for comparison: the scene colour and the metadata prepass depth,
//! and the terrain, caster, impostor card and water scene the route tests draw.
use super::*;
use crate::compositor::{FeatureTarget, FrameRenderer, StockReplacement, declaring_stock};
use worldrunner_render_post::{FrameConfig, Readback, ToneMap};

fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        2_000_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

/// One shadowed frame.
pub(super) struct Captured {
    /// The scene colour, `Rgba16Float`.
    pub(super) colour: Vec<u8>,
    /// The metadata prepass depth in view-space metres, present with the depth prepass.
    pub(super) depth: Option<Vec<f32>>,
}

/// The SSAO settings that give the shadowed frame its depth prepass.
fn ssao() -> SceneSsao {
    SceneSsao {
        receiver: SceneSsaoReceiver::Nearest,
        depth_footprint: SceneSsaoDepthFootprint::default(),
        kernel: SceneSsaoKernel::Hemisphere,
        filter: SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 2000,
        intensity_micro: 1_000_000,
        sample_count: 32,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    }
}

/// `source` with the depth prepass.
pub(super) fn with_prepass(source: &RenderScene) -> RenderScene {
    let mut parts = fixture::effects_parts(600_000);
    parts.ssao = Some(ssao());
    RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: source.terrain().cloned(),
        effects: Some(SceneEffects::new(parts).unwrap()),
    })
    .unwrap()
}

/// The HDR frame renderer of `source`'s effects at `view`'s extent.
pub(super) fn frame(context: &GpuContext, source: &RenderScene, view: &SceneView) -> FrameRenderer {
    FrameRenderer::hdr(
        context,
        [view.viewport().width, view.viewport().height],
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap()
}

/// One shadowed frame of `source` with a game that draws what `replaces` says.
pub(super) fn capture_drawing(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    replaces: StockReplacement,
) -> Result<Captured, ShadowError> {
    let mut frame = frame_declaring(context, source, view, replaces);
    let mut renderer = ShadowedScene::new(
        context,
        PreparedScene::new(source).unwrap(),
        settings(),
        ShadowedTarget::new(wgpu::TextureFormat::Rgba16Float, 1),
    )
    .unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    let mut output = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    renderer.encode_with(context, view, (&color, &depth), &mut encoder, &mut frame)?;
    frame
        .encode_output(&mut encoder, output.color_view())
        .unwrap();
    output.encode_readback(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    output.read_encoded(context, submission).unwrap();
    Ok(captured(context, &frame))
}

/// [`frame`] with a game that draws what `replaces` says; the stock renderer when it says
/// nothing. The game installs before the scene's water does, as in a configured renderer, so the
/// renderer is built without the water and takes the bodies afterwards.
pub(super) fn frame_declaring(
    context: &GpuContext,
    source: &RenderScene,
    view: &SceneView,
    replaces: StockReplacement,
) -> FrameRenderer {
    if replaces == StockReplacement::NONE {
        return frame(context, source, view);
    }
    let effects = source.effects().unwrap();
    let bodies = effects.water_bodies().to_vec();
    let dry = effects.clone().with_water_bodies(Vec::new()).unwrap();
    let mut frame = FrameRenderer::hdr(
        context,
        [view.viewport().width, view.viewport().height],
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        Some(&dry),
    )
    .unwrap();
    declaring_stock(replaces)
        .install(&mut frame, context, None)
        .unwrap();
    frame
        .install_water_bodies(context.device(), context.submission(), bodies)
        .unwrap();
    frame
}

/// A renderer with no scene, for a host that supplies the frame's attachments, whose game draws
/// what `replaces` says; the stock renderer when it says nothing.
pub(super) fn scheduler(context: &GpuContext, replaces: StockReplacement) -> FrameRenderer {
    let mut frame = FrameRenderer::new(context.device());
    if replaces != StockReplacement::NONE {
        declaring_stock(replaces)
            .install(&mut frame, context, None)
            .unwrap();
    }
    frame
}

pub(super) fn captured(context: &GpuContext, frame: &FrameRenderer) -> Captured {
    Captured {
        colour: read(context, frame.targets().texture()),
        depth: frame.feature_target(FeatureTarget::Depth).map(|texture| {
            read(context, texture)
                .chunks_exact(4)
                .map(|value| f32::from_le_bytes([value[0], value[1], value[2], value[3]]))
                .collect()
        }),
    }
}

/// A masked, double-sided impostor card material over texture `albedo` and normal `normal`.
fn card_material(albedo: u32, normal: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(albedo),
        normal: Some(TextureHandle::from_raw(normal)),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
    .with_impostor(
        SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false).unwrap(),
    )
}

/// A sky drawn where no surface is.
fn sky() -> SceneAtmosphere {
    SceneAtmosphere {
        celestial_composition: SceneCelestialComposition::Inherit,
        celestial_exposure_micro: None,
        background_depth_policy: SceneBackgroundDepthPolicy::ConstantDistance,
        scattering: None,
        horizon_radiance_micro: [200_000, 300_000, 2_000_000],
        zenith_radiance_micro: [100_000, 200_000, 2_000_000],
        gradient_exponent_micro: 1_000_000,
        sun_angular_radius_millidegrees: 0,
        fog: None,
        discs: Vec::new(),
        meshes: Vec::new(),
        background_distance_millimeters: 20_000,
    }
}

/// A water body over the east half of the terrain, 1 mm above it, with a rippling normal map
/// at texture `normal`.
fn water(normal: u32) -> SceneWaterBody {
    let mut water = unsupported_water_body();
    water.height_millimeters = 1;
    water.east_min_millimeters = 0;
    water.north_min_millimeters = -3000;
    water.east_extent_millimeters = 3000;
    water.north_extent_millimeters = 6000;
    water.material.normal = Some(TextureHandle::from_raw(normal));
    water
}

/// Terrain in place of the ground quad, the caster mesh and an impostor card under a drawn sky,
/// with SSAO for the depth prepass and, with `with_water`, water over the east half.
pub(super) fn source(with_water: bool) -> RenderScene {
    let base = super::terrain::terrain_scene(&scene(&[[0, 0, 1_000]], 600_000, None), false, false);
    let mut textures = base.resources().textures().to_vec();
    let normal = u32::try_from(textures.len()).unwrap();
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![128, 128, 0, 255].into(),
    });
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rg8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 1,
        mips: 1,
        bytes: vec![32, 128, 64, 128, 192, 128, 224, 128].into_boxed_slice(),
    });
    let mut materials = base.resources().materials().to_vec();
    let card = u32::try_from(materials.len()).unwrap();
    materials.push(card_material(0, normal));
    let mut meshes = base.geometry().meshes().to_vec();
    let card_mesh = u32::try_from(meshes.len()).unwrap();
    meshes.push(super::geometry::card(BillboardPolicy::YawLocked));
    let mut objects = base.geometry().objects().to_vec();
    objects.push(fixture::object([-1_500, 500, 300], card_mesh, card));
    let mut parts = fixture::effects_parts(600_000);
    parts.ssao = Some(ssao());
    if with_water {
        parts.water = Some(water(normal + 1));
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials).unwrap(),
        geometry: SceneGeometry::new(meshes, objects).unwrap(),
        terrain: base.terrain().cloned(),
        effects: Some(
            SceneEffects::new(parts)
                .unwrap()
                .with_atmosphere(sky())
                .unwrap(),
        ),
    })
    .unwrap()
}
