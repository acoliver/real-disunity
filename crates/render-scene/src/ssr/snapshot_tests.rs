use super::*;
use crate::water::snapshots::{SnapshotFeature, Snapshots};

fn camera() -> FrameUniform {
    let eye = glam::Vec3::new(972.0, 853.0, 125.0);
    let view = glam::camera::rh::view::look_at_mat4(
        eye,
        eye + glam::Vec3::new(0.3, -0.8, -0.2),
        glam::Vec3::Z,
    );
    let projection = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 9000.0);
    FrameUniform {
        view,
        view_projection: projection * view,
        camera_position: eye,
        camera_position_millimeters: [972_000, 853_000, 125_000],
        raster_viewport: SIZE,
        ..frame()
    }
}

fn case(water: bool) -> Case {
    Case {
        size: SIZE,
        rounding: worldrunner_render_plan::SceneDownsampleRounding::Floor,
        scale: 500_000,
        roughness: 0.1,
        samples: 1,
        edge: Edge::None,
        ao: true,
        full: true,
        water,
    }
}

fn snapshot(
    context: &GpuContext,
    snapshots: &Snapshots,
    raw: &wgpu::Texture,
    target: &wgpu::Texture,
) {
    let input = AttachmentView::new(ResourceRef::new("snapshot fixture depth"), raw).unwrap();
    let group = snapshots.source_group(context.device(), &input, true);
    let mut feature = SnapshotFeature {
        snapshots: snapshots.clone(),
        frame: camera(),
        color_source: None,
        depth_source: None,
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("matching snapshot depth"),
        attachment(&view(target)),
        &snapshots.depth_pipeline,
        &[&group],
    );
    encoder.finish().submit().unwrap();
}

fn commit_camera(context: &GpuContext, ssr: &Ssr, target: &wgpu::Texture) {
    let mut feature = ReflectionFeature {
        water: None,
        ssr,
        frame: camera(),
        color: AttachmentView::new(ResourceRef::new("snapshot resolve target"), target).unwrap(),
        groups: Vec::new(),
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    feature.commit(&mut prepare);
    prepare.finish();
    encoder.finish().submit().unwrap();
}

fn depths(context: &GpuContext, config: FrameConfig, metres: &str) -> wgpu::Texture {
    let projection = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 9000.0);
    water_tests::raw_depth::projected_texture(context, config, metres, projection)
}

fn inputs(
    context: &GpuContext,
    ssr: &Ssr,
    water: &crate::water::Water,
    config: FrameConfig,
    metres: &str,
    final_metres: &str,
) -> [Vec<wgpu::TextureView>; 2] {
    let snapshots = &water.snapshots[0];
    let opaque = depths(context, config, metres);
    let final_raw = depths(context, config, final_metres);
    snapshot(context, snapshots, &opaque, &snapshots.depth);
    snapshot(context, snapshots, &final_raw, &snapshots.final_depth);
    let mut views = vec![
        view(&ssr.material),
        view(&ssr.normals),
        view(&snapshots.depth),
        view(&snapshots.final_depth),
    ];
    views.extend(ssr.reflection_views());
    views.extend(ssr.forward_products.iter().map(view));
    let mut opaque_views = views.clone();
    water::Inputs::new(water, 0).resolve_views(&mut views, view(&final_raw));
    ssr.application_views(&mut views);
    ssr.application_views(&mut opaque_views);
    [opaque_views, views]
}

fn capture_snapshot(context: &GpuContext, metres: f32, covered: bool) -> [Vec<f32>; 5] {
    let case = case(true);
    let config = FrameConfig::new(SIZE, wgpu::TextureFormat::Rgba16Float, 1);
    let mut screen = SceneMetadata::new(context.device(), SIZE, neutral_ao()).unwrap();
    let ssr = Ssr::new(context.device(), config, policy(case), &mut screen).unwrap();
    let water = water_tests::water_fixture(context, config);
    initialize(context, &ssr, &water, case);
    fill(
        context,
        &water.receivers[0][0],
        if covered {
            "vec4<f32>(0.0, 0.0, 1.0, 1.0)"
        } else {
            "vec4<f32>(0.0)"
        },
    );
    for texture in &ssr.reflections {
        upload(
            context,
            texture,
            &[0.25, 0.5, 0.75, 1.0]
                .repeat(usize::try_from(texture.width() * texture.height()).unwrap()),
        );
    }
    let target = &water.receivers[1][0];
    commit_camera(context, &ssr, target);
    let final_metres = format!("{:.1}", if covered { metres * 0.5 } else { metres });
    let views = inputs(
        context,
        &ssr,
        &water,
        config,
        &format!("{metres:.1}"),
        &final_metres,
    );
    [0, 1, 2, 3, 4].map(|mode| {
        let bound = mode != 2 && mode != 3;
        let source = snapshot_source(case, mode);
        let pipeline = gpu::pipeline(context.device(), &source, 1, false);
        let selected = &views[usize::from(bound)];
        let group = gpu::group(context.device(), &pipeline, Some(&ssr.uniform), selected);
        render(context, target, &pipeline, &[&group]);
        read_lane(context, target, 0)
    })
}

fn snapshot_source(case: Case, mode: u32) -> String {
    let case = Case {
        water: mode != 2 && mode != 3,
        ..case
    };
    let source = source(case, mode.is_multiple_of(2));
    if mode != 4 {
        return source;
    }
    source.replace(
        "return reflection + vec4<f32>(reflection_delta",
        "return vec4<f32>(select(0.0, 1.0, reflection_receiver_visible(p, textureLoad(opaque_depth, p, 0).r))) + reflection * 0.0 + vec4<f32>(reflection_delta",
    )
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|value| value.to_bits()).collect()
}

