//! The detailed look at a camera render a host asks for: how the target was set up, where the
//! draws' vertices land in clip space and what the fragment stage reads.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::ids::{MaterialId, RendererId, ShaderId, TextureId};
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{
    Camera, Material, OffscreenFrame, Record, Renderer,
};
use worldrunner_render_program::unity::offscreen::{
    CameraProbe, ClipCounts, DRAW_LAYERS, MemberOrigin, Origin, SampledTexture, StateVariant,
    TriangleCounts,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::SyntheticShader;
use worldrunner_render_program::unity::synthetic::fixed;
use worldrunner_render_program::unity::synthetic::shaders::camera_matrices;
use worldrunner_unity_serialized::shader_state::ShaderState;

use super::camera::{CAMERA, TARGET, alpha};
use super::rig::{QUAD, Rig, layer, quad, source, texture};

const SHADER: ShaderId = ShaderId(1);
const ALBEDO: TextureId = TextureId(20);

/// Translates by 1.5 along x. Matrices are column-major: the translation is the fourth column.
const TRANSLATE_X: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.5, 0.0, 0.0, 1.0,
];
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn names() -> CameraProbe {
    CameraProbe {
        targets: BTreeSet::from([TARGET]),
        test_targets: BTreeSet::new(),
        object_to_world: "_ObjectToWorld".to_owned(),
        view_projection: "_ViewProjection".to_owned(),
        variants: BTreeMap::new(),
    }
}

/// Two renderers of the quad (four vertices at depth 0.5, corners at x and y of plus and minus
/// one), each translated by 1.5 along x, drawn by a camera whose view-projection is the identity.
/// The material sets `_Tint`, sets `_Bias` to zero, leaves `_Gain` unset and names a 2x2 albedo
/// texture of which three texels are non-zero.
fn frame(rig: &mut Rig, depth_bits: u8) -> OffscreenFrame {
    frame_of(rig, depth_bits, camera_matrices())
}

/// The frame of [`frame`] drawn with the shader `synthetic`.
fn frame_of(rig: &mut Rig, depth_bits: u8, synthetic: SyntheticShader) -> OffscreenFrame {
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, QUAD, &quad(0.5))
        .unwrap();
    let bytes = [[9_u8, 0, 0, 0], [0, 0, 0, 0], [0, 7, 0, 0], [0, 0, 0, 1]].concat();
    rig.state
        .set_texture(&mut rig.runtime, &source(20, [2, 2], bytes))
        .unwrap();
    let mut properties = layer(&[("_Tint", [1.0; 4]), ("_Bias", [0.0; 4])]);
    properties.textures.insert(
        "_Albedo".to_owned(),
        TextureInput {
            texture: ALBEDO,
            sampler: None,
        },
    );
    let renderer = |draw: u64| {
        let mut per_draw = PropertyLayer::default();
        per_draw
            .values
            .set("_ObjectToWorld", PropertyValue::Matrix(TRANSLATE_X));
        (
            RendererId(draw),
            Renderer {
                mesh: QUAD,
                materials: vec![MaterialId(1)],
                layer: 1,
                block: PropertyLayer::default(),
                per_draw,
            },
        )
    };
    let mut values = PropertyLayer::default();
    values
        .values
        .set("_ViewProjection", PropertyValue::Matrix(IDENTITY));
    let mut target = texture(10, [8, 8]);
    target.depth_bits = depth_bits;
    OffscreenFrame {
        render_textures: vec![target],
        materials: BTreeMap::from([(
            MaterialId(1),
            Material {
                shader: Some(SHADER),
                keywords: Vec::new(),
                properties,
                fallback: PropertyLayer::default(),
                render_queue: 2000,
            },
        )]),
        renderers: BTreeMap::from([renderer(1), renderer(2)]),
        cameras: BTreeMap::from([(
            CAMERA,
            Camera {
                culling_mask: 1 << 1,
                clear_color: Some([1.0, 0.5, 0.0, 1.0]),
                clear_depth: true,
                pass: alpha(),
                values,
                front_face: wgpu::FrontFace::Cw,
            },
        )]),
        records: vec![Record::CameraRender {
            camera: CAMERA,
            target: TARGET,
        }],
        probe: names(),
        ..OffscreenFrame::default()
    }
}

