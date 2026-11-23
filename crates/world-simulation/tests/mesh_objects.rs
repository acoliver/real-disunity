//! Which mesh renderers a frame lists and what each carries: the renderer must be enabled, on an
//! active object, with a mesh the host holds, on a layer the presented camera admits.
use glam::Vec3;
use worldrunner_world_simulation::graphics::{
    MaterialEdit, MaterialId, PropertyValue, RenderTextureId, ShaderProperties, TextureRef,
    TextureSlot, WorldRenderTexture, WorldSampler, WorldShader,
};
use worldrunner_world_simulation::presentation::{Camera, mesh_objects::extract_mesh_objects};
use worldrunner_world_simulation::{
    ComponentDesc, DrawnMaterial, LocalTransform, MeshFilter, MeshRenderer, ObjectDesc,
    ObjectHandle, World, WorldSettings,
};

const MESH: u64 = (1 << 41) + 5;

fn new_world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

/// A renderer with `slots` material slots, none holding a material.
fn renderer(slots: u32) -> MeshRenderer {
    MeshRenderer {
        material_count: slots,
        ..MeshRenderer::default()
    }
}

/// An object with a filter naming `drawn_mesh` and `renderer`.
fn add_mesh_object(
    world: &mut World,
    drawn_mesh: u64,
    renderer: MeshRenderer,
    desc: ObjectDesc,
) -> ObjectHandle {
    let object = world.create_object(desc).unwrap();
    let filter = MeshFilter {
        mesh: 3,
        drawn_mesh,
    };
    world
        .add_component(object, ComponentDesc::MeshFilter(filter))
        .unwrap();
    world
        .add_component(object, ComponentDesc::MeshRenderer(renderer))
        .unwrap();
    object
}

fn simple(world: &mut World) -> ObjectHandle {
    add_mesh_object(world, MESH, renderer(1), ObjectDesc::default())
}

fn listed(world: &World) -> Vec<ObjectHandle> {
    extract_mesh_objects(world)
        .unwrap()
        .into_iter()
        .map(|o| o.object)
        .collect()
}

fn add_camera(world: &mut World, culling_mask: u32) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    let camera = Camera {
        culling_mask,
        ..Camera::default()
    };
    world
        .add_component(object, ComponentDesc::Camera(camera))
        .unwrap();
}

#[test]
fn an_enabled_renderer_with_a_drawn_mesh_is_listed_with_its_mesh_and_slots() {
    let mut world = new_world();
    let (first, second) = (material(&mut world), material(&mut world));
    set_vector(&mut world, first, "_Color", [0.1, 0.5, 0.25, 1.0]);
    set_vector(&mut world, second, "_Color", [0.2, 0.5, 0.25, 1.0]);
    set_texture(
        &mut world,
        second,
        "_MainTex",
        TextureRef::Asset((1 << 40) + 3),
    );
    let object = object_with(&mut world, with_materials(&[Some(first), Some(second)]));
    let listed = extract_mesh_objects(&world).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].object, listed[0].mesh), (object, MESH));
    assert_eq!(
        listed[0].slots,
        vec![
            colored([0.1, 0.5, 0.25, 1.0]),
            DrawnMaterial {
                color: [0.2, 0.5, 0.25, 1.0],
                texture: (1 << 40) + 3,
            },
        ]
    );
    assert_eq!(listed[0].materials, vec![Some(first), Some(second)]);
}

#[test]
fn a_renderer_without_a_drawn_mesh_or_a_filter_is_left_out() {
    let mut world = new_world();
    add_mesh_object(&mut world, 0, renderer(1), ObjectDesc::default());
    let bare = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(bare, ComponentDesc::MeshRenderer(MeshRenderer::default()))
        .unwrap();
    assert!(listed(&world).is_empty());
}

#[test]
fn a_disabled_renderer_or_one_forced_off_is_left_out_and_the_others_stay() {
    let mut world = new_world();
    let mut disabled = renderer(1);
    disabled.switches.enabled = false;
    let mut forced_off = renderer(1);
    forced_off.switches.force_rendering_off = true;
    add_mesh_object(&mut world, MESH, disabled, ObjectDesc::default());
    add_mesh_object(&mut world, MESH, forced_off, ObjectDesc::default());
    let shown = simple(&mut world);
    assert_eq!(listed(&world), vec![shown]);
}