#[test]
fn gpu_snapshot_water_bound_opaque_receivers_match_opaque_only_bit_for_bit() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut failures = Vec::new();
    for metres in [50.0, 200.0, 800.0] {
        let [pair, delta, opaque_pair, opaque_delta, visibility] =
            capture_snapshot(&context, metres, false);
        let accepted = pair.chunks_exact(4).filter(|pixel| pixel[3] > 0.0).count();
        println!(
            "snapshot {metres}m: accepted {accepted}/{}",
            SIZE[0] * SIZE[1]
        );
        assert!(opaque_pair.chunks_exact(4).all(|pixel| pixel[3] > 0.0));
        assert!(opaque_delta.chunks_exact(4).all(|pixel| pixel[0] < 0.0));
        if accepted != usize::try_from(SIZE[0] * SIZE[1]).unwrap()
            || bits(&pair) != bits(&opaque_pair)
            || bits(&delta) != bits(&opaque_delta)
            || !visibility
                .iter()
                .all(|value| value.to_bits() == 1.0f32.to_bits())
        {
            failures.push(format!(
                "{metres}m: accepted {accepted}, pair/delta mismatch"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn gpu_snapshot_water_masks_opaque_and_preserves_water_pair_and_delta() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for metres in [50.0, 200.0, 800.0] {
        let [pair, delta, opaque_pair, opaque_delta, visibility] =
            capture_snapshot(&context, metres, true);
        assert!(visibility.iter().all(|&value| value == 0.0));
        println!(
            "covered {metres}m: pair {:?}, delta {:?}",
            &pair[..4],
            &delta[..4]
        );
        assert!(opaque_pair.iter().chain(&opaque_delta).all(|&v| v == 0.0));
        // Pin the unfixed water-bound resolve, including f16 readback bit patterns.
        assert_eq!(bits(&pair[..4]), bits(&[0.25, 0.5, 0.75, 1.0]));
        assert_eq!(bits(&delta[..4]), bits(&[-1.875, -1.75, -1.625, 0.0]));
    }
}

fn owner_source(mut case: Case, water: bool) -> String {
    case.water = water;
    source(case, true)
        // Ownership must remain observable even where the sparse trace has no hit.
        .replace("if reflection.a <= 0.0 { discard; }", "")
        .replace(
            "return reflection + vec4<f32>(reflection_delta",
            "return vec4<f32>(f32(reflection_surface(p, sample).owner)) + reflection * 0.0 + vec4<f32>(reflection_delta",
        )
}

#[test]
fn gpu_snapshot_single_water_lane_rejects_opaque_but_not_neighbour() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let case = Case {
        samples: 4,
        ..case(true)
    };
    let config = FrameConfig::new(SIZE, wgpu::TextureFormat::Rgba16Float, 4);
    let mut screen = SceneMetadata::new(context.device(), SIZE, neutral_ao()).unwrap();
    let ssr = Ssr::new(context.device(), config, policy(case), &mut screen).unwrap();
    let water = water_tests::water_fixture(&context, config);
    initialize(&context, &ssr, &water, case);
    fill(
        &context,
        &water.receivers[0][0],
        "vec4<f32>(0.0, 0.0, 1.0, select(0.0, 1.0, p.x < 1.0 && lane == 0u))",
    );
    // Water replaces a 100 m lane with 75 m. A different opaque lane at 50 m
    // keeps both snapshot minima equal, so main accepts this mixed pixel.
    let views = inputs(
        &context,
        &ssr,
        &water,
        config,
        "select(100.0, 50.0, lane == 1u)",
        "select(select(100.0, 50.0, lane == 1u), 75.0, p.x < 1.0 && lane == 0u)",
    );
    let target = &water.receivers[1][0];
    commit_camera(&context, &ssr, target);
    for (bound, selected) in views.iter().enumerate() {
        let source = owner_source(case, bound == 1);
        let pipeline = gpu::pipeline(context.device(), &source, 4, false);
        let group = gpu::group(context.device(), &pipeline, Some(&ssr.uniform), selected);
        render(&context, target, &pipeline, &[&group]);
        for lane in 0..4 {
            let coverage = read_lane(&context, &water.receivers[0][0], lane);
            let alpha = if lane == 0 { 1.0f32 } else { 0.0f32 };
            assert_eq!(coverage[3].to_bits(), alpha.to_bits());
            assert_eq!(coverage[7].to_bits(), 0.0f32.to_bits());
            let owners = read_lane(&context, target, lane);
            let covered_owner = match (bound, lane) {
                (0, _) => 1.0,
                (_, 0) => 2.0,
                _ => 0.0,
            };
            assert_eq!(
                &owners[..4],
                &[covered_owner; 4],
                "bound={bound}, lane={lane}"
            );
            assert_eq!(
                &owners[4..8],
                &[1.0; 4],
                "neighbour bound={bound}, lane={lane}"
            );
        }
    }
}

#[test]
fn water_visibility_source_checks_all_lanes_without_comparing_depths() {
    let source = snapshot_source(
        Case {
            samples: 4,
            ..case(true)
        },
        4,
    );
    let function = source
        .split("fn reflection_receiver_visible(")
        .nth(1)
        .unwrap()
        .split("fn reflection_surface(")
        .next()
        .unwrap();
    assert!(function.contains("sample < 4u"));
    assert!(
        function.contains("textureLoad(water_normals, p, i32(sample)).a != 0.0 { return false; }")
    );
    assert!(function.contains("return true;"));
    assert!(!function.contains("inverse_projection"));
    assert!(!function.contains("textureLoad(water_depth"));
    assert!(!function.contains("abs("));
    assert!(!function.contains("max("));
}
