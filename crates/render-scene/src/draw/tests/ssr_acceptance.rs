#[test]
fn narrow_sloped_water_publishes_sample_origins_on_its_geometric_plane() {
    let context = context();
    for samples in [1, 4] {
        for scale in [500_000, 1_000_000] {
            let source = source_geometry(true, false, true, scale);
            let renderer = render(&context, &source, samples);
            let (normal, material, _) = renderer.water_receivers().unwrap();
            for sample in 0..samples {
                let coverage = channel(&context, normal.texture(), sample, 3);
                let offset_x = channel(&context, material.texture(), sample, 0);
                let offset_y = channel(&context, material.texture(), sample, 1);
                let depth = channel(&context, renderer.targets().depth().texture(), sample, 0);
                let inverse = frame(&source).view_projection.as_dmat4().inverse();
                let mut checked = 0;
                for i in 0..32 * 32 {
                    if coverage[i] == 0.0 {
                        continue;
                    }
                    let x = gpu_scalar(i % 32) + offset_x[i];
                    let y = gpu_scalar(i / 32) + offset_y[i];
                    let h = inverse
                        * glam::DVec4::new(
                            f64::from(x / 16.0 - 1.0),
                            f64::from(1.0 - y / 16.0),
                            f64::from(depth[i]),
                            1.0,
                        );
                    let world = h.truncate() / h.w;
                    assert!(
                        world.z.abs() < 0.0001,
                        "sample origin left water plane: {world:?}, {samples}x scale {scale} sample {sample} pixel {i}, offsets {},{}, depth {}",
                        offset_x[i],
                        offset_y[i],
                        depth[i]
                    );
                    assert!(
                        world.x.abs() <= 0.5001,
                        "sample origin left narrow water bounds"
                    );
                    checked += 1;
                }
                assert!(checked > 8);
            }
        }
    }
}
fn gpu_scalar(value: usize) -> f32 {
    f32::from(u16::try_from(value).unwrap())
}

use super::*;
use crate::compositor::{FrameRenderer, encode_scene};
use worldrunner_render_plan::{SceneSsr, SceneWaterBody, SceneWaterSettings, SceneWaterSurface};
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef, ToneMap};