#[test]
fn a_renderer_that_only_casts_shadows_is_left_out_and_the_other_modes_stay() {
    let mut world = new_world();
    let mut kept = Vec::new();
    for mode in [0, 1, 2, 3] {
        let mut renderer = renderer(1);
        renderer.shadow_casting_mode = mode;
        let object = add_mesh_object(&mut world, MESH, renderer, ObjectDesc::default());
        if mode != 3 {
            kept.push(object);
        }
    }
    assert_eq!(listed(&world), kept);
}

#[test]
fn an_inactive_object_or_a_child_of_one_is_left_out_until_it_is_activated() {
    let mut world = new_world();
    let object = simple(&mut world);
    world.set_active(object, false).unwrap();
    assert!(listed(&world).is_empty());
    world.set_active(object, true).unwrap();
    assert_eq!(listed(&world), vec![object]);
}

#[test]
fn the_cameras_culling_mask_decides_by_layer() {
    let mut world = new_world();
    let on_layer_five = ObjectDesc {
        layer: 5,
        ..ObjectDesc::default()
    };
    let five = add_mesh_object(&mut world, MESH, renderer(1), on_layer_five);
    let zero = simple(&mut world);
    assert_eq!(listed(&world), vec![five, zero], "no camera culls nothing");
    add_camera(&mut world, 1 << 5);
    assert_eq!(listed(&world), vec![five]);
}

#[test]
fn an_empty_slot_draws_white_with_texture_zero_and_there_is_one_slot_per_material() {
    let mut world = new_world();
    add_mesh_object(&mut world, MESH, renderer(3), ObjectDesc::default());
    add_mesh_object(&mut world, MESH, renderer(0), ObjectDesc::default());
    let listed = extract_mesh_objects(&world).unwrap();
    assert_eq!(listed[0].slots, vec![WHITE; 3]);
    assert!(listed[1].slots.is_empty());
}

#[test]
fn the_matrix_places_the_object_in_the_view_basis() {
    let mut world = new_world();
    let desc = ObjectDesc {
        local_transform: LocalTransform {
            translation: Vec3::new(1.0, 2.0, 3.0),
            ..LocalTransform::default()
        },
        ..ObjectDesc::default()
    };
    add_mesh_object(&mut world, MESH, renderer(1), desc);
    let matrix = extract_mesh_objects(&world).unwrap()[0].local_to_world;
    // The engine's y becomes the view's up, and its z the view's depth: (x, y, z) -> (x, -z, y).
    assert_eq!(&matrix[12..15], &[1.0, -3.0, 2.0]);
    assert_eq!(&matrix[0..3], &[1.0, 0.0, 0.0]);
    assert_eq!(&matrix[4..7], &[0.0, 0.0, 1.0]);
    assert_eq!(&matrix[8..11], &[0.0, -1.0, 0.0]);
}

const WHITE: DrawnMaterial = DrawnMaterial {
    color: [1.0; 4],
    texture: 0,
};

fn colored(color: [f32; 4]) -> DrawnMaterial {
    DrawnMaterial { color, texture: 0 }
}

fn material(world: &mut World) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Lit".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

fn set_vector(world: &mut World, material: MaterialId, name: &str, value: [f32; 4]) {
    let edit = MaterialEdit::Property(PropertyValue::Vector(name.into(), value));
    world.edit_material(material, edit).unwrap();
}

fn texture_slot(texture: TextureRef) -> TextureSlot {
    TextureSlot {
        texture,
        ..TextureSlot::default()
    }
}

