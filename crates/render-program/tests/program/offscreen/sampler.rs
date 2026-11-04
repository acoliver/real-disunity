//! Sampler state: wrap and filter modes from a texture input or the texture itself.

use worldrunner_render_program::ids::{CameraId, MaterialId, RendererId, TextureId};
use worldrunner_render_program::layers::TextureInput;
use worldrunner_render_program::offscreen::frame::OffscreenFrame;
use worldrunner_render_program::sampler::{Filter, SamplerState, Wrap};
use worldrunner_render_program::shader::PassSelector;

use super::camera::fixture;
use super::rig::{BLUE, Rig, layer, pixel, source, texture};

const TARGET: TextureId = TextureId(10);
const ALBEDO: TextureId = TextureId(7);
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const WHITE: [u8; 4] = [255; 4];

fn gamma() -> PassSelector {
    PassSelector::Tag {
        key: "LightMode".to_owned(),
        values: vec!["Gamma".to_owned()],
    }
}

fn point(wrap: Wrap) -> SamplerState {
    SamplerState {
        wrap_u: wrap,
        wrap_v: wrap,
        filter: Filter::Point,
    }
}

/// A 4x1 target covered by one renderer that samples a 4x1 albedo texture (red, green, blue,
/// white), half a texture to the right of each fragment. With uv at the pixel centers 1/8, 3/8,
/// 5/8 and 7/8 the shifted coordinates are 5/8, 7/8, 9/8 and 11/8. Point sampling reads texel
/// floor(4u): blue and white inside the texture, and past its edge
/// - clamp: white, white
/// - repeat: 1/8 and 3/8 give red and green
/// - mirror: 7/8 and 5/8 give white and blue.
fn shifted(rig: &mut Rig, sampler: Option<SamplerState>) -> OffscreenFrame {
    let mut frame = fixture(rig, 1 << 1);
    frame.render_textures = vec![texture(10, [4, 1])];
    frame.cameras.get_mut(&CameraId(1)).unwrap().pass = gamma();
    frame.renderers.get_mut(&RendererId(1)).unwrap().per_draw =
        layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    let mut albedo = source(7, [4, 1], [RED, GREEN, BLUE, WHITE].concat());
    albedo.sampler = sampler;
    rig.state.set_texture(&mut rig.runtime, &albedo).unwrap();
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .properties
        .textures
        .insert(
            "_Albedo".to_owned(),
            TextureInput {
                texture: ALBEDO,
                sampler: None,
            },
        );
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .properties
        .values = layer(&[("_Tint", [1.0; 4])]).values;
    frame
}

fn row(rig: &mut Rig, frame: &OffscreenFrame) -> Vec<[u8; 4]> {
    let image = rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    (0..4).map(|x| pixel(&image, 4, (x, 0))).collect()
}

#[test]
fn a_texture_without_a_state_keeps_the_fixed_sampler_that_clamps_to_the_edge() {
    let mut rig = Rig::new();
    let frame = shifted(&mut rig, None);
    let row = row(&mut rig, &frame);
    // 9/8 and 11/8 are past the right edge, where clamping reads the last texel. The filter is
    // linear, so only the pixels that read the last texel alone are exact.
    assert_eq!(row[2], WHITE);
    assert_eq!(row[3], WHITE);
}

#[test]
fn a_texture_with_a_repeat_state_wraps_the_coordinate() {
    let mut rig = Rig::new();
    let frame = shifted(&mut rig, Some(point(Wrap::Repeat)));
    assert_eq!(row(&mut rig, &frame), [BLUE, WHITE, RED, GREEN]);
}

#[test]
fn a_texture_with_a_clamp_state_reads_the_edge_texel_past_the_edge() {
    let mut rig = Rig::new();
    let frame = shifted(&mut rig, Some(point(Wrap::Clamp)));
    assert_eq!(row(&mut rig, &frame), [BLUE, WHITE, WHITE, WHITE]);
}

#[test]
fn a_mirror_state_reflects_the_coordinate() {
    let mut rig = Rig::new();
    let frame = shifted(&mut rig, Some(point(Wrap::Mirror)));
    assert_eq!(row(&mut rig, &frame), [BLUE, WHITE, WHITE, BLUE]);
}

#[test]
fn the_state_of_a_texture_input_overrides_the_state_of_the_texture() {
    let mut rig = Rig::new();
    let mut frame = shifted(&mut rig, Some(point(Wrap::Clamp)));
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .properties
        .textures
        .get_mut("_Albedo")
        .unwrap()
        .sampler = Some(point(Wrap::Repeat));
    assert_eq!(row(&mut rig, &frame), [BLUE, WHITE, RED, GREEN]);
}

#[test]
fn a_texture_a_higher_layer_names_replaces_the_one_below() {
    let mut rig = Rig::new();
    let mut frame = shifted(&mut rig, Some(point(Wrap::Repeat)));
    rig.state
        .set_texture(
            &mut rig.runtime,
            &source(8, [2, 1], [GREEN, GREEN].concat()),
        )
        .unwrap();
    frame
        .renderers
        .get_mut(&RendererId(1))
        .unwrap()
        .block
        .textures
        .insert(
            "_Albedo".to_owned(),
            TextureInput {
                texture: TextureId(8),
                sampler: None,
            },
        );
    assert_eq!(row(&mut rig, &frame), [GREEN; 4]);
}

#[test]
fn a_pass_that_samples_the_texture_it_renders_into_is_refused_by_the_executor() {
    use worldrunner_render_post::contracts::ContractError;
    let mut rig = Rig::new();
    let mut frame = shifted(&mut rig, None);
    frame
        .materials
        .get_mut(&MaterialId(1))
        .unwrap()
        .properties
        .textures
        .insert(
            "_Albedo".to_owned(),
            TextureInput {
                texture: TARGET,
                sampler: None,
            },
        );
    let operations = rig.build(&frame).unwrap();
    let encoder = rig.gpu.authority.begin(&rig.gpu.device).unwrap();
    let error = operations.admit(&encoder).err().unwrap();
    let hazard = |error: &ContractError| match error {
        ContractError::Pass { source, .. } => matches!(**source, ContractError::Hazard(_)),
        _ => false,
    };
    assert!(
        matches!(&error, worldrunner_render_program::offscreen::OffscreenError::Contract(c) if hazard(c)),
        "{error}"
    );
    encoder.finish().cancel();
}
