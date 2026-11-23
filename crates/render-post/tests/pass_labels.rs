use worldrunner_render_post::{PassEntry, PassId, PassSlot, StageId, contracts::RasterId};

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
        ContourIds,
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
    let identity = PassId::Geometry(PassEntry::new(slot));
    let label = identity.encoder_label();
    // Before a role could repeat, the label was `Geometry(` and the role's Debug spelling.
    assert_eq!(label.as_ref(), format!("Geometry({slot:?})"));
    assert_eq!(identity.to_string(), label.as_ref());
    assert!(!label.as_ref().is_empty());
    assert!(!label.as_ref().contains("stock geometry"));
}

#[test]
fn first_entry_labels_are_the_names_geometry_passes_always_had() {
    for (slot, expected) in [
        (PassSlot::Opaque, "Geometry(Opaque)"),
        (PassSlot::DepthNormalPrepass, "Geometry(DepthNormalPrepass)"),
        (PassSlot::TransparentWater, "Geometry(TransparentWater)"),
        (PassSlot::ShadowCasters(0), "Geometry(ShadowCasters(0))"),
        (PassSlot::ShadowCasters(3), "Geometry(ShadowCasters(3))"),
        (PassSlot::ReflectionMip(2), "Geometry(ReflectionMip(2))"),
    ] {
        let id = PassId::Geometry(PassEntry::new(slot));
        assert_eq!(id.encoder_label().as_ref(), expected);
        assert_eq!(id.to_string(), expected);
        assert_eq!(PassId::Geometry(PassEntry::nth(slot, 0)), id);
    }
}

#[test]
fn a_repeated_role_is_named_by_its_occurrence() {
    let name = |slot, occurrence| PassId::Geometry(PassEntry::nth(slot, occurrence)).to_string();
    assert_eq!(name(PassSlot::Opaque, 1), "Geometry(Opaque#1)");
    assert_eq!(name(PassSlot::Opaque, 2), "Geometry(Opaque#2)");
    assert_eq!(
        name(PassSlot::ShadowCasters(2), 1),
        "Geometry(ShadowCasters(2)#1)"
    );
    assert_ne!(name(PassSlot::Opaque, 0), name(PassSlot::Opaque, 1));
    assert_ne!(name(PassSlot::Opaque, 1), name(PassSlot::Opaque, 2));
    // The longest label still fits the stack buffer and encoder label is the reported name.
    let longest = PassId::Geometry(PassEntry::nth(
        PassSlot::ReflectionDepthPyramid(u32::MAX),
        u32::MAX,
    ));
    assert_eq!(
        longest.encoder_label().as_ref(),
        "Geometry(ReflectionDepthPyramid(4294967295)#4294967295)"
    );
    for slot in [PassSlot::Opaque, PassSlot::ShadowCasters(1)] {
        for occurrence in [1, 7] {
            let id = PassId::Geometry(PassEntry::nth(slot, occurrence));
            assert_eq!(id.encoder_label().as_ref(), id.to_string());
        }
    }
}

#[test]
fn contour_encoder_labels_preserve_occurrence_identity() {
    for (occurrence, expected) in [(0, "Geometry(ContourIds)"), (1, "Geometry(ContourIds#1)")] {
        let id = PassId::Geometry(PassEntry::nth(PassSlot::ContourIds, occurrence));
        assert_eq!(id.encoder_label().as_ref(), expected);
        assert_eq!(id.to_string(), expected);
    }
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
