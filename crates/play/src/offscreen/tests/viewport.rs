//! A command buffer's viewport: a draw that reaches the bound target while a recorded viewport
//! leaves part of it uncovered is left out by reason, and a viewport that covers the target, a
//! new target or a camera setup leaves drawing as it was. The worlds are synthetic.

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::unity::offscreen::frame::Record;
use worldrunner_world_simulation::graphics::{
    CameraBuffers, DrawTopology, MaterialId, OffscreenWork, RenderTextureId, RendererListDesc,
    TargetRef, TextureRef,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{ComponentDesc, ComponentHandle, World};

use super::super::frame::{Adapted, Settings, Skip, adapt};
use super::support::{available, material, object, render_texture, shader, snapshot, world};

const MESH: u64 = 77;
const SIDE: f32 = 8.0;

/// A world with a target of 8 by 8 texels and a material, and the camera a setup may name.
struct Rig {
    world: World,
    target: RenderTextureId,
    material: MaterialId,
    camera: ComponentHandle,
}

fn rig() -> Rig {
    let mut world = world();
    let target = render_texture(&mut world, [8, 8]);
    let shader = shader(&mut world, "viewport");
    let material = material(&mut world, shader);
    let holder = object(&mut world, 0, Vec3::ZERO, Quat::IDENTITY);
    let camera = world
        .add_component(holder, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    Rig {
        world,
        target,
        material,
        camera,
    }
}

impl Rig {
    fn record(&mut self, work: OffscreenWork) {
        self.world.record_offscreen(work).unwrap();
    }

    fn bind(&mut self) {
        let target = TargetRef::Render(self.target);
        self.record(OffscreenWork::SetRenderTarget {
            color: vec![target],
            depth: Some(target),
            mip: 0,
            face: -1,
            slice: -1,
        });
    }

    fn viewport(&mut self, rect: [f32; 4]) {
        self.record(OffscreenWork::SetViewport { rect });
    }

    fn setup_camera(&mut self) {
        self.record(OffscreenWork::SetupCamera {
            camera: self.camera,
            view: Mat4::IDENTITY.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target: TargetRef::Render(self.target),
            buffers: None,
        });
    }

    fn clear(&mut self) {
        self.record(OffscreenWork::ClearRenderTarget {
            color: true,
            depth: true,
            stencil: false,
            value: [0.0, 0.0, 0.0, 1.0],
            depth_value: 1.0,
            stencil_value: 0,
        });
    }

    fn draw_mesh(&mut self) {
        self.record(OffscreenWork::DrawMesh {
            mesh: MESH,
            matrix: Mat4::IDENTITY.to_cols_array(),
            material: self.material,
            submesh: 0,
            pass: -1,
            block: None,
            instances: 1,
        });
    }

    fn draw_procedural(&mut self) {
        self.record(OffscreenWork::DrawProcedural {
            matrix: Mat4::IDENTITY.to_cols_array(),
            material: self.material,
            pass: -1,
            topology: DrawTopology::TriangleList,
            vertex_count: 3,
            instance_count: 1,
        });
    }

    /// Culls with an empty result and draws a list of it, which holds no renderer.
    fn draw_list(&mut self) {
        let holder = object(&mut self.world, 0, Vec3::ZERO, Quat::IDENTITY);
        let camera = self
            .world
            .add_component(holder, ComponentDesc::Camera(Camera::default()))
            .unwrap();
        self.record(OffscreenWork::Cull {
            id: 5,
            camera,
            culling_mask: u32::MAX,
            planes: Vec::new(),
            options: 0,
        });
        let desc = RendererListDesc::Draw {
            cull: 5,
            pass_tags: vec!["UniversalForward".to_owned()],
            queue_min: 0,
            queue_max: 2500,
            layer_mask: u32::MAX,
            rendering_layer_mask: u32::MAX,
            sorting: 0,
            per_object: 0,
            override_material: None,
            override_pass: 0,
        };
        let list = self.world.create_renderer_list(desc).unwrap();
        self.record(OffscreenWork::DrawRendererList { list });
    }

    fn run(&mut self) -> Adapted {
        let snapshot = snapshot(&mut self.world);
        let mut available = available(&snapshot, 1, &[]);
        available.meshes.insert(MESH);
        available.sub_meshes.insert(MESH, vec![3]);
        adapt(&snapshot, &available, &Settings::default())
    }
}

/// What the frame's records are, in order, without the globals the frame adds between them.
fn kinds(adapted: &Adapted) -> Vec<&'static str> {
    adapted
        .frame
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Clear { .. } => Some("clear"),
            Record::Draws(_) => Some("draws"),
            Record::Procedural(_) => Some("procedural"),
            Record::Blit { .. } => Some("blit"),
            Record::CopyTexture { .. } => Some("copy"),
            _ => None,
        })
        .collect()
}

