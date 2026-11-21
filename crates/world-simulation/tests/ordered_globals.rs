//! Direct writes to the global shader state keep their place among the frame's offscreen work.
//! A host replays the work in order from the state at the flush, so a value a script sets before
//! a record and changes after it has to reach the host as an ordered record, not only as the last
//! value the state holds.
use worldrunner_world_simulation::graphics::{
    MAX_OFFSCREEN_WORK, OffscreenWork, PropertyValue, RenderTextureId, TargetRef, TextureRef,
    TextureSlot, WorldRenderTexture, WorldSampler,
};
use worldrunner_world_simulation::{TimeSpan, World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn render_texture(world: &mut World) -> RenderTextureId {
    world
        .create_render_texture(WorldRenderTexture {
            width: 8,
            height: 8,
            format: 8,
            depth_bits: 0,
            mip_count: 1,
            srgb: false,
            dimension: 2,
            volume_depth: 1,
            auto_generate_mips: false,
            sampler: WorldSampler::default(),
            generation: 0,
        })
        .unwrap()
}

fn slot(texture: TextureRef) -> TextureSlot {
    TextureSlot {
        texture,
        ..TextureSlot::default()
    }
}

fn set_texture(world: &mut World, name: &str, texture: TextureRef) {
    world
        .set_global_property(PropertyValue::Texture(name.into(), slot(texture)))
        .unwrap();
}

/// A record that needs no resource but a live render texture to draw into.
fn blit(target: RenderTextureId) -> OffscreenWork {
    OffscreenWork::Blit {
        source: TextureRef::None,
        target: TargetRef::Render(target),
        material: None,
        pass: -1,
    }
}

fn set_global(value: PropertyValue) -> OffscreenWork {
    OffscreenWork::SetGlobal(value)
}

fn keyword(name: &str, enabled: bool) -> OffscreenWork {
    OffscreenWork::SetGlobalKeyword {
        name: name.into(),
        enabled,
    }
}

#[test]
fn a_global_texture_set_and_cleared_around_two_records_reaches_each_in_order() {
    let mut world = world();
    let target = render_texture(&mut world);
    let source = render_texture(&mut world);
    set_texture(&mut world, "_Last", TextureRef::Render(source));
    world.record_offscreen(blit(target)).unwrap();
    set_texture(&mut world, "_Last", TextureRef::None);
    world.record_offscreen(blit(target)).unwrap();
    assert_eq!(
        world.take_offscreen(),
        vec![
            set_global(PropertyValue::Texture(
                "_Last".into(),
                slot(TextureRef::Render(source))
            )),
            blit(target),
            set_global(PropertyValue::Texture(
                "_Last".into(),
                slot(TextureRef::None)
            )),
            blit(target),
        ]
    );
}

#[test]
fn a_direct_write_still_changes_the_global_state_at_once() {
    let mut world = world();
    let target = render_texture(&mut world);
    world
        .set_global_property(PropertyValue::Float("_Gain".into(), 3.0))
        .unwrap();
    world.set_global_keyword("_BOOST", true).unwrap();
    world.record_offscreen(blit(target)).unwrap();
    let state = world.global_shader_state();
    assert_eq!(state.properties.float("_Gain"), Some(3.0));
    assert_eq!(state.keywords, vec!["_BOOST".to_owned()]);
    assert_eq!(state.generation, 2);
}

#[test]
fn a_global_keyword_enabled_and_disabled_around_two_records_reaches_each_in_order() {
    let mut world = world();
    let target = render_texture(&mut world);
    world.set_global_keyword("_PVT_NORMAL", true).unwrap();
    world.record_offscreen(blit(target)).unwrap();
    world.set_global_keyword("_PVT_NORMAL", false).unwrap();
    world.record_offscreen(blit(target)).unwrap();
    assert_eq!(
        world.take_offscreen(),
        vec![
            keyword("_PVT_NORMAL", true),
            blit(target),
            keyword("_PVT_NORMAL", false),
            blit(target),
        ]
    );
}

#[test]
fn only_the_names_written_since_the_previous_record_are_emitted_with_their_last_value() {
    let mut world = world();
    let target = render_texture(&mut world);
    for value in [1.0, 2.0, 3.0] {
        world
            .set_global_property(PropertyValue::Float("_Gain".into(), value))
            .unwrap();
    }
    world.set_global_keyword("_BOOST", true).unwrap();
    world.set_global_keyword("_BOOST", false).unwrap();
    world.record_offscreen(blit(target)).unwrap();
    world.record_offscreen(blit(target)).unwrap();
    world
        .set_global_property(PropertyValue::Int("_Mode".into(), 4))
        .unwrap();
    world.record_offscreen(blit(target)).unwrap();
    assert_eq!(
        world.take_offscreen(),
        vec![
            set_global(PropertyValue::Float("_Gain".into(), 3.0)),
            keyword("_BOOST", false),
            blit(target),
            blit(target),
            set_global(PropertyValue::Int("_Mode".into(), 4)),
            blit(target),
        ]
    );
}

#[test]
fn every_kind_of_global_value_is_ordered() {
    let mut world = world();
    let target = render_texture(&mut world);
    let matrix = std::array::from_fn(|index| f32::from(u8::try_from(index).unwrap()));
    let values = [
        PropertyValue::Float("_F".into(), 0.5),
        PropertyValue::Int("_I".into(), -7),
        PropertyValue::Vector("_V".into(), [1.0, 2.0, 3.0, 4.0]),
        PropertyValue::Matrix("_M".into(), matrix),
        PropertyValue::VectorArray("_VA".into(), vec![[1.0; 4], [2.0; 4]]),
        PropertyValue::MatrixArray("_MA".into(), vec![matrix]),
        PropertyValue::Texture("_T".into(), slot(TextureRef::Asset(9))),
    ];
    for value in values.clone() {
        world.set_global_property(value).unwrap();
    }
    world.record_offscreen(blit(target)).unwrap();
    let work = world.take_offscreen();
    assert_eq!(work.len(), values.len() + 1);
    assert_eq!(work.last(), Some(&blit(target)));
    for value in values {
        assert!(work.contains(&set_global(value.clone())), "{value:?}");
    }
}

#[test]
fn a_name_set_as_two_kinds_is_ordered_as_two_values() {
    let mut world = world();
    let target = render_texture(&mut world);
    world
        .set_global_property(PropertyValue::Float("_Same".into(), 1.0))
        .unwrap();
    world
        .set_global_property(PropertyValue::Vector("_Same".into(), [2.0; 4]))
        .unwrap();
    world.record_offscreen(blit(target)).unwrap();
    let work = world.take_offscreen();
    assert!(work.contains(&set_global(PropertyValue::Float("_Same".into(), 1.0))));
    assert!(work.contains(&set_global(PropertyValue::Vector("_Same".into(), [2.0; 4]))));
}

#[test]
fn a_write_after_the_last_record_is_not_ordered_and_does_not_carry_into_the_next_frame() {
    let mut world = world();
    let target = render_texture(&mut world);
    world.record_offscreen(blit(target)).unwrap();
    world.set_global_keyword("_LATE", true).unwrap();
    assert_eq!(world.take_offscreen(), vec![blit(target)]);
    world.record_offscreen(blit(target)).unwrap();
    assert_eq!(world.take_offscreen(), vec![blit(target)]);
}

#[test]
fn a_write_in_a_frame_nobody_took_does_not_carry_into_the_next_frame() {
    let mut world = world();
    let target = render_texture(&mut world);
    world
        .set_global_property(PropertyValue::Float("_Old".into(), 1.0))
        .unwrap();
    world.record_offscreen(blit(target)).unwrap();
    world
        .advance(TimeSpan::from_nanos(16_000_000).unwrap())
        .unwrap();
    world.record_offscreen(blit(target)).unwrap();
    assert_eq!(world.take_offscreen(), vec![blit(target)]);
}

#[test]
fn a_recorded_global_follows_the_direct_writes_made_before_it() {
    let mut world = world();
    let target = render_texture(&mut world);
    world
        .set_global_property(PropertyValue::Float("_Direct".into(), 1.0))
        .unwrap();
    world
        .record_offscreen(set_global(PropertyValue::Float("_Recorded".into(), 2.0)))
        .unwrap();
    world.record_offscreen(blit(target)).unwrap();
    assert_eq!(
        world.take_offscreen(),
        vec![
            set_global(PropertyValue::Float("_Direct".into(), 1.0)),
            set_global(PropertyValue::Float("_Recorded".into(), 2.0)),
            blit(target),
        ]
    );
}

#[test]
fn a_refused_record_leaves_the_direct_writes_for_the_next_one() {
    let mut world = world();
    let target = render_texture(&mut world);
    world
        .set_global_property(PropertyValue::Float("_Gain".into(), 1.0))
        .unwrap();
    let stale = {
        let doomed = render_texture(&mut world);
        world.destroy_render_texture(doomed).unwrap();
        doomed
    };
    assert!(world.record_offscreen(blit(stale)).is_err());
    assert!(world.offscreen_work().is_empty());
    world.record_offscreen(blit(target)).unwrap();
    assert_eq!(
        world.take_offscreen(),
        vec![
            set_global(PropertyValue::Float("_Gain".into(), 1.0)),
            blit(target),
        ]
    );
}

#[test]
fn a_global_texture_of_a_render_texture_destroyed_before_the_record_reads_as_unsent() {
    let mut world = world();
    let target = render_texture(&mut world);
    let doomed = render_texture(&mut world);
    set_texture(&mut world, "_Last", TextureRef::Render(doomed));
    world.destroy_render_texture(doomed).unwrap();
    world.record_offscreen(blit(target)).unwrap();
    assert_eq!(
        world.take_offscreen(),
        vec![
            set_global(PropertyValue::Texture(
                "_Last".into(),
                slot(TextureRef::Unsent)
            )),
            blit(target),
        ]
    );
}

#[test]
fn a_record_that_would_pass_the_frame_cap_with_its_globals_is_refused_whole() {
    let mut world = world();
    let target = render_texture(&mut world);
    for _ in 0..MAX_OFFSCREEN_WORK - 1 {
        world.record_offscreen(blit(target)).unwrap();
    }
    world
        .set_global_property(PropertyValue::Float("_Gain".into(), 1.0))
        .unwrap();
    let refused = world.record_offscreen(blit(target));
    assert!(matches!(refused, Err(WorldError::CapacityExceeded(_))));
    assert_eq!(world.offscreen_work().len(), MAX_OFFSCREEN_WORK - 1);
}
