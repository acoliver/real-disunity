use super::*;

fn assert_detected(name: &str, cleared: &[Vec<u8>], poison: &[Vec<u8>], side: u32) {
    for (attachment, channels) in [
        (10, [12344.0, -6788.0, 4320.0, -9872.0]),
        (11, [12344.0, -6788.0, 0.0, 0.0]),
    ] {
        let expected: Vec<_> = channels
            .into_iter()
            .flat_map(|v| half::f16::from_f32(v * [1.0, 2.0][side as usize]).to_le_bytes())
            .collect();
        assert!(
            cleared[attachment] != poison[attachment],
            "{name}: comparison must detect attachment={attachment}"
        );
        assert!(
            cleared[attachment]
                .chunks_exact(8)
                .zip(poison[attachment].chunks_exact(8))
                .any(|(zero, loaded)| zero == [0; 8] && loaded == expected),
            "{name}: attachment={attachment} must directly expose the uncovered texel"
        );
    }
}

pub(super) fn assert_poison_corner(cleared: &[Vec<u8>], poison: &[Vec<u8>], sides: usize) {
    assert_eq!(
        &cleared[1][..4],
        &1.0_f32.to_le_bytes(),
        "probe texel must be background"
    );
    let payload = cleared.len() - 2;
    for (index, channels) in [4, 2].into_iter().enumerate() {
        let bytes = channels * 4;
        let layer = 257 * 241 * bytes;
        for side in 0..sides {
            let offset = side * layer;
            assert_eq!(
                &cleared[payload + index][offset..offset + bytes],
                vec![0; bytes]
            );
            let expected: Vec<_> = [12344.0_f32, -6788.0, 4320.0, -9872.0][..channels]
                .iter()
                .flat_map(|v| (v * [1.0, 2.0][side]).to_le_bytes())
                .collect();
            assert_eq!(&poison[payload + index][offset..offset + bytes], expected);
        }
    }
}

#[test]
fn gpu_receiver_poison_comparison_detects_uncovered_loads_in_live_consumers() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = surfaces::fixture();
    for bias in [0.0, 0.012] {
        for (baseline, _) in modes() {
            let depth = baseline.map_or(
                crate::renderer::depth::DepthMode::Reuse,
                ReceiverReference::depth,
            );
            for family in ConsumerFamily::ALL {
                let capture = |poison| {
                    frame(
                        &context,
                        &source,
                        Some(ReceiverReference::UncoveredProbe {
                            depth,
                            family,
                            poison,
                        }),
                        bias,
                    )
                };
                let cleared = capture(false);
                let poisoned = capture(true);
                assert_poison_corner(&cleared, &poisoned, if bias == 0.0 { 1 } else { 2 });
                assert_detected(
                    &format!("{family:?} {depth:?} bias={bias}"),
                    &cleared,
                    &poisoned,
                    family.side(if bias == 0.0 {
                        ReceiverLayout::Front
                    } else {
                        ReceiverLayout::Opposing
                    }),
                );
                assert_eq!(
                    cleared[1], poisoned[1],
                    "wrong consumer load must not change coverage"
                );
                println!("DETECTED_UNCOVERED {family:?} {depth:?} bias={bias} both_textures=true");
            }
        }
    }
}

#[test]
fn gpu_receiver_poison_fixture_has_visible_surfaces_and_backlight() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = surfaces::fixture();
    let cleared = frame(&context, &source, None, 0.012);
    for (index, name) in ["opaque", "masked", "transmitting masked"]
        .into_iter()
        .enumerate()
    {
        let absent = frame(
            &context,
            &surfaces::without_surface(&source, index),
            None,
            0.012,
        );
        assert_ne!(cleared[0], absent[0], "{name} must affect scene colour");
        assert_ne!(cleared[1], absent[1], "{name} must have visible coverage");
    }
    let unlit_back = frame(
        &context,
        &surfaces::without_transmission(&source),
        None,
        0.012,
    );
    assert!(
        cleared[0] != unlit_back[0],
        "ordinary surface backlight must contribute"
    );
    assert!(
        cleared[1] == unlit_back[1],
        "transmission must not change depth"
    );
    let payload = cleared.len() - 2;
    assert!(
        cleared[payload..] == unlit_back[payload..],
        "transmission control must preserve the producer"
    );
}