#[test]
fn a_probed_camera_counts_where_its_vertices_land_in_clip_space() {
    let mut rig = Rig::new();
    let frame = frame(&mut rig, 0);
    let passes = rig.build(&frame).unwrap().camera_passes();
    let programs = &passes[0].programs;
    assert_eq!(programs.len(), 1);
    let program = &programs[0];
    assert_eq!(
        (program.shader, program.pass, program.draws),
        (SHADER, 0, 2)
    );
    assert_eq!(program.vertices, 8);
    // Each draw's corners at x of -1 and 1 land at 0.5 and 2.5: half of them are inside.
    assert_eq!(
        program.clip,
        ClipCounts {
            inside: 4,
            outside: 4,
            behind: 0
        }
    );
    // Read as the transposes, the translation becomes the w row: w is 2.5 at x of 1 and -0.5
    // at x of -1, so half of the vertices are behind the camera and the rest inside.
    assert_eq!(
        program.clip_transposed,
        ClipCounts {
            inside: 4,
            outside: 0,
            behind: 4
        }
    );
    assert_eq!(
        (program.without_positions, program.without_matrices),
        (0, 0)
    );
    assert!(program.vertex_without_value.is_empty());
    assert!(program.vertex_zero.is_empty());
}

#[test]
fn a_draw_that_binds_no_view_projection_is_counted_and_its_vertex_constants_named() {
    let mut rig = Rig::new();
    let mut frame = frame(&mut rig, 0);
    frame.cameras.get_mut(&CAMERA).unwrap().values = PropertyLayer::default();
    let passes = rig.build(&frame).unwrap().camera_passes();
    let program = &passes[0].programs[0];
    assert_eq!(program.without_matrices, 2);
    assert_eq!(program.clip, ClipCounts::default());
    assert_eq!(
        program.vertex_without_value,
        vec!["_ViewProjection".to_owned()]
    );
}

#[test]
fn a_probed_camera_names_the_fragment_constants_with_no_value_or_zero_and_the_textures_it_reads() {
    let mut rig = Rig::new();
    let frame = frame(&mut rig, 0);
    let passes = rig.build(&frame).unwrap().camera_passes();
    let program = &passes[0].programs[0];
    assert_eq!(program.fragment_without_value, vec!["_Gain".to_owned()]);
    assert_eq!(program.fragment_zero, vec!["_Bias".to_owned()]);
    assert_eq!(
        program.textures,
        vec![SampledTexture {
            name: "_Albedo".to_owned(),
            texture: ALBEDO,
            uploaded: true,
            texels: Some(4),
            nonzero: Some(3),
        }]
    );
}

#[test]
fn a_camera_the_host_did_not_ask_about_reports_no_programs() {
    let mut rig = Rig::new();
    let mut frame = frame(&mut rig, 0);
    frame.probe = CameraProbe::default();
    let passes = rig.build(&frame).unwrap().camera_passes();
    assert!(passes[0].programs.is_empty());
}

#[test]
fn a_camera_reports_its_viewport_and_what_it_clears() {
    let mut rig = Rig::new();
    let frame = frame(&mut rig, 0);
    let setup = rig.build(&frame).unwrap().camera_passes().remove(0).setup;
    assert_eq!(setup.viewport, [8, 8]);
    assert_eq!(setup.color, "clear [1.0, 0.5, 0.0, 1.0]");
    assert_eq!(setup.depth, "no depth");

    let mut rig = Rig::new();
    let mut frame = self::frame(&mut rig, 24);
    frame.cameras.get_mut(&CAMERA).unwrap().clear_color = None;
    let setup = rig.build(&frame).unwrap().camera_passes().remove(0).setup;
    assert_eq!(setup.color, "load");
    assert_eq!(setup.depth, "clear 1.0");

    frame.cameras.get_mut(&CAMERA).unwrap().clear_depth = false;
    let setup = rig.build(&frame).unwrap().camera_passes().remove(0).setup;
    assert_eq!(setup.depth, "load");
}