fn skipped(adapted: &Adapted, kind: &str) -> usize {
    adapted
        .report
        .skipped_by_kind
        .get(&format!("{kind}: PartialViewport"))
        .copied()
        .unwrap_or(0)
}

#[test]
fn a_viewport_that_covers_the_target_draws_as_before() {
    for rect in [
        [0.0, 0.0, SIDE, SIDE],
        [-3.0, -2.0, 20.0, 30.0],
        [0.0, 0.0, SIDE + 0.5, SIDE],
    ] {
        let mut rig = rig();
        rig.bind();
        rig.viewport(rect);
        rig.clear();
        rig.draw_mesh();
        rig.draw_procedural();
        rig.draw_list();
        let adapted = rig.run();
        assert_eq!(
            kinds(&adapted),
            ["clear", "draws", "procedural"],
            "{rect:?}"
        );
        assert_eq!(adapted.report.lists.empty, 1, "{rect:?}");
        assert!(
            adapted.report.skipped.is_empty(),
            "{:?}",
            adapted.report.skipped
        );
    }
}

#[test]
fn a_viewport_that_leaves_part_of_the_target_out_skips_each_kind_of_draw_by_reason() {
    let mut rig = rig();
    rig.bind();
    rig.viewport([0.0, 0.0, 4.0, SIDE]);
    rig.clear();
    rig.draw_mesh();
    rig.draw_procedural();
    rig.draw_list();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), Vec::<&str>::new());
    for kind in [
        "ClearRenderTarget",
        "DrawMesh",
        "DrawProcedural",
        "DrawRendererList",
    ] {
        assert_eq!(skipped(&adapted, kind), 1, "{kind}");
    }
    assert_eq!(adapted.report.skipped[&Skip::PartialViewport], 4);
    assert_eq!(adapted.report.lists.empty, 0);
}

#[test]
fn a_viewport_short_of_any_one_edge_of_the_target_leaves_the_draw_out() {
    for rect in [
        [1.0, 0.0, 7.0, SIDE],
        [0.0, 1.0, SIDE, 7.0],
        [0.0, 0.0, 7.5, SIDE],
        [0.0, 0.0, SIDE, 7.5],
        [0.0, 0.0, 0.0, 0.0],
        [2.0, 2.0, 4.0, 4.0],
    ] {
        let mut rig = rig();
        rig.bind();
        rig.viewport(rect);
        rig.clear();
        let adapted = rig.run();
        assert_eq!(kinds(&adapted), Vec::<&str>::new(), "{rect:?}");
        assert_eq!(skipped(&adapted, "ClearRenderTarget"), 1, "{rect:?}");
    }
}

#[test]
fn the_latest_viewport_decides_and_a_later_full_one_restores_drawing() {
    let mut rig = rig();
    rig.bind();
    rig.viewport([0.0, 0.0, 2.0, 2.0]);
    rig.clear();
    rig.viewport([0.0, 0.0, SIDE, SIDE]);
    rig.clear();
    rig.viewport([0.0, 0.0, SIDE, 1.0]);
    rig.clear();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), ["clear"]);
    assert_eq!(skipped(&adapted, "ClearRenderTarget"), 2);
}

#[test]
fn setting_a_render_target_restores_the_whole_target() {
    let mut rig = rig();
    rig.bind();
    rig.viewport([1.0, 1.0, 2.0, 2.0]);
    rig.bind();
    rig.clear();
    rig.draw_procedural();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), ["clear", "procedural"]);
    assert!(adapted.report.skipped.is_empty());
}

#[test]
fn a_viewport_set_before_the_target_is_bound_does_not_outlive_the_binding() {
    let mut rig = rig();
    rig.viewport([1.0, 1.0, 2.0, 2.0]);
    rig.clear();
    rig.bind();
    rig.clear();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), ["clear"]);
    assert_eq!(adapted.report.skipped[&Skip::UnboundTarget], 1);
    assert_eq!(adapted.report.skipped.get(&Skip::PartialViewport), None);
}

