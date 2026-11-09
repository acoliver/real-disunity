//! Clears, draw lists and global records: the records a scripted frame is made of. Each test
//! predicts the texels it reads.

use std::sync::Arc;

use worldrunner_render_program::ids::{MaterialId, RendererId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, DrawList, GlobalState, ListDraw, OffscreenFrame, Record,
};

use super::camera::{TARGET, alpha, fixture, material, renderer};
use super::rig::{BLUE, CLEAR, GREEN, RED, Rig, ZERO, layer, pixel, quad, texture};

const NEAR: TextureId = TextureId(11);

fn whole(texture: TextureId) -> Attachments {
    let attachment = Some(Attachment { texture, mip: 0 });
    Attachments {
        color: attachment,
        depth: attachment,
    }
}

fn clear_to(color: [f64; 4]) -> Clear {
    Clear {
        color: Some(color),
        depth: None,
        stencil: None,
    }
}

fn item(renderer: u64, material: Option<u64>) -> ListDraw {
    ListDraw {
        renderer: RendererId(renderer),
        slot: 0,
        material: material.map(MaterialId),
        pass: alpha(),
    }
}

fn list(target: Attachments, draws: Vec<ListDraw>) -> Record {
    Record::Draws(Arc::new(DrawList {
        target,
        values: PropertyLayer::default(),
        front_face: wgpu::FrontFace::Cw,
        draws,
    }))
}

/// The frame of `camera::fixture` with `records` in place of its camera render.
fn frame_of(rig: &mut Rig, records: Vec<Record>) -> OffscreenFrame {
    let mut frame = fixture(rig, 0);
    frame.records = records;
    frame
}

fn render(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<u8> {
    rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0)
}

#[test]
fn a_clear_sets_the_color_of_the_texture_it_names() {
    let mut rig = Rig::new();
    let records = vec![Record::Clear {
        target: whole(TARGET),
        clear: clear_to([1.0, 0.0, 0.0, 1.0]),
    }];
    let frame = frame_of(&mut rig, records);
    let image = render(&mut rig, &frame);
    assert!(image.chunks(4).all(|texel| texel == RED));
}

#[test]
fn a_list_draws_its_items_over_what_the_target_holds() {
    let mut rig = Rig::new();
    let records = vec![
        Record::Clear {
            target: whole(TARGET),
            clear: clear_to([1.0, 1.0, 0.0, 1.0]),
        },
        list(whole(TARGET), vec![item(1, None)]),
    ];
    let frame = frame_of(&mut rig, records);
    let image = render(&mut rig, &frame);
    for y in 0..8 {
        for x in 0..4 {
            assert_eq!(pixel(&image, 8, (x, y)), RED, "left ({x}, {y})");
            assert_eq!(pixel(&image, 8, (x + 4, y)), CLEAR, "right ({x}, {y})");
        }
    }
}

#[test]
fn a_list_item_with_a_material_draws_that_material_in_place_of_the_slots() {
    let mut rig = Rig::new();
    let records = vec![list(whole(TARGET), vec![item(1, Some(2))])];
    let frame = frame_of(&mut rig, records);
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (1, 3)), BLUE, "the left renderer in blue");
    assert_eq!(pixel(&image, 8, (6, 3)), ZERO, "nothing on the right");
}

#[test]
fn items_draw_in_the_order_the_list_gives_them() {
    let mut rig = Rig::new();
    let mut frame = frame_of(&mut rig, Vec::new());
    for id in [RendererId(1), RendererId(2)] {
        frame.renderers.get_mut(&id).unwrap().per_draw = layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    }
    frame.records = vec![list(whole(TARGET), vec![item(1, None), item(2, None)])];
    assert_eq!(pixel(&render(&mut rig, &frame), 8, (4, 4)), BLUE);
    frame.records = vec![list(whole(TARGET), vec![item(2, None), item(1, None)])];
    assert_eq!(pixel(&render(&mut rig, &frame), 8, (4, 4)), RED);
}