#[test]
fn a_test_render_is_drawn_like_the_camera_but_left_out_of_the_frames_counts() {
    let mut rig = Rig::new();
    let mut frame = frame(&mut rig, 0);
    let test_target = TextureId(11);
    frame.render_textures.push(texture(11, [8, 8]));
    frame.probe.test_targets = BTreeSet::from([test_target]);
    frame.records.push(Record::CameraRender {
        camera: CAMERA,
        target: test_target,
    });
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.camera_draws().len(), 1);
    assert_eq!(operations.camera_passes().len(), 1);
    assert_eq!(operations.camera_causes().len(), 1);
    assert_eq!(operations.camera_draws()[0].target, TARGET);
    let images = rig
        .run(&frame, &[(TARGET, 0, 0), (test_target, 0, 0)])
        .unwrap();
    // The shader's gain is unset, so a fragment that is drawn writes zeros over the clear.
    for image in images {
        let written = image
            .chunks(4)
            .filter(|texel| *texel == [0, 0, 0, 0])
            .count();
        let cleared = image
            .chunks(4)
            .filter(|texel| *texel == [255, 128, 0, 255])
            .count();
        assert_eq!((written, cleared), (16, 48));
    }
}

type StateEdit = dyn Fn(&mut ShaderState);

/// The camera shader with `edit` applied to the state of every pass.
fn shader_with(edit: impl Fn(&mut ShaderState)) -> SyntheticShader {
    let mut synthetic = camera_matrices();
    for pass in &mut synthetic.shader.parsed.sub_shaders[0].passes {
        edit(&mut pass.state);
    }
    synthetic
}

/// The triangles of the one probed program.
fn triangles(rig: &mut Rig, frame: &OffscreenFrame) -> TriangleCounts {
    let passes = rig.build(frame).unwrap().camera_passes();
    passes[0].programs[0].triangles
}

#[test]
fn the_triangles_of_a_probed_camera_are_counted_by_winding_and_by_what_the_declared_state_culls() {
    // The quad's two triangles are counter-clockwise in clip space with Y up. Clip Y grows down
    // the image, so they are clockwise in the image: front faces when clockwise is front.
    let back = |state: &mut ShaderState| state.depth.cull = fixed(2.0);
    let front = |state: &mut ShaderState| state.depth.cull = fixed(1.0);
    let expected = |culled, drawn| TriangleCounts {
        total: 4,
        counter_clockwise: 4,
        culled,
        drawn,
        ..TriangleCounts::default()
    };
    for (front_face, edit, culled, drawn) in [
        (
            wgpu::FrontFace::Cw,
            &back as &dyn Fn(&mut ShaderState),
            0,
            4,
        ),
        (wgpu::FrontFace::Ccw, &back, 4, 0),
        (wgpu::FrontFace::Cw, &front, 4, 0),
        (wgpu::FrontFace::Ccw, &front, 0, 4),
        (wgpu::FrontFace::Ccw, &|_: &mut ShaderState| {}, 0, 4),
    ] {
        let mut rig = Rig::new();
        let mut frame = frame_of(&mut rig, 0, shader_with(edit));
        frame.cameras.get_mut(&CAMERA).unwrap().front_face = front_face;
        assert_eq!(
            triangles(&mut rig, &frame),
            expected(culled, drawn),
            "{front_face:?}"
        );
    }
}