fn request() -> SceneSsr {
    SceneSsr {
        enabled: true,
        max_steps: 256,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 1_000_000,
        ..Default::default()
    }
}
fn body() -> SceneWaterBody {
    SceneWaterBody {
        material: SceneWaterSettings::STOCK,
        surface: SceneWaterSurface::Rectangle,
        flow_millimeters_per_second: [0; 2],
        height_millimeters: 0,
        east_min_millimeters: -20000,
        north_min_millimeters: -20000,
        east_extent_millimeters: 40000,
        north_extent_millimeters: 40000,
        color_micro: [0; 3],
        absorption_micro: [10_000_000; 3],
        roughness_micro: 0,
    }
}
fn source(water: bool, missing: bool) -> RenderScene {
    source_geometry(water, missing, false, 1_000_000)
}
fn source_geometry(water: bool, missing: bool, narrow: bool, scale: u32) -> RenderScene {
    let mut parts = effect_parts();
    parts.sun.irradiance_micro = 0;
    parts.ambient.sky_irradiance_micro = 250_000;
    parts.ambient.ground_irradiance_micro = 250_000;
    parts.water = water.then(|| {
        let mut body = body();
        if narrow {
            body.east_min_millimeters = -500;
            body.east_extent_millimeters = 1000;
            body.north_min_millimeters = -1000;
            body.north_extent_millimeters = 4000;
        }
        body
    });
    let mut material = SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    };
    material.roughness_micro = 500_000;
    material.self_illumination = water.then(|| TextureHandle::from_raw(0));
    let mut placement = identity();
    if water {
        placement.position_millimeters = [if missing { 20000 } else { 0 }, 2000, 100];
        placement.scale_millionths = if narrow { 1_000_000 } else { 3_000_000 };
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![white_texture()],
            vec![SceneMaterial::new(material).unwrap()],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![if water { card(None) } else { quad() }],
            vec![SceneObject::new(
                placement,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: Some(
            SceneEffects::new(parts)
                .unwrap()
                .with_ssr(SceneSsr {
                    resolution_scale_micro: scale,
                    ..request()
                })
                .unwrap(),
        ),
    })
    .unwrap()
}
fn frame(source: &RenderScene) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -3000, 2000],
            target_millimeters: [0, 0, 0],
        },
        camera(),
        Viewport {
            width: 32,
            height: 32,
        },
    )
    .unwrap();
    FrameUniform::new(&view, source.effects()).unwrap()
}
fn render(context: &GpuContext, source: &RenderScene, samples: u32) -> FrameRenderer {
    render_with_environment(
        context,
        source,
        samples,
        &environment(context),
        &frame(source),
    )
}
fn render_with_environment(
    context: &GpuContext,
    source: &RenderScene,
    samples: u32,
    maps: &crate::sky::preparation::PreparedEnvironmentMaps,
    frame: &FrameUniform,
) -> FrameRenderer {
    let mut renderer = FrameRenderer::hdr(
        context,
        [32, 32],
        COLOR_FORMAT,
        samples,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    renderer
        .set_reflection_environment(context.device(), maps)
        .unwrap();
    let pipelines =
        ScenePipelines::with_samples(context.device(), TextureFormat::Rgba16Float, samples)
            .unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), source, &pipelines).unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("custom color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("custom depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (&mut gpu, &pipelines),
        (&color, &depth),
        *frame,
    )
    .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
    renderer
}
fn environment(context: &GpuContext) -> crate::sky::preparation::PreparedEnvironmentMaps {
    environment_faces(context, false)
}
fn environment_faces(
    context: &GpuContext,
    directional: bool,
) -> crate::sky::preparation::PreparedEnvironmentMaps {
    environment_convention(
        context,
        directional,
        worldrunner_render_plan::SceneSpecularConvention::default(),
    )
}
fn environment_convention(
    context: &GpuContext,
    directional: bool,
    convention: worldrunner_render_plan::SceneSpecularConvention,
) -> crate::sky::preparation::PreparedEnvironmentMaps {
    let source = context.device().create_texture(&TextureDescriptor {
        label: Some("acceptance environment"),
        size: Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 6,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba16Float,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes: Vec<_> = [0x3800u16, 0x3400, 0x3000, 0x3c00]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    let faces = if directional {
        [0x3000u16, 0x3c00, 0x3400, 0x3800, 0x4000, 0x2c00]
            .into_iter()
            .flat_map(|red| {
                [red, 0x3400, 0x3000, 0x3c00]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
            })
            .collect()
    } else {
        bytes.repeat(6)
    };
    context.queue().write_texture(
        source.as_image_copy(),
        &faces,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(1),
        },
        source.size(),
    );
    let view = source.create_view(&TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let maps = crate::sky::preparation::encode(
        context.device(),
        &mut encoder,
        &view,
        selected_environment_settings(convention),
        worldrunner_render_gpu::configuration::AllocationBudget(1_000_000),
    )
    .unwrap();
    encoder.finish().submit().unwrap();
    maps
}
fn channel(context: &GpuContext, texture: &Texture, sample: u32, component: u32) -> Vec<f32> {
    let source = channel_source(texture, sample, component);
    let view = texture.create_view(&TextureViewDescriptor::default());
    channel_view(context, &view, &source)
}
fn channel_view(context: &GpuContext, view: &wgpu::TextureView, source: &str) -> Vec<f32> {
    let pipeline = crate::ssao::passes::pipeline(context.device(), source);
    let group = context
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            }],
        });
    let output = crate::ssao::passes::texture(context.device(), [32, 32], TextureFormat::R32Float);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        None,
        RenderPassColorAttachment {
            view: &output.create_view(&TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color::BLACK),
                store: StoreOp::Store,
            },
        },
        &pipeline,
        &[&group],
    );
    encoder.finish().submit().unwrap();
    Readback::new(
        context.device(),
        FrameConfig::new([32, 32], TextureFormat::R32Float, 1),
        100_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), &output)
    .unwrap()
    .chunks_exact(4)
    .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
    .collect()
}
#[test]
fn geometric_water_hit_and_offscreen_miss_at_one_and_four_samples() {
    let context = context();
    for samples in [1, 4] {
        let hit = render(&context, &source(true, false), samples);
        let miss = render(&context, &source(true, true), samples);
        for sample in 0..samples {
            let mut checked = 0;
            let hit_color = channel(&context, hit.targets().scene().texture(), sample, 0);
            let miss_color = channel(&context, miss.targets().scene().texture(), sample, 0);
            let hit_env = channel(
                &context,
                hit.water_receivers().unwrap().2.texture(),
                sample,
                0,
            );
            let miss_env = channel(
                &context,
                miss.water_receivers().unwrap().2.texture(),
                sample,
                0,
            );
            let confidence = channel(&context, hit.ssr.as_ref().unwrap().textures()[4], sample, 3);
            for y in 16..30 {
                for x in 13..19 {
                    let i = y * 32 + x;
                    assert!(
                        (miss_color[i] - miss_env[i]).abs() <= 0.000_061_035_156,
                        "miss must preserve environment at {x},{y}"
                    );
                    if hit_color[i] > hit_env[i] + 0.005 {
                        let coefficients =
                            worldrunner_render_lighting::ibl::brdf_integral(0.5, 0.5, 64)
                                .map(rounded_half);
                        let factor = rounded_half(0.02037) * coefficients[0] + coefficients[1];
                        let incident = 1.25 + 0.5 * cpu_factor();
                        let error =
                            2.0f32.powf(hit_color[i].log2().floor() - 10.0) + factor / 1024.0;
                        let (lower, upper) = confidence_bounds(&confidence, i, samples);
                        let expected =
                            |weight| hit_env[i] + weight * (incident * factor - hit_env[i]);
                        assert!(
                            hit_color[i] >= expected(lower) - error
                                && hit_color[i] <= expected(upper) + error,
                            "water CPU replacement {samples}x sample {sample} ({x},{y}): {} outside {}..{}",
                            hit_color[i],
                            expected(lower),
                            expected(upper)
                        );
                        checked += 1;
                    }
                }
            }
            assert!(
                checked >= 4,
                "geometric emissive hit, samples {samples}, sample {sample}: {checked}"
            );
        }
    }
}
#[test]
fn prepared_forward_publication_matches_split_sum_without_screen_reflections() {
    let context = context();
    let maps = environment(&context);
    let reference_factor = prepared_cpu_factor(&context, &maps);
    let reference_radiance = prepared_cpu_radiance(&context, &maps);
    for samples in [1, 4] {
        let source = source(false, false);
        let renderer = render_with_environment(
            &context,
            &source,
            samples,
            &maps,
            &texel_center_frame(&source),
        );
        let ssr = renderer.ssr.as_ref().unwrap();
        assert!(
            channel(&context, ssr.textures()[4], 0, 3)
                .iter()
                .all(|value| *value == 0.0)
        );
        for sample in 0..samples {
            for component in 0..3 {
                let published = channel(&context, ssr.textures()[2], sample, component);
                let color = channel(
                    &context,
                    renderer.targets().scene().texture(),
                    sample,
                    component,
                );
                let factor = channel(&context, ssr.textures()[3], sample, component);
                let radiance = reference_radiance[usize::try_from(component).unwrap()];
                let mut covered = 0;
                for i in 0..32 * 32 {
                    if factor[i] > 0.0 {
                        let reference = radiance * reference_factor;
                        let ulp = 2.0f32.powf(published[i].log2().floor() - 10.0);
                        assert!(
                            (published[i] - reference).abs() <= ulp,
                            "CPU split sum samples {samples} at {i}: {} != {reference}",
                            published[i]
                        );
                        assert!((published[i] - radiance * factor[i]).abs() <= ulp);
                        assert!(
                            (color[i] - (0.25 + published[i])).abs() <= 0.000_244_140_63,
                            "diffuse + published: {} != {}",
                            color[i],
                            0.25 + published[i]
                        );
                        covered += 1;
                    }
                }
                assert!(covered > 16);
            }
        }
    }
}

