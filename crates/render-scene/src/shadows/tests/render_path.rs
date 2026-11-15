//! Rendering the opaque frame from the G-buffer through the shadowed scene, the path a game with
//! shadows and screen-space AO draws: terrain, meshes and an impostor card are stored and lit
//! before Opaque, which draws only the sky. The frame stays close to Forward, every stored pixel
//! is lit, the sky appears exactly where nothing is stored, and water over the deferred terrain
//! refracts the lit opaque snapshot as it refracts the forward one.
use super::deferred::{Captured, capture, ssao};
use super::*;

/// Relative error a stored pixel may have against Forward, and the absolute floor below which
/// values count as equal: the G-buffer quantisation the lighting step measured (shading normal
/// within 0.25 degrees facing the camera, base colour within 0.0045, scalars within half a code)
/// moves a lit value by up to 5%.
const RELATIVE: f32 = 0.05;
const ABSOLUTE: f32 = 2e-3;
/// Share of stored pixels that may exceed [`RELATIVE`]: shadow receiver-plane derivatives come
/// from neighbouring screen pixels in the lighting pass and from the triangle in forward, so
/// they differ along depth and normal edges, where the lit value can move by more.
const EDGE_SHARE: f64 = 0.01;
/// Mean relative error over the stored pixels.
const MEAN: f64 = 0.01;

