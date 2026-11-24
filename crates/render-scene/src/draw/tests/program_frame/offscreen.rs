//! Offscreen work and a scene draw in one frame.
//!
//! A camera renders three renderers into a render texture, a blit inverts it into a second one,
//! a copy puts that into element 2 of a texture array, and a program draw admitted to the scene
//! samples that element in the stock opaque pass. All of it is in the frame's one submission.
//! Every expected pixel is derived from the placements, colors and the inversion the passes
//! apply, not from the output.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::super::*;
use super::rig::{Fixture, NEAR_DEPTH, RED, engine_depth, engine_scene};
use super::settings;
mod cancellation;
mod composition;
use composition::check_timing;

use super::DEPTH_FORMAT;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::FrameError;
use worldrunner_render_post::{
    FrameOperations, OperationsOutput, PassEntry, PassSlot, ResourceRef, TimingMode,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::ids::{
    CameraId, MaterialId, MeshId, RendererId, ShaderId, TextureId,
};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::{
    Camera, Material, OffscreenFrame, ReadRange, Record, RenderDimension, RenderTextureSpec,
    Renderer,
};
use worldrunner_render_program::unity::offscreen::{
    OffscreenError, OffscreenOperations, OffscreenState, PendingImage,
};
use worldrunner_render_program::unity::runtime::{
    BoundResource, DrawPacket, Geometry, GraphicsProgram, ProgramArena, ProgramRuntime,
    TargetSignature, VertexInput,
};
use worldrunner_render_program::unity::shader::{PassSelector, ProgramShader};
use worldrunner_render_program::unity::state::RenderState;
use worldrunner_render_program::unity::synthetic::shaders::{
    array_patch, blit_passes, blit_quad, camera_passes, quad,
};

const CAMERA_SHADER: ShaderId = ShaderId(1);
const BLIT_SHADER: ShaderId = ShaderId(2);
const A: TextureId = TextureId(10);
const B: TextureId = TextureId(11);
const ARRAY: TextureId = TextureId(12);
const ARENA: u64 = 1 << 20;

const CYAN: [u8; 4] = [0, 255, 255, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];
const ZERO: [u8; 4] = [0, 0, 0, 0];

/// The player's side of a frame: its own runtime and arena on the compositor's authority, the
/// offscreen state and the scene program.
struct Player {
    runtime: ProgramRuntime,
    arena: ProgramArena,
    state: OffscreenState,
    patch: GraphicsProgram,
}

/// A fixture on a device with timestamp queries, which the shared test device does not request.
/// Timing is required, so a device without them fails here rather than skipping the check.
fn timed() -> Fixture {
    let mut settings = settings();
    settings.timing = TimingMode::Required;
    let context = pollster::block_on(GpuContext::request_with_features_for_test(
        crate::device::REQUIRED_FEATURES | wgpu::Features::TIMESTAMP_QUERY,
    ))
    .expect("a device with timestamp queries");
    Fixture::with_context(context, &engine_scene(), settings, |device| {
        ScenePipelines::new(device, SCENE_FORMAT, 1).unwrap()
    })
}

fn player(fixture: &mut Fixture) -> Player {
    let reservation = fixture
        .renderer
        .reserve(&fixture.context, ProgramRuntime::arena_allocations(ARENA))
        .unwrap();
    let mut runtime = ProgramRuntime::new(
        fixture.context.device().clone(),
        fixture.context.submission().clone(),
        AllocationBudget(ARENA),
    );
    let arena = runtime.open_arena(reservation).unwrap();
    let mut state = OffscreenState::new(&runtime);
    for (id, synthetic) in [
        (CAMERA_SHADER, camera_passes()),
        (BLIT_SHADER, blit_passes("_MainTex")),
    ] {
        state.add_shader(
            id,
            ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
        );
    }
    state.set_mesh(&mut runtime, MeshId(1), &quad(0.5)).unwrap();
    state
        .set_mesh(&mut runtime, MeshId(2), &blit_quad())
        .unwrap();
    let synthetic = array_patch();
    let mut shader = ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap();
    let resolved = shader.resolve(&PassSelector::Index(0), &[]).unwrap();
    let patch = runtime
        .graphics_program(&resolved.variant.vertex, &resolved.variant.fragment)
        .unwrap();
    Player {
        runtime,
        arena,
        state,
        patch,
    }
}

fn layer(values: &[(&str, [f32; 4])]) -> PropertyLayer {
    let mut layer = PropertyLayer::default();
    for (name, value) in values {
        layer.values.set(name, PropertyValue::Vector(*value));
    }
    layer
}

fn material(shader: ShaderId, tint: [f32; 4]) -> Material {
    Material {
        shader: Some(shader),
        keywords: Vec::new(),
        properties: layer(&[("_Tint", tint)]),
        fallback: PropertyLayer::default(),
        below_globals: PropertyLayer::default(),
        render_queue: 2000,
    }
}

fn renderer(material: u64, layer_index: u32, place: [f32; 4]) -> Renderer {
    Renderer {
        mesh: MeshId(1),
        materials: vec![MaterialId(material)],
        layer: layer_index,
        block: PropertyLayer::default(),
        slot_blocks: std::collections::BTreeMap::new(),
        per_draw: layer(&[("_Place", place)]),
    }
}

fn texture(id: TextureId, layers: u32) -> RenderTextureSpec {
    RenderTextureSpec {
        id,
        size: [8, 8],
        dimension: if layers == 1 {
            RenderDimension::D2
        } else {
            RenderDimension::D2Array(layers)
        },
        graphics_format: 8,
        depth_bits: 0,
        mips: 1,
        srgb: false,
        generation: 0,
    }
}

/// A camera that sees layers 1 and 2 of three renderers: red over columns 0 to 3 of an 8 by 8
/// target on layer 1, blue over columns 6 and 7 on layer 2, and a green band over rows 3 and 4 on
/// layer 3, which the camera does not see. It clears to yellow. The blit inverts the target and
/// the copy puts the result in element 2 of a three element array.
fn frame() -> OffscreenFrame {
    let alpha = PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Alpha".to_owned()],
    };
    OffscreenFrame {
        blit_mesh: Some(MeshId(2)),
        render_textures: vec![texture(A, 1), texture(B, 1), texture(ARRAY, 3)],
        materials: BTreeMap::from([
            (MaterialId(1), material(CAMERA_SHADER, [1.0, 0.0, 0.0, 1.0])),
            (MaterialId(2), material(CAMERA_SHADER, [0.0, 0.0, 1.0, 1.0])),
            (MaterialId(3), material(CAMERA_SHADER, [0.0, 1.0, 0.0, 1.0])),
            (MaterialId(4), material(BLIT_SHADER, [1.0; 4])),
        ]),
        renderers: BTreeMap::from([
            (RendererId(1), renderer(1, 1, [0.5, 1.0, -0.5, 0.0])),
            (RendererId(2), renderer(2, 2, [0.25, 1.0, 0.75, 0.0])),
            (RendererId(3), renderer(3, 3, [1.0, 0.25, 0.0, 0.0])),
        ]),
        cameras: BTreeMap::from([(
            CameraId(1),
            Camera {
                culling_mask: 0b110,
                clear_color: Some([1.0, 1.0, 0.0, 1.0]),
                clear_depth: true,
                pass: alpha,
                values: PropertyLayer::default(),
                front_face: wgpu::FrontFace::Cw,
                renderers: Arc::new(BTreeSet::from([
                    RendererId(1),
                    RendererId(2),
                    RendererId(3),
                ])),
            },
        )]),
        records: vec![
            Record::CameraRender {
                camera: CameraId(1),
                target: A,
            },
            Record::Blit {
                source: A,
                target: B,
                material: Some(MaterialId(4)),
                pass: 2,
            },
            Record::CopyTexture {
                source: B,
                source_element: 0,
                source_mip: 0,
                target: ARRAY,
                target_element: 2,
                target_mip: 0,
            },
        ],
        later_reads: vec![ReadRange {
            texture: ARRAY,
            mips: 0..1,
            layers: 2..3,
        }],
        ..OffscreenFrame::default()
    }
}

