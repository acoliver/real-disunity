//! Serialized shader objects for tests: a shader whose passes carry the render state the
//! planner skips, so the object decodes through its declared name, queue tag and pass counts.

#![allow(dead_code)]

use worldrunner_unity_serialized::write::ObjectWriter;

/// Render state values in serialized order: 8 targets of 7 blend values, 8 depth values, 3
/// stencil operation sets of 4, 3 stencil values, 3 fog values and 4 fog color values.
const STATE_VALUES: u16 = 86;

/// Writes a pass's render state. A built player keeps the name a pass declares here, in the state,
/// and leaves the pass's own name field empty.
fn write_state(w: &mut ObjectWriter, name: &str) {
    w.string(name).unwrap();
    for value in 1..=STATE_VALUES {
        if value == 57 {
            w.u8(1).align();
        }
        w.f32(f32::from(value)).string("<noninit>").unwrap();
    }
    w.string("_FogColor").unwrap().i32(2).i32(77);
    w.i32(0).i32(150).u8(1).align();
}

fn write_empty_program(w: &mut ObjectWriter) {
    w.i32(0).i32(0).i32(0);
    for _ in 0..8 {
        w.i32(0);
    }
    w.i32(1).u16(0).align();
}

fn write_pass(w: &mut ObjectWriter, name: &str) {
    w.i32(0).i32(0);
    write_state(w, name);
    w.u32(0);
    for _ in 0..6 {
        write_empty_program(w);
    }
    w.u8(0).u8(0).align();
    w.string("")
        .unwrap()
        .string("")
        .unwrap()
        .string("")
        .unwrap();
    w.i32(0);
}

/// A shader declared as `name` whose sub-shaders hold `passes` unnamed passes each, tagged with
/// the queue `queue` when there is one.
pub fn shader(name: &str, queue: Option<(&str, &str)>, passes: &[usize]) -> Vec<u8> {
    shader_with_platforms(name, queue, passes, &[])
}

/// A shader as [`shader`] writes it, compiled for `platforms`, each with no chunks.
pub fn shader_with_platforms(
    name: &str,
    queue: Option<(&str, &str)>,
    passes: &[usize],
    platforms: &[u32],
) -> Vec<u8> {
    let names: Vec<Vec<&str>> = passes.iter().map(|&count| vec![""; count]).collect();
    let names: Vec<&[&str]> = names.iter().map(Vec::as_slice).collect();
    named_shader_with_platforms(name, queue, &names, platforms)
}

/// A shader declared as `name` with one sub-shader per entry of `sub_shaders`, whose passes
/// carry the names listed, tagged with the queue `queue` when there is one.
pub fn named_shader(name: &str, queue: Option<(&str, &str)>, sub_shaders: &[&[&str]]) -> Vec<u8> {
    named_shader_with_platforms(name, queue, sub_shaders, &[])
}

/// A shader as [`named_shader`] writes it, compiled for `platforms`, each with no chunks.
pub fn named_shader_with_platforms(
    name: &str,
    queue: Option<(&str, &str)>,
    sub_shaders: &[&[&str]],
    platforms: &[u32],
) -> Vec<u8> {
    let tags: Vec<(&str, &str)> = queue
        .map(|queue| vec![("RenderType", "Opaque"), queue])
        .unwrap_or_default();
    shader_with_tags(name, &tags, "", sub_shaders, platforms, &[])
}

/// A shader declared as `name` that falls back to the shader `fallback` (none when empty), with
/// one sub-shader per entry of `sub_shaders`, whose passes carry the names listed. Every
/// sub-shader carries `tags`.
pub fn tagged_shader(
    name: &str,
    tags: &[(&str, &str)],
    fallback: &str,
    sub_shaders: &[&[&str]],
) -> Vec<u8> {
    shader_with_tags(name, tags, fallback, sub_shaders, &[], &[])
}

/// A shader as [`lit`] writes it that declares the properties `properties`, each a name and the
/// serialized property kind (0 color, 1 vector, 2 float).
pub fn declaring(name: &str, properties: &[(&str, i32)]) -> Vec<u8> {
    let tags = [("RenderType", "Opaque"), ("QUEUE", "Transparent")];
    shader_with_tags(name, &tags, "", &[&[""]], &[], properties)
}

fn shader_with_tags(
    name: &str,
    tags: &[(&str, &str)],
    fallback: &str,
    sub_shaders: &[&[&str]],
    platforms: &[u32],
    properties: &[(&str, i32)],
) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("").unwrap();
    w.i32(i32::try_from(properties.len()).unwrap());
    for &(property, kind) in properties {
        w.string(property).unwrap().string("").unwrap().i32(0);
        w.i32(kind).u32(0).floats(&[0.0; 4]);
        w.string("").unwrap().i32(0);
    }
    w.i32(i32::try_from(sub_shaders.len()).unwrap());
    for &pass_names in sub_shaders {
        w.i32(i32::try_from(pass_names.len()).unwrap());
        for &pass_name in pass_names {
            write_pass(&mut w, pass_name);
        }
        w.i32(i32::try_from(tags.len()).unwrap());
        for &(key, value) in tags {
            w.string(key).unwrap().string(value).unwrap();
        }
        w.i32(100);
    }
    w.i32(0).byte_array(&[]).unwrap();
    w.string(name)
        .unwrap()
        .string("")
        .unwrap()
        .string(fallback)
        .unwrap();
    w.i32(0).i32(0).u8(0).align();
    w.i32(i32::try_from(platforms.len()).unwrap());
    for &platform in platforms {
        w.u32(platform);
    }
    for _ in 0..3 {
        w.i32(i32::try_from(platforms.len()).unwrap());
        for _ in platforms {
            w.i32(0);
        }
    }
    w.byte_array(&[]).unwrap().i32(0);
    w.pptrs(&[]).unwrap().i32(0).u8(0).align();
    w.bytes(&[0; 16]).i64(0);
    w.finish()
}

pub fn lit() -> Vec<u8> {
    shader("Test/Lit", Some(("QUEUE", "Transparent")), &[2])
}