fn set_texture(world: &mut World, material: MaterialId, name: &str, texture: TextureRef) {
    let edit = MaterialEdit::Property(PropertyValue::Texture(name.into(), texture_slot(texture)));
    world.edit_material(material, edit).unwrap();
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

/// A renderer with one slot per entry of `materials`.
fn with_materials(materials: &[Option<MaterialId>]) -> MeshRenderer {
    MeshRenderer {
        slot_materials: materials.to_vec(),
        ..renderer(u32::try_from(materials.len()).unwrap())
    }
}

fn object_with(world: &mut World, renderer: MeshRenderer) -> ObjectHandle {
    add_mesh_object(world, MESH, renderer, ObjectDesc::default())
}

fn slots_of(world: &World) -> Vec<Vec<DrawnMaterial>> {
    extract_mesh_objects(world)
        .unwrap()
        .into_iter()
        .map(|o| o.slots)
        .collect()
}

fn slot_of(world: &World) -> DrawnMaterial {
    slots_of(world)[0][0]
}

#[test]
fn a_color_and_texture_set_on_the_material_after_the_renderer_exists_are_drawn() {
    let mut world = new_world();
    let material = material(&mut world);
    object_with(&mut world, with_materials(&[Some(material)]));
    assert_eq!(slot_of(&world), WHITE);
    set_vector(&mut world, material, "_Color", [0.2, 0.4, 0.6, 0.8]);
    set_texture(
        &mut world,
        material,
        "_MainTex",
        TextureRef::Asset((1 << 40) + 9),
    );
    assert_eq!(
        slot_of(&world),
        DrawnMaterial {
            color: [0.2, 0.4, 0.6, 0.8],
            texture: (1 << 40) + 9,
        }
    );
    set_vector(&mut world, material, "_Color", [0.9, 0.1, 0.0, 1.0]);
    set_texture(
        &mut world,
        material,
        "_MainTex",
        TextureRef::Asset((1 << 40) + 11),
    );
    assert_eq!(
        slot_of(&world),
        DrawnMaterial {
            color: [0.9, 0.1, 0.0, 1.0],
            texture: (1 << 40) + 11,
        }
    );
}

#[test]
fn base_color_draws_when_the_material_has_no_color_and_color_wins_when_it_has_both() {
    let mut world = new_world();
    let material = material(&mut world);
    object_with(&mut world, with_materials(&[Some(material)]));
    set_vector(&mut world, material, "_BaseColor", [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(slot_of(&world), colored([0.1, 0.2, 0.3, 0.4]));
    set_vector(&mut world, material, "_Color", [0.5, 0.6, 0.7, 0.8]);
    assert_eq!(slot_of(&world), colored([0.5, 0.6, 0.7, 0.8]));
}

#[test]
fn a_live_material_with_neither_color_nor_texture_draws_white_and_untextured() {
    let mut world = new_world();
    let material = material(&mut world);
    object_with(&mut world, with_materials(&[Some(material)]));
    assert_eq!(slot_of(&world), WHITE);
}

#[test]
fn a_main_texture_that_is_not_an_asset_or_was_not_sent_draws_untextured() {
    let mut world = new_world();
    let material = material(&mut world);
    let render = render_texture(&mut world);
    object_with(&mut world, with_materials(&[Some(material)]));
    for texture in [
        TextureRef::Render(render),
        TextureRef::None,
        TextureRef::Unsent,
    ] {
        set_texture(&mut world, material, "_MainTex", texture);
        assert_eq!(slot_of(&world).texture, 0);
    }
    set_texture(&mut world, material, "_MainTex", TextureRef::Asset(7));
    assert_eq!(slot_of(&world).texture, 7);
}

#[test]
fn a_texture_under_another_name_is_not_the_main_texture() {
    let mut world = new_world();
    let material = material(&mut world);
    object_with(&mut world, with_materials(&[Some(material)]));
    set_texture(&mut world, material, "_BumpMap", TextureRef::Asset(7));
    assert_eq!(slot_of(&world).texture, 0);
}

#[test]
fn a_negative_channel_draws_as_zero() {
    let mut world = new_world();
    let material = material(&mut world);
    object_with(&mut world, with_materials(&[Some(material)]));
    set_vector(&mut world, material, "_Color", [-1.0, 0.5, 2.0, -0.25]);
    assert_eq!(slot_of(&world), colored([0.0, 0.5, 2.0, 0.0]));
}

#[test]
fn the_world_refuses_a_color_that_is_not_finite_so_a_slot_never_draws_one() {
    let mut world = new_world();
    let material = material(&mut world);
    object_with(&mut world, with_materials(&[Some(material)]));
    let edit = MaterialEdit::Property(PropertyValue::Vector(
        "_Color".into(),
        [f32::NAN, 0.0, 0.0, 1.0],
    ));
    assert!(world.edit_material(material, edit).is_err());
    assert_eq!(slot_of(&world), WHITE);
}

#[test]
fn a_destroyed_material_and_an_empty_slot_draw_white_with_texture_zero() {
    let mut world = new_world();
    let gone = material(&mut world);
    set_vector(&mut world, gone, "_Color", [0.9, 0.9, 0.9, 0.9]);
    let live = material(&mut world);
    set_vector(&mut world, live, "_Color", [0.2, 0.2, 0.2, 0.2]);
    let mut renderer = with_materials(&[Some(gone), None, Some(live)]);
    renderer.material_count = 4;
    object_with(&mut world, renderer);
    world.destroy_material(gone).unwrap();
    assert_eq!(
        slots_of(&world),
        vec![vec![WHITE, WHITE, colored([0.2; 4]), WHITE]]
    );
}

#[test]
fn renderers_that_share_a_material_all_draw_its_change() {
    let mut world = new_world();
    let material = material(&mut world);
    object_with(&mut world, with_materials(&[Some(material)]));
    object_with(&mut world, with_materials(&[Some(material)]));
    set_texture(&mut world, material, "_MainTex", TextureRef::Asset(21));
    set_vector(&mut world, material, "_Color", [0.5; 4]);
    let both = DrawnMaterial {
        color: [0.5; 4],
        texture: 21,
    };
    assert_eq!(slots_of(&world), vec![vec![both], vec![both]]);
}

fn block(vectors: &[(&str, [f32; 4])], textures: &[(&str, TextureRef)]) -> ShaderProperties {
    let mut block = ShaderProperties::default();
    for (name, value) in vectors {
        block
            .set(PropertyValue::Vector((*name).into(), *value))
            .unwrap();
    }
    for (name, texture) in textures {
        let value = PropertyValue::Texture((*name).into(), texture_slot(texture.clone()));
        block.set(value).unwrap();
    }
    block
}

#[test]
fn a_property_block_of_the_renderer_overrides_the_material_for_what_it_sets() {
    let mut world = new_world();
    let material = material(&mut world);
    set_vector(&mut world, material, "_Color", [0.2; 4]);
    set_texture(&mut world, material, "_MainTex", TextureRef::Asset(3));
    let mut tinted = with_materials(&[Some(material)]);
    tinted.property_block = Some(block(&[("_Color", [0.7, 0.6, 0.5, 0.4])], &[]));
    object_with(&mut world, tinted);
    let mut painted = with_materials(&[Some(material)]);
    painted.property_block = Some(block(&[], &[("_MainTex", TextureRef::Asset(8))]));
    object_with(&mut world, painted);
    let [first, second] = [0, 1].map(|i| slots_of(&world)[i][0]);
    assert_eq!(
        first,
        DrawnMaterial {
            color: [0.7, 0.6, 0.5, 0.4],
            texture: 3
        }
    );
    assert_eq!(
        second,
        DrawnMaterial {
            color: [0.2; 4],
            texture: 8
        }
    );
}

#[test]
fn a_slot_block_overrides_only_its_slot_and_wins_over_the_all_slot_block() {
    let mut world = new_world();
    let material = material(&mut world);
    set_vector(&mut world, material, "_Color", [0.2; 4]);
    set_texture(&mut world, material, "_MainTex", TextureRef::Asset(3));
    let mut renderer = with_materials(&[Some(material), Some(material), Some(material)]);
    renderer.property_block = Some(block(&[("_Color", [0.7; 4])], &[]));
    renderer.slot_blocks.insert(
        1,
        block(
            &[("_Color", [0.1, 0.2, 0.3, 0.4])],
            &[("_MainTex", TextureRef::Asset(9))],
        ),
    );
    object_with(&mut world, renderer);
    let slots = &slots_of(&world)[0];
    let draws = |color, texture| DrawnMaterial { color, texture };
    assert_eq!(slots[0], draws([0.7; 4], 3));
    assert_eq!(slots[1], draws([0.1, 0.2, 0.3, 0.4], 9));
    assert_eq!(slots[2], draws([0.7; 4], 3));
}

#[test]
fn a_mesh_object_carries_the_slot_blocks_of_its_renderer() {
    let mut world = new_world();
    let mut renderer = renderer(2);
    renderer.property_block = Some(block(&[("_Color", [0.7; 4])], &[]));
    renderer
        .slot_blocks
        .insert(1, block(&[("_Color", [0.1; 4])], &[]));
    renderer
        .slot_blocks
        .insert(5, block(&[], &[("_MainTex", TextureRef::Asset(9))]));
    object_with(&mut world, renderer.clone());
    let objects = extract_mesh_objects(&world).unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].slot_blocks, renderer.slot_blocks);
    assert_eq!(objects[0].property_block, renderer.property_block);
}