/// The share of `errors` over [`RELATIVE`], and their mean.
fn share_and_mean(errors: &[f32]) -> (usize, f64, f64) {
    let count = f64::from(u32::try_from(errors.len()).unwrap());
    let over = errors.iter().filter(|error| **error > RELATIVE).count();
    let share = f64::from(u32::try_from(over).unwrap()) / count;
    let mean = errors.iter().map(|error| f64::from(*error)).sum::<f64>() / count;
    (over, share, mean)
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

/// A sky the opaque pass draws where nothing is stored, the forward-only draw the frame keeps.
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
/// with SSAO for the depth prepass when `prepass` and, with `with_water`, water over the east
/// half.
fn source(with_water: bool, prepass: bool) -> RenderScene {
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
    if prepass {
        parts.ssao = Some(ssao());
    }
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

fn halves(bytes: &[u8]) -> Vec<f32> {
    crate::terrain::deferred_tests::halves(bytes)
}

/// The relative error of each stored pixel's worst channel against Forward.
fn stored_errors(rendered: &[f32], forward: &[f32], g3: &[u16]) -> Vec<f32> {
    rendered
        .chunks_exact(4)
        .zip(forward.chunks_exact(4))
        .zip(g3)
        .filter(|(_, code)| **code != 0)
        .map(|((got, want), _)| {
            got[..3]
                .iter()
                .zip(&want[..3])
                .map(|(got, want)| (got - want).abs() / want.abs().max(ABSOLUTE / RELATIVE))
                .fold(0.0_f32, f32::max)
        })
        .collect()
}

/// Checks the rendered frame against Forward and returns the share of stored pixels over
/// [`RELATIVE`] and the mean error.
fn compare(label: &str, rendered: &Captured, forward: &Captured) -> (f64, f64) {
    let g3 = rendered.g3.as_ref().expect("stored targets");
    let got = halves(&rendered.colour);
    let want = halves(&forward.colour);
    let mut stored = 0usize;
    let mut sky = 0usize;
    for (pixel, ((got, want), code)) in got
        .chunks_exact(4)
        .zip(want.chunks_exact(4))
        .zip(g3)
        .enumerate()
    {
        if *code == 0 {
            // Nothing stored: Opaque's sky draw is the only writer in both frames.
            assert!(
                got.iter()
                    .zip(want)
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
                "{label}: unstored pixel {pixel} differs from Forward: {got:?} {want:?}"
            );
            if want[..3].iter().any(|value| *value > 0.0) {
                sky += 1;
            }
        } else {
            stored += 1;
            // No unlit holes: every stored pixel is lit and opaque.
            assert!(
                got[..3].iter().any(|value| *value > 0.0) && got[3].to_bits() == 1.0_f32.to_bits(),
                "{label}: stored pixel {pixel} is unlit: {got:?}"
            );
        }
    }
    assert!(stored > g3.len() / 4, "{label}: {stored} stored pixels");
    assert!(sky > 0, "{label}: the sky does not appear");
    let errors = stored_errors(&got, &want, g3);
    let (over, share, mean) = share_and_mean(&errors);
    let worst = errors.iter().copied().fold(0.0_f32, f32::max);
    eprintln!(
        "{label}: {stored} stored, {sky} sky pixels; over {RELATIVE}: {over} ({share:.4}); \
         mean {mean:.5}; worst {worst:.4}"
    );
    (share, mean)
}

#[test]
fn gpu_shadowed_render_path_matches_forward_without_unlit_holes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for with_water in [false, true] {
        let source = source(with_water, true);
        let label = format!("water {with_water}");
        let forward = capture(&context, &source, &view(), SceneOpaquePath::Forward).unwrap();
        let rendered =
            capture(&context, &source, &view(), SceneOpaquePath::GBUFFER_RENDER).unwrap();
        // Rendering allocates no diagnostic lighting target: the lit surfaces are the scene colour.
        assert!(rendered.lit.is_none(), "{label}");
        let (share, mean) = compare(&label, &rendered, &forward);
        assert!(
            share <= EDGE_SHARE,
            "{label}: {share} of stored pixels over {RELATIVE}"
        );
        assert!(mean <= MEAN, "{label}: mean error {mean}");
        // Opaque no longer draws terrain or objects, so the lit stored pixels can only come
        // from the lighting pass; a second capture through the same path is identical.
        let again = capture(&context, &source, &view(), SceneOpaquePath::GBUFFER_RENDER).unwrap();
        assert!(again.colour == rendered.colour, "{label}: not repeatable");
    }
}

#[test]
fn gpu_water_over_deferred_terrain_refracts_the_lit_opaque_snapshot() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let dry = source(false, true);
    let wet = source(true, true);
    let render = |source: &RenderScene, path| capture(&context, source, &view(), path).unwrap();
    let forward_dry = halves(&render(&dry, SceneOpaquePath::Forward).colour);
    let forward_wet = halves(&render(&wet, SceneOpaquePath::Forward).colour);
    let rendered_dry = halves(&render(&dry, SceneOpaquePath::GBUFFER_RENDER).colour);
    let wet_capture = render(&wet, SceneOpaquePath::GBUFFER_RENDER);
    let rendered_wet = halves(&wet_capture.colour);
    let g3 = wet_capture.g3.unwrap();
    // Water pixels: those the water changes in Forward. Through the G-buffer path the water
    // changes the same pixels, by the same amount within the stored-surface bound, so it
    // refracted the lit terrain and not a cleared or stale snapshot.
    let mut water = 0usize;
    let mut changed_errors = Vec::new();
    for (index, (((fd, fw), rd), rw)) in forward_dry
        .chunks_exact(4)
        .zip(forward_wet.chunks_exact(4))
        .zip(rendered_dry.chunks_exact(4))
        .zip(rendered_wet.chunks_exact(4))
        .enumerate()
    {
        let forward_change = fd[..3]
            .iter()
            .zip(&fw[..3])
            .any(|(a, b)| (a - b).abs() > 0.01);
        if !forward_change || g3[index] == 0 {
            continue;
        }
        water += 1;
        let rendered_change = rd[..3]
            .iter()
            .zip(&rw[..3])
            .any(|(a, b)| (a - b).abs() > 0.005);
        assert!(
            rendered_change,
            "pixel {index}: the water is missing over deferred terrain"
        );
        changed_errors.push(
            rw[..3]
                .iter()
                .zip(&fw[..3])
                .map(|(got, want)| (got - want).abs() / want.abs().max(ABSOLUTE / RELATIVE))
                .fold(0.0_f32, f32::max),
        );
    }
    assert!(water > g3.len() / 10, "{water} water pixels");
    let (over, share, mean) = share_and_mean(&changed_errors);
    eprintln!("water: {water} pixels; over {RELATIVE}: {over} ({share:.4}); mean {mean:.5}");
    assert!(
        share <= EDGE_SHARE,
        "{share} of water pixels over {RELATIVE}"
    );
    assert!(mean <= MEAN, "mean water error {mean}");
}

