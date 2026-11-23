//! Clears, draw lists and global records: the records a scripted frame is made of. Each test
//! predicts the texels it reads.

use std::sync::Arc;

use worldrunner_render_program::ids::{MaterialId, RendererId, TextureId};
use worldrunner_render_program::layers::{PropertyLayer, TextureInput};
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, DrawList, DrawRaster, GlobalState, Instances, ListDraw,
    OffscreenFrame, Record,
};
use worldrunner_render_program::unity::offscreen::{FeedbackLoopDraws, ListDraws};
use worldrunner_render_program::unity::shader::PassSelector;

use super::camera::{TARGET, alpha, fixture, material, renderer};
use super::rig::{BLUE, CLEAR, GREEN, RED, Rig, ZERO, layer, pixel, quad, source, texture};

const NEAR: TextureId = TextureId(11);

pub fn whole(texture: TextureId) -> Attachments {
    let attachment = Some(Attachment { texture, mip: 0 });
    Attachments::single(attachment, attachment)
}

fn clear_to(color: [f64; 4]) -> Clear {
    Clear {
        color: Some(color),
        depth: None,
        stencil: None,
    }
}

pub fn item(renderer: u64, material: Option<u64>) -> ListDraw {
    ListDraw {
        renderer: RendererId(renderer),
        slot: 0,
        material: material.map(MaterialId),
        pass: alpha(),
        instances: Instances::Count(1),
        per_draw: PropertyLayer::default(),
    }
}