/// The scene draw: a rectangle at the near patch's place whose texture coordinates run from 0 to 1
/// across it, sampling the texture array the offscreen copy filled.
fn scene_patch() -> Vec<u8> {
    let [x0, y0, x1, y1] = [0.2_f32, -0.2, 0.8, 0.2];
    let corners = [
        ([x0, y0], [0.0, 0.0]),
        ([x1, y0], [1.0, 0.0]),
        ([x1, y1], [1.0, 1.0]),
        ([x0, y0], [0.0, 0.0]),
        ([x1, y1], [1.0, 1.0]),
        ([x0, y1], [0.0, 1.0]),
    ];
    corners
        .iter()
        .flat_map(|(position, uv)| [position[0], position[1], NEAR_DEPTH, uv[0], uv[1]])
        .flat_map(f32::to_le_bytes)
        .collect()
}

struct Readbacks {
    camera: PendingImage,
    blit: PendingImage,
    element: PendingImage,
    untouched: PendingImage,
}

/// Builds the frame and appends the readbacks of every stage.
fn build(player: &mut Player) -> (OffscreenOperations, Readbacks) {
    build_frame(player, &frame())
}

/// Builds `frame` and appends the readbacks of the textures the standard frame writes.
fn build_frame(player: &mut Player, frame: &OffscreenFrame) -> (OffscreenOperations, Readbacks) {
    let mut operations = player
        .state
        .build(&mut player.runtime, &player.arena, frame)
        .unwrap();
    let readbacks = Readbacks {
        camera: operations.read_back(A, 0, 0).unwrap(),
        blit: operations.read_back(B, 0, 0).unwrap(),
        element: operations.read_back(ARRAY, 0, 2).unwrap(),
        untouched: operations.read_back(ARRAY, 0, 0).unwrap(),
    };
    (operations, readbacks)
}

