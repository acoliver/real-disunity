//! The scene producer on object scenes (review 2909): an object's folded shading normal is
//! stored as its unit direction, and a frame whose Opaque pass does not reuse the prepass depth
//! stores nothing instead of refusing the frame.

use std::collections::BTreeMap;

use glam::{Mat3, Vec3};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, ROTATION_MICRO_SCALE, RenderScene, SceneAlphaMode, SceneEffects,
    SceneGeometry, SceneMaterial, SceneMaterialParts, SceneMesh, SceneMeshLod, SceneObject,
    SceneOpaquePath, SceneParts, SceneResources, SceneRotation, SceneTexture, SceneTransform,
    SceneVertex, TexelFormat, TextureColorSpace, TextureHandle,
};
use worldrunner_render_post::FrameError;

use super::{Frame, covered, gbuffer, prepare_with, reflections, render, scenes};
use crate::CameraPose;
use crate::device::GpuContext;
use crate::gbuffer::reference::{self, Codes, Surface, angle_degrees};
use crate::gbuffer::tests::{GEOMETRIC_DEGREES, shading_bound};

/// The draw tests' camera: two metres back and two up, looking at the origin.
const POSE: CameraPose = CameraPose {
    eye_millimeters: [0, -2_000, 2_000],
    target_millimeters: [0, 0, 0],
};

/// The four targets one observed frame of `scene` stores, with reflections requested so the
/// depth prepass runs, and the frame's scene-from-view rotation. The draw fixtures use
/// BC-compressed textures, so the context has the stock scene features.
fn observe(scene: &RenderScene) -> Result<([Vec<u8>; 4], Mat3), FrameError> {
    let effects = scene
        .effects()
        .cloned()
        .unwrap_or_else(|| {
            SceneEffects::new(crate::draw::tests::effect_parts()).expect("the fixture effects")
        })
        .with_ssr(reflections())
        .expect("the reflection request validates");
    let scene = scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects))
        .expect("the fixture validates with reflections");
    let uniform = scenes::frame_with(POSE, scene.effects().expect("the scene effects"));
    let context = GpuContext::shared_for_test().expect("actual GPU required");
    let configuration = Frame::Reflecting.configuration(SceneOpaquePath::GBUFFER_OBSERVATION);
    let mut drawn = prepare_with(&context, &scene, &uniform, configuration);
    render(&context, &mut drawn, 1)?;
    let scene_from_view = Mat3::from_mat4(uniform.view).transpose();
    Ok((gbuffer(&context, &drawn.renderer), scene_from_view))
}

/// The fold blends the fixture slope's ground normal (-0.6, 0, 0.8) with the quad's +Z by the
/// fold weight, here 0.25 at a height of 500 mm, and forward lights that blend unnormalized
/// (length 0.96). The G-buffer stores its direction. Storing the raw blend decodes about 1.7
/// degrees off in this view, while the store's own quantisation stays under 0.4 degrees.
#[test]
fn the_scene_producer_stores_a_folded_object_normal_as_its_direction() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let scene = crate::draw::tests::fold_translucency::folded_scene(500);
    let (targets, scene_from_view) = observe(&scene).expect("the folded frame renders");
    let expected = Vec3::new(-0.6, 0.0, 0.8).lerp(Vec3::Z, 0.25).normalize();
    let mut objects = 0;
    for (pixel, code) in covered(&targets[3]) {
        // The quad's geometric normal is +Z; the sloped terrain's is 37 degrees from it.
        if angle_degrees(reference::g3_normal(code), Vec3::Z) > 3.0 {
            continue;
        }
        objects += 1;
        let codes = Codes::from_targets(&targets[0], &targets[1], &targets[2], &targets[3], pixel);
        let shading = reference::decode(&codes, scene_from_view).shading_normal;
        let off = angle_degrees(shading, expected);
        assert!(
            off <= 0.8,
            "{pixel}: the folded normal is off by {off} degrees"
        );
    }
    assert!(objects >= 4, "the folded quad covers {objects} texels");
}

/// A relief impostor keeps Opaque from reusing the prepass depth (it takes the separate-depth
/// fallback), so the producer has no completed depth to test against. The frame still renders,
/// and the G-buffer stays clear.
#[test]
fn a_frame_without_reused_depth_renders_and_stores_nothing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let scene = crate::draw::tests::impostor::relief_scene();
    let (targets, _) = observe(&scene).expect("the relief frame renders");
    assert_eq!(
        covered(&targets[3]).count(),
        0,
        "the relief frame stored texels"
    );
}

