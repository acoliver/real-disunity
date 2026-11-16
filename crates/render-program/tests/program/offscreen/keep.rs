//! A render texture keeps what was drawn into it when the host raises its generation without
//! changing anything that shapes its storage, as a sampler edit does.

use std::collections::BTreeMap;

use worldrunner_render_program::ids::{MaterialId, ShaderId, TextureId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::{
    Material, OffscreenFrame, Record, RenderTextureSpec,
};
use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::blit_passes;

use super::camera::fixture;
use super::rig::{BLIT_QUAD, CLEAR, RED, Rig, ZERO, blit_quad, layer, pixel, texture};

const BLIT_SHADER: ShaderId = ShaderId(2);
/// The camera fixture's target: red over its left half, the clear color over the right.
const DRAWN: TextureId = TextureId(10);
/// Where the later frame's draw samples `DRAWN` into.
const SEEN: TextureId = TextureId(11);

/// A rig whose first frame has the camera draw into `DRAWN`, and the blit shader and mesh the
/// later frame samples it with.
fn drawn() -> Rig {
    let mut rig = Rig::new();
    let frame = fixture(&mut rig, 1 << 1);
    let image = rig.run(&frame, &[(DRAWN, 0, 0)]).unwrap().remove(0);
    assert_eq!(pixel(&image, 8, (0, 0)), RED);
    rig.runtime.collect();
    let synthetic = blit_passes("_MainTex");
    rig.state.add_shader(
        BLIT_SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    rig.state
        .set_mesh(&mut rig.runtime, BLIT_QUAD, &blit_quad())
        .unwrap();
    rig
}

/// The textures of a frame that only samples `DRAWN`, with the spec of `DRAWN` changed by `edit`.
fn specs(edit: impl FnOnce(&mut RenderTextureSpec)) -> Vec<RenderTextureSpec> {
    let mut drawn = texture(DRAWN.0, [8, 8]);
    edit(&mut drawn);
    vec![drawn, texture(SEEN.0, [8, 8])]
}

/// A frame whose one record is a blit of `DRAWN` into `SEEN` through the tint pass, which leaves
/// a white tint's source as it is.
fn sampling(render_textures: Vec<RenderTextureSpec>) -> OffscreenFrame {
    OffscreenFrame {
        blit_mesh: Some(BLIT_QUAD),
        render_textures,
        materials: BTreeMap::from([(
            MaterialId(7),
            Material {
                shader: Some(BLIT_SHADER),
                keywords: Vec::new(),
                properties: layer(&[("_Tint", [1.0; 4])]),
                fallback: PropertyLayer::default(),
                render_queue: 2000,
            },
        )]),
        records: vec![Record::Blit {
            source: DRAWN,
            target: SEEN,
            material: Some(MaterialId(7)),
            pass: 0,
        }],
        ..OffscreenFrame::default()
    }
}

#[test]
fn a_draw_samples_what_the_camera_drew_after_the_generation_rose_alone() {
    let mut rig = drawn();
    // Each rise is one edit of the texture's sampler.
    for generation in 1..=3 {
        let frame = sampling(specs(|spec| spec.generation = generation));
        let seen = rig.run(&frame, &[(SEEN, 0, 0)]).unwrap().remove(0);
        rig.runtime.collect();
        for y in 0..8 {
            for x in 0..4 {
                assert_eq!(pixel(&seen, 8, (x, y)), RED, "generation {generation}");
                assert_eq!(
                    pixel(&seen, 8, (x + 4, y)),
                    CLEAR,
                    "generation {generation}"
                );
            }
        }
    }
}

#[test]
fn the_texture_itself_still_holds_its_pixels_after_the_generation_rose_alone() {
    let mut rig = drawn();
    let mut frame = sampling(specs(|spec| spec.generation = 4));
    frame.records.clear();
    let image = rig.run(&frame, &[(DRAWN, 0, 0)]).unwrap().remove(0);
    assert_eq!(pixel(&image, 8, (0, 0)), RED);
    assert_eq!(pixel(&image, 8, (7, 7)), CLEAR);
}

#[test]
fn a_generation_that_rose_alone_adds_no_storage_and_gives_none_back() {
    let mut rig = drawn();
    let live = rig.runtime.live_bytes();
    let raised = specs(|spec| spec.generation = 5);
    assert_eq!(
        rig.state
            .new_render_texture_bytes(&rig.runtime, &raised[..1]),
        0,
        "the texture the state holds is not made again"
    );
    rig.run(&sampling(raised), &[(SEEN, 0, 0)]).unwrap();
    rig.runtime.collect();
    // `SEEN` is new, 8 x 8 texels of 4 bytes; `DRAWN` is neither added nor released.
    assert_eq!(rig.runtime.live_bytes(), live + 8 * 8 * 4);
    assert_eq!(
        rig.state.render_textures_held(),
        [(DRAWN, 8 * 8 * 4), (SEEN, 8 * 8 * 4)]
    );
}

#[test]
fn a_field_that_shapes_the_storage_still_gives_an_empty_texture_with_the_new_generation() {
    type Edit = fn(&mut RenderTextureSpec);
    let edits: [(&str, Edit, usize); 3] = [
        ("size", |spec| spec.size = [4, 2], 4 * 2 * 4),
        ("depth", |spec| spec.depth_bits = 16, 8 * 8 * 4),
        ("mips", |spec| spec.mips = 2, 8 * 8 * 4),
    ];
    for (name, edit, bytes) in edits {
        let mut rig = drawn();
        let mut frame = sampling(specs(|spec| {
            spec.generation = 1;
            edit(spec);
        }));
        frame.records.clear();
        let image = rig.run(&frame, &[(DRAWN, 0, 0)]).unwrap().remove(0);
        assert_eq!(image.len(), bytes, "{name}");
        assert!(image.chunks(4).all(|texel| texel == ZERO), "{name}");
    }
}