#[test]
fn a_triangle_is_outside_when_its_vertices_are_beyond_one_side_and_clockwise_ones_are_counted() {
    let mut rig = Rig::new();
    let mut frame = frame(&mut rig, 0);
    // The first renderer is moved to x of 4 and 6, beyond the right side for both triangles.
    let mut moved = PropertyLayer::default();
    let mut matrix = IDENTITY;
    matrix[12] = 5.0;
    moved
        .values
        .set("_ObjectToWorld", PropertyValue::Matrix(matrix));
    frame.renderers.get_mut(&RendererId(1)).unwrap().per_draw = moved;
    let counts = triangles(&mut rig, &frame);
    assert_eq!(
        (counts.total, counts.outside, counts.counter_clockwise),
        (4, 2, 2)
    );

    // Mirrored in x, the winding reverses: the other renderer's triangles are clockwise.
    let mut mirrored = IDENTITY;
    mirrored[0] = -1.0;
    mirrored[12] = 0.5;
    let mut layer = PropertyLayer::default();
    layer
        .values
        .set("_ObjectToWorld", PropertyValue::Matrix(mirrored));
    frame.renderers.get_mut(&RendererId(2)).unwrap().per_draw = layer;
    let counts = triangles(&mut rig, &frame);
    assert_eq!(
        (counts.clockwise, counts.counter_clockwise, counts.outside),
        (2, 0, 2)
    );
}

#[test]
fn a_triangle_with_a_vertex_at_or_behind_the_camera_is_behind_and_one_without_area_is_degenerate() {
    let mut rig = Rig::new();
    let mut frame = frame(&mut rig, 0);
    // w becomes x, so the vertices at x of 0.5 are in front and a draw moved to x of -2.5 and
    // -0.5 is behind.
    let mut w_is_x = IDENTITY;
    w_is_x[3] = 1.0;
    w_is_x[15] = 0.0;
    let mut values = PropertyLayer::default();
    values
        .values
        .set("_ViewProjection", PropertyValue::Matrix(w_is_x));
    frame.cameras.get_mut(&CAMERA).unwrap().values = values;
    let mut behind = IDENTITY;
    behind[12] = -1.5;
    let mut layer = PropertyLayer::default();
    layer
        .values
        .set("_ObjectToWorld", PropertyValue::Matrix(behind));
    frame.renderers.get_mut(&RendererId(1)).unwrap().per_draw = layer;
    // Flattened onto a line, the second renderer's triangles have no area.
    let mut flat = TRANSLATE_X;
    flat[5] = 0.0;
    let mut layer = PropertyLayer::default();
    layer
        .values
        .set("_ObjectToWorld", PropertyValue::Matrix(flat));
    frame.renderers.get_mut(&RendererId(2)).unwrap().per_draw = layer;
    let counts = triangles(&mut rig, &frame);
    assert_eq!(
        (counts.total, counts.behind, counts.degenerate),
        (4, 2, 2),
        "{counts:?}"
    );
}

#[test]
fn the_fragment_constants_with_no_value_or_zeros_are_traced_to_the_layer_that_gave_them() {
    let mut rig = Rig::new();
    let frame = frame(&mut rig, 0);
    let passes = rig.build(&frame).unwrap().camera_passes();
    let origins = &passes[0].programs[0].fragment_origins;
    let material = DRAW_LAYERS
        .iter()
        .position(|name| *name == "material")
        .unwrap();
    // `_Bias` is zero in the material for both draws; `_Gain` is in no layer.
    assert_eq!(
        origins,
        &vec![
            MemberOrigin {
                name: "_Bias".to_owned(),
                origin: Origin::Layer(material),
                draws: 2,
                zero_draws: 2
            },
            MemberOrigin {
                name: "_Gain".to_owned(),
                origin: Origin::Unset,
                draws: 2,
                zero_draws: 2
            },
        ]
    );
}