fn pixel(bytes: &[u8], (x, y): (usize, usize)) -> [u8; 4] {
    bytes[(y * 8 + x) * 4..][..4].try_into().unwrap()
}

/// The scene patch as a program draw that samples element 2 of the texture array.
fn admit_scene_draw(fixture: &mut Fixture, player: &mut Player, recording: &wgpu::CommandEncoder) {
    let signature = fixture.signature();
    admit_scene_draw_for(fixture, player, recording, signature);
}

/// Admits the scene patch draw prepared for `signature`, which need not be the frame's pass.
fn admit_scene_draw_for(
    fixture: &mut Fixture,
    player: &mut Player,
    recording: &wgpu::CommandEncoder,
    signature: TargetSignature,
) {
    let vertices = player
        .runtime
        .create_buffer("scene patch", &scene_patch())
        .unwrap();
    let attribute = |format, offset, shader_location| wgpu::VertexAttribute {
        format,
        offset,
        shader_location,
    };
    let input = VertexInput {
        buffer: vertices,
        stride: 20,
        step: wgpu::VertexStepMode::Vertex,
        attributes: vec![
            attribute(wgpu::VertexFormat::Float32x3, 0, 0),
            attribute(wgpu::VertexFormat::Float32x2, 12, 1),
        ],
    };
    let bindings = [BoundResource::texture(
        0,
        0,
        player.state.texture_handle(ARRAY).unwrap(),
    )];
    let state = RenderState {
        cull: None,
        depth_compare: wgpu::CompareFunction::LessEqual,
        depth_write: true,
        color_write: wgpu::ColorWrites::ALL,
        blend: None,
        stencil: None,
    };
    let packet = DrawPacket {
        program: &player.patch,
        state: &state,
        front_face: wgpu::FrontFace::Cw,
        vertex_inputs: std::slice::from_ref(&input),
        bindings: &bindings,
        geometry: Geometry::Vertices { first: 0, count: 6 },
    };
    let programs = player
        .runtime
        .admit_draws(recording, &player.arena, signature, &[packet])
        .unwrap();
    fixture.gpu.admit_programs(programs);
}

/// The camera and blit stages read back from the render textures.
fn check_offscreen_stages(readbacks: Readbacks, device: &wgpu::Device) {
    // The camera: layers 1 and 2 over the yellow clear color; the green band on layer 3 is culled.
    let camera = readbacks.camera.bytes(device);
    assert_eq!(pixel(&camera, (1, 0)), RED);
    assert_eq!(pixel(&camera, (3, 7)), RED);
    assert_eq!(
        pixel(&camera, (1, 3)),
        RED,
        "the culled band does not cover the red renderer"
    );
    assert_eq!(pixel(&camera, (4, 0)), YELLOW);
    assert_eq!(pixel(&camera, (5, 4)), YELLOW, "nor the clear color");
    assert_eq!(pixel(&camera, (6, 2)), BLUE);
    assert_eq!(pixel(&camera, (7, 7)), BLUE);
    // The blit inverts: red is cyan, yellow is blue, blue is yellow.
    let blit = readbacks.blit.bytes(device);
    assert_eq!(pixel(&blit, (1, 0)), CYAN);
    assert_eq!(pixel(&blit, (4, 4)), BLUE);
    assert_eq!(pixel(&blit, (7, 7)), YELLOW);
    // The copy fills element 2 and leaves element 0 as it was.
    assert_eq!(readbacks.element.bytes(device), blit);
    assert!(
        readbacks
            .untouched
            .bytes(device)
            .chunks(4)
            .all(|texel| texel == ZERO)
    );
}

