use super::*;
use std::collections::BTreeMap;

#[test]
fn rotated_graphic_and_text_cannot_paint_outside_renderer_clip_after_resize() {
    let (mut world, object) = graphic_world();
    world
        .set_local_transform(
            object,
            worldrunner_world_simulation::LocalTransform {
                rotation: glam::Quat::from_rotation_z(0.25),
                ..Default::default()
            },
        )
        .unwrap();
    let renderer = world
        .published_components()
        .find(|c| c.object == object && matches!(c.desc, ComponentDesc::CanvasRenderer(_)))
        .unwrap();
    let handle = renderer.handle;
    let ComponentDesc::CanvasRenderer(mut desc) = renderer.desc.clone() else {
        unreachable!()
    };
    desc.clip_rect = Some([-20., -16., 8., 8.]);
    world
        .set_component(handle, ComponentDesc::CanvasRenderer(desc))
        .unwrap();
    let mut ui = extracted(&world);
    assert_eq!(ui.graphics[0].clip_rect, [12, 16, 8, 8]);
    let mut text = plan([0., 1., 0., 1.]).texts.remove(0);
    text.clip_rect = ui.graphics[0].clip_rect;
    text.texture.as_mut().unwrap().bytes.fill(255);
    text.hierarchy_order = ui.graphics[0].hierarchy_order + 1;
    ui.texts.push(text);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    for size in [[64, 64], [48, 32]] {
        run.resize(size).unwrap();
        let mut empty = ui.clone();
        empty.graphics.clear();
        empty.texts.clear();
        apply(&mut run, empty);
        let baseline = capture(&mut run, &events).1;
        apply(&mut run, ui.clone());
        let image = capture(&mut run, &events).1;
        for y in 0..size[1] {
            for x in 0..size[0] {
                let expected = if (12..20).contains(&x) && (16..24).contains(&y) {
                    &[0, 255, 0, 255]
                } else {
                    pixel(&baseline, x, y)
                };
                assert_eq!(pixel(&image, x, y), expected, "pixel {x},{y}");
            }
        }
    }
    run.shutdown().unwrap();
}

#[test]
fn image_identity_is_resolved_and_sampled_in_the_overlay_stage() {
    let (world, _) = graphic_world();
    let mut ui = extracted(&world);
    ui.graphics[0].graphic.texture = Some(77);
    assert!(UiOverlay::new(ui.clone()).is_err());
    let textures = BTreeMap::from([(
        77,
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: vec![0, 0, 255, 255].into_boxed_slice(),
        },
    )]);
    ui.graphics[0].graphic.quads[0].color = [1.; 4];
    let overlay = UiOverlay::with_textures(ui, &textures).unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    run.host_mut()
        .apply(RetainedUpdate::Ui(std::sync::Arc::new(overlay)))
        .unwrap();
    region(&capture(&mut run, &events).1, [0, 0, 255, 255]);
    assert_statistics(&run, 1, 1);
    run.shutdown().unwrap();
}

#[test]
fn empty_or_offscreen_clips_draw_nothing_and_are_not_counted() {
    let (world, _) = graphic_world();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    let baseline = capture(&mut run, &events).1;
    for clip in [[8, 8, 0, 16], [100, 100, 16, 16]] {
        let mut ui = extracted(&world);
        ui.graphics[0].clip_rect = clip;
        apply(&mut run, ui);
        assert_eq!(capture(&mut run, &events).1, baseline);
        assert_statistics(&run, run.host().accepted_generation(), 0);
    }
    run.shutdown().unwrap();
}

#[test]
fn viewport_and_per_draw_clips_bound_identical_rectangles_independently() {
    let (world, _) = graphic_world();
    let mut ui = extracted(&world);
    ui.graphics[0].graphic.quads[0].corners =
        [[56., 8., 0.], [80., 8., 0.], [80., 24., 0.], [56., 24., 0.]];
    ui.graphics[0].clip_rect = [0, 0, 64, 64];
    let mut front = ui.graphics[0].clone();
    front.clip_rect = [60, 8, 2, 16];
    front.graphic.quads[0].color = [0., 1., 0., 1.];
    front.hierarchy_order += 1;
    ui.graphics.push(front);
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    run.resize([96, 64]).unwrap();
    let baseline = capture(&mut run, &events).1;
    apply(&mut run, ui);
    let image = capture(&mut run, &events).1;
    for x in 56..80 {
        let expected = match x {
            60..62 => &[0, 255, 0, 255],
            56..64 => &[255, 0, 0, 255],
            _ => pixel(&baseline, x, 16),
        };
        assert_eq!(pixel(&image, x, 16), expected, "pixel {x},16");
    }
    assert_statistics(&run, 1, 2);
    run.shutdown().unwrap();
}
