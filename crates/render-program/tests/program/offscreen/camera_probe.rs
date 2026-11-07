//! The detailed look at a camera render a host asks for: how the target was set up, where the
//! draws' vertices land in clip space and what the fragment stage reads.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::ids::{MaterialId, RendererId, ShaderId, TextureId};
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{
    Camera, Material, OffscreenFrame, Record, Renderer,
};
use worldrunner_render_program::unity::offscreen::{CameraProbe, ClipCounts, SampledTexture};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::camera_matrices;

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
        object_to_world: "_ObjectToWorld".to_owned(),
        view_projection: "_ViewProjection".to_owned(),
    }
}

/// Two renderers of the quad (four vertices at depth 0.5, corners at x and y of plus and minus
/// one), each translated by 1.5 along x, drawn by a camera whose view-projection is the identity.
/// The material sets `_Tint`, sets `_Bias` to zero, leaves `_Gain` unset and names a 2x2 albedo
/// texture of which three texels are non-zero.
fn frame(rig: &mut Rig, depth_bits: u8) -> OffscreenFrame {
    let synthetic = camera_matrices();
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
