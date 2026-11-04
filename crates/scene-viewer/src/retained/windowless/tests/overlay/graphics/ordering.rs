use super::*;
use worldrunner_world_simulation::ui::{UiAction, UiControl, UiRouter};
use worldrunner_world_simulation::{PointerButton, PointerEvent};

fn overlapping(text_on_top: bool) -> (World, UiRenderPlan, ObjectHandle) {
    let (mut world, rect) = graphic_world();
    let canvas = world.parent(rect).unwrap().unwrap();
    let text = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(text, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    if text_on_top {
        world
            .set_parent(text, Some(rect), ParentSpace::KeepLocal)
            .unwrap();
    } else {
        world
            .set_parent(text, Some(canvas), ParentSpace::KeepLocal)
            .unwrap();
        world
            .set_parent(rect, Some(text), ParentSpace::KeepLocal)
            .unwrap();
    }
    let mut ui = extracted(&world);
    let mut glyphs = plan([0., 1., 0., 1.]).texts.remove(0);
    glyphs.object = text;
    glyphs.canvas = canvas;
    glyphs.hierarchy_order = ui.layout.rect(text).unwrap().draw_order().2;
    glyphs.texture.as_mut().unwrap().bytes.fill(255);
    ui.texts.push(glyphs);
    (world, ui, if text_on_top { text } else { rect })
}

fn clicked(world: &World, ui: &UiRenderPlan) -> ObjectHandle {
    let mut router = UiRouter::default();
    router
        .controls
        .insert(ui.texts[0].object, UiControl::button());
    router
        .controls
        .insert(ui.graphics[0].graphic.object, UiControl::button());
    router
        .pointer(world, &ui.layout, PointerEvent::Moved([10., 43.]))
        .unwrap();
    let event = |pressed| PointerEvent::Button {
        button: PointerButton::Left,
        pressed,
    };
    router.pointer(world, &ui.layout, event(true)).unwrap();
    let response = router.pointer(world, &ui.layout, event(false)).unwrap();
    let [UiAction::Activated { object }] = response.actions.as_slice() else {
        panic!("expected activation")
    };
    *object
}

#[test]
fn text_and_rect_follow_the_same_hierarchy_order_as_pointer_hits_in_both_orders() {
    for text_on_top in [true, false] {
        let (world, ui, top) = overlapping(text_on_top);
        assert_eq!(clicked(&world, &ui), top);
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut run = overlay_host().windowless(&mut driver).unwrap();
        apply(&mut run, ui);
        let expected = if text_on_top {
            [0, 255, 0, 255]
        } else {
            [255, 0, 0, 255]
        };
        region(&capture(&mut run, &events).1, expected);
        assert_statistics(&run, 1, 2);
        run.shutdown().unwrap();
    }
}

#[test]
fn translucent_rect_blends_straight_alpha_over_retained_geometry() {
    for profile in [
        crate::FrameProfile::Legacy,
        crate::FrameProfile::Hdr {
            samples: 4,
            tone_map: crate::ToneMap::Reinhard,
        },
    ] {
        let (world, _) = graphic_world();
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut host = overlay_host();
        host.options.frame_profile = profile;
        let mut run = host.windowless(&mut driver).unwrap();
        let baseline = capture(&mut run, &events).1;
        let [x, y] = geometry_pixel(&baseline);
        let mut ui = extracted(&world);
        let [px, py] = glam::UVec2::new(x, y).as_vec2().to_array();
        ui.graphics[0].graphic.quads[0].corners = [
            [px, py, 0.],
            [px + 1., py, 0.],
            [px + 1., py + 1., 0.],
            [px, py + 1., 0.],
        ];
        ui.graphics[0].graphic.quads[0].color = [1., 0., 0., 0.5];
        apply(&mut run, ui);
        let source = pixel(&baseline, x, y);
        let expected = [
            blend(source[0], 1.),
            blend(source[1], 0.),
            blend(source[2], 0.),
            255,
        ];
        assert_blended_within_one(pixel(&capture(&mut run, &events).1, x, y), expected);
        run.shutdown().unwrap();
    }
}

fn blend(background: u8, foreground: f64) -> u8 {
    let value = f64::from(background) / 255.;
    let linear = if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    };
    let mixed = (linear + foreground) * 0.5;
    let encoded = if mixed <= 0.003_130_8 {
        mixed * 12.92
    } else {
        1.055 * mixed.powf(1. / 2.4) - 0.055
    };
    (0..=u8::MAX)
        .min_by(|a, b| {
            (f64::from(*a) - encoded * 255.)
                .abs()
                .total_cmp(&(f64::from(*b) - encoded * 255.).abs())
        })
        .unwrap()
}

#[test]
fn graphic_pixels_survive_resize_exposure_and_ui_only_replacement() {
    let (world, _) = graphic_world();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = overlay_host().windowless(&mut driver).unwrap();
    apply(&mut run, extracted(&world));
    region(&capture(&mut run, &events).1, [255, 0, 0, 255]);
    change_camera_and_exposure(&mut run);
    region(&capture(&mut run, &events).1, [255, 0, 0, 255]);
    run.resize([48, 32]).unwrap();
    region(&capture(&mut run, &events).1, [255, 0, 0, 255]);
    let mut ui = extracted(&world);
    ui.graphics[0].graphic.quads[0].color = [0., 1., 0., 1.];
    apply(&mut run, ui);
    region(&capture(&mut run, &events).1, [0, 255, 0, 255]);
    assert_statistics(&run, run.host().accepted_generation(), 1);
    run.shutdown().unwrap();
}