/// Each object stores its material's lighting record and terrain stores record 0: the folded
/// quad's material transmits light, so its record is the scene's only non-neutral one, index 1,
/// which G1.a holds as 1 / 255.
#[test]
fn objects_store_their_lighting_record_and_terrain_the_neutral_one() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let folded = crate::draw::tests::fold_translucency::folded_scene(500);
    let mut materials = folded.resources().materials().to_vec();
    materials[0] = materials[0]
        .with_translucency(333_337)
        .expect("the fixture translucency validates");
    let scene = folded
        .with_resources(
            worldrunner_render_plan::SceneResources::new(
                folded.resources().textures().to_vec(),
                materials,
            )
            .expect("the fixture resources validate"),
            folded.geometry().clone(),
            folded.effects().cloned(),
        )
        .expect("the translucent fixture validates");
    let (targets, _) = observe(&scene).expect("the translucent frame renders");
    let (mut objects, mut ground) = (0, 0);
    for (pixel, code) in covered(&targets[3]) {
        let record = targets[1][pixel * 4 + 3];
        if angle_degrees(reference::g3_normal(code), Vec3::Z) <= 3.0 {
            objects += 1;
            assert_eq!(record, 1, "{pixel}: the quad's record");
        } else {
            ground += 1;
            assert_eq!(record, 0, "{pixel}: terrain's record");
        }
    }
    assert!(
        objects >= 4 && ground >= 4,
        "{objects} quad and {ground} terrain texels"
    );
}

/// Micro-units of one whole unit, the scale of the alpha, cutoff and roughness fields.
const MICRO: i64 = 1_000_000;

/// The G1 base colours of the boundary panels: each linear channel is 0 or 1, which sRGB encoding
/// keeps at 0 or 255. The fourth byte is the lighting record, zero for every fixture material.
const RED: [u8; 4] = [255, 0, 0, 0];
const GREEN: [u8; 4] = [0, 255, 0, 0];
const BLUE: [u8; 4] = [0, 0, 255, 0];
const YELLOW: [u8; 4] = [255, 255, 0, 0];
const WHITE: [u8; 4] = [255, 255, 255, 0];

/// The tilt of the normal texel in [`survivor_scene`] from the panel normal. Its xy is
/// (191 / 255 * 2 - 1, 128 / 255 * 2 - 1), about (0.498, 0.004), and z is rebuilt from xy, so the
/// tilt is 29.87 degrees. The check allows one degree, the resolution of the stored code.
const MAPPED_TILT_DEGREES: f32 = 29.9;

/// A one-texel RGBA8 texture whose bytes are sampled as they are, without colour decoding.
fn texel(rgba: [u8; 4]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: rgba.to_vec().into(),
    }
}

fn identity() -> SceneTransform {
    SceneTransform {
        position_millimeters: [0, 0, 0],
        rotation: SceneRotation {
            x: 0,
            y: 0,
            z: 0,
            w: ROTATION_MICRO_SCALE,
        },
        scale_millionths: 1_000_000,
    }
}

/// A rectangle from `west` to `east` and `south` to `north` millimetres at `height`, wound
/// counter-clockwise seen from above so that its upward face is the front.
fn panel(west: i64, east: i64, south: i64, north: i64, height: i64) -> SceneMesh {
    let vertex = |position: [i64; 2], texcoord: [i64; 2]| SceneVertex {
        tangent_micro: None,
        position_millimeters: [position[0], position[1], height],
        normal_micro: [0, 0, MICRO],
        texcoord_micro: texcoord,
    };
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vec![
                vertex([west, south], [0, 0]),
                vertex([east, south], [MICRO, 0]),
                vertex([east, north], [MICRO, MICRO]),
                vertex([west, north], [0, MICRO]),
            ],
            vec![0, 1, 2, 0, 2, 3],
            None,
        )
        .expect("the fixture panel is a valid level"),
    ])
    .expect("a one-level chain is valid")
}

/// Full roughness, no metalness and no occlusion map, with the given alpha mode, albedo texture
/// and optional normal texture.
fn fixture_material(alpha_mode: SceneAlphaMode, albedo: u32, normal: Option<u32>) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(albedo),
        normal: normal.map(TextureHandle::from_raw),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode,
        double_sided: false,
    })
    .expect("the fixture material carries in-range scalars")
}