fn texel_center_frame(source: &RenderScene) -> FrameUniform {
    // Reflection points along (-1, 2, 1), the +Y face's (0.25, 0.75) texel
    // center at roughness 0.5's integer LOD 1 (a 2x2 face).
    let view = SceneView::orthographic(
        CameraPose {
            eye_millimeters: [1000, -2000, 1000],
            target_millimeters: [0, 0, 0],
        },
        crate::view::OrthographicLens {
            vertical_half_extent_meters: 1.5,
            near_meters: 0.1,
            far_meters: 10.0,
        },
        Viewport {
            width: 32,
            height: 32,
        },
    )
    .unwrap();
    FrameUniform::new(&view, source.effects()).unwrap()
}

fn prepared_cpu_radiance(
    context: &GpuContext,
    maps: &crate::sky::preparation::PreparedEnvironmentMaps,
) -> [f32; 3] {
    let (cube, _) = maps.views().unwrap();
    // texel_center_frame selects +Y face texel (0, 1), mip 1. Read the stored
    // prefilter result, whose f32 integration and half conversion vary by backend.
    let view = cube.texture().create_view(&TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2),
        base_mip_level: 1,
        mip_level_count: Some(1),
        base_array_layer: 2,
        array_layer_count: Some(1),
        ..Default::default()
    });
    [0, 1, 2].map(|component| {
        let source = format!(
            "@group(0) @binding(0) var image: texture_2d<f32>;\n@fragment fn fs_screen() -> @location(0) f32 {{ return textureLoad(image, vec2<i32>(0, 1), 0)[{component}]; }}"
        );
        let value = channel_view(context, &view, &source)[0];
        let expected = [0.5, 0.25, 0.125][component];
        let ulp = 2.0f32.powf(value.log2().floor() - 10.0);
        assert!(
            (value - expected).abs() <= ulp,
            "prepared radiance {value} != constant environment {expected}"
        );
        value
    })
}

