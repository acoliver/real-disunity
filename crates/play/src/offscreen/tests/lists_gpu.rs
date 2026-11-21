//! A renderer list drawn through the synchronous flush on a real adapter (lavapipe in CI): the
//! commands a render pipeline records (a target, a clear, a camera, a culling, a list and its
//! draw) run in call order, and a pixel read sees the texels they wrote.
//!
//! The camera shader is the synthetic player shader of render-program, tagged `Alpha` and
//! `Gamma`: it fills with the material's `_Tint` where the property block's `_Place` puts the quad.
//! Every expected texel comes from the placement and the colors, not from the output.

use glam::{Mat4, Quat, Vec3};
use worldrunner_world_simulation::graphics::{
    OffscreenWork, PropertyValue, RenderTextureId, RendererListDesc, ShaderProperties, TargetRef,
    TextureRef,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{ComponentHandle, MeshRenderer, World};

use super::flush::host;
use super::gpu::{MESH, Quad, SIDE, shader, target, vector};
use super::support::{camera, renderer_with, snapshot, world};

const ARGB32: u32 = 8;

/// A renderer of the camera shader with `tint`, over the part of the target `place` says: the
/// scale of the quad along x and y, then its offset.
fn quad(world: &mut World, tint: [f32; 4], place: [f32; 4]) {
    let shader = shader(world, "camera");
    let material = world.create_material(shader).unwrap();
    vector(world, material, "_Tint", tint);
    let mut block = ShaderProperties::default();
    block
        .set(PropertyValue::Vector("_Place".into(), place))
        .unwrap();
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        property_block: Some(block),
        slot_blocks: std::collections::BTreeMap::new(),
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, renderer);
}

/// The commands of a pipeline that clears `drawn` to `clear`, sets a camera up for it and draws
/// the forward list of everything its culling keeps. A camera render into `unrelated` makes the
/// world copy the renderers, as it copies only those of the layers that camera renders name.
fn pipeline(
    world: &mut World,
    drawn: RenderTextureId,
    unrelated: RenderTextureId,
    clear: [f32; 4],
) {
    let camera = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let commands = [
        OffscreenWork::CameraRender {
            camera,
            target: unrelated,
        },
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(drawn)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: true,
            stencil: false,
            value: clear,
            depth_value: 1.0,
            stencil_value: 0,
        },
        OffscreenWork::SetupCamera {
            camera,
            view: Mat4::IDENTITY.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target: TargetRef::Render(drawn),
            buffers: None,
        },
        OffscreenWork::Cull {
            id: 5,
            camera,
            culling_mask: u32::MAX,
            planes: Vec::new(),
            options: 0,
        },
    ];
    for command in commands {
        world.record_offscreen(command).unwrap();
    }
    let list = world
        .create_renderer_list(RendererListDesc::Draw {
            cull: 5,
            pass_tags: vec!["Alpha".to_owned()],
            queue_min: 0,
            queue_max: 5000,
            layer_mask: u32::MAX,
            rendering_layer_mask: u32::MAX,
            sorting: 0,
            per_object: 0,
            override_material: None,
            override_pass: 0,
        })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list })
        .unwrap();
}

/// What the world should read back: the columns left of `drawn_columns` hold `drawn`, the rest
/// `cleared`, in every row.
fn texels(drawn_columns: u32, drawn: [f32; 4], cleared: [f32; 4]) -> Vec<f32> {
    (0..SIDE)
        .flat_map(|_| {
            (0..SIDE).flat_map(move |column| {
                if column < drawn_columns {
                    drawn
                } else {
                    cleared
                }
            })
        })
        .collect()
}

#[test]
fn a_list_drawn_after_a_clear_writes_its_renderers_over_the_cleared_target() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (drawn, unrelated) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    // A red quad over the left half; the target is cleared to green first.
    quad(&mut world, [1.0, 0.0, 0.0, 1.0], [0.5, 1.0, -0.5, 0.0]);
    pipeline(&mut world, drawn, unrelated, [0.0, 1.0, 0.0, 1.0]);
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();

    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();

    assert_eq!(
        world.take_texture_pixels(request),
        Some(texels(2, [1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0]))
    );
    assert!(
        outcome.report.failures.is_empty(),
        "{:?}",
        outcome.report.failures
    );
    assert_eq!(outcome.report.list_draws.len(), 1);
    assert_eq!(outcome.report.list_draws[0].drawn, 1);
    assert_eq!(outcome.report.lists.draws_by_tag["Alpha"], 1);
}

#[test]
fn a_list_draws_its_renderers_in_the_order_the_world_listed_them() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (drawn, unrelated) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    // Red over the left half, then blue over the middle two columns.
    quad(&mut world, [1.0, 0.0, 0.0, 1.0], [0.5, 1.0, -0.5, 0.0]);
    quad(&mut world, [0.0, 0.0, 1.0, 1.0], [0.5, 1.0, 0.0, 0.0]);
    pipeline(&mut world, drawn, unrelated, [0.0, 1.0, 0.0, 1.0]);
    let request = world
        .request_texture_pixels(drawn, 0, 0, SIDE, SIDE)
        .unwrap();

    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();

    // The renderers of one list draw in the order the world listed them, and the blue quad
    // (offset 0, half the width) covers columns 1 and 2 where it overlaps the red one at column 1.
    let read = world.take_texture_pixels(request).unwrap();
    // The texels are stored 8-bit values read back as floats, so equal ones have equal bits.
    let column = |index: usize| -> [u32; 4] {
        let texel: [f32; 4] = read[index * 4..index * 4 + 4].try_into().unwrap();
        texel.map(f32::to_bits)
    };
    let bits = |texel: [f32; 4]| texel.map(f32::to_bits);
    assert_eq!(
        column(0),
        bits([1.0, 0.0, 0.0, 1.0]),
        "left of the red quad only"
    );
    assert_eq!(
        column(1),
        bits([0.0, 0.0, 1.0, 1.0]),
        "blue drawn after red covers it"
    );
    assert_eq!(column(2), bits([0.0, 0.0, 1.0, 1.0]), "blue only");
    assert_eq!(column(3), bits([0.0, 1.0, 0.0, 1.0]), "the cleared green");
}

