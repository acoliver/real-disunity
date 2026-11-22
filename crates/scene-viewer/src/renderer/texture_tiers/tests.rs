use super::*;
use worldrunner_render_gpu::census::{MemoryCensus, ResourceClass};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace, TextureHandle};

fn prepared(reducible: &[u32]) -> PreparedScene {
    let base = super::super::retained_tests::source();
    let bytes = (0..7).map(|level| 4 * (64_usize >> level).pow(2)).sum();
    let texture = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 64,
        height: 64,
        mips: 7,
        bytes: vec![128; bytes].into(),
    };
    let scene = base
        .with_resources(
            worldrunner_render_plan::SceneResources::new(
                vec![texture.clone(), texture],
                base.resources().materials().to_vec(),
            )
            .unwrap(),
            base.geometry().clone(),
            base.effects().cloned(),
        )
        .unwrap()
        .with_reducible_textures(
            reducible
                .iter()
                .copied()
                .map(TextureHandle::from_raw)
                .collect(),
        )
        .unwrap();
    PreparedScene::new(&scene).unwrap()
}

fn tiers() -> TextureTiers {
    TextureTiers {
        policy: Some(TextureTierPolicy {
            budget_fraction_micro: 1_000_000,
            max_dropped_levels: 2,
        }),
        chosen: None,
        memory_override: None,
    }
}

#[test]
fn new_fixed_textures_consume_the_budget_before_reducible_textures() {
    let choice = tiers()
        .prepare(&mut prepared(&[0]), Some(30_000), CensusSnapshot::default())
        .unwrap();
    assert_eq!(choice.dropped_levels, 1);
    assert_eq!(choice.authored_bytes, 21_844);
    assert_eq!(choice.kept_bytes, 5_460);
    assert_eq!(choice.saved_bytes(), 16_384);
    assert!(choice.fits);
}

#[test]
fn previous_textures_stay_counted_while_a_replacement_uploads() {
    let census = MemoryCensus::new();
    let _previous = census.track(ResourceClass::Texture, 30_000);
    let choice = tiers()
        .prepare(&mut prepared(&[0, 1]), Some(60_000), census.snapshot())
        .unwrap();
    assert_eq!(choice.dropped_levels, 1);
    assert!(choice.fits);
}

#[test]
fn unknown_memory_keeps_authored_levels_and_disabled_policy_has_no_choice() {
    let mut source = prepared(&[0]);
    let choice = tiers()
        .prepare(&mut source, None, CensusSnapshot::default())
        .unwrap();
    assert_eq!(choice.dropped_levels, 0);
    assert_eq!(choice.saved_bytes(), 0);
    assert_eq!(
        TextureTiers::default().prepare(&mut source, Some(1), CensusSnapshot::default()),
        None
    );
}

#[test]
fn preparation_does_not_publish_a_pending_generations_statistics() {
    let state = tiers();
    let pending = state.prepare(
        &mut prepared(&[0, 1]),
        Some(30_000),
        CensusSnapshot::default(),
    );
    assert!(pending.is_some());
    assert_eq!(state.chosen, None);
}

#[test]
fn texture_edits_with_a_policy_queue_a_complete_tier_recalculation() {
    use worldrunner_render_plan::SceneAppend;
    let source = prepared(&[0]);
    let texture = source.scene().resources().textures()[0].clone();
    let updates = [
        crate::RetainedUpdate::ReplaceTexture {
            handle: TextureHandle::from_raw(0),
            texture: texture.clone(),
        },
        crate::RetainedUpdate::AppendResources(SceneAppend {
            textures: vec![texture],
            materials: vec![],
            meshes: vec![],
        }),
    ];
    for update in updates {
        let mut options = crate::tests::options();
        options.texture_tiers = tiers().policy;
        let mut host = crate::RetainedHost::new(source.scene().clone(), options).unwrap();
        host.apply(update).unwrap();
        let pending = host.readiness.queue.back().unwrap();
        assert!(
            pending.scene.is_some(),
            "texture edits must reselect the tier"
        );
        assert!(pending.edit.is_none());
    }
}

struct Driver;
impl crate::RetainedDriver for Driver {
    fn update(
        &mut self,
        _: &mut crate::RetainedHost,
        _: &crate::RetainedTick,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn frame_statistics_follow_the_committed_texture_generation() {
    use std::time::{Duration, Instant};
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut options = crate::tests::options();
    options.size = [64, 64];
    options.texture_tiers = tiers().policy;
    let host = crate::RetainedHost::new(prepared(&[0]).scene().clone(), options).unwrap();
    let mut driver = Driver;
    let mut run = host.windowless(&mut driver, None).unwrap();
    run.frame().unwrap();
    let initial = run.host().statistics().unwrap().texture_tier.unwrap();
    assert_eq!(initial.authored_bytes, 21_844);
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    run.host_mut().readiness.preparation_hook = Some(Box::new(move || {
        receive.recv_timeout(Duration::from_secs(10)).unwrap();
    }));
    let replacement = super::super::retained_tests::source()
        .resources()
        .textures()[0]
        .clone();
    run.host_mut()
        .apply(crate::RetainedUpdate::ReplaceTexture {
            handle: TextureHandle::from_raw(0),
            texture: replacement,
        })
        .unwrap();
    run.frame().unwrap();
    assert_eq!(run.host().statistics().unwrap().generation, 0);
    assert_eq!(run.host().statistics().unwrap().texture_tier, Some(initial));
    send.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while run.host().statistics().unwrap().generation == 0 {
        run.frame().unwrap();
        run.poll().unwrap();
        assert!(Instant::now() < deadline);
    }
    let choice = run.host().statistics().unwrap().texture_tier.unwrap();
    assert_eq!(choice.authored_bytes, 4);
    assert_eq!(choice.kept_bytes, 4);
    assert_eq!(choice.saved_bytes(), 0);
    run.shutdown().unwrap();
}