/// The frame with a test render of the camera into a second target of the same shape, in which
/// the draws are made as `variant` says.
fn test_render(
    rig: &mut Rig,
    synthetic: SyntheticShader,
    variant: StateVariant,
) -> (OffscreenFrame, TextureId) {
    let test_target = TextureId(11);
    let mut frame = frame_of(rig, 16, synthetic);
    let mut copy = texture(11, [8, 8]);
    copy.depth_bits = 16;
    frame.render_textures.push(copy);
    frame.probe.test_targets = BTreeSet::from([test_target]);
    frame.probe.variants = BTreeMap::from([(test_target, variant)]);
    frame.records.push(Record::CameraRender {
        camera: CAMERA,
        target: test_target,
    });
    (frame, test_target)
}

/// The texels of the test render that differ from the camera's clear, and those of `colour`.
fn written(rig: &mut Rig, frame: &OffscreenFrame, target: TextureId) -> Vec<[u8; 4]> {
    let image = rig.run(frame, &[(target, 0, 0)]).unwrap().remove(0);
    image
        .chunks(4)
        .map(|texel| <[u8; 4]>::try_from(texel).unwrap())
        .filter(|texel| *texel != [255, 128, 0, 255])
        .collect()
}

#[test]
fn a_test_render_made_as_a_variant_shows_which_part_of_the_declared_state_leaves_no_texel() {
    let cull_back = |state: &mut ShaderState| state.depth.cull = fixed(2.0);
    let never = |state: &mut ShaderState| state.depth.test = fixed(1.0);
    let keep = |state: &mut ShaderState| {
        state.blend[0].src = fixed(0.0);
        state.blend[0].dst = fixed(1.0);
        state.blend[0].src_alpha = fixed(0.0);
        state.blend[0].dst_alpha = fixed(1.0);
    };
    let cases: [(&StateEdit, StateVariant); 3] = [
        (
            &cull_back,
            StateVariant {
                cull_none: true,
                ..StateVariant::default()
            },
        ),
        (
            &never,
            StateVariant {
                depth_always: true,
                ..StateVariant::default()
            },
        ),
        (
            &keep,
            StateVariant {
                blend_off: true,
                ..StateVariant::default()
            },
        ),
    ];
    for (declared, variant) in cases {
        // The camera front face is counter-clockwise in the image, so the quad is a back face
        // where the pass culls back faces.
        let mut rig = Rig::new();
        let (mut frame, target) =
            test_render(&mut rig, shader_with(declared), StateVariant::default());
        frame.cameras.get_mut(&CAMERA).unwrap().front_face = wgpu::FrontFace::Ccw;
        assert!(
            written(&mut rig, &frame, target).is_empty(),
            "{variant:?}: the declared state draws nothing"
        );
        frame.probe.variants = BTreeMap::from([(target, variant)]);
        let texels = written(&mut rig, &frame, target);
        // Both renderers cover the same 2 of 8 columns, and the gain is unset, so a drawn
        // fragment is zeros.
        assert_eq!(texels, vec![[0, 0, 0, 0]; 16], "{variant:?}");
    }
}

#[test]
fn a_constant_fragment_writes_its_colour_where_the_declared_one_writes_zeros() {
    let mut rig = Rig::new();
    let variant = StateVariant {
        constant_fragment: true,
        ..StateVariant::default()
    };
    let (frame, target) = test_render(&mut rig, camera_matrices(), variant);
    let texels = written(&mut rig, &frame, target);
    // 0.0625 and 0.9375 of 255 are 15.9 and 239.1 to the nearest byte.
    assert_eq!(texels, vec![[16, 239, 16, 239]; 16]);
}

#[test]
fn a_test_render_of_a_target_without_a_variant_draws_as_declared() {
    let mut rig = Rig::new();
    let (mut frame, target) = test_render(&mut rig, camera_matrices(), StateVariant::default());
    frame.probe.variants.clear();
    assert_eq!(written(&mut rig, &frame, target), vec![[0, 0, 0, 0]; 16]);
}

