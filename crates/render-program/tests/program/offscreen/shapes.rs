//! Every shape a render texture spec names: the state makes the layers the dimension states
//! before anything touches the texture, binds it through the view of that dimension, and refuses
//! by name a dimension it cannot make.

use wgpu::TextureViewDimension as View;
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::OffscreenError;
use worldrunner_render_program::unity::offscreen::frame::{
    OffscreenFrame, Record, RenderDimension, RenderTextureSpec,
};

use super::rig::{BLUE, CYAN, GREEN, RED, Rig, image, pixel, source, texture};

const TARGET: TextureId = TextureId(20);
const FACE_COLORS: [[u8; 4]; 6] = [
    RED,
    GREEN,
    BLUE,
    CYAN,
    [255, 0, 255, 255],
    [255, 255, 255, 255],
];

fn frame_of(dimension: RenderDimension) -> OffscreenFrame {
    let spec = RenderTextureSpec {
        dimension,
        ..texture(TARGET.0, [4, 4])
    };
    OffscreenFrame {
        render_textures: vec![spec],
        ..OffscreenFrame::default()
    }
}

fn built(dimension: RenderDimension) -> Rig {
    let mut rig = Rig::new();
    rig.build(&frame_of(dimension)).unwrap();
    rig
}

#[test]
fn each_dimension_is_made_with_the_layers_it_states_and_bound_through_its_own_view() {
    let shapes = [
        (RenderDimension::D2, View::D2, View::D2),
        (RenderDimension::D2Array(1), View::D2Array, View::D2Array),
        (RenderDimension::D2Array(4), View::D2Array, View::D2Array),
        (RenderDimension::D3(8), View::D3, View::D3),
        (RenderDimension::Cube, View::Cube, View::D2Array),
        (
            RenderDimension::CubeArray(2),
            View::CubeArray,
            View::D2Array,
        ),
    ];
    for (dimension, sampled, storage) in shapes {
        let rig = built(dimension);

        assert_eq!(rig.state.texture_dimension(TARGET), Some(dimension));
        assert_eq!(
            rig.state.texture_view_dimension(TARGET),
            Some(sampled),
            "{dimension:?}"
        );
        assert_eq!(
            rig.state.storage_view_dimension(TARGET),
            Some(storage),
            "{dimension:?}"
        );
    }
}

#[test]
fn the_layers_of_a_dimension_are_the_ones_it_states() {
    assert_eq!(RenderDimension::D2.layers(), 1);
    assert_eq!(RenderDimension::D2Array(4).layers(), 4);
    assert_eq!(RenderDimension::D3(8).layers(), 8);
    assert_eq!(RenderDimension::Cube.layers(), 6);
    assert_eq!(RenderDimension::CubeArray(2).layers(), 12);
}

#[test]
fn a_cube_has_six_faces_and_an_array_exactly_its_layers() {
    for (dimension, layers) in [
        (RenderDimension::Cube, 6),
        (RenderDimension::CubeArray(2), 12),
        (RenderDimension::D2Array(1), 1),
        (RenderDimension::D2Array(4), 4),
    ] {
        let mut rig = Rig::new();
        let frame = frame_of(dimension);

        let last = rig.run(&frame, &[(TARGET, 0, layers - 1)]).unwrap();
        assert!(last[0].iter().all(|byte| *byte == 0), "{dimension:?}");
        let error = rig.run(&frame, &[(TARGET, 0, layers)]).err().unwrap();
        assert!(
            matches!(error, OffscreenError::NoSuchSubresource { element, .. } if element == layers),
            "{dimension:?}: {error}"
        );
    }
}

#[test]
fn each_face_of_a_cube_holds_what_was_copied_into_it() {
    let mut rig = Rig::new();
    for (face, color) in (5..).zip(FACE_COLORS) {
        let bytes = image([4, 4], |_, _| color);
        rig.state
            .set_texture(&mut rig.runtime, &source(face, [4, 4], bytes))
            .unwrap();
    }
    let mut frame = frame_of(RenderDimension::Cube);
    frame.records = (0..6)
        .map(|face| Record::CopyTexture {
            source: TextureId(5 + u64::from(face)),
            source_element: 0,
            source_mip: 0,
            target: TARGET,
            target_element: face,
            target_mip: 0,
        })
        .collect();
    let reads: Vec<_> = (0..6).map(|face| (TARGET, 0, face)).collect();

    let faces = rig.run(&frame, &reads).unwrap();

    for (face, color) in FACE_COLORS.iter().enumerate() {
        assert_eq!(pixel(&faces[face], 4, (0, 0)), *color, "face {face}");
        assert_eq!(pixel(&faces[face], 4, (3, 3)), *color, "face {face}");
    }
}

#[test]
fn a_dimension_the_host_could_not_map_is_refused_by_name() {
    let mut rig = Rig::new();

    let error = rig
        .build(&frame_of(RenderDimension::Unknown(9)))
        .err()
        .unwrap();

    assert!(
        matches!(
            error,
            OffscreenError::UnknownDimension {
                id: TARGET,
                code: 9
            }
        ),
        "{error}"
    );
}

#[test]
fn a_cube_must_be_square() {
    for dimension in [RenderDimension::Cube, RenderDimension::CubeArray(1)] {
        let mut rig = Rig::new();
        let mut frame = frame_of(dimension);
        frame.render_textures[0].size = [4, 2];

        let error = rig.build(&frame).err().unwrap();

        assert!(
            matches!(
                error,
                OffscreenError::CubeShape {
                    id: TARGET,
                    reason: "is square"
                }
            ),
            "{dimension:?}: {error}"
        );
    }
}

#[test]
fn a_depth_past_what_the_device_allows_is_refused_by_name() {
    let mut rig = Rig::new();
    let limits = rig.gpu.device.limits();
    for (dimension, limit) in [
        (
            RenderDimension::D3(limits.max_texture_dimension_3d + 1),
            limits.max_texture_dimension_3d,
        ),
        (
            RenderDimension::D2Array(limits.max_texture_array_layers + 1),
            limits.max_texture_array_layers,
        ),
        (
            RenderDimension::CubeArray(limits.max_texture_array_layers),
            limits.max_texture_array_layers,
        ),
    ] {
        let error = rig.build(&frame_of(dimension)).err().unwrap();

        assert!(
            matches!(error, OffscreenError::DepthLimit { id: TARGET, limit: found, .. } if found == limit),
            "{dimension:?}: {error}"
        );
    }
}

#[test]
fn a_texture_the_state_does_not_hold_has_no_dimension() {
    let rig = built(RenderDimension::D2);

    assert_eq!(rig.state.texture_dimension(TextureId(99)), None);
    assert_eq!(rig.state.storage_view_dimension(TextureId(99)), None);
}
