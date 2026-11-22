//! A draw that names a material reads the material as it stood when the script recorded the draw,
//! whatever the script changes on the material afterwards. A host replays the frame's work from
//! the snapshot taken at the flush, so the snapshot has to hold the material as each record saw
//! it, for blits, mesh draws and procedural draws and for every kind of material change.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode, encode};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, DrawTopology, GraphicsSnapshot, MaterialEdit, MaterialId, OffscreenWork,
    PropertyValue, RenderTextureId, ShaderId, TargetRef, TextureRef, TextureSlot,
    WorldRenderTexture, WorldSampler, WorldShader,
};
use worldrunner_world_simulation::{World, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn shader(world: &mut World, name: &str) -> ShaderId {
    world
        .create_shader(WorldShader {
            name: name.into(),
            source: None,
        })
        .unwrap()
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

fn texture(name: &str, texture: RenderTextureId) -> MaterialEdit {
    MaterialEdit::Property(PropertyValue::Texture(
        name.into(),
        TextureSlot {
            texture: TextureRef::Render(texture),
            ..TextureSlot::default()
        },
    ))
}

fn blit(source: RenderTextureId, target: RenderTextureId, material: MaterialId) -> OffscreenWork {
    OffscreenWork::Blit {
        source: TextureRef::Render(source),
        target: TargetRef::Render(target),
        material: Some(material),
        pass: 0,
    }
}

fn draw_mesh(material: MaterialId) -> OffscreenWork {
    OffscreenWork::DrawMesh {
        mesh: 1 << 41,
        matrix: [0.0; 16],
        material,
        submesh: 0,
        pass: 0,
        block: None,
        instances: 1,
    }
}

fn draw_procedural(material: MaterialId) -> OffscreenWork {
    OffscreenWork::DrawProcedural {
        matrix: [0.0; 16],
        material,
        pass: 0,
        topology: DrawTopology::TriangleList,
        vertex_count: 3,
        instance_count: 1,
    }
}

/// The material a draw record names, which the snapshot holds as the draw saw it.
fn named(work: &OffscreenWork) -> MaterialId {
    match work {
        OffscreenWork::Blit {
            material: Some(material),
            ..
        }
        | OffscreenWork::DrawMesh { material, .. }
        | OffscreenWork::DrawProcedural { material, .. } => *material,
        other => panic!("{other:?} names no material"),
    }
}

fn draws(snapshot: &GraphicsSnapshot) -> Vec<&OffscreenWork> {
    snapshot
        .work
        .iter()
        .filter(|work| {
            matches!(
                work,
                OffscreenWork::Blit { .. }
                    | OffscreenWork::DrawMesh { .. }
                    | OffscreenWork::DrawProcedural { .. }
            )
        })
        .collect()
}

#[test]
fn two_blits_of_one_material_with_a_texture_set_between_them_each_see_their_own() {
    let mut world = world();
    let shader = shader(&mut world, "Terrain/PostProcess");
    let material = world.create_material(shader).unwrap();
    let (first, second) = (render_texture(&mut world), render_texture(&mut world));
    let (target_a, target_b) = (render_texture(&mut world), render_texture(&mut world));
    world
        .edit_material(material, texture("_PrimaryTex", first))
        .unwrap();
    world
        .record_offscreen(blit(first, target_a, material))
        .unwrap();
    world
        .edit_material(material, texture("_PrimaryTex", second))
        .unwrap();
    world
        .record_offscreen(blit(second, target_b, material))
        .unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    let seen: Vec<_> = draws(&snapshot)
        .into_iter()
        .map(|work| {
            snapshot.materials[&named(work)]
                .properties
                .texture("_PrimaryTex")
        })
        .map(|slot| slot.unwrap().texture)
        .collect();
    assert_eq!(
        seen,
        [TextureRef::Render(first), TextureRef::Render(second)]
    );
}

/// One change of every kind a material takes, each different from what `base` holds.
fn changes(world: &mut World, other_shader: ShaderId) -> Vec<(&'static str, MaterialEdit)> {
    let buffer: ComputeBufferId = world.create_compute_buffer(4, 4, 0).unwrap();
    let sampled = render_texture(world);
    vec![
        (
            "float",
            MaterialEdit::Property(PropertyValue::Float("_Strength".into(), 0.75)),
        ),
        (
            "int",
            MaterialEdit::Property(PropertyValue::Int("_Mode".into(), 3)),
        ),
        (
            "vector",
            MaterialEdit::Property(PropertyValue::Vector("_Tint".into(), [0.1, 0.2, 0.3, 0.4])),
        ),
        (
            "matrix",
            MaterialEdit::Property(PropertyValue::Matrix("_Frame".into(), [2.0; 16])),
        ),
        ("texture", texture("_PrimaryTex", sampled)),
        (
            "vector array",
            MaterialEdit::Property(PropertyValue::VectorArray(
                "_Points".into(),
                vec![[1.0, 2.0, 3.0, 4.0]],
            )),
        ),
        (
            "matrix array",
            MaterialEdit::Property(PropertyValue::MatrixArray(
                "_Frames".into(),
                vec![[3.0; 16]],
            )),
        ),
        (
            "keyword",
            MaterialEdit::Keyword {
                name: "_FOG".into(),
                enabled: true,
            },
        ),
        (
            "pass",
            MaterialEdit::Pass {
                name: "Shadow".into(),
                enabled: false,
            },
        ),
        ("render queue", MaterialEdit::RenderQueue(3000)),
        (
            "buffer",
            MaterialEdit::Buffer {
                name: "_Instances".into(),
                buffer: Some(buffer),
            },
        ),
        ("shader", MaterialEdit::Shader(other_shader)),
        ("clear", MaterialEdit::ClearProperties),
    ]
}

type Record = fn(MaterialId, RenderTextureId, RenderTextureId) -> OffscreenWork;

fn kinds() -> [(&'static str, Record); 3] {
    [
        ("blit", |material, source, target| {
            blit(source, target, material)
        }),
        ("mesh", |material, _, _| draw_mesh(material)),
        ("procedural", |material, _, _| draw_procedural(material)),
    ]
}

#[test]
fn every_kind_of_draw_sees_the_material_as_it_was_for_every_kind_of_change() {
    let mut world = world();
    let shader = shader(&mut world, "Custom/Draw");
    let other_shader = self::shader(&mut world, "Custom/Other");
    let (source, target) = (render_texture(&mut world), render_texture(&mut world));
    let mut checked = 0;
    for (kind, record) in kinds() {
        for (change, edit) in changes(&mut world, other_shader) {
            let material = world.create_material(shader).unwrap();
            // A property the clear has something to remove.
            world
                .edit_material(
                    material,
                    MaterialEdit::Property(PropertyValue::Float("_Strength".into(), 0.25)),
                )
                .unwrap();
            world
                .record_offscreen(record(material, source, target))
                .unwrap();
            let before = world.material(material).unwrap().clone();
            world.edit_material(material, edit).unwrap();
            world
                .record_offscreen(record(material, source, target))
                .unwrap();
            let after = world.material(material).unwrap().clone();
            assert_ne!(before, after, "{kind}: the {change} change edits nothing");
            let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
            let held: Vec<_> = draws(&snapshot)
                .into_iter()
                .map(|work| snapshot.materials.get(&named(work)).cloned())
                .collect();
            assert_eq!(
                held,
                [Some(before), Some(after)],
                "{kind} draws around a {change} change"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 39);
}

#[test]
fn a_draw_keeps_the_material_it_saw_when_the_script_edits_it_after_the_last_draw() {
    let mut world = world();
    let shader = shader(&mut world, "Custom/Draw");
    let material = world.create_material(shader).unwrap();
    let (source, target) = (render_texture(&mut world), render_texture(&mut world));
    let (red, green) = (render_texture(&mut world), render_texture(&mut world));
    world
        .edit_material(material, texture("_PrimaryTex", red))
        .unwrap();
    world
        .record_offscreen(blit(source, target, material))
        .unwrap();
    world
        .edit_material(material, texture("_PrimaryTex", green))
        .unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    let work = draws(&snapshot);
    let slot = snapshot.materials[&named(work[0])]
        .properties
        .texture("_PrimaryTex")
        .unwrap();
    assert_eq!(slot.texture, TextureRef::Render(red));
}

#[test]
fn a_material_nobody_edits_between_draws_stays_one_material_under_its_own_id() {
    let mut world = world();
    let shader = shader(&mut world, "Custom/Draw");
    let material = world.create_material(shader).unwrap();
    let (source, target) = (render_texture(&mut world), render_texture(&mut world));
    world
        .edit_material(material, texture("_PrimaryTex", source))
        .unwrap();
    for _ in 0..3 {
        world
            .record_offscreen(blit(source, target, material))
            .unwrap();
    }
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert!(draws(&snapshot).iter().all(|work| named(work) == material));
    let materials: Vec<_> = snapshot.materials.keys().copied().collect();
    assert_eq!(materials, [material]);
}

#[test]
fn a_material_edited_in_one_frame_is_whole_again_in_the_next() {
    let mut world = world();
    let shader = shader(&mut world, "Custom/Draw");
    let material = world.create_material(shader).unwrap();
    let (source, target) = (render_texture(&mut world), render_texture(&mut world));
    world
        .edit_material(material, texture("_PrimaryTex", source))
        .unwrap();
    world
        .record_offscreen(blit(source, target, material))
        .unwrap();
    world
        .edit_material(material, texture("_PrimaryTex", target))
        .unwrap();
    world
        .record_offscreen(blit(source, target, material))
        .unwrap();
    let first = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(first.materials.len(), 2, "two states the draws saw");
    world
        .record_offscreen(blit(source, target, material))
        .unwrap();
    let next = GraphicsSnapshot::take(&mut world).unwrap();
    let materials: Vec<_> = next.materials.keys().copied().collect();
    assert_eq!(materials, [material], "nothing of the last frame remains");
}

#[test]
fn a_material_destroyed_after_a_draw_is_stale_for_the_draw_as_before() {
    let mut world = world();
    let shader = shader(&mut world, "Custom/Draw");
    let material = world.create_material(shader).unwrap();
    let (source, target) = (render_texture(&mut world), render_texture(&mut world));
    world
        .record_offscreen(blit(source, target, material))
        .unwrap();
    world
        .edit_material(material, texture("_PrimaryTex", source))
        .unwrap();
    world
        .record_offscreen(blit(source, target, material))
        .unwrap();
    world.destroy_material(material).unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert!(draws(&snapshot).iter().all(|work| named(work) == material));
    assert!(snapshot.materials.is_empty());
}

#[test]
fn a_snapshot_with_a_material_as_each_draw_saw_it_round_trips_through_the_wire() {
    let mut world = world();
    let shader = shader(&mut world, "Custom/Draw");
    let material = world.create_material(shader).unwrap();
    let (source, target) = (render_texture(&mut world), render_texture(&mut world));
    for texture_of in [source, target] {
        world
            .edit_material(material, texture("_PrimaryTex", texture_of))
            .unwrap();
        world
            .record_offscreen(blit(source, target, material))
            .unwrap();
    }
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(decode(&encode(&snapshot).unwrap()).unwrap(), snapshot);
}

#[test]
fn take_offscreen_returns_the_copies_that_the_records_name() {
    let mut world = world();
    let shader = shader(&mut world, "Terrain/PostProcess");
    let material = world.create_material(shader).unwrap();
    let (first, second) = (render_texture(&mut world), render_texture(&mut world));
    let target = render_texture(&mut world);
    world
        .edit_material(material, texture("_PrimaryTex", first))
        .unwrap();
    world
        .record_offscreen(blit(first, target, material))
        .unwrap();
    world
        .edit_material(material, texture("_PrimaryTex", second))
        .unwrap();
    world
        .record_offscreen(blit(second, target, material))
        .unwrap();
    let (work, copies, _) = world.take_offscreen();
    assert_eq!(work.len(), 2);
    assert_eq!(
        named(&work[1]),
        material,
        "the last blit names the material"
    );
    let copy = named(&work[0]);
    assert_ne!(copy, material);
    let held = copies[&copy].properties.texture("_PrimaryTex").unwrap();
    assert_eq!(held.texture, TextureRef::Render(first));
    assert_eq!(copies.len(), 1, "the live material is not a copy");
    assert!(
        world.take_offscreen().1.is_empty(),
        "a frame's copies are taken once"
    );
}
