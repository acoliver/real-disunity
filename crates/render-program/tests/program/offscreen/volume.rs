//! Render textures of several layers, of a depth or of cube faces: the state makes the dimension
//! the spec names with the layers it states, a kernel's or a draw's view of it is the matching one,
//! and the graph of records refuses a 3D texture by name.

use wgpu::TextureViewDimension as View;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::{
    OffscreenFrame, Record, RenderDimension, RenderTextureSpec,
};

use super::rig::{Rig, texture};

const ARRAY: TextureId = TextureId(10);
const VOLUME: TextureId = TextureId(11);
const PLAIN: TextureId = TextureId(12);

fn spec(id: TextureId, dimension: RenderDimension) -> RenderTextureSpec {
    RenderTextureSpec {
        dimension,
        ..texture(id.0, [4, 4])
    }
}

fn frame(specs: Vec<RenderTextureSpec>) -> OffscreenFrame {
    OffscreenFrame {
        render_textures: specs,
        ..OffscreenFrame::default()
    }
}

fn all_three() -> OffscreenFrame {
    frame(vec![
        spec(ARRAY, RenderDimension::D2Array(4)),
        spec(VOLUME, RenderDimension::D3(5)),
        spec(PLAIN, RenderDimension::D2),
    ])
}

#[test]
fn the_state_makes_the_dimension_and_extent_the_spec_asks_for() {
    let mut rig = Rig::new();
    rig.state.bind_as_storage([ARRAY, VOLUME, PLAIN]);
    rig.build(&all_three()).unwrap();

    assert_eq!(rig.state.texture_view_dimension(ARRAY), Some(View::D2Array));
    assert_eq!(rig.state.texture_view_dimension(VOLUME), Some(View::D3));
    assert_eq!(rig.state.texture_view_dimension(PLAIN), Some(View::D2));
    for id in [ARRAY, VOLUME, PLAIN] {
        assert_eq!(
            rig.state.texture_storage(id),
            Some((wgpu::TextureFormat::Rgba8Unorm, true)),
            "{id:?} takes a kernel's writes"
        );
    }
    assert_eq!(rig.state.texture_view_dimension(TextureId(99)), None);
}

#[test]
fn a_texture_whose_dimension_changed_is_made_again() {
    let mut rig = Rig::new();
    rig.build(&frame(vec![spec(VOLUME, RenderDimension::D2Array(5))]))
        .unwrap();
    assert_eq!(
        rig.state.texture_view_dimension(VOLUME),
        Some(View::D2Array)
    );
    let before = rig.state.texture_handle(VOLUME).unwrap();

    rig.build(&frame(vec![spec(VOLUME, RenderDimension::D3(5))]))
        .unwrap();

    assert_eq!(rig.state.texture_view_dimension(VOLUME), Some(View::D3));
    assert_ne!(rig.state.texture_handle(VOLUME), Some(before));
}

#[test]
fn a_3d_texture_has_one_mip_and_no_depth_buffer() {
    let mut rig = Rig::new();
    let mut mips = spec(VOLUME, RenderDimension::D3(5));
    mips.mips = 2;
    let error = rig.build(&frame(vec![mips])).err().unwrap();
    assert!(
        matches!(
            error,
            OffscreenError::VolumeShape {
                id: VOLUME,
                reason: "has one mip"
            }
        ),
        "{error}"
    );
    let mut depth = spec(VOLUME, RenderDimension::D3(5));
    depth.depth_bits = 16;
    let error = rig.build(&frame(vec![depth])).err().unwrap();
    assert!(
        matches!(
            error,
            OffscreenError::VolumeShape {
                id: VOLUME,
                reason: "has no depth buffer"
            }
        ),
        "{error}"
    );
}

#[test]
fn a_record_that_names_a_3d_texture_is_refused_by_name() {
    let mut rig = Rig::new();
    let mut named = all_three();
    named.records = vec![Record::CopyTexture {
        source: VOLUME,
        source_element: 0,
        source_mip: 0,
        target: PLAIN,
        target_element: 0,
        target_mip: 0,
    }];
    let error = rig.build(&named).err().unwrap();
    assert!(
        matches!(error, OffscreenError::VolumeInGraph(VOLUME)),
        "{error}"
    );
    // Without the record the same textures build.
    rig.build(&all_three()).unwrap();
}
