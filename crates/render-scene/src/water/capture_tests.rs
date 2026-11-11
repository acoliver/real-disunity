//! Byte-equality captures of the water body draw: the bodies of one `WaterFeature` are recorded
//! through `WaterFeature::draw` and through the frozen copy in `fixed`, into the same targets,
//! and the scene colour, the three receivers and the depth are compared byte for byte.
//!
//! Every capture starts from a cleared scene colour, cleared receivers and a cleared depth, and
//! the live capture has to differ from a capture that draws nothing, so a path that draws
//! nothing cannot pass. The snapshot is a checker, so refraction reads something that varies.
//!
//! Both paths share the feature's bind groups, buffers and vertex counts, and the live text
//! specialisers and override constants. The captures establish that the two paths write the
//! same bytes from the same inputs. They do not independently establish that the uniforms or
//! the specialised text are correct; the equation tests beside this file do that.
//!
//! Covered: one sample and, where the adapter has it, four; no shadow group (the third test
//! below adds one);
//! `DepthMode::Independent`; the default specular convention. At four samples the scene colour
//! and the receivers are compared sample by sample. The depth is compared as the snapshot's
//! depth copy reads it, which is the nearest sample of each pixel, so a depth difference that
//! leaves every pixel's nearest sample alone is not seen.
//!
//! The matrix includes wind, the only term the vertex stage reads: its field varies across the
//! surface and its force is nowhere zero, and the wind body's live draw must differ from the
//! same body without wind. That shows the wind reaches the pixels, not which stage carries it; dropping
//! the vertex stage's `WATER_TERMS` is the mutant that checks the vertex stage.
//!
//! One separate test repeats the default body at one sample with `DepthMode::Shared`, on a
//! renderer built with SSAO so that its scene depth is `Depth32FloatStencil8`. It needs a device
//! that was asked for `DEPTH32FLOAT_STENCIL8` and prints a skip line when the adapter lacks it; a
//! skip says nothing about `Shared`. Its printed `depth=Shared` summary line is the evidence of a
//! run.
//!
//! A third test draws three bodies with a shadow group, at the same sample counts and with
//! `DepthMode::Independent`: the default body, a shadow response with a diffuse floor, and the
//! weighted five-texel filter over the same floor. The group's one cascade divides the surface
//! into a lit and an unlit side over a cleared map, so the live draw must also differ from the
//! same body drawn without the group, the floor must change the shadowed default's pixels, and
//! the filter (the only difference between the last two bodies) must change the floor's pixels,
//! and only in a band of columns around the edge.
//!
//! Not covered here: a shadow map holding casters, more than one cascade, `Shared` for any other
//! body, sample count or a shadowed feature, that a frame selects `Shared`, a non-default
//! specular convention, wind modifiers, and wind at more than one time.

use super::*;
use crate::dynamic::capture_tests::shadow_fixture;
use crate::water::{shadow_filter, shadow_response};
use worldrunner_render_plan::{
    SceneEffects, SceneEffectsParts, ScenePlanarWindField, SceneSsao, SceneSsaoDepthFootprint,
    SceneSsaoFilter, SceneSsaoKernel, SceneSsaoReceiver, SceneWaterSeedCoordinates,
    SceneWaterSeedSampling, SceneWaterShadowFilter, SceneWaterShadowResponse, SceneWaterWind,
    SceneWindBand, TextureHandle,
};

/// What every capture's scene colour is cleared to.
const CLEARED: wgpu::Color = wgpu::Color {
    r: 0.5,
    g: 0.125,
    b: 0.0625,
    a: 1.0,
};

/// What is read of each sample, in the order [`Captured::samples`] holds it.
const PER_SAMPLE: [&str; 4] = ["scene colour", "receiver 0", "receiver 1", "receiver 2"];

struct Case {
    name: &'static str,
    body: SceneWaterBody,
    /// The term mask and seed key the body must reach, which select the pipeline.
    options: u32,
}

fn default_case() -> Case {
    Case {
        name: "default",
        body: settings(),
        options: 0,
    }
}

/// Where [`cases`] puts the wind body.
const WIND: usize = 4;