/// The test render's written texels with `edit` applied to the frame.
fn fallback_written(edit: impl Fn(&mut OffscreenFrame)) -> Vec<[u8; 4]> {
    let mut rig = Rig::new();
    let (mut frame, target) = test_render(&mut rig, camera_matrices(), StateVariant::default());
    frame.probe.variants.clear();
    edit(&mut frame);
    written(&mut rig, &frame, target)
}

#[test]
fn a_materials_fallback_gives_a_property_no_other_layer_sets_and_sits_below_the_rest() {
    let gain = |value: [f32; 4]| layer(&[("_Gain", value)]);
    // The material leaves `_Gain` unset, so the output is zeros; the fallback's gain of one lets
    // the albedo through.
    let unset = fallback_written(|_| {});
    assert_eq!(unset, vec![[0, 0, 0, 0]; 16]);
    let filled = fallback_written(|frame| {
        frame.materials.get_mut(&MaterialId(1)).unwrap().fallback = gain([1.0; 4]);
    });
    assert!(
        filled.iter().any(|texel| *texel != [0, 0, 0, 0]),
        "the fallback's gain reaches the shader"
    );
    // The material's own value, even zero, wins over the fallback.
    let material = fallback_written(|frame| {
        let material = frame.materials.get_mut(&MaterialId(1)).unwrap();
        material.fallback = gain([1.0; 4]);
        material
            .properties
            .values
            .set("_Gain", PropertyValue::Vector([0.0; 4]));
    });
    assert_eq!(material, vec![[0, 0, 0, 0]; 16]);
    // So does a renderer's property block.
    let block = fallback_written(|frame| {
        frame.materials.get_mut(&MaterialId(1)).unwrap().fallback = gain([1.0; 4]);
        for renderer in frame.renderers.values_mut() {
            renderer.block = gain([0.0; 4]);
        }
    });
    assert_eq!(block, vec![[0, 0, 0, 0]; 16]);
}

#[test]
fn a_property_only_the_fallback_sets_is_traced_to_it() {
    let mut rig = Rig::new();
    let mut frame = frame(&mut rig, 0);
    frame.materials.get_mut(&MaterialId(1)).unwrap().fallback = layer(&[("_Gain", [1.0; 4])]);
    let passes = rig.build(&frame).unwrap().camera_passes();
    let program = &passes[0].programs[0];
    assert!(program.fragment_without_value.is_empty());
    // `_Gain` is no longer unset or zero; only `_Bias`, which the material sets to zero, is.
    let fallback = DRAW_LAYERS
        .iter()
        .position(|name| *name == "material fallback");
    assert_eq!(fallback, Some(4));
    assert_eq!(
        program
            .fragment_origins
            .iter()
            .map(|origin| origin.name.as_str())
            .collect::<Vec<_>>(),
        vec!["_Bias"]
    );
}

#[test]
fn the_triangles_the_declared_state_draws_are_the_ones_the_gpu_draws() {
    // The CPU count of drawn triangles against the texels a render writes: the quad is drawn
    // when clockwise is front and culled when counter-clockwise is.
    for (front_face, drawn, texels) in [(wgpu::FrontFace::Cw, 4, 16), (wgpu::FrontFace::Ccw, 0, 0)]
    {
        let mut rig = Rig::new();
        let (mut frame, target) = test_render(
            &mut rig,
            shader_with(|state| state.depth.cull = fixed(2.0)),
            StateVariant::default(),
        );
        frame.probe.variants.clear();
        frame.cameras.get_mut(&CAMERA).unwrap().front_face = front_face;
        let counted = triangles(&mut rig, &frame);
        assert_eq!(counted.drawn, drawn, "{front_face:?}");
        assert_eq!(
            written(&mut rig, &frame, target).len(),
            texels,
            "{front_face:?}"
        );
    }
}