/// The alpha boundary: four front panels side by side at height zero over a white panel 300 mm
/// below them. From left to right the front panels are red and opaque with alpha zero; green is
/// masked with alpha zero against a 0.5 cutoff; blue is masked with alpha 1 scaled by a 0.5
/// factor, so its alpha is exactly the cutoff; and yellow is masked with alpha zero against a
/// zero cutoff. Every factor and cutoff is a power of two, so the comparisons are exact.
fn boundary_scene() -> RenderScene {
    let textures = vec![
        texel([255, 0, 0, 0]),
        texel([0, 255, 0, 0]),
        texel([0, 0, 255, 255]),
        texel([255, 255, 0, 0]),
        texel([255, 255, 255, 255]),
    ];
    let materials = vec![
        fixture_material(SceneAlphaMode::Opaque, 0, None),
        fixture_material(
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            },
            1,
            None,
        ),
        fixture_material(
            SceneAlphaMode::Mask {
                cutoff_micro: 500_000,
            },
            2,
            None,
        )
        .with_alpha_factor(500_000)
        .expect("the fixture alpha factor validates"),
        fixture_material(SceneAlphaMode::Mask { cutoff_micro: 0 }, 3, None),
        fixture_material(SceneAlphaMode::Opaque, 4, None),
    ];
    let meshes = vec![
        panel(-800, -400, -500, 500, 0),
        panel(-400, 0, -500, 500, 0),
        panel(0, 400, -500, 500, 0),
        panel(400, 800, -500, 500, 0),
        panel(-1000, 1000, -1000, 1000, -300),
    ];
    let objects = (0..5)
        .map(|index| {
            SceneObject::new(
                identity(),
                MeshHandle::from_raw(index),
                MaterialHandle::from_raw(index),
            )
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials)
            .expect("the fixture resources validate"),
        geometry: SceneGeometry::new(meshes, objects).expect("the fixture geometry validates"),
        terrain: None,
        effects: None,
    })
    .expect("the fixture scene validates")
}

/// One panel with a tilted normal map and the stock decal over it, masked or opaque, with a
/// fully covered albedo texel (alpha 1). The decal's material is texture 1, so the decal reaches
/// the panel's colour and roughness.
fn survivor_scene(alpha_mode: SceneAlphaMode) -> RenderScene {
    let textures = vec![
        texel([255, 255, 255, 255]),
        texel([204, 51, 26, 128]),
        texel([191, 128, 255, 255]),
    ];
    let materials = vec![
        fixture_material(alpha_mode, 0, Some(2)),
        fixture_material(SceneAlphaMode::Opaque, 1, None),
    ];
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, materials)
            .expect("the fixture resources validate"),
        geometry: SceneGeometry::new(
            vec![panel(-1000, 1000, -1000, 1000, 0)],
            vec![SceneObject::new(
                identity(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .expect("the fixture geometry validates"),
        terrain: None,
        effects: None,
    })
    .expect("the fixture scene validates")
    .with_decals(vec![crate::draw::tests::decals::decal()])
    .expect("the fixture decal validates")
}

/// The G1 base colour bytes of one stored texel.
fn base_colour(targets: &[Vec<u8>; 4], pixel: usize) -> [u8; 4] {
    targets[1][pixel * 4..pixel * 4 + 4]
        .try_into()
        .expect("four bytes per texel")
}

/// The surface one stored texel decodes to, in scene space.
fn decoded_at(targets: &[Vec<u8>; 4], pixel: usize, scene_from_view: Mat3) -> Surface {
    let codes = Codes::from_targets(&targets[0], &targets[1], &targets[2], &targets[3], pixel);
    reference::decode(&codes, scene_from_view)
}

/// An opaque texel of alpha zero survives. A masked texel below its cutoff is rejected before the
/// material stores anything, so it writes neither a surface nor depth, and the white panel behind
/// shows through it. Alpha exactly at the cutoff survives, including alpha zero at a zero cutoff.
/// Every stored surface is the same fully rough, unscattered plane facing up.
#[test]
fn object_alpha_boundaries_reject_below_cutoff_and_keep_the_rest() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (targets, scene_from_view) =
        observe(&boundary_scene()).expect("the boundary frame renders");
    let view_from_scene = scene_from_view.transpose();
    let mut stored: BTreeMap<[u8; 4], usize> = BTreeMap::new();
    for (pixel, _) in covered(&targets[3]) {
        *stored.entry(base_colour(&targets, pixel)).or_default() += 1;
        assert_eq!(
            &targets[2][pixel * 4..pixel * 4 + 3],
            &[255, 0, 0],
            "{pixel}: roughness, metalness and emission"
        );
        let surface = decoded_at(&targets, pixel, scene_from_view);
        assert!(
            angle_degrees(surface.geometric_normal, Vec3::Z) <= GEOMETRIC_DEGREES,
            "{pixel}: the geometric normal is {:?}",
            surface.geometric_normal
        );
        let bound = shading_bound((view_from_scene * surface.shading_normal).z);
        assert!(
            angle_degrees(surface.shading_normal, Vec3::Z) <= bound,
            "{pixel}: the shading normal is {:?}",
            surface.shading_normal
        );
        assert_eq!(surface.flags, 0, "{pixel}: the scattering flag");
    }
    assert!(
        !stored.contains_key(&GREEN),
        "a masked texel below its cutoff stored a surface: {stored:?}"
    );
    for colour in [RED, BLUE, YELLOW, WHITE] {
        assert!(
            stored.get(&colour).copied().unwrap_or(0) >= 4,
            "{colour:?} stored too few texels: {stored:?}"
        );
    }
    assert_eq!(stored.len(), 4, "unexpected surfaces: {stored:?}");
}