fn cases() -> Vec<Case> {
    let mut terms = settings();
    terms.color_micro = [250_000, 500_000, 750_000];
    terms.material.direct_surface = true;
    terms.material.lighting_normal_micro = Some(0);
    terms.material.metallic_micro = Some(100_000);
    let seeded = |mode, sampling| {
        let mut body = seed_tests::body(mode);
        body.material.flow_seed_sampling = sampling;
        body
    };
    vec![
        default_case(),
        // Lighting normal 1, metallic 2 and direct surface 16: override constants only.
        Case {
            name: "terms",
            body: terms,
            options: 19,
        },
        // Flow coordinates 64 and vertex seeds 512: seed text and a vertex-stage constant.
        Case {
            name: "vertex seeds",
            body: seeded(
                SceneWaterSeedCoordinates::Repeat,
                SceneWaterSeedSampling::Vertex,
            ),
            options: 64 | 512,
        },
        // Flow coordinates 64 and endpoint addressing 256: endpoint text and a second vertex
        // buffer.
        Case {
            name: "endpoint",
            body: seeded(
                SceneWaterSeedCoordinates::EndpointClamped,
                SceneWaterSeedSampling::Fragment,
            ),
            options: 64 | 256,
        },
        // Flow coordinates 64 and wind 128: wind is the only term bit the vertex stage reads,
        // through a vertex-stage constant and a field the vertex stage samples. Wind needs flow
        // seeds, a normal map and a two-phase period.
        Case {
            name: "wind",
            body: windy(),
            options: 64 | 128,
        },
        Case {
            name: "endpoint vertex seeds",
            body: seeded(
                SceneWaterSeedCoordinates::EndpointClamped,
                SceneWaterSeedSampling::Vertex,
            ),
            options: 64 | 256 | 512,
        },
    ]
}

/// The seeded body with a normal map and a two-phase period, which wind requires, under wind
/// whose field covers the whole surface, so the force the vertex stage samples varies across the
/// view.
fn windy() -> SceneWaterBody {
    let mut body = seed_tests::body(SceneWaterSeedCoordinates::Repeat);
    body.material.normal = Some(TextureHandle::from_raw(1));
    body.material.flow_period_milliseconds = Some(4000);
    body.material.wind = Some(SceneWaterWind {
        field: ScenePlanarWindField::new(
            TextureHandle::from_raw(0),
            [-10_000; 2],
            [20_000; 2],
            [None; 2],
        )
        .unwrap(),
        band: SceneWindBand::new(1_000_000, 500_000, 2_000_000).unwrap(),
        amplitude_micro: 1_000_000,
    });
    body.validate()
        .expect("the wind body is a valid water body");
    body
}

/// The first body's normal map and wind field, as 17x17 `Rg8Unorm` textures varying in both
/// axes, for whichever of the two its material names. The wind's force is nowhere zero.
fn install_surface_maps(context: &GpuContext, water: &mut Water) {
    let material = water.settings[0].material;
    if material.normal.is_some() {
        let normal = (0u8..17)
            .flat_map(|y| (0u8..17).flat_map(move |x| [100 + 3 * x, 140 - 2 * y]))
            .collect::<Vec<u8>>();
        water.surface_inputs[0][0] = Some(rg8_texture(context, "capture normal map", &normal));
    }
    if material.wind.is_some() {
        let wind = (0u8..17)
            .flat_map(|y| (0u8..17).flat_map(move |x| [200 - 7 * x, 30 + 9 * y]))
            .collect::<Vec<u8>>();
        water.surface_inputs[0][2] = Some(rg8_texture(context, "capture wind field", &wind));
    }
}