#[test]
fn a_camera_a_blit_a_copy_and_a_scene_draw_run_in_the_frames_one_submission() {
    assert!(NEAR_DEPTH < engine_depth());
    let mut fixture = timed();
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let before = authority.submitted_serial();

    let (operations, readbacks) = build(&mut player);
    let mut recording = authority.begin(&device).unwrap();
    fixture.renderer.begin_timing(7).unwrap();
    let admitted = operations.admit(&recording).unwrap();
    let ids: Vec<_> = admitted.ids().collect();
    fixture.gpu.admit_offscreen(admitted);
    admit_scene_draw(&mut fixture, &mut player, &recording);

    fixture.encode(&mut recording).unwrap();
    let output = fixture.output_copy(&mut recording);
    assert_eq!(
        authority.submitted_serial(),
        before,
        "recording must not submit"
    );
    recording.finish().submit().unwrap();
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "one frame, one submission"
    );

    check_offscreen_stages(readbacks, &device);
    // The scene draw sampled element 2: the near patch spans columns 38.4 to 57.6 and rows 25.6
    // to 38.4 of the 64 pixel frame, 2.4 pixels for each of the 8 texels across. Pixels well
    // inside the cyan, blue and yellow texels read them exactly. The patch is in front of the
    // engine's red card at columns 42 and 50, where the card would otherwise show.
    let image = fixture.read(&output);
    assert_eq!(image.pixel(42, 32), CYAN);
    assert_eq!(image.pixel(50, 32), BLUE);
    assert_eq!(image.pixel(55, 32), YELLOW);
    assert_eq!(image.pixel(30, 20), RED, "the stock scene drew");
    assert_eq!(image.pixel(3, 3), [0, 0, 0, 255]);
    check_timing(&fixture, &ids, &[PassEntry::new(PassSlot::Opaque)]);
}