/// A masked texel that survives stores what the same material stores when it is opaque: the
/// tilted mapped normal, the decal's colour and roughness, and the same geometric metadata, in
/// the same texels. The rejection now runs before the normal, decal and surface terms, so this
/// checks that the moved statements still read the same inputs.
#[test]
fn surviving_masked_texels_store_the_mapped_and_decalled_surface_as_the_opaque_material_does() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let masked_scene = survivor_scene(SceneAlphaMode::Mask {
        cutoff_micro: 500_000,
    });
    let (masked, scene_from_view) = observe(&masked_scene).expect("the masked frame renders");
    let (opaque, _) =
        observe(&survivor_scene(SceneAlphaMode::Opaque)).expect("the opaque frame renders");
    let view_from_scene = scene_from_view.transpose();
    let pixels: Vec<usize> = covered(&masked[3]).map(|(pixel, _)| pixel).collect();
    let opaque_pixels: Vec<usize> = covered(&opaque[3]).map(|(pixel, _)| pixel).collect();
    assert_eq!(
        pixels, opaque_pixels,
        "the masked panel covers other texels than the opaque one"
    );
    assert!(
        pixels.len() >= 16,
        "the panel covers {} texels",
        pixels.len()
    );
    let mut decalled = 0;
    for &pixel in &pixels {
        let surface = decoded_at(&masked, pixel, scene_from_view);
        let reference_surface = decoded_at(&opaque, pixel, scene_from_view);
        let tilt = angle_degrees(surface.shading_normal, Vec3::Z);
        assert!(
            (tilt - MAPPED_TILT_DEGREES).abs() <= 1.0,
            "{pixel}: the mapped normal tilts {tilt} degrees"
        );
        let bound = shading_bound((view_from_scene * reference_surface.shading_normal).z);
        let apart = angle_degrees(surface.shading_normal, reference_surface.shading_normal);
        assert!(
            apart <= bound,
            "{pixel}: the masked normal is {apart} degrees from the opaque one"
        );
        assert!(
            angle_degrees(surface.geometric_normal, Vec3::Z) <= GEOMETRIC_DEGREES,
            "{pixel}: the geometric normal is {:?}",
            surface.geometric_normal
        );
        assert_eq!(surface.flags, reference_surface.flags, "{pixel}: flags");
        assert_eq!(
            masked[1][pixel * 4 + 3],
            opaque[1][pixel * 4 + 3],
            "{pixel}: the lighting record"
        );
        for (target, channels) in [(1, 3), (2, 4)] {
            for channel in 0..channels {
                let at = pixel * 4 + channel;
                let (a, b) = (masked[target][at], opaque[target][at]);
                assert!(
                    a.abs_diff(b) <= 1,
                    "{pixel}: target {target} channel {channel} is {a}, opaque {b}"
                );
            }
        }
        if base_colour(&masked, pixel) != WHITE {
            decalled += 1;
        }
    }
    assert!(
        decalled >= 4,
        "the decal reached {decalled} of {} texels",
        pixels.len()
    );
}

/// The alpha boundary fixtures are valid scenes, so a failure of the GPU tests above is the
/// renderer's and not the fixture's. This needs no device.
#[test]
fn alpha_boundary_fixtures_validate_without_a_device() {
    assert_eq!(boundary_scene().geometry().objects().len(), 5);
    assert_eq!(survivor_scene(SceneAlphaMode::Opaque).decals().len(), 1);
    survivor_scene(SceneAlphaMode::Mask {
        cutoff_micro: 500_000,
    });
}