/// A 17x17 `Rg8Unorm` texture holding `texels`.
fn rg8_texture(context: &GpuContext, label: &str, texels: &[u8]) -> wgpu::TextureView {
    let texture = context.device().create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: 17,
            height: 17,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rg8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    context.queue().write_texture(
        texture.as_image_copy(),
        texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(34),
            rows_per_image: Some(17),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// A lit frame, so the direct and ambient terms are not all zero.
fn frame() -> FrameUniform {
    let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
    frame.unlit = false;
    frame.sun_direction = glam::Vec3::new(0.0, 0.6, 0.8);
    frame.sun_irradiance = glam::Vec3::ONE;
    frame.ambient_sky = glam::Vec3::ONE;
    frame.ambient_ground = glam::Vec3::ONE;
    frame
}

/// The sample counts the captures run at: one, and four where the adapter has it.
/// `supported_samples` logs a count the adapter lacks.
fn sample_counts(context: &GpuContext) -> Vec<u32> {
    crate::msaa_tests::supported_samples(context)
        .into_iter()
        .filter(|samples| matches!(samples, 1 | 4))
        .collect()
}

/// What a capture is set up with: the renderer's sample count, and the depth mode and shadow
/// group its feature is built for.
#[derive(Clone, Copy)]
struct Setup<'a> {
    samples: u32,
    depth: DepthMode,
    shadow: Option<&'a wgpu::BindGroup>,
}

/// A renderer of `samples` samples without effects, prepared as [`prepare`] says.
fn prepared(context: &GpuContext, case: &Case, samples: u32) -> crate::compositor::FrameRenderer {
    prepare(context, case, regressions::renderer(context, samples))
}

/// `renderer` with the case's body installed, one frame encoded in slot 1 so the body's uniforms
/// are uploaded, and a checker in that slot's snapshot.
fn prepare(
    context: &GpuContext,
    case: &Case,
    mut renderer: crate::compositor::FrameRenderer,
) -> crate::compositor::FrameRenderer {
    renderer.install_water(context, case.body.clone()).unwrap();
    if case.body.material.flow.is_some() {
        seed_tests::install_field(context, renderer.water.as_mut().unwrap());
    }
    install_surface_maps(context, renderer.water.as_mut().unwrap());
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    renderer.select_frame_slot(1);
    encode_fixture(context, &mut renderer, (&color, &depth), &frame(), 0.25)
        .finish()
        .submit()
        .unwrap();
    wait(context.device());
    optical_render_tests::upload_checker_snapshot(context, &renderer);
    renderer
}

#[derive(Clone, Copy, PartialEq)]
enum Recorder {
    Nothing,
    Live,
    Frozen,
}

/// What a capture leaves in the targets.
struct Captured {
    /// The scene colour after the resolve.
    resolved: Vec<u8>,
    /// The depth as the snapshot's depth copy reads it: the nearest sample of each pixel.
    depth: Vec<u8>,
    /// For each sample, what [`PER_SAMPLE`] names.
    samples: Vec<[Vec<u8>; 4]>,
}

impl Captured {
    /// Every image with its name, in one order for any two captures of one renderer.
    fn images(&self) -> Vec<(String, &[u8])> {
        let mut images = vec![
            ("resolved scene colour".to_owned(), self.resolved.as_slice()),
            ("depth".to_owned(), self.depth.as_slice()),
        ];
        for (sample, read) in self.samples.iter().enumerate() {
            for (name, bytes) in PER_SAMPLE.iter().zip(read) {
                images.push((format!("{name}, sample {sample}"), bytes.as_slice()));
            }
        }
        images
    }
}

/// The targets after `recorder` drew slot 1's feature, built for `depth` and `shadow`.
fn capture(
    context: &GpuContext,
    renderer: &crate::compositor::FrameRenderer,
    (depth, shadow): (DepthMode, Option<&wgpu::BindGroup>),
    recorder: Recorder,
) -> Captured {
    let mut drawn = None;
    draw_water_with(
        context,
        renderer,
        (1, wgpu::LoadOp::Clear(CLEARED), depth, shadow),
        |feature, pass| match recorder {
            Recorder::Nothing => {}
            Recorder::Live => feature.draw(
                &worldrunner_render_post::DrawContext {
                    slot: PassSlot::TransparentWater,
                },
                pass,
            ),
            Recorder::Frozen => {
                let pipelines = fixed::pipelines(context.device(), feature);
                drawn = Some(fixed::record(feature, &pipelines, pass));
            }
        },
    );
    if recorder == Recorder::Frozen {
        assert_eq!(drawn, Some(1), "the fixed path draws the one body");
    }
    let water = renderer.water.as_ref().unwrap();
    let receivers = &water.receivers[1];
    let sample_read = lighting_tests::environment_tests::sample_read;
    let samples = (0..water.samples)
        .map(|sample| {
            [
                sample_read(context, renderer.targets().scene().texture(), sample),
                sample_read(context, &receivers[0], sample),
                sample_read(context, &receivers[1], sample),
                sample_read(context, &receivers[2], sample),
            ]
        })
        .collect();
    Captured {
        resolved: read(context, renderer.targets().texture()),
        depth: surface_depth(context, renderer),
        samples,
    }
}

/// Compares the live draw of `case` at `samples` samples with the frozen one and with a draw of
/// nothing, and returns how many images were equal.
fn compare(context: &GpuContext, case: &Case, samples: u32) -> usize {
    let name = format!("{} at {samples} samples", case.name);
    let renderer = prepared(context, case, samples);
    let setup = Setup {
        samples,
        depth: DepthMode::Independent,
        shadow: None,
    };
    compare_prepared(context, case, (&renderer, setup), &name)
}

/// [`compare`] over a renderer that [`prepare`] has set up as `setup` says.
fn compare_prepared(
    context: &GpuContext,
    case: &Case,
    (renderer, setup): (&crate::compositor::FrameRenderer, Setup<'_>),
    name: &str,
) -> usize {
    let Setup {
        samples,
        depth,
        shadow,
    } = setup;
    let settings = &renderer.water.as_ref().unwrap().settings;
    assert_eq!(
        terms::mask(settings) | crate::water::vertex_seeds::key(&settings[0]),
        case.options,
        "{name}: the body does not reach the pipeline key this case is for"
    );
    let nothing = capture(context, renderer, (depth, shadow), Recorder::Nothing);
    let live = capture(context, renderer, (depth, shadow), Recorder::Live);
    let frozen = capture(context, renderer, (depth, shadow), Recorder::Frozen);
    assert_eq!(
        live.samples.len(),
        usize::try_from(samples).unwrap(),
        "{name}: every sample is read"
    );
    let (live_images, frozen_images) = (live.images(), frozen.images());
    for ((image, live), (_, frozen)) in live_images.iter().zip(&frozen_images) {
        assert!(
            live == frozen,
            "{name}: {image} differs from the fixed path's output (if \
             the_live_water_wgsl_equals_the_frozen_snapshot fails too, copy the live WGSL \
             into water/fixtures)"
        );
    }
    assert!(
        live.resolved != nothing.resolved,
        "{name}: the water left the scene colour as it was cleared"
    );
    assert!(
        live.depth != nothing.depth,
        "{name}: the water left the depth as it was cleared"
    );
    for (sample, (drawn, cleared)) in live.samples.iter().zip(&nothing.samples).enumerate() {
        assert!(
            drawn[0] != cleared[0],
            "{name}: the water left sample {sample} of the scene colour as it was cleared"
        );
        assert!(
            (1..4).any(|index| drawn[index] != cleared[index]),
            "{name}: the water wrote no receiver in sample {sample}"
        );
    }
    live_images.len()
}

#[test]
fn the_body_draw_matches_the_frozen_fixed_path_byte_for_byte() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let adapter = context.adapter().get_info().name;
    let counts = sample_counts(&context);
    assert!(
        counts.contains(&1),
        "one sample is always captured: {counts:?}"
    );
    if !counts.contains(&4) {
        println!("WATER-CAPTURE adapter={adapter:?} samples=4 skipped: the adapter lacks it");
    }
    for case in cases() {
        for &samples in &counts {
            let equal = compare(&context, &case, samples);
            println!(
                "WATER-CAPTURE adapter={adapter:?} case={} options={} samples={samples} images={equal} equal={equal}",
                case.name, case.options
            );
        }
    }
    let live = |case: &Case| {
        let renderer = prepared(&context, case, 1);
        capture(
            &context,
            &renderer,
            (DepthMode::Independent, None),
            Recorder::Live,
        )
        .resolved
    };
    let wind = &cases()[WIND];
    assert_eq!(wind.name, "wind");
    let mut still = wind.body.clone();
    still.material.wind = None;
    let calm = Case {
        name: "wind without wind",
        body: still,
        options: 64,
    };
    let changed = differing_pixels(&live(&calm), &live(wind));
    assert!(
        changed > 0,
        "the wind left every pixel of the same body without wind as it was"
    );
    println!("WATER-CAPTURE adapter={adapter:?} case=wind samples=1 wind_changed_pixels={changed}");
}

/// A one-sample renderer built with SSAO, whose scene depth is therefore the combined depth and
/// stencil format that a frame with SSAO hands to the water pass.
fn shared_depth_renderer(context: &GpuContext) -> crate::compositor::FrameRenderer {
    let effects = SceneEffects::new(SceneEffectsParts {
        ssao: Some(SceneSsao {
            receiver: SceneSsaoReceiver::Nearest,
            depth_footprint: SceneSsaoDepthFootprint::default(),
            kernel: SceneSsaoKernel::Hemisphere,
            filter: SceneSsaoFilter::DepthAwareBox,
            radius_millimeters: 2000,
            intensity_micro: 1_000_000,
            sample_count: 16,
            bias_millimeters: 10,
            horizon_angle_millidegrees: 90_000,
        }),
        ..crate::draw::tests::effect_parts()
    })
    .unwrap();
    let renderer = crate::compositor::FrameRenderer::hdr(
        context,
        [32, 32],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        ToneMap::Reinhard,
        Some(&effects),
    )
    .unwrap();
    assert!(
        renderer.ssao.is_some(),
        "the renderer owns the SSAO metadata that makes its depth the stencil format"
    );
    assert_eq!(
        renderer.targets().depth().texture().format(),
        wgpu::TextureFormat::Depth32FloatStencil8,
        "a Shared capture over a plain depth target would repeat the Independent one"
    );
    renderer
}

#[test]
fn the_shared_depth_body_draw_matches_the_frozen_fixed_path_byte_for_byte() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let stencil = wgpu::Features::DEPTH32FLOAT_STENCIL8;
    let context = match GpuContext::shared_for_test_with_features(
        crate::device::REQUIRED_FEATURES | stencil,
    ) {
        Ok(context) => context,
        Err(crate::device::DeviceError::MissingFeatures { adapter, missing })
            if missing.contains(stencil) =>
        {
            println!(
                "WATER-CAPTURE adapter={adapter:?} depth=Shared skipped: the adapter lacks {missing:?}"
            );
            return;
        }
        Err(error) => panic!("the shared depth capture could not acquire its device: {error}"),
    };
    assert!(
        context.device().features().contains(stencil),
        "the device was created without the stencil depth format"
    );
    let adapter = context.adapter().get_info().name;
    let case = default_case();
    let renderer = prepare(&context, &case, shared_depth_renderer(&context));
    let setup = Setup {
        samples: 1,
        depth: DepthMode::Shared,
        shadow: None,
    };
    let equal = compare_prepared(
        &context,
        &case,
        (&renderer, setup),
        "default at 1 sample with shared depth",
    );
    assert_eq!(equal, 6, "six images are compared at one sample");
    println!(
        "WATER-CAPTURE adapter={adapter:?} depth=Shared format=Depth32FloatStencil8 case={} options={} samples=1 images={equal} equal={equal}",
        case.name, case.options
    );
}

