mod admission;
mod capture;
use super::*;
use crate::draw::PreparedScene;
use crate::terrain::pages::{layered::LayeredReference, scene::TerrainPageSource};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::{
    SceneNormalEncoding, TerrainLayer, TerrainLayeredSource, TerrainPageSettings,
    TerrainPageSettingsParts, TerrainSplatFilter, TerrainTypeGrid, TexelFormat,
};

fn texels(size: [u32; 2], bytes: &[u8]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: size[0],
        height: size[1],
        mips: 1,
        bytes: bytes.into(),
    }
}

fn layered_scene(filter: TerrainSplatFilter) -> RenderScene {
    let resources = SceneResources::new(
        vec![
            texels([2, 1], &[60, 110, 160, 70, 150, 80, 100, 110]),
            texels([1, 2], &[128, 128, 255, 130, 145, 115, 250, 170]),
            SceneTexture::Texels {
                format: TexelFormat::R8Unorm,
                color_space: TextureColorSpace::Linear,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![160].into(),
            },
            texels([1, 1], &[140, 90, 60, 100]),
        ],
        vec![material(SceneAlphaMode::Opaque, false)],
    )
    .unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 2000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0, 1, 1, 0],
        surfaces: (0..2).map(layered_surface).collect(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_layered_source(TerrainLayeredSource {
        type_grid: TerrainTypeGrid::new([2, 2], [0; 2], 1000, vec![0, 1, 1, 0]).unwrap(),
        filter,
        colorization: None,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources,
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
    .unwrap()
}

fn layered_surface(index: u32) -> SceneTerrainSurface {
    SceneTerrainSurface::new(
        MaterialHandle::from_raw(0),
        SceneTerrainAppearance {
            layered: Some(TerrainLayer {
                albedo_metallic: TextureHandle::from_raw(if index == 0 { 0 } else { 3 }),
                normal_roughness: TextureHandle::from_raw(1),
                height: TextureHandle::from_raw(2),
                normal_encoding: SceneNormalEncoding::Rgb,
                repeat_millimeters: 2000,
                offset_millimeters: [0; 2],
                roughness_metallic_offset_micro: [0; 2],
            }),
            blur_radius_millimeters: 500,
            ..SceneTerrainAppearance::NEUTRAL
        },
    )
}

fn matching_frame() -> FrameUniform {
    let mut frame = frame_from(pose());
    frame.view_projection = glam::Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -0.1, 0.0, -1.0, -1.0, 0.5, 1.0,
    ]);
    frame.camera_position = glam::Vec3::new(1.0, 1.0, 5.0);
    frame.sun_irradiance = glam::Vec3::ZERO;
    frame.ambient_sky = glam::Vec3::ONE;
    frame.ambient_ground = glam::Vec3::ONE;
    frame
}

fn page_source() -> TerrainPageSource {
    TerrainPageSource {
        settings: TerrainPageSettings::new(TerrainPageSettingsParts {
            interior: [TARGET; 2],
            border: 1,
            filter_radius: 1,
            maximum_depth: 0,
            split_threshold_millipixels: 2000,
            merge_threshold_millipixels: 1000,
            maximum_selected_pages: 1,
            resident_bytes: 1_000_000,
            upload_bytes_per_frame: 1_000_000,
        })
        .unwrap(),
        slots: 1,
        budget: AllocationBudget(20_000_000),
        sample: Box::new(|_| panic!("typed layered sources never use the linear callback")),
    }
}

fn render_prepared(context: &GpuContext, scene: &RenderScene, cached: bool) -> Vec<u8> {
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let pipelines = ScenePipelines::new(device, COLOR_FORMAT);
    let mut gpu = SceneGpu::upload_prepared(
        device,
        &authority,
        PreparedScene::new(scene).unwrap(),
        &pipelines,
    )
    .unwrap();
    let frame = matching_frame();
    gpu.set_frame(context.queue(), frame);
    if cached {
        gpu.enable_terrain_pages(device, &authority, &pipelines, page_source())
            .unwrap();
        let mut transaction = authority.begin(device).unwrap();
        gpu.prepare_terrain_pages(device, &mut transaction, &frame, [TARGET; 2])
            .unwrap();
        transaction.finish().submit().unwrap();
        assert!(gpu.terrain_pages_ready());
    }
    draw_pixels(context, &authority, &gpu, &pipelines)
}

