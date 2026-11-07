use worldrunner_render_post::{PassId, PassSlot, StageId, contracts::RasterId};

#[test]
fn geometry_encoder_labels_match_timing_identities() {
    use PassSlot::*;
    let fixed = [
        DepthNormalPrepass,
        Ssao,
        SsaoBlurHorizontal,
        SsaoBlur,
        Celestial,
        Opaque,
        AfterOpaqueSnapshot,
        AfterOpaqueDepth,
        TransparentWater,
        TransparentWaterMask,
        TransparentAlpha,
        AfterWaterDepth,
        ReflectionTrace,
        ScreenSpaceAfterTransparent,
    ];
    for slot in fixed {
        assert_geometry_label(slot);
    }
    for index in [0, 2, u32::MAX] {
        for slot in [
            ShadowCasters(index),
            DepthPyramid(index),
            OcclusionMetadata(index),
            ReflectionDepthPyramid(index),
            ReflectionMip(index),
        ] {
            assert_geometry_label(slot);
        }
    }
}

fn assert_geometry_label(slot: PassSlot) {
    let identity = PassId::Geometry(slot);
    let label = identity.encoder_label();
    assert_eq!(label.as_ref(), format!("{identity:?}"));
    assert!(!label.as_ref().is_empty());
    assert!(!label.as_ref().contains("stock geometry"));
}

#[test]
fn resolve_and_authored_encoder_labels_preserve_specific_timing_names() {
    assert_eq!(PassId::Resolve.encoder_label().as_ref(), "Resolve");
    for name in [
        "bloom downsample",
        "bloom upsample",
        "smaa_edges",
        "smaa_weights",
        "smaa_blend",
        "fxaa",
        "taa",
        "receiver depth statistics",
        "terrain material composition",
    ] {
        for level in [0, 2, u32::MAX] {
            let stage = StageId::new(name, level);
            let expected = if level == 0 {
                name.to_owned()
            } else {
                format!("{name} level {level}")
            };
            let id = PassId::Stage(stage);
            assert_eq!(id.encoder_label().as_ref(), expected);
            assert_eq!(id.to_string(), expected);
            for occurrence in [0, 2, u32::MAX] {
                let id = PassId::Operation(RasterId::new(stage, occurrence));
                assert_eq!(id.encoder_label().as_ref(), format!("{id:?}"));
                assert_eq!(id.to_string(), format!("{id:?}"));
            }
        }
    }
}
