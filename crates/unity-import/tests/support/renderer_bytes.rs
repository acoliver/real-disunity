//! Serialized game objects, transforms and renderers for tests.

use worldrunner_unity_serialized::{object::PPtr, write::ObjectWriter};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

pub fn game_object(name: &str, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components).unwrap().u32(0).string(name).unwrap();
    w.u16(0).u8(1);
    w.finish()
}

pub fn transform(game_object: i64, father: PPtr, children: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, game_object)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[0.0; 3]).floats(&[1.0; 3]);
    w.pptrs(children).unwrap().pptr(father);
    w.finish()
}

/// The values a renderer's plan carries, set apart from the defaults. The three renderer flags are
/// bytes, 0 or 1, as the serialized form stores them.
pub struct Settings {
    pub enabled: bool,
    pub cast_shadows: u8,
    pub receive_shadows: u8,
    pub dynamic_occludee: u8,
    pub static_shadow_caster: u8,
    pub motion_vectors: u8,
    pub light_probe_usage: u8,
    pub reflection_probe_usage: u8,
    pub layer_mask: u32,
    pub priority: i32,
    pub sorting_layer_id: i32,
    pub sorting_order: i16,
}

pub fn distinct() -> Settings {
    Settings {
        enabled: false,
        cast_shadows: 3,
        receive_shadows: 0,
        dynamic_occludee: 0,
        static_shadow_caster: 1,
        motion_vectors: 2,
        light_probe_usage: 2,
        reflection_probe_usage: 3,
        layer_mask: 0xF0F0,
        priority: -4,
        sorting_layer_id: 0,
        sorting_order: 7,
    }
}

fn write_renderer(w: &mut ObjectWriter, owner: i64, s: &Settings, materials: &[PPtr]) {
    w.pptr(ptr(0, owner)).u8(u8::from(s.enabled));
    w.bytes(&[
        s.cast_shadows,
        s.receive_shadows,
        s.dynamic_occludee,
        s.static_shadow_caster,
        s.motion_vectors,
        s.light_probe_usage,
        s.reflection_probe_usage,
        0,
        0,
        0,
        0,
        0,
    ])
    .align();
    w.i16(-1).align().f32(0.0).u32(s.layer_mask).i32(s.priority);
    w.u16(0).u16(0).floats(&[1.0, 1.0, 0.0, 0.0]);
    w.floats(&[1.0, 1.0, 0.0, 0.0]);
    w.pptrs(materials).unwrap();
    w.u16(0)
        .u16(0)
        .pptr(ptr(0, 0))
        .pptr(ptr(0, 0))
        .pptr(ptr(0, 0));
    w.align();
    w.i32(s.sorting_layer_id).i16(0).i16(s.sorting_order).i32(0);
}

pub fn filter_bytes(owner: i64, mesh: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, owner)).pptr(mesh);
    w.finish()
}

pub fn mesh_renderer_bytes(owner: i64, s: &Settings, materials: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    write_renderer(&mut w, owner, s, materials);
    w.pptr(ptr(0, 0)).pptr(ptr(0, 0));
    w.finish()
}

pub fn skinned_bytes(owner: i64, s: &Settings, mesh: PPtr, materials: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    write_renderer(&mut w, owner, s, materials);
    w.i32(4).u8(1).u8(0).align().pptr(mesh);
    w.pptrs(&[]).unwrap();
    w.i32(0).pptr(ptr(0, 0));
    w.floats(&[0.0; 6]).u8(0).align();
    w.finish()
}