/// The cascade of the shadowed captures: east and north scaled by 0.2, and a light depth of
/// 0.5 + 0.1 * east. The surface lies at height 0, so the map's cleared depth of 0.5 divides it at
/// east 0: one side passes the `LessEqual` comparison and the other does not, which gives the
/// filters an edge to work on.
fn shadow_cascade() -> glam::Mat4 {
    glam::Mat4::from_cols_array(&[
        0.2, 0.0, 0.1, 0.0, //
        0.0, 0.2, 0.0, 0.0, //
        0.0, 0.0, 0.2, 0.0, //
        0.0, 0.0, 0.5, 1.0,
    ])
}

fn shadowed_cases() -> Vec<Case> {
    let floor = SceneWaterShadowResponse {
        diffuse_floor_micro: 600_000,
        ..SceneWaterShadowResponse::default()
    };
    let responding = |response| {
        let mut body = settings();
        body.material.shadow_response = Some(response);
        body
    };
    vec![
        Case {
            name: "shadowed default",
            body: settings(),
            options: 0,
        },
        // Shadow response 1024: a diffuse floor through the uniforms, on its own pipeline key.
        Case {
            name: "shadow response floor",
            body: responding(floor),
            options: shadow_response::SPECIALIZATION,
        },
        // Weighted filter 2048 over the same floor: the five-texel filter text and its uniform
        // flag are all that differ from the case before.
        Case {
            name: "weighted filter",
            body: responding(SceneWaterShadowResponse {
                filter: SceneWaterShadowFilter::WeightedPcf5,
                ..floor
            }),
            options: shadow_response::SPECIALIZATION | shadow_filter::SPECIALIZATION,
        },
    ]
}