/// A renderer on `layer` whose material draws the camera shader's `Gamma` pass, which samples
/// `_Albedo`, with `_Albedo` set to `texture`.
fn sampling_quad(world: &mut World, layer: u8, texture: RenderTextureId) {
    let shader = shader(world, "camera");
    let material = world.create_material(shader).unwrap();
    super::support::set(
        world,
        material,
        PropertyValue::Texture(
            "_Albedo".into(),
            super::support::texture_slot(TextureRef::Render(texture)),
        ),
    );
    let renderer = MeshRenderer {
        material_count: 1,
        slot_materials: vec![Some(material)],
        ..MeshRenderer::default()
    };
    renderer_with(world, (layer, true), MESH, renderer);
}

/// Sets `target` as the render target, clears it to `clear`, culls everything and draws the list
/// of the passes tagged `tag` on the layers of `layer_mask` into it.
fn clear_and_draw(
    world: &mut World,
    camera: ComponentHandle,
    (target, clear): (RenderTextureId, [f32; 4]),
    (id, tag, layer_mask): (u64, &str, u32),
) {
    let commands = [
        OffscreenWork::SetRenderTarget {
            color: vec![TargetRef::Render(target)],
            depth: None,
            mip: 0,
            face: -1,
            slice: -1,
        },
        OffscreenWork::ClearRenderTarget {
            color: true,
            depth: true,
            stencil: false,
            value: clear,
            depth_value: 1.0,
            stencil_value: 0,
        },
        OffscreenWork::SetupCamera {
            camera,
            view: Mat4::IDENTITY.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target: TargetRef::Render(target),
            buffers: None,
        },
        OffscreenWork::Cull {
            id,
            camera,
            culling_mask: u32::MAX,
            planes: Vec::new(),
            options: 0,
        },
    ];
    for command in commands {
        world.record_offscreen(command).unwrap();
    }
    let list = world
        .create_renderer_list(RendererListDesc::Draw {
            cull: id,
            pass_tags: vec![tag.to_owned()],
            queue_min: 0,
            queue_max: 5000,
            layer_mask,
            rendering_layer_mask: u32::MAX,
            sorting: 0,
            per_object: 0,
            override_material: None,
            override_pass: 0,
        })
        .unwrap();
    world
        .record_offscreen(OffscreenWork::DrawRendererList { list })
        .unwrap();
}

#[test]
fn a_list_draw_that_samples_its_own_target_is_refused_alone_and_the_rest_of_the_flush_runs() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let (looped, independent) = (target(&mut world, ARGB32), target(&mut world, ARGB32));
    // Layer 1 holds a renderer that samples the texture its list draws into; layer 0 a red quad
    // over the left half of the other target.
    sampling_quad(&mut world, 1, looped);
    quad(&mut world, [1.0, 0.0, 0.0, 1.0], [0.5, 1.0, -0.5, 0.0]);
    let camera = camera(&mut world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    // A camera render into a texture no list touches makes the world copy the renderers.
    let unrelated = target(&mut world, ARGB32);
    world
        .record_offscreen(OffscreenWork::CameraRender {
            camera,
            target: unrelated,
        })
        .unwrap();
    clear_and_draw(
        &mut world,
        camera,
        (looped, [0.0, 0.0, 1.0, 1.0]),
        (5, "Gamma", 1 << 1),
    );
    clear_and_draw(
        &mut world,
        camera,
        (independent, [0.0, 1.0, 0.0, 1.0]),
        (6, "Alpha", 1 << 0),
    );
    let looped_read = world
        .request_texture_pixels(looped, 0, 0, SIDE, SIDE)
        .unwrap();
    let independent_read = world
        .request_texture_pixels(independent, 0, 0, SIDE, SIDE)
        .unwrap();

    let snapshot = snapshot(&mut world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(&mut world).unwrap();

    assert_eq!(
        world.take_texture_pixels(independent_read),
        Some(texels(2, [1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0])),
        "the list into the other target ran"
    );
    assert_eq!(
        world.take_texture_pixels(looped_read),
        Some(texels(0, [0.0; 4], [0.0, 0.0, 1.0, 1.0])),
        "the self-sampling list left the cleared target as it was"
    );
    assert_eq!(outcome.report.failures.get("draw:FeedbackLoop"), Some(&1));
    assert_eq!(
        outcome.report.failures.len(),
        1,
        "{:?}",
        outcome.report.failures
    );
    let lists = &outcome.report.list_draws;
    assert_eq!(lists.len(), 2);
    assert_eq!((lists[0].drawn, lists[0].feedback_loop), (0, 1));
    assert_eq!((lists[1].drawn, lists[1].feedback_loop), (1, 0));
}