fn draw_pixels(
    context: &GpuContext,
    authority: &SubmissionAuthority,
    gpu: &SceneGpu,
    pipelines: &ScenePipelines,
) -> Vec<u8> {
    let device = context.device();
    let color = color_target(device);
    let depth = depth_texture(
        device,
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    );
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("layered terrain pixels"),
        size: u64::from(TARGET * TARGET * 4),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut transaction = authority.begin(device).unwrap();
    gpu.prepare_terrain_pages(device, &mut transaction, &matching_frame(), [TARGET; 2])
        .unwrap();
    {
        record_scene(
            &mut transaction,
            &color.create_view(&TextureViewDescriptor::default()),
            &depth.create_view(&TextureViewDescriptor::default()),
            gpu,
            pipelines,
        );
    }
    transaction.copy_texture_to_buffer(
        color.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TARGET * 4),
                rows_per_image: Some(TARGET),
            },
        },
        Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
    );
    transaction.finish().submit().unwrap();
    map_pixels(device, &readback)
}

#[test]
fn layered_cached_and_uncached_production_draws_agree_at_matching_resolution() {
    let context = context();
    for filter in [TerrainSplatFilter::Bilinear, TerrainSplatFilter::Spiral14] {
        let scene = layered_scene(filter);
        let direct = render_prepared(&context, &scene, false);
        let cached = render_prepared(&context, &scene, true);
        assert!(lit_pixels(&direct) > 4000);
        // Pixel centers coincide with page centers. Bilinear page filtering and
        // RGBA8 material quantization precede sRGB conversion only in the cache;
        // two output bytes cover that quantization and f32 filtering error.
        for (index, (a, b)) in direct.iter().zip(&cached).enumerate() {
            assert!(a.abs_diff(*b) <= 2, "{filter:?} byte {index}: {a} vs {b}");
        }
    }
}

#[test]
fn layered_prepared_scene_pixels_match_cpu_reference_with_metallic() {
    let context = context();
    let scene = layered_scene(TerrainSplatFilter::Bilinear);
    let pixels = render_prepared(&context, &scene, false);
    let reference = LayeredReference::new(scene.terrain().unwrap(), scene.resources()).unwrap();
    for [x, y] in [[8_u16, 12_u16], [42, 19], [53, 51]] {
        let world = [(f32::from(x) + 0.5) / 32.0, (63.5 - f32::from(y)) / 32.0];
        let sample = reference.sample(world).unwrap();
        assert!(sample.metallic > 0.2);
        for (channel, albedo) in sample.albedo.into_iter().enumerate() {
            let linear = albedo * (1.0 - sample.metallic);
            let srgb = if linear <= 0.003_130_8 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            let expected = srgb * 255.0;
            assert!(
                (expected.fract() - 0.5).abs() >= 0.1,
                "pixel {x},{y} channel {channel}: rounding margin for {expected}"
            );
            let actual = f32::from(pixels[(usize::from(y) * 64 + usize::from(x)) * 4 + channel]);
            assert!(
                (actual - srgb * 255.0).abs() <= 1.0,
                "pixel {x},{y} channel {channel}: {actual} vs {}",
                srgb * 255.0
            );
        }
    }
}

#[test]
fn scene_policy_installs_layered_pages_and_enforces_allocation_budget() {
    let context = context();
    let source = layered_scene(TerrainSplatFilter::Bilinear);
    let config = page_source();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let authority = SubmissionAuthority::new(context.queue().clone());
    for budget in [config.budget.0, config.settings.page_bytes()] {
        let policy =
            worldrunner_render_plan::TerrainPagePolicy::new(config.settings, config.slots, budget)
                .unwrap();
        let terrain = source
            .terrain()
            .unwrap()
            .clone()
            .with_page_policy(Some(policy))
            .unwrap();
        let scene = source
            .with_terrain(Some(std::sync::Arc::new(terrain)))
            .unwrap();
        let uploaded = SceneGpu::upload(context.device(), &authority, &scene, &pipelines);
        if budget != config.budget.0 {
            assert!(matches!(uploaded, Err(DrawError::TerrainPages(_))));
            continue;
        }
        let gpu = uploaded.unwrap();
        assert!(gpu.has_terrain_pages());
        let frame = matching_frame();
        gpu.set_frame(context.queue(), frame);
        let request = gpu
            .terrain_residency_request(&frame, [TARGET; 2])
            .unwrap()
            .unwrap();
        assert!(!gpu.terrain_residency_ready(&request).unwrap());
        let mut transaction = authority.begin(context.device()).unwrap();
        gpu.prepare_terrain_pages(context.device(), &mut transaction, &frame, [TARGET; 2])
            .unwrap();
        transaction.finish().submit().unwrap();
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert!(gpu.terrain_residency_ready(&request).unwrap());
        let actual = draw_pixels(&context, &authority, &gpu, &pipelines);
        assert_eq!(actual, render_prepared(&context, &source, true));
    }
}