/// How many pixels differ between two 32x32 images of one format.
fn differing_pixels(left: &[u8], right: &[u8]) -> usize {
    assert_eq!(left.len(), right.len(), "the images have one size");
    let pixel = left.len() / (32 * 32);
    assert_eq!(pixel * 32 * 32, left.len(), "the images are 32x32");
    left.chunks(pixel)
        .zip(right.chunks(pixel))
        .filter(|(left, right)| left != right)
        .count()
}

/// The first and last column holding a pixel that differs between two 32x32 images of one
/// format, or `None` when none does.
fn differing_columns(left: &[u8], right: &[u8]) -> Option<(usize, usize)> {
    let pixel = left.len() / (32 * 32);
    let columns: Vec<_> = left
        .chunks(pixel)
        .zip(right.chunks(pixel))
        .enumerate()
        .filter(|(_, (left, right))| left != right)
        .map(|(index, _)| index % 32)
        .collect();
    Some((*columns.iter().min()?, *columns.iter().max()?))
}

/// The columns the shadow edge may move under the weighted filter. The camera sits at east 0
/// and the cascade puts the edge at east 0, so the edge projects to the centre of the 32 columns;
/// five texels of the cascade span at most a few columns at the fixture's distance, and this
/// band allows eight on each side.
const EDGE_COLUMNS: std::ops::Range<usize> = 8..24;