fn prepared_cpu_factor(
    context: &GpuContext,
    maps: &crate::sky::preparation::PreparedEnvironmentMaps,
) -> f32 {
    let (_, lut) = maps.views().unwrap();
    assert_eq!([lut.texture().width(), lut.texture().height()], [1, 1]);
    let expected = worldrunner_render_lighting::ibl::brdf_integral(0.5, 0.5, 64);
    // A one-texel LUT has zero interpolation error at any sub-texel precision.
    // Use stored coefficients: GPU half conversion need not round like rounded_half.
    let stored = [0, 1].map(|component| channel(context, lut.texture(), 0, component)[0]);
    for (value, expected) in stored.into_iter().zip(expected) {
        let ulp = 2.0f32.powf(expected.log2().floor() - 10.0);
        assert!(
            (value - expected).abs() <= ulp,
            "prepared BRDF coefficient {value} != CPU integral {expected}"
        );
    }
    0.04 * stored[0] + stored[1]
}

fn rounded_half(value: f32) -> f32 {
    let step = 2.0f32.powf(value.max(2.0f32.powi(-14)).log2().floor() - 10.0);
    (value / step).round() * step
}
fn cpu_factor() -> f32 {
    let coefficients =
        worldrunner_render_lighting::ibl::brdf_integral(0.5, 0.5, 64).map(rounded_half);
    0.04 * coefficients[0] + coefficients[1]
}

fn confidence_bounds(values: &[f32], pixel: usize, samples: u32) -> (f32, f32) {
    if samples == 1 {
        return (values[pixel], values[pixel]);
    }
    let mut lower = values[pixel];
    let mut upper = values[pixel];
    for row in [pixel - 32, pixel, pixel + 32] {
        for &value in &values[row - 1..=row + 1] {
            lower = lower.min(value);
            upper = upper.max(value);
        }
    }
    (lower, upper)
}

fn channel_source(texture: &Texture, sample: u32, component: u32) -> String {
    let kind = if texture.format() == TextureFormat::Depth32Float {
        if texture.sample_count() == 1 {
            "texture_depth_2d"
        } else {
            "texture_depth_multisampled_2d"
        }
    } else if texture.sample_count() == 1 {
        "texture_2d<f32>"
    } else {
        "texture_multisampled_2d<f32>"
    };
    let index = if texture.sample_count() == 1 {
        0
    } else {
        sample
    };
    let offset = if texture.width() > 32 { sample * 32 } else { 0 };
    let component = if texture.format() == TextureFormat::Depth32Float {
        String::new()
    } else {
        format!("[{component}]")
    };
    format!(
        "@group(0) @binding(0) var image: {kind};\n@fragment fn fs_screen(@builtin(position) p: vec4<f32>) -> @location(0) f32 {{ return textureLoad(image, vec2<i32>(p.xy) + vec2<i32>({offset}, 0), {index}){component}; }}"
    )
}

#[path = "ssr_fold.rs"]
mod fold_reflections;

fn environment_settings() -> crate::sky::preparation::EnvironmentPreparation {
    crate::sky::preparation::EnvironmentPreparation {
        convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        cube_size: 4,
        mip_count: 3,
        lut_size: 1,
        samples: 64,
    }
}

#[path = "specular_render_tests.rs"]
mod specular_render_tests;

fn selected_environment_settings(
    convention: worldrunner_render_plan::SceneSpecularConvention,
) -> crate::sky::preparation::EnvironmentPreparation {
    crate::sky::preparation::EnvironmentPreparation {
        convention,
        lut_size: if convention == worldrunner_render_plan::SceneSpecularConvention::default() {
            1
        } else {
            64
        },
        ..environment_settings()
    }
}
