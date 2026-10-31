use super::*;
use crate::compositor::terms::{CaptureTerm, TermCapture};
use crate::offscreen::LinearRgbaImage;
use worldrunner_render_post::{FrameConfig, Readback};

#[test]
fn lighting_terms_reconstruct_hdr_and_direct_diffuse_matches_shadow_edge() {
    let context = context();
    for terrain in [false, true] {
        check_scene(&context, terrain);
    }
}

fn check_scene(context: &GpuContext, terrain: bool) {
    let source = lit_scene(terrain);
    let view = view();
    let size = [view.viewport().width, view.viewport().height];
    let captures = [
        CaptureTerm::DirectDiffuse,
        CaptureTerm::AmbientDiffuse,
        CaptureTerm::EnvironmentSpecular,
        CaptureTerm::SunSpecular,
    ]
    .map(|term| TermCapture::new(context.device(), size, term));
    let mut renderer = ShadowedScene::with_target(
        context,
        &source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let color =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    let depth =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Depth32Float);
    let color_view = AttachmentView::new(ResourceRef::new("term test color"), &color).unwrap();
    let depth_view = AttachmentView::new(ResourceRef::new("term test depth"), &depth).unwrap();
    let prepared = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            renderer.encode(context, &view, &color_view, &depth_view, encoder)?;
            for capture in &captures {
                renderer.encode_term_capture(context.device(), encoder, capture);
            }
            Ok::<_, ShadowError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let images = captures
        .each_ref()
        .map(|capture| read(context, &capture.texture));
    let hdr = read(context, &color);
    check_reconstruction(&images, &hdr);
    check_direct_edge(&images[0], &source, &view);
}

fn read(context: &GpuContext, texture: &wgpu::Texture) -> LinearRgbaImage {
    let size = [texture.width(), texture.height()];
    let mut read = Readback::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        1024 * 1024,
    )
    .unwrap();
    let bytes = read
        .read(context.device(), context.submission(), texture)
        .unwrap();
    LinearRgbaImage::from_native(
        crate::Viewport {
            width: size[0],
            height: size[1],
        },
        wgpu::TextureFormat::Rgba16Float,
        &bytes,
    )
    .unwrap()
}

fn check_reconstruction(images: &[LinearRgbaImage; 4], hdr: &LinearRgbaImage) {
    let mut covered = 0;
    for (i, pixel) in hdr.pixels.iter().enumerate() {
        if images[0].pixels[i][3] == 0.0 {
            continue;
        }
        for (channel, expected) in pixel.iter().enumerate().take(3) {
            let sum: f32 = images.iter().map(|image| image.pixels[i][channel]).sum();
            // Four rounded terms and one rounded sum, each bounded by one binary16 ULP.
            let tolerance = 5.0 * (expected.abs().max(1.0) / 1024.0);
            assert!((sum - expected).abs() <= tolerance, "{sum} != {expected}");
        }
        covered += 1;
    }
    assert!(covered > 20);
}

fn check_direct_edge(image: &LinearRgbaImage, source: &RenderScene, view: &SceneView) {
    let frame = FrameUniform::new(view, source.effects()).unwrap();
    let albedo = 128.0_f32 / 255.0;
    let no_l = frame.sun_direction.normalize().z;
    for (x, visibility) in [(-1.3, 1.0), (-0.75, 0.0), (-0.2, 1.0)] {
        let clip = frame.view_projection * Vec4::new(x, 0.0, 0.0, 1.0);
        let ndc = clip.truncate() / clip.w;
        let size = glam::Vec2::new(
            f32::from(u16::try_from(image.viewport.width).unwrap()),
            f32::from(u16::try_from(image.viewport.height).unwrap()),
        );
        let pixel = ((glam::Vec2::new(ndc.x, -ndc.y) * 0.5 + 0.5) * size).as_uvec2();
        let index = usize::try_from(pixel.y * image.viewport.width + pixel.x).unwrap();
        for (value, sun) in image.pixels[index][..3]
            .iter()
            .zip(frame.sun_irradiance.to_array())
        {
            let expected = albedo * no_l * sun * visibility;
            assert!(
                (value - expected).abs() <= 1.0 / 1024.0,
                "{value} != {expected}"
            );
        }
    }
}

fn lit_scene(terrain: bool) -> RenderScene {
    let source = linear_albedo_scene();
    let effects =
        source
            .effects()
            .unwrap()
            .clone()
            .with_specular_convention(SceneSpecularConvention {
                direct_diffuse: SceneDirectDiffuse::LambertianPrepared,
                ..Default::default()
            });
    let source = source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects))
        .unwrap();
    if !terrain {
        return source;
    }
    let source = terrain_resources(&source);
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: -3_000,
        origin_north_millimeters: -3_000,
        sample_spacing_millimeters: 6_000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_material_normal_strength(Some(0.0))
    .unwrap();
    source
        .with_objects_and_effects(
            source.geometry().objects()[1..].to_vec(),
            source.effects().cloned(),
        )
        .unwrap()
        .with_terrain(Some(std::sync::Arc::new(terrain)))
        .unwrap()
}

fn terrain_resources(source: &RenderScene) -> RenderScene {
    let mut textures = source.resources().textures().to_vec();
    for pixel in [[128, 128, 255, 255], [0, 255, 0, 255]] {
        textures.push(SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 4,
            height: 1,
            mips: 1,
            bytes: pixel.repeat(4).into(),
        });
    }
    let mut materials = source.resources().materials().to_vec();
    materials[0] = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(2)),
        roughness_metallic: Some(TextureHandle::from_raw(3)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    source
        .with_resources(
            SceneResources::new(textures, materials).unwrap(),
            source.geometry().clone(),
            source.effects().cloned(),
        )
        .unwrap()
}

fn linear_albedo_scene() -> RenderScene {
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut textures = source.resources().textures().to_vec();
    // Linear authored bytes avoid the terrain palette's separate sRGB byte conversion.
    textures[0] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 4,
        height: 1,
        mips: 1,
        bytes: [128u8, 128, 128, 255].repeat(4).into(),
    };
    source
        .with_resources(
            SceneResources::new(textures, source.resources().materials().to_vec()).unwrap(),
            source.geometry().clone(),
            source.effects().cloned(),
        )
        .unwrap()
}