/// Draws `case` at `samples` samples with `shadow`, compares it with the fixed path, checks that
/// the group changed the resolved scene colour, prints the summary and returns that colour.
fn compare_shadowed(
    context: &GpuContext,
    case: &Case,
    samples: u32,
    shadow: &wgpu::BindGroup,
) -> Vec<u8> {
    let adapter = context.adapter().get_info().name;
    let name = format!("{} at {samples} samples with a shadow group", case.name);
    let renderer = prepared(context, case, samples);
    let setup = Setup {
        samples,
        depth: DepthMode::Independent,
        shadow: Some(shadow),
    };
    let equal = compare_prepared(context, case, (&renderer, setup), &name);
    let inputs = |shadow| (DepthMode::Independent, shadow);
    let shadowed = capture(context, &renderer, inputs(Some(shadow)), Recorder::Live);
    let unshadowed = capture(context, &renderer, inputs(None), Recorder::Live);
    let changed = differing_pixels(&shadowed.resolved, &unshadowed.resolved);
    assert!(
        changed > 0,
        "{name}: the shadow group changed no pixel of the resolved scene colour"
    );
    println!(
        "WATER-CAPTURE adapter={adapter:?} shadow=cascade case={} options={} samples={samples} images={equal} equal={equal} shadow_changed_pixels={changed}",
        case.name, case.options
    );
    shadowed.resolved
}

#[test]
fn the_shadowed_body_draw_matches_the_frozen_fixed_path_byte_for_byte() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let adapter = context.adapter().get_info().name;
    let shadow = shadow_fixture::group_with_cascade(&context, shadow_cascade());
    let cases = shadowed_cases();
    for samples in sample_counts(&context) {
        let resolved: Vec<_> = cases
            .iter()
            .map(|case| compare_shadowed(&context, case, samples, &shadow))
            .collect();
        let floor = differing_pixels(&resolved[1], &resolved[0]);
        let filter = differing_pixels(&resolved[2], &resolved[1]);
        assert!(
            floor > 0,
            "the diffuse floor changed no shadowed pixel at {samples} samples"
        );
        assert!(
            filter > 0,
            "the weighted filter changed no pixel along the shadow edge at {samples} samples"
        );
        let (first, last) = differing_columns(&resolved[2], &resolved[1]).unwrap();
        assert!(
            EDGE_COLUMNS.contains(&first) && EDGE_COLUMNS.contains(&last),
            "the weighted filter changed pixels in columns {first}..={last}, outside the band \
             {EDGE_COLUMNS:?} around the shadow edge, at {samples} samples"
        );
        println!(
            "WATER-CAPTURE adapter={adapter:?} shadow=cascade samples={samples} floor_changed_pixels={floor} filter_changed_pixels={filter} filter_columns={first}..={last}"
        );
    }
}
