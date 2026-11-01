//! The mesh a canvas renderer draws: write validation, read-back, and the quads a frame shows.
use worldrunner_disunity_ffi::{
    abi::*, camera::*, canvas_geometry::*, canvas_renderer::*, hierarchy::*, presentation::*,
    scheduling::*, sprites::*, textures::FIRST_PLAYER_TEXTURE, ui::*,
};

const PLAYER: u64 = FIRST_PLAYER_TEXTURE;

fn vertex(x: f32, y: f32, u: f32, v: f32) -> DisunityUiVertex {
    DisunityUiVertex {
        x,
        y,
        u,
        v,
        r: 1.,
        g: 1.,
        b: 1.,
        a: 1.,
        ..DisunityUiVertex::default()
    }
}

/// A 20 by 10 quad around the origin showing the lower left quarter of the texture.
fn quad() -> [DisunityUiVertex; 4] {
    [
        vertex(-10., -5., 0., 0.),
        vertex(-10., 5., 0., 0.5),
        vertex(10., 5., 0.5, 0.5),
        vertex(10., -5., 0.5, 0.),
    ]
}

const INDICES: [u32; 6] = [0, 1, 2, 2, 3, 0];

struct Scene {
    world: u64,
    renderer: u64,
}

fn component(world: u64, object: u64, kind: u32) -> u64 {
    let mut handle = 0;
    unsafe {
        assert_eq!(
            disunity_ui_create(world, object, kind, 0, &mut handle).code,
            0
        )
    };
    handle
}

fn scene() -> Scene {
    let (mut world, mut camera_object, mut camera) = (0, 0, 0);
    let (mut canvas, mut element, mut renderer) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(
            disunity_gameobject_create(world, &mut camera_object).code,
            0
        );
        assert_eq!(
            disunity_camera_create(world, camera_object, &mut camera).code,
            0
        );
        let lens = DisunityCamera {
            enabled: 1,
            clear_flags: 2,
            orthographic: 1,
            orthographic_size: 8.,
            near: 0.3,
            far: 1000.,
            a: 1.,
            ..DisunityCamera::default()
        };
        assert_eq!(disunity_camera_write(world, camera, &lens).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut canvas).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut element).code, 0);
        let mut advance = DisunityAdvance::default();
        assert_eq!(
            disunity_transform_parent(world, element, canvas, 0, &mut advance).code,
            0
        );
        component(world, canvas, 0);
        component(world, canvas, 1);
        component(world, element, 0);
        assert_eq!(
            disunity_canvas_renderer_create(world, element, &mut renderer).code,
            0
        );
    }
    Scene { world, renderer }
}

fn write(scene: &Scene, vertices: &[DisunityUiVertex], indices: &[u32]) -> DisunityResult {
    unsafe {
        disunity_canvas_renderer_geometry_write(
            scene.world,
            scene.renderer,
            if vertices.is_empty() {
                std::ptr::null()
            } else {
                vertices.as_ptr()
            },
            vertices.len() as u32,
            if indices.is_empty() {
                std::ptr::null()
            } else {
                indices.as_ptr()
            },
            indices.len() as u32,
        )
    }
}

fn info(scene: &Scene) -> DisunityUiGraphicInfo {
    let mut info = DisunityUiGraphicInfo::default();
    let result = unsafe {
        disunity_canvas_renderer_geometry_read(
            scene.world,
            scene.renderer,
            &mut info,
            std::ptr::null_mut(),
            0,
        )
    };
    assert_eq!(result.code, 0);
    info
}

fn frame(scene: &Scene) -> Vec<DisunityQuad> {
    let mut header = DisunityPresentation::default();
    let result = unsafe { disunity_presentation_prepare(scene.world, 64, 32, 1, &mut header) };
    assert_eq!(result.code, 0);
    (0..header.quads)
        .map(|index| {
            let mut quad = DisunityQuad::default();
            unsafe {
                assert_eq!(
                    disunity_presentation_quad(scene.world, index, &mut quad).code,
                    0
                )
            };
            quad
        })
        .collect()
}

#[test]
fn a_written_mesh_reads_back_with_its_texture() {
    let scene = scene();
    assert_eq!(write(&scene, &quad(), &INDICES).code, 0);
    assert_eq!(
        disunity_canvas_renderer_texture_write(scene.world, scene.renderer, PLAYER + 2).code,
        0
    );
    let held = info(&scene);
    assert_eq!(
        (held.texture, held.vertex_count, held.index_count),
        (PLAYER + 2, 4, 6)
    );
    let mut copy = [DisunityUiVertex::default(); 4];
    let result = unsafe {
        disunity_canvas_renderer_geometry_read(
            scene.world,
            scene.renderer,
            &mut DisunityUiGraphicInfo::default(),
            copy.as_mut_ptr(),
            4,
        )
    };
    assert_eq!(result.code, 0);
    assert_eq!(copy, quad());
}