#[test]
fn a_camera_setup_restores_the_whole_target() {
    let mut rig = rig();
    rig.bind();
    rig.viewport([1.0, 1.0, 2.0, 2.0]);
    rig.setup_camera();
    rig.clear();
    rig.draw_mesh();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), ["clear", "draws"]);
    assert!(adapted.report.skipped.is_empty());
}

#[test]
fn a_blit_and_a_copy_name_their_own_target_and_ignore_the_viewport() {
    let mut rig = rig();
    let source = render_texture(&mut rig.world, [8, 8]);
    rig.bind();
    rig.viewport([1.0, 1.0, 2.0, 2.0]);
    rig.record(OffscreenWork::Blit {
        source: TextureRef::Render(source),
        target: TargetRef::Render(rig.target),
        material: None,
        pass: 0,
    });
    rig.record(OffscreenWork::CopyTexture {
        source: TextureRef::Render(source),
        source_element: 0,
        source_mip: 0,
        target: TargetRef::Render(rig.target),
        target_element: 0,
        target_mip: 0,
    });
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), ["blit", "copy"]);
    assert!(adapted.report.skipped.is_empty());
}

#[test]
fn a_viewport_changes_nothing_when_no_target_is_bound() {
    let mut rig = rig();
    rig.viewport([1.0, 1.0, 2.0, 2.0]);
    rig.clear();
    rig.draw_procedural();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), Vec::<&str>::new());
    assert_eq!(adapted.report.skipped[&Skip::UnboundTarget], 2);
    assert_eq!(adapted.report.skipped.get(&Skip::PartialViewport), None);
}

/// Binds two color textures of 8 by 8 texels and the first one's depth, and returns the textures.
fn bind_two_colors(rig: &mut Rig) -> [RenderTextureId; 2] {
    let second = render_texture(&mut rig.world, [8, 8]);
    let colors = [rig.target, second];
    rig.record(OffscreenWork::SetRenderTarget {
        color: colors.map(TargetRef::Render).to_vec(),
        depth: Some(TargetRef::Render(rig.target)),
        mip: 0,
        face: -1,
        slice: -1,
    });
    colors
}

#[test]
fn one_viewport_covers_every_color_of_the_bound_set_and_its_depth() {
    let mut rig = rig();
    bind_two_colors(&mut rig);
    rig.viewport([0.0, 0.0, SIDE, SIDE]);
    rig.clear();
    rig.viewport([0.0, 0.0, 4.0, 4.0]);
    rig.clear();
    rig.draw_procedural();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), ["clear"]);
    assert_eq!(skipped(&adapted, "ClearRenderTarget"), 1);
    assert_eq!(skipped(&adapted, "DrawProcedural"), 1);
    let Some(Record::Clear { target, .. }) = adapted
        .frame
        .records
        .iter()
        .find(|record| matches!(record, Record::Clear { .. }))
    else {
        panic!("the covering clear is a record");
    };
    assert_eq!(target.colors.len(), 2);
    assert!(target.depth.is_some());
}

#[test]
fn buffers_that_cannot_be_bound_leave_nothing_bound_under_a_viewport() {
    let mut rig = rig();
    let small = render_texture(&mut rig.world, [4, 4]);
    rig.bind();
    rig.viewport([0.0, 0.0, 2.0, 2.0]);
    let buffers = CameraBuffers::new(&[rig.target, small], None).unwrap();
    rig.record(OffscreenWork::SetupCamera {
        camera: rig.camera,
        view: Mat4::IDENTITY.to_cols_array(),
        projection: Mat4::IDENTITY.to_cols_array(),
        target: TargetRef::CameraTarget,
        buffers: Some(buffers),
    });
    rig.clear();
    let adapted = rig.run();
    assert_eq!(kinds(&adapted), Vec::<&str>::new());
    assert_eq!(adapted.report.skipped[&Skip::ColorSizeMismatch], 1);
    assert_eq!(adapted.report.skipped[&Skip::UnboundTarget], 1);
    assert_eq!(adapted.report.skipped.get(&Skip::PartialViewport), None);
}