pub fn list(target: Attachments, draws: Vec<ListDraw>) -> Record {
    Record::Draws(Arc::new(DrawList {
        raster: DrawRaster::default(),
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
fn a_list_item_of_no_instances_draws_nothing_and_its_neighbours_still_draw() {
    let mut rig = Rig::new();
    let none = ListDraw {
        instances: Instances::Count(0),
        ..item(1, None)
    };
    let records = vec![list(whole(TARGET), vec![none, item(2, None)])];
    let frame = frame_of(&mut rig, records);
    let image = render(&mut rig, &frame);
    assert_eq!(
        pixel(&image, 8, (1, 3)),
        ZERO,
        "the left renderer has no instances"
    );
    assert_eq!(pixel(&image, 8, (6, 3)), BLUE, "the right renderer draws");
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
fn a_list_items_own_values_win_over_its_renderers_for_that_item_only() {
    let mut rig = Rig::new();
    // Renderer 1 sits over the left half. An item with values moves it over the whole target and
    // an item with none draws it where the renderer puts it.
    let moved = ListDraw {
        per_draw: layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]),
        ..item(1, None)
    };
    let frame = frame_of(&mut rig, vec![list(whole(TARGET), vec![moved])]);
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (6, 3)), RED, "moved over the right half");
    let mut rig = Rig::new();
    let frame = frame_of(&mut rig, vec![list(whole(TARGET), vec![item(1, None)])]);
    let image = render(&mut rig, &frame);
    assert_eq!(pixel(&image, 8, (6, 3)), ZERO, "an item with no values");
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
    let target = Attachments::single(Some(level_one), None);
    frame.records = vec![
        Record::Clear {
            target: target.clone(),
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
        .set_mesh(&mut rig.runtime, super::rig::QUAD, &quad(0.2))
        .unwrap();
    rig.state
        .set_mesh(&mut rig.runtime, super::rig::NEAR_QUAD, &quad(0.8))
        .unwrap();
    frame.renderers.get_mut(&RendererId(2)).unwrap().mesh = super::rig::NEAR_QUAD;
    for id in [RendererId(1), RendererId(2)] {
        frame.renderers.get_mut(&id).unwrap().per_draw = layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    }
    let target = Attachments::single(
        Some(Attachment {
            texture: TARGET,
            mip: 0,
        }),
        Some(Attachment {
            texture: NEAR,
            mip: 0,
        }),
    );
    let depth_clear = Clear {
        color: None,
        depth: Some(1.0),
        stencil: None,
    };
    // The far red quad is drawn after the near blue one and stays hidden behind it.
    frame.records = vec![
        Record::Clear {
            target: target.clone(),
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
    assert!(depths.iter().all(|d| d.to_bits() == 0.8_f32.to_bits()));
}

const ALBEDO: TextureId = TextureId(7);

fn gamma() -> PassSelector {
    PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Gamma".to_owned()],
    }
}

fn sampling(id: TextureId) -> TextureInput {
    TextureInput {
        texture: id,
        sampler: None,
    }
}

/// The camera fixture over a 16 by 16 `TARGET` of `mips` levels with no records. Renderer 1's
/// material samples `TARGET` itself, so a list or camera render into it reads what it writes.
/// Renderer 2's material samples a white texture of its own, so it draws blue over the right
/// half of its target through the `Gamma` pass.
fn self_sampling(rig: &mut Rig, mips: u32) -> OffscreenFrame {
    let mut frame = frame_of(rig, Vec::new());
    frame.render_textures[0] = texture(10, [16, 16]);
    frame.render_textures[0].mips = mips;
    let white = source(7, [4, 1], super::rig::image([4, 1], |_, _| [255; 4]));
    rig.state.set_texture(&mut rig.runtime, &white).unwrap();
    for (material, sampled) in [(1, TARGET), (2, ALBEDO)] {
        frame
            .materials
            .get_mut(&MaterialId(material))
            .unwrap()
            .properties
            .textures
            .insert("_Albedo".to_owned(), sampling(sampled));
    }
    frame
}

fn gamma_item(renderer: u64) -> ListDraw {
    ListDraw {
        pass: gamma(),
        ..item(renderer, None)
    }
}

fn clear_mip(mip: u32) -> Record {
    Record::Clear {
        target: Attachments::single(
            Some(Attachment {
                texture: TARGET,
                mip,
            }),
            None,
        ),
        clear: clear_to([1.0, 1.0, 0.0, 1.0]),
    }
}

fn into_mip(mip: u32) -> Attachments {
    Attachments::single(
        Some(Attachment {
            texture: TARGET,
            mip,
        }),
        None,
    )
}

/// A frame whose list into level 1 of a two-level `TARGET` draws renderer 1, which samples
/// `TARGET`, then renderer 2. Level 0 is cleared first, and level 1 as well when `both_levels`.
fn mip_chain_list(rig: &mut Rig, both_levels: bool) -> OffscreenFrame {
    let mut frame = self_sampling(rig, 2);
    frame.records = vec![clear_mip(0)];
    if both_levels {
        frame.records.push(clear_mip(1));
    }
    frame
        .records
        .push(list(into_mip(1), vec![gamma_item(1), gamma_item(2)]));
    frame
}

fn list_counts(drawn: usize, feedback_loop: usize) -> Vec<ListDraws> {
    vec![ListDraws {
        target: TARGET,
        drawn,
        unmatched: 0,
        missing_texture: 0,
        missing_channel: 0,
        feedback_loop,
    }]
}

#[test]
fn a_list_draw_that_samples_the_level_it_draws_into_is_refused_and_the_other_draws_run() {
    let mut rig = Rig::new();
    let frame = mip_chain_list(&mut rig, true);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.list_draws(), list_counts(1, 1));
    assert_eq!(
        operations.feedback_loop_draws(),
        vec![FeedbackLoopDraws {
            texture: TARGET,
            draws: 1
        }]
    );
    drop(operations);
    // The contract admits the list without the refused draw: renderer 2's blue is over the right
    // half of level 1, and the left half holds the clear color.
    let image = rig.run(&frame, &[(TARGET, 1, 0)]).unwrap().remove(0);
    assert_eq!(pixel(&image, 8, (1, 3)), CLEAR, "left: the refused draw");
    assert_eq!(pixel(&image, 8, (6, 3)), BLUE, "right: renderer 2");
}

#[test]
fn a_list_draw_that_samples_only_levels_other_than_the_one_it_draws_into_is_recorded() {
    let mut rig = Rig::new();
    // Level 1 was never written, so the draw reads level 0 alone and writes level 1.
    let frame = mip_chain_list(&mut rig, false);
    let operations = rig.build(&frame).unwrap();
    assert_eq!(operations.list_draws(), list_counts(2, 0));
    assert!(operations.feedback_loop_draws().is_empty());
    drop(operations);
    let image = rig.run(&frame, &[(TARGET, 1, 0)]).unwrap().remove(0);
    assert_eq!(
        pixel(&image, 8, (1, 3)),
        RED,
        "left: level 0's clear color times red"
    );
    assert_eq!(pixel(&image, 8, (6, 3)), BLUE, "right: renderer 2");
}

#[test]
fn a_camera_draw_that_samples_its_own_target_is_refused_and_the_other_draws_run() {
    let mut rig = Rig::new();
    let mut frame = self_sampling(&mut rig, 1);
    let camera = frame.cameras.get_mut(&super::camera::CAMERA).unwrap();
    camera.pass = gamma();
    camera.culling_mask = (1 << super::camera::LEFT) | (1 << 2);
    frame.records = vec![Record::CameraRender {
        camera: super::camera::CAMERA,
        target: TARGET,
    }];
    let operations = rig.build(&frame).unwrap();
    let draws = operations.camera_draws();
    assert_eq!((draws[0].drawn, draws[0].feedback_loop), (1, 1));
    assert_eq!(operations.feedback_loop_draws().len(), 1);
    drop(operations);
    let image = rig.run(&frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    assert_eq!(pixel(&image, 16, (3, 6)), CLEAR, "left: the refused draw");
    assert_eq!(pixel(&image, 16, (12, 6)), BLUE, "right: renderer 2");
}