#[test]
fn a_globals_record_gives_the_draws_after_it_the_values_it_holds() {
    let mut rig = Rig::new();
    let mut frame = frame_of(&mut rig, Vec::new());
    // Two renderers whose material sets no tint, so the tint is the global one.
    let bare = material([0.0; 4], 2000);
    frame.materials.insert(
        MaterialId(3),
        worldrunner_render_program::unity::offscreen::frame::Material {
            properties: PropertyLayer::default(),
            ..bare
        },
    );
    frame
        .renderers
        .insert(RendererId(3), renderer(3, 1, [0.5, 1.0, -0.5, 0.0]));
    frame
        .renderers
        .insert(RendererId(4), renderer(3, 1, [0.5, 1.0, 0.5, 0.0]));
    let tint = |value: [f32; 4]| {
        Record::Globals(Arc::new(GlobalState {
            values: layer(&[("_Tint", value)]),
            keywords: Vec::new(),
        }))
    };
    frame.records = vec![
        tint([0.0, 1.0, 0.0, 1.0]),
        list(whole(TARGET), vec![item(3, None)]),
        tint([0.0, 0.0, 1.0, 1.0]),
        list(whole(TARGET), vec![item(4, None)]),
    ];
    let image = render(&mut rig, &frame);
    assert_eq!(
        pixel(&image, 8, (1, 3)),
        GREEN,
        "drawn under the first globals"
    );
    assert_eq!(
        pixel(&image, 8, (6, 3)),
        BLUE,
        "drawn under the second globals"
    );
}

#[test]
fn a_list_draws_into_the_mip_it_names_and_leaves_the_others() {
    let mut rig = Rig::new();
    let mut frame = frame_of(&mut rig, Vec::new());
    frame.render_textures[0] = texture(10, [16, 16]);
    frame.render_textures[0].mips = 2;
    let level_one = Attachment {
        texture: TARGET,
        mip: 1,
    };
    let target = Attachments {
        color: Some(level_one),
        depth: None,
    };
    frame.records = vec![
        Record::Clear {
            target,
            clear: clear_to([1.0, 1.0, 0.0, 1.0]),
        },
        list(target, vec![item(1, None)]),
    ];
    let images = rig.run(&frame, &[(TARGET, 1, 0), (TARGET, 0, 0)]).unwrap();
    assert_eq!(pixel(&images[0], 8, (1, 3)), RED, "left of the 8 by 8 mip");
    assert_eq!(
        pixel(&images[0], 8, (6, 3)),
        CLEAR,
        "right of the 8 by 8 mip"
    );
    assert!(
        images[1].chunks(4).all(|texel| texel == ZERO),
        "the base level is untouched"
    );
}

#[test]
fn a_list_uses_the_depth_of_another_texture_than_its_color() {
    let mut rig = Rig::new();
    let mut frame = frame_of(&mut rig, Vec::new());
    let mut depth = texture(11, [8, 8]);
    depth.graphics_format = 0;
    depth.depth_bits = 32;
    frame.render_textures.push(depth);
    rig.state
        .set_mesh(&mut rig.runtime, super::rig::QUAD, &quad(0.8))
        .unwrap();
    rig.state
        .set_mesh(&mut rig.runtime, super::rig::NEAR_QUAD, &quad(0.2))
        .unwrap();
    frame.renderers.get_mut(&RendererId(2)).unwrap().mesh = super::rig::NEAR_QUAD;
    for id in [RendererId(1), RendererId(2)] {
        frame.renderers.get_mut(&id).unwrap().per_draw = layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    }
    let target = Attachments {
        color: Some(Attachment {
            texture: TARGET,
            mip: 0,
        }),
        depth: Some(Attachment {
            texture: NEAR,
            mip: 0,
        }),
    };
    let depth_clear = Clear {
        color: None,
        depth: Some(1.0),
        stencil: None,
    };
    // The far red quad is drawn after the near blue one and stays hidden behind it.
    frame.records = vec![
        Record::Clear {
            target,
            clear: depth_clear,
        },
        list(target, vec![item(2, None), item(1, None)]),
    ];
    let images = rig.run(&frame, &[(TARGET, 0, 0), (NEAR, 0, 0)]).unwrap();
    assert_eq!(pixel(&images[0], 8, (4, 4)), BLUE);
    let depths: Vec<f32> = images[1]
        .chunks(4)
        .map(|word| f32::from_le_bytes(word.try_into().unwrap()))
        .collect();
    assert!(depths.iter().all(|d| d.to_bits() == 0.2_f32.to_bits()));
}