#[test]
fn gpu_render_path_without_the_depth_prepass_is_refused_by_name() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = source(false, false);
    match capture(&context, &source, &view(), SceneOpaquePath::GBUFFER_RENDER) {
        Err(ShadowError::Schedule(worldrunner_render_post::FrameError::FeaturePreparation {
            feature,
            reason,
        })) => {
            assert_eq!(feature, "opaque G-buffer");
            assert_eq!(
                reason,
                "rendering from the G-buffer needs the depth prepass"
            );
        }
        Err(other) => panic!("unexpected error {other:?}"),
        Ok(_) => panic!("the G-buffer path rendered without a prepass"),
    }
    // Forward draws the same scene.
    capture(&context, &source, &view(), SceneOpaquePath::Forward).unwrap();
}

/// A same-frame depth-fitted capture, the route the game's captures take, owns every write of
/// the G-buffer slot and the lighting pass: both G-buffer paths render, and rendering from the
/// G-buffer stays close to Forward under the fitted cascades.
#[test]
fn gpu_depth_fitted_capture_renders_both_gbuffer_paths() {
    use crate::compositor::{FeatureTarget, FrameRenderer};
    use crate::gbuffer::GbufferTarget;
    use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = source(false, true);
    let view = view();
    let size = [view.viewport().width, view.viewport().height];
    let mut captured = Vec::new();
    for path in [
        SceneOpaquePath::Forward,
        SceneOpaquePath::GBUFFER_OBSERVATION,
        SceneOpaquePath::GBUFFER_RENDER,
    ] {
        let effects = source
            .effects()
            .unwrap()
            .clone()
            .with_opaque_path(path)
            .unwrap();
        let mut frame = FrameRenderer::hdr(
            &context,
            size,
            OffscreenTarget::COLOR_FORMAT,
            1,
            worldrunner_render_post::ToneMap::Reinhard,
            Some(&effects),
        )
        .unwrap();
        let mut renderer = ShadowedScene::with_target(
            &context,
            &source,
            settings(),
            wgpu::TextureFormat::Rgba16Float,
            1,
        )
        .unwrap();
        renderer
            .set_depth_fitting(
                &context,
                Some(SdsmSettings {
                    policy: AdoptionPolicy::SameFrameCapture,
                    range: super::sdsm::policy(),
                }),
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
        let fit = renderer
            .render_depth_fitted_with(&context, &view, (&color, &depth), 10, &mut frame)
            .unwrap_or_else(|error| panic!("{path:?}: {error:?}"));
        assert_eq!(fit.map(|fit| fit.source_frame), Some(10), "{path:?}");
        context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        captured.push(Captured {
            colour: super::deferred::read(&context, frame.targets().texture()),
            lit: None,
            g3: frame
                .feature_target(FeatureTarget::Gbuffer(GbufferTarget::G3))
                .map(|texture| {
                    super::deferred::read(&context, texture)
                        .chunks_exact(2)
                        .map(|code| u16::from_le_bytes([code[0], code[1]]))
                        .collect()
                }),
        });
    }
    // The observation keeps the forward frame.
    assert!(
        captured[1].colour == captured[0].colour,
        "the observed frame differs"
    );
    let (share, mean) = compare("depth-fitted", &captured[2], &captured[0]);
    assert!(
        share <= EDGE_SHARE,
        "{share} of stored pixels over {RELATIVE}"
    );
    assert!(mean <= MEAN, "mean error {mean}");
}
