//! The forward frame's pass sequence, written once.
//!
//! Features contribute their passes from several places (the scene, shadows, SSAO,
//! celestial meshes, snapshots, water, reflections), so the frame is collected first and
//! ordered here. The render-post executor runs the list it is given in the order it is
//! given; nothing downstream sorts.
use worldrunner_render_post::{GamePlace, PassDeclaration, PassSlot};

/// Where a role sits in the forward frame, as `(stage, layer)`.
///
/// The match has no wildcard arm, so a new `PassSlot` cannot compile until its place in the
/// forward frame is chosen here. The sequence is:
///
/// 1. shadow caster layers, by layer
/// 2. the depth/normal prepass
/// 3. the depth pyramid, by level
/// 4. occlusion metadata, by level
/// 5. SSAO, its horizontal blur, its final blur
/// 6. celestial radiance
/// 7. the opaque G-buffer (present only when the scene selects the G-buffer opaque path)
/// 8. opaque lighting of the G-buffer (present with the G-buffer)
/// 9. the game's passes before Opaque
/// 10. opaque
/// 11. the game's passes after Opaque
/// 12. visible contour IDs, then the opaque colour and depth snapshots
/// 13. water, then the water mask
/// 14. transparent alpha (rejected by validation today)
/// 15. the post-water depth
/// 16. the game's passes after the transparent passes
/// 17. the reflection depth pyramid, by level
/// 18. the reflection trace, then reflection mips by level
/// 19. the screen-space pass after transparency
///
/// The game's passes are the only roles whose place is not their position in the `PassSlot`
/// enum; within one place they keep the order of their entries' occurrences.
pub(crate) const fn forward_position(slot: PassSlot) -> (u8, u32) {
    match slot {
        PassSlot::ShadowCasters(layer) => (0, layer),
        PassSlot::DepthNormalPrepass => (1, 0),
        PassSlot::DepthPyramid(level) => (2, level),
        PassSlot::OcclusionMetadata(level) => (3, level),
        PassSlot::Ssao => (4, 0),
        PassSlot::SsaoBlurHorizontal => (5, 0),
        PassSlot::SsaoBlur => (6, 0),
        PassSlot::Celestial => (7, 0),
        PassSlot::OpaqueGbuffer => (8, 0),
        PassSlot::OpaqueLighting => (9, 0),
        PassSlot::Game(GamePlace::BeforeOpaque) => (10, 0),
        PassSlot::Opaque => (11, 0),
        PassSlot::Game(GamePlace::AfterOpaque) => (12, 0),
        PassSlot::ContourIds => (13, 0),
        PassSlot::AfterOpaqueSnapshot => (13, 1),
        PassSlot::AfterOpaqueDepth => (14, 0),
        PassSlot::TransparentWater => (15, 0),
        PassSlot::TransparentWaterMask => (16, 0),
        PassSlot::TransparentAlpha => (17, 0),
        PassSlot::AfterWaterDepth => (18, 0),
        PassSlot::Game(GamePlace::AfterTransparent) => (19, 0),
        PassSlot::ReflectionDepthPyramid(level) => (20, level),
        PassSlot::ReflectionTrace => (21, 0),
        PassSlot::ReflectionMip(level) => (22, level),
        PassSlot::ScreenSpaceAfterTransparent => (23, 0),
    }
}