#[test]
fn writing_no_vertices_and_no_indices_clears_the_mesh() {
    let scene = scene();
    assert_eq!(write(&scene, &quad(), &INDICES).code, 0);
    assert_eq!(write(&scene, &[], &[]).code, 0);
    assert_eq!(info(&scene).vertex_count, 0);
    assert!(frame(&scene).is_empty());
}

#[test]
fn a_bad_mesh_is_rejected_and_leaves_the_old_one() {
    let scene = scene();
    assert_eq!(write(&scene, &quad(), &INDICES).code, 0);
    assert_ne!(
        write(&scene, &quad(), &[0, 1]).code,
        0,
        "not whole triangles"
    );
    assert_ne!(
        write(&scene, &quad(), &[0, 1, 4]).code,
        0,
        "index past the vertices"
    );
    let mut nan = quad();
    nan[2].x = f32::NAN;
    assert_ne!(write(&scene, &nan, &INDICES).code, 0, "non-finite position");
    let mut colour = quad();
    colour[0].a = f32::NAN;
    assert_ne!(write(&scene, &colour, &INDICES).code, 0, "NaN colour");
    let held = info(&scene);
    assert_eq!((held.vertex_count, held.index_count), (4, 6));
}

#[test]
fn null_pointers_are_rejected_for_nonzero_counts() {
    let scene = scene();
    let result = unsafe {
        disunity_canvas_renderer_geometry_write(
            scene.world,
            scene.renderer,
            std::ptr::null(),
            4,
            INDICES.as_ptr(),
            6,
        )
    };
    assert_eq!(result.code, result_code::INVALID_ARGUMENT);
    let result = unsafe {
        disunity_canvas_renderer_geometry_write(
            scene.world,
            scene.renderer,
            quad().as_ptr(),
            4,
            std::ptr::null(),
            6,
        )
    };
    assert_eq!(result.code, result_code::INVALID_ARGUMENT);
    let result = unsafe {
        disunity_canvas_renderer_geometry_read(
            scene.world,
            scene.renderer,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    assert_eq!(result.code, result_code::INVALID_ARGUMENT);
}

#[test]
fn a_texture_identity_below_the_player_range_is_rejected() {
    let scene = scene();
    let write =
        |identity| disunity_canvas_renderer_texture_write(scene.world, scene.renderer, identity);
    assert_ne!(write(5).code, 0);
    assert_eq!(write(PLAYER).code, 0);
    assert_eq!(write(0).code, 0);
    assert_eq!(info(&scene).texture, 0);
}

#[test]
fn a_frame_shows_the_meshs_quad_with_its_texture_rectangle() {
    let scene = scene();
    assert_eq!(write(&scene, &quad(), &INDICES).code, 0);
    disunity_canvas_renderer_texture_write(scene.world, scene.renderer, PLAYER + 7);
    let quads = frame(&scene);
    assert_eq!(quads.len(), 1);
    let shown = quads[0];
    assert_eq!(
        (shown.textured, shown.texture, shown.overlay),
        (1, PLAYER + 7, 1)
    );
    assert_eq!([shown.u0, shown.v0, shown.u1, shown.v1], [0., 0.5, 0.5, 1.]);
    assert_eq!([shown.r, shown.g, shown.b, shown.a], [1.; 4]);
    let along = |a: [f32; 3], b: [f32; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    let origin = [shown.p0_x, shown.p0_y, shown.p0_z];
    let width = along(origin, [shown.p1_x, shown.p1_y, shown.p1_z]);
    let height = along(origin, [shown.p3_x, shown.p3_y, shown.p3_z]);
    assert!((width - 2. * height).abs() < 1e-3, "{width} by {height}");
}

#[test]
fn clearing_the_managed_mesh_or_texture_through_the_record_clears_the_graphic() {
    let scene = scene();
    assert_eq!(write(&scene, &quad(), &INDICES).code, 0);
    let mut record = DisunityCanvasRenderer::default();
    unsafe {
        disunity_canvas_renderer_read(scene.world, scene.renderer, &mut record);
        disunity_canvas_renderer_texture_write(scene.world, scene.renderer, PLAYER);
        record.texture = 9;
        record.mesh = 8;
        assert_eq!(
            disunity_canvas_renderer_write(scene.world, scene.renderer, &record).code,
            0
        );
    }
    let held = info(&scene);
    assert_eq!((held.texture, held.vertex_count), (PLAYER, 4));
    record.texture = 0;
    record.mesh = 0;
    unsafe { disunity_canvas_renderer_write(scene.world, scene.renderer, &record) };
    let held = info(&scene);
    assert_eq!((held.texture, held.vertex_count), (0, 0));
}