#[test]
fn a_cancelled_frame_submits_nothing_and_the_next_frame_does_not_see_its_offscreen_work() {
    let mut fixture = timed();
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let before = authority.submitted_serial();
    let (operations, _) = build(&mut player);
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    fixture
        .gpu
        .admit_offscreen(operations.admit(&recording).unwrap());
    fixture.encode(&mut recording).unwrap();
    recording.finish().cancel();
    assert_eq!(authority.submitted_serial(), before);
    // The frame took the work when it encoded; the next one has none and draws the stock scene.
    assert!(!fixture.gpu.has_offscreen());
    let mut next = authority.begin(fixture.context.device()).unwrap();
    fixture.encode(&mut next).unwrap();
    let readback = fixture.output_copy(&mut next);
    next.finish().submit().unwrap();
    let image = fixture.read(&readback);
    assert_eq!(image.pixel(30, 20), RED);
    player
        .arena
        .retire_settled(
            fixture.context.device(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("the cancelled frame released the arena");
}

#[test]
fn a_draw_prepared_for_another_target_refuses_the_frame_by_name_and_takes_its_offscreen_work() {
    let mut fixture = timed();
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let before = authority.submitted_serial();
    let (operations, _) = build(&mut player);
    let mut recording = authority.begin(fixture.context.device()).unwrap();
    fixture
        .gpu
        .admit_offscreen(operations.admit(&recording).unwrap());
    // The stock scene colour is Rgba16Float; this draw was prepared for an 8-bit target.
    let prepared =
        TargetSignature::new(&[TextureFormat::Rgba8UnormSrgb], Some(DEPTH_FORMAT), 1).unwrap();
    assert_ne!(prepared, fixture.signature());
    admit_scene_draw_for(&mut fixture, &mut player, &recording, prepared);

    let refused = fixture.encode(&mut recording).unwrap_err();
    let FrameError::FeaturePreparation {
        feature: "player programs",
        reason,
    } = refused
    else {
        panic!("expected a refusal by the player programs, got {refused:?}");
    };
    assert_eq!(
        reason,
        ProgramError::TargetMismatch {
            prepared: Box::new(prepared),
            pass: Box::new(fixture.signature()),
        }
        .to_string()
    );
    // The refused frame took both admissions, so no later frame can run the offscreen work or
    // the draw for a frame they were not encoded in.
    assert!(!fixture.gpu.has_programs(), "the draw is not left waiting");
    assert!(
        !fixture.gpu.has_offscreen(),
        "the offscreen work is not left waiting"
    );
    recording.finish().cancel();
    assert_eq!(authority.submitted_serial(), before);

    let mut next = authority.begin(fixture.context.device()).unwrap();
    fixture.encode(&mut next).unwrap();
    let readback = fixture.output_copy(&mut next);
    next.finish().submit().unwrap();
    assert_eq!(authority.submitted_serial(), before + 1);
    let image = fixture.read(&readback);
    assert_eq!(image.pixel(30, 20), RED, "the stock scene drew");
    assert_eq!(image.pixel(42, 32), RED, "no patch: the engine card shows");
    player
        .arena
        .retire_settled(
            fixture.context.device(),
            std::time::Instant::now() + std::time::Duration::from_secs(30),
        )
        .expect("the refused frame released the arena");
}

#[test]
fn the_admitted_work_reports_only_the_written_textures_the_host_exported() {
    let mut fixture = timed();
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let (mut operations, _) = build(&mut player);
    let element = ResourceRef {
        layer: 2,
        ..ResourceRef::new("element")
    };
    let camera = ResourceRef::new("camera");
    operations.export(A, 0, 0, camera).unwrap();
    operations.export(ARRAY, 0, 2, element).unwrap();
    let refused = |result: Result<(), OffscreenError>| result.unwrap_err().to_string();
    assert_eq!(
        refused(operations.export(ARRAY, 0, 0, ResourceRef::new("untouched"))),
        "no operation of the frame writes mip 0 of element 0 of TextureId(12)"
    );
    assert_eq!(
        refused(operations.export(ARRAY, 1, 2, ResourceRef::new("small"))),
        "texture TextureId(12) has no mip 1 of element 2"
    );
    let misnamed = ResourceRef::new("misnamed");
    assert_eq!(
        refused(operations.export(ARRAY, 0, 2, misnamed)),
        format!("{misnamed:?} names mip 0 of layer 0, but the export is mip 0 of element 2")
    );
    let other_mip = ResourceRef {
        mip: 1,
        layer: 2,
        ..ResourceRef::new("other mip")
    };
    assert_eq!(
        refused(operations.export(ARRAY, 0, 2, other_mip)),
        format!("{other_mip:?} names mip 1 of layer 2, but the export is mip 0 of element 2")
    );
    assert_eq!(
        refused(operations.export(TextureId(99), 0, 0, ResourceRef::new("unknown"))),
        "texture TextureId(99) is neither registered nor in the frame"
    );
    assert_eq!(
        refused(operations.export(B, 0, 0, camera)),
        format!("{camera:?} is exported twice")
    );
    let recording = authority.begin(fixture.context.device()).unwrap();
    let admitted = operations.admit(&recording).unwrap();
    let format = wgpu::TextureFormat::Rgba8Unorm;
    assert_eq!(
        admitted.produces(),
        [
            OperationsOutput::new(camera, format, [8, 8]),
            OperationsOutput::new(element, format, [8, 8]),
        ]
    );
    recording.finish().cancel();
}

#[test]
fn a_render_texture_with_a_depth_buffer_exports_its_colour() {
    let mut fixture = timed();
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let mut frame = frame();
    // A's depth buffer is Depth24PlusStencil8; the export must still describe the colour.
    frame.render_textures[0].depth_bits = 24;
    let mut operations = player
        .state
        .build(&mut player.runtime, &player.arena, &frame)
        .unwrap();
    let camera = ResourceRef::new("camera");
    operations.export(A, 0, 0, camera).unwrap();
    let recording = authority.begin(fixture.context.device()).unwrap();
    let admitted = operations.admit(&recording).unwrap();
    assert_eq!(
        admitted.produces(),
        [OperationsOutput::new(
            camera,
            wgpu::TextureFormat::Rgba8Unorm,
            [8, 8]
        )]
    );
    recording.finish().cancel();
}

mod game_screen;
mod shadowed;