/// Puts the collected passes of one forward frame into execution order. Entries of the same
/// role keep their occurrence order.
pub(super) fn forward_order(passes: &mut [PassDeclaration]) {
    passes.sort_by_key(|pass| (forward_position(pass.slot), pass.occurrence));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One of every role, with several levels of the indexed roles.
    fn every_role() -> Vec<PassSlot> {
        let mut slots = Vec::new();
        slots.extend((0..4).map(PassSlot::ShadowCasters));
        slots.push(PassSlot::DepthNormalPrepass);
        slots.extend((0..3).map(PassSlot::DepthPyramid));
        slots.extend((0..3).map(PassSlot::OcclusionMetadata));
        slots.extend([
            PassSlot::Ssao,
            PassSlot::SsaoBlurHorizontal,
            PassSlot::SsaoBlur,
            PassSlot::Celestial,
            PassSlot::OpaqueGbuffer,
            PassSlot::OpaqueLighting,
            PassSlot::Opaque,
            PassSlot::ContourIds,
            PassSlot::AfterOpaqueSnapshot,
            PassSlot::AfterOpaqueDepth,
            PassSlot::TransparentWater,
            PassSlot::TransparentWaterMask,
            PassSlot::TransparentAlpha,
            PassSlot::AfterWaterDepth,
        ]);
        slots.extend((0..3).map(PassSlot::ReflectionDepthPyramid));
        slots.push(PassSlot::ReflectionTrace);
        slots.extend((0..3).map(PassSlot::ReflectionMip));
        slots.push(PassSlot::ScreenSpaceAfterTransparent);
        slots
    }

    fn ordered(slots: &[PassSlot]) -> Vec<PassSlot> {
        let mut passes: Vec<_> = slots
            .iter()
            .map(|slot| PassDeclaration::new(*slot))
            .collect();
        forward_order(&mut passes);
        passes.iter().map(|pass| pass.slot).collect()
    }

    #[test]
    fn the_forward_sequence_is_the_one_documented_above() {
        use PassSlot::*;
        let expected = [
            ShadowCasters(0),
            ShadowCasters(1),
            DepthNormalPrepass,
            DepthPyramid(0),
            DepthPyramid(1),
            OcclusionMetadata(0),
            OcclusionMetadata(1),
            Ssao,
            SsaoBlurHorizontal,
            SsaoBlur,
            Celestial,
            OpaqueGbuffer,
            OpaqueLighting,
            Opaque,
            ContourIds,
            AfterOpaqueSnapshot,
            AfterOpaqueDepth,
            TransparentWater,
            TransparentWaterMask,
            TransparentAlpha,
            AfterWaterDepth,
            ReflectionDepthPyramid(0),
            ReflectionDepthPyramid(1),
            ReflectionTrace,
            ReflectionMip(0),
            ReflectionMip(1),
            ScreenSpaceAfterTransparent,
        ];
        let mut reversed = expected.to_vec();
        reversed.reverse();
        assert_eq!(ordered(&reversed), expected);
    }

    /// The frame used to sort by the `PassSlot` enum; the explicit sequence must not move
    /// any pass relative to that, whatever order the features contributed them in.
    #[test]
    fn every_arrangement_matches_the_order_the_frame_had_when_it_sorted_by_enum() {
        let sorted = {
            let mut slots = every_role();
            slots.sort();
            slots
        };
        let roles = every_role();
        for rotation in 0..roles.len() {
            let mut arranged = roles.clone();
            arranged.rotate_left(rotation);
            assert_eq!(ordered(&arranged), sorted, "rotated by {rotation}");
            arranged.reverse();
            assert_eq!(
                ordered(&arranged),
                sorted,
                "rotated by {rotation}, reversed"
            );
        }
    }

    /// Callers that place a pass relative to a role compare positions, not the enum.
    #[test]
    fn positions_compare_the_way_the_enum_did() {
        let roles = every_role();
        for a in &roles {
            for b in &roles {
                assert_eq!(
                    forward_position(*a).cmp(&forward_position(*b)),
                    a.cmp(b),
                    "{a:?} against {b:?}"
                );
            }
        }
    }

    #[test]
    fn a_role_that_appears_twice_keeps_its_occurrence_order() {
        let mut passes = vec![
            PassDeclaration::nth(PassSlot::Opaque, 1),
            PassDeclaration::new(PassSlot::TransparentWater),
            PassDeclaration::nth(PassSlot::Opaque, 0),
        ];
        forward_order(&mut passes);
        assert_eq!(
            passes
                .iter()
                .map(PassDeclaration::entry)
                .collect::<Vec<_>>(),
            [
                worldrunner_render_post::PassEntry::nth(PassSlot::Opaque, 0),
                worldrunner_render_post::PassEntry::nth(PassSlot::Opaque, 1),
                worldrunner_render_post::PassEntry::new(PassSlot::TransparentWater),
            ]
        );
    }

    /// The game's places are not where the enum puts them: they sit around Opaque, after the
    /// transparent passes and before the screen-space tail, and keep occurrence order.
    #[test]
    fn the_games_places_sit_around_opaque_the_transparent_passes_and_the_screen_space_tail() {
        use worldrunner_render_post::PassEntry;
        let game = |place, occurrence| PassEntry::nth(PassSlot::Game(place), occurrence);
        let stock = |slot| PassEntry::new(slot);
        let mut passes: Vec<_> = [
            game(GamePlace::AfterTransparent, 0),
            stock(PassSlot::ScreenSpaceAfterTransparent),
            game(GamePlace::AfterOpaque, 0),
            stock(PassSlot::ContourIds),
            stock(PassSlot::Opaque),
            game(GamePlace::BeforeOpaque, 1),
            stock(PassSlot::ReflectionDepthPyramid(0)),
            game(GamePlace::BeforeOpaque, 0),
            stock(PassSlot::OpaqueLighting),
            stock(PassSlot::AfterWaterDepth),
        ]
        .into_iter()
        .map(|entry| PassDeclaration::nth(entry.slot(), entry.occurrence()))
        .collect();
        forward_order(&mut passes);
        assert_eq!(
            passes
                .iter()
                .map(PassDeclaration::entry)
                .collect::<Vec<_>>(),
            [
                stock(PassSlot::OpaqueLighting),
                game(GamePlace::BeforeOpaque, 0),
                game(GamePlace::BeforeOpaque, 1),
                stock(PassSlot::Opaque),
                game(GamePlace::AfterOpaque, 0),
                stock(PassSlot::ContourIds),
                stock(PassSlot::AfterWaterDepth),
                game(GamePlace::AfterTransparent, 0),
                stock(PassSlot::ReflectionDepthPyramid(0)),
                stock(PassSlot::ScreenSpaceAfterTransparent),
            ]
        );
    }

    #[test]
    fn contours_follow_all_opaque_occurrences_before_the_snapshot() {
        use worldrunner_render_post::PassEntry;
        let mut passes = vec![
            PassDeclaration::new(PassSlot::AfterOpaqueSnapshot),
            PassDeclaration::nth(PassSlot::Opaque, 7),
            PassDeclaration::nth(PassSlot::ContourIds, 1),
            PassDeclaration::new(PassSlot::Opaque),
            PassDeclaration::new(PassSlot::ContourIds),
        ];
        forward_order(&mut passes);
        assert_eq!(
            passes
                .iter()
                .map(PassDeclaration::entry)
                .collect::<Vec<_>>(),
            [
                PassEntry::new(PassSlot::Opaque),
                PassEntry::nth(PassSlot::Opaque, 7),
                PassEntry::new(PassSlot::ContourIds),
                PassEntry::nth(PassSlot::ContourIds, 1),
                PassEntry::new(PassSlot::AfterOpaqueSnapshot),
            ]
        );
    }
}
