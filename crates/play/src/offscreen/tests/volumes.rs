//! Kernels that write every layer of a 2D-array texture or every slice of a 3D one, in the
//! synchronous flush on a real adapter (lavapipe in CI), and kernels that sample those textures
//! and copy one layer or slice into a plain texture a pixel read answers.
//!
//! The world's render texture description carries a size and no layer count or depth, so the
//! host takes the dimension from the view the kernel declares and the extent along the layer axis
//! from the texels the kernel's dispatches cover. Every expected value comes from the formula the
//! kernel writes, `(16x + 1, 16y + 2, 64z + 3, 255)` in 8-bit channels for texel (x, y) of layer
//! or slice z, not from the output.

use std::path::Path;

use worldrunner_render_program::unity::synthetic::compute_spirv;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    ComputeKernel, ComputeShaderId, OffscreenWork, PropertyValue, RenderTextureId, ShaderSource,
    TextureRef, WorldComputeShader,
};

use super::super::flush::{FlushOutcome, OffscreenHost};
use super::flush::host;
use super::gpu::{Quad, SIDE, target, vector};
use super::kernels::compute_bytes::{Binding, Kernel, Variant};
use super::kernels::{shader_bytes, write_compute};
use super::support::{snapshot, texture_slot, world};

/// `GraphicsFormat` 8, four 8-bit channels; 52, four 32-bit floats.
const RGBA8: u32 = 8;
const RGBA32_FLOAT: u32 = 52;
/// The layers or slices the writing dispatch covers.
const DEPTH: u32 = 3;
/// The dimension codes a kernel states for a sampled 2D, 3D and 2D-array texture.
const ARRAY: i32 = 5;
const VOLUME: i32 = 3;

fn code(wgsl: &str) -> Vec<u8> {
    compute_spirv(wgsl, "main")
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
}

/// A kernel that writes the pattern into every layer or slice of `_Volume`.
fn writer(declaration: &str, store: &str, dimension: i32) -> Kernel {
    let wgsl = format!(
        "@group(0) @binding(0) var volume: {declaration};
         @compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
             {store}
         }}"
    );
    let mut variant = Variant::new(&code(&wgsl), [2, 2, 1]);
    variant.writable = vec![Binding::image("_Volume", 0, dimension)];
    Kernel::single("Write", variant)
}

const PATTERN: &str = "vec4<f32>(
    f32(16u * id.x + 1u) / 255.0,
    f32(16u * id.y + 2u) / 255.0,
    f32(64u * id.z + 3u) / 255.0,
    1.0)";

fn array_writer() -> Kernel {
    writer(
        "texture_storage_2d_array<rgba8unorm, write>",
        &format!("textureStore(volume, vec2<i32>(id.xy), i32(id.z), {PATTERN});"),
        ARRAY,
    )
}

fn volume_writer() -> Kernel {
    writer(
        "texture_storage_3d<rgba8unorm, write>",
        &format!("textureStore(volume, vec3<i32>(id), {PATTERN});"),
        VOLUME,
    )
}

/// A kernel that copies layer or slice `at` of the sampled `_Source` into the plain `_Out`.
fn gatherer(sampled: &str, load: &str, dimension: i32, at: u32) -> Kernel {
    let wgsl = format!(
        "@group(0) @binding(0) var source: {sampled};
         @group(0) @binding(1) var out: texture_storage_2d<rgba8unorm, write>;
         @compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
             textureStore(out, vec2<i32>(id.xy), {load});
         }}"
    )
    .replace("AT", &at.to_string());
    let mut variant = Variant::new(&code(&wgsl), [2, 2, 1]);
    variant.textures = vec![Binding::texture("_Source", 0, None, dimension)];
    variant.writable = vec![Binding::image("_Out", 1, 2)];
    Kernel::single(&format!("Gather{at}"), variant)
}

fn array_gatherer(at: u32) -> Kernel {
    gatherer(
        "texture_2d_array<f32>",
        "textureLoad(source, vec2<i32>(id.xy), AT, 0)",
        ARRAY,
        at,
    )
}

fn volume_gatherer(at: u32) -> Kernel {
    gatherer(
        "texture_3d<f32>",
        "textureLoad(source, vec3<i32>(vec2<i32>(id.xy), AT), 0)",
        VOLUME,
        at,
    )
}

/// A host whose `volumes` shader has `kernels`, the first of which writes.
fn volumes_host(directory: &Path, kernels: Vec<Kernel>) -> OffscreenHost {
    write_compute(
        directory,
        "volumes",
        vec![(1, shader_bytes("Volumes", kernels))],
    );
    host(directory)
}

fn compute_shader_of(world: &mut World, kernels: &[&str]) -> ComputeShaderId {
    world
        .create_compute_shader(WorldComputeShader {
            name: "volumes".into(),
            source: ShaderSource {
                file: "volumes".into(),
                path_id: 1,
            },
            kernels: kernels
                .iter()
                .map(|name| ComputeKernel {
                    name: (*name).into(),
                    thread_group_size: Some([2, 2, 1]),
                })
                .collect(),
            generation: 0,
        })
        .unwrap()
}

fn bind(world: &mut World, shader: ComputeShaderId, kernel: u32, name: &str, to: RenderTextureId) {
    world
        .set_compute_property(
            shader,
            Some(kernel),
            PropertyValue::Texture(name.into(), texture_slot(TextureRef::Render(to))),
        )
        .unwrap();
}

/// The texels of the pattern at layer or slice `z`, row by row from row 0, as floats.
fn pattern(z: u32) -> Vec<f32> {
    (0..SIDE)
        .flat_map(|y| (0..SIDE).flat_map(move |x| [16 * x + 1, 16 * y + 2, 64 * z + 3, 255]))
        .map(|byte| f32::from(u8::try_from(byte).unwrap()) / 255.0)
        .collect()
}

fn flush(host: &mut OffscreenHost, world: &mut World) -> FlushOutcome {
    let snapshot = snapshot(world);
    let outcome = host.flush(&snapshot, &Quad).unwrap();
    outcome.apply(world).unwrap();
    outcome
}

fn failures(outcome: &FlushOutcome) -> Vec<(&str, usize)> {
    outcome
        .report
        .failures
        .iter()
        .map(|(name, count)| (name.as_str(), *count))
        .collect()
}

/// Writes a texture of `format` with the kernel `writer`, which `DEPTH` layers or slices of
/// `SIDE` by `SIDE` texels take, then gathers each of them into a plain texture and asks for its
/// pixels. Returns the flush and the answers, in the order of the layers or slices.
fn written_and_gathered(
    kernels: Vec<Kernel>,
    format: u32,
) -> (FlushOutcome, Vec<Option<Vec<f32>>>) {
    let directory = tempfile::tempdir().unwrap();
    let mut host = volumes_host(directory.path(), kernels);
    let mut world = world();
    let volume = target(&mut world, format);
    let names = kernel_names();
    let shader = compute_shader_of(
        &mut world,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    bind(&mut world, shader, 0, "_Volume", volume);
    world.dispatch_compute(shader, 0, [2, 2, DEPTH]).unwrap();
    let mut requests = Vec::new();
    for at in 0..DEPTH {
        let out = target(&mut world, RGBA8);
        bind(&mut world, shader, at + 1, "_Source", volume);
        bind(&mut world, shader, at + 1, "_Out", out);
        world.dispatch_compute(shader, at + 1, [2, 2, 1]).unwrap();
        requests.push(world.request_texture_pixels(out, 0, 0, SIDE, SIDE).unwrap());
    }
    let outcome = flush(&mut host, &mut world);
    let answers = requests
        .into_iter()
        .map(|request| world.take_texture_pixels(request))
        .collect();
    (outcome, answers)
}

#[test]
fn a_kernel_writes_every_layer_of_an_array_texture_and_each_layer_reads_back_exactly() {
    let kernels = std::iter::once(array_writer())
        .chain((0..DEPTH).map(array_gatherer))
        .collect();

    let (outcome, answers) = written_and_gathered(kernels, RGBA8);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(outcome.report.census["dispatches ran"], 4);
    let expected: Vec<_> = (0..DEPTH).map(|z| Some(pattern(z))).collect();
    assert_eq!(answers, expected);
}

#[test]
fn a_kernel_writes_every_slice_of_a_3d_texture_and_each_slice_reads_back_exactly() {
    let kernels = std::iter::once(volume_writer())
        .chain((0..DEPTH).map(volume_gatherer))
        .collect();

    let (outcome, answers) = written_and_gathered(kernels, RGBA8);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(outcome.report.census["dispatches ran"], 4);
    let expected: Vec<_> = (0..DEPTH).map(|z| Some(pattern(z))).collect();
    assert_eq!(answers, expected);
}

#[test]
fn a_pixel_read_of_a_3d_texture_of_floats_answers_its_first_slice() {
    let directory = tempfile::tempdir().unwrap();
    let write = writer(
        "texture_storage_3d<rgba32float, write>",
        "textureStore(volume, vec3<i32>(id), vec4<f32>(f32(id.x + 4u * id.y), f32(id.z), 0.5, 1.0));",
        VOLUME,
    );
    let mut host = volumes_host(directory.path(), vec![write]);
    let mut world = world();
    let volume = target(&mut world, RGBA32_FLOAT);
    let shader = compute_shader_of(&mut world, &["Write"]);
    bind(&mut world, shader, 0, "_Volume", volume);
    world.dispatch_compute(shader, 0, [2, 2, DEPTH]).unwrap();
    let request = world
        .request_texture_pixels(volume, 0, 0, SIDE, SIDE)
        .unwrap();

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    let first_slice: Vec<f32> = (0..SIDE)
        .flat_map(|y| {
            (0..SIDE)
                .flat_map(move |x| [f32::from(u8::try_from(x + 4 * y).unwrap()), 0.0, 0.5, 1.0])
        })
        .collect();
    assert_eq!(world.take_texture_pixels(request), Some(first_slice));
}

#[test]
fn a_blit_that_names_a_3d_texture_is_refused_by_name_and_the_rest_runs() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = volumes_host(directory.path(), vec![volume_writer()]);
    let mut world = world();
    let volume = target(&mut world, RGBA8);
    let copied = target(&mut world, RGBA8);
    let shader = compute_shader_of(&mut world, &["Write"]);
    bind(&mut world, shader, 0, "_Volume", volume);
    world.dispatch_compute(shader, 0, [2, 2, DEPTH]).unwrap();
    let blit = super::gpu::shader(&mut world, "blit");
    let material = world.create_material(blit).unwrap();
    vector(&mut world, material, "_Tint", [1.0; 4]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(volume),
            target: copied,
            material: Some(material),
            pass: 0,
        })
        .unwrap();

    let outcome = flush(&mut host, &mut world);

    assert_eq!(outcome.report.census["dispatches ran"], 1);
    assert!(
        failures(&outcome)
            .iter()
            .any(|(name, _)| name.ends_with(":VolumeInGraph")),
        "{:?}",
        failures(&outcome)
    );
}

/// The names of the writer and of the gatherers, in the order of their kernels.
fn kernel_names() -> Vec<String> {
    std::iter::once("Write".to_owned())
        .chain((0..DEPTH).map(|at| format!("Gather{at}")))
        .collect()
}

#[test]
fn a_layer_is_still_there_in_a_later_flush_that_only_reads_it() {
    let directory = tempfile::tempdir().unwrap();
    let kernels = std::iter::once(array_writer())
        .chain((0..DEPTH).map(array_gatherer))
        .collect();
    let mut host = volumes_host(directory.path(), kernels);
    let mut world = world();
    let volume = target(&mut world, RGBA8);
    let names = kernel_names();
    let shader = compute_shader_of(
        &mut world,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    bind(&mut world, shader, 0, "_Volume", volume);
    world.dispatch_compute(shader, 0, [2, 2, DEPTH]).unwrap();
    assert!(failures(&flush(&mut host, &mut world)).is_empty());

    // This flush has no dispatch that binds the texture as a storage image.
    let out = target(&mut world, RGBA8);
    bind(&mut world, shader, 3, "_Source", volume);
    bind(&mut world, shader, 3, "_Out", out);
    world.dispatch_compute(shader, 3, [2, 2, 1]).unwrap();
    let request = world.request_texture_pixels(out, 0, 0, SIDE, SIDE).unwrap();
    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(world.take_texture_pixels(request), Some(pattern(2)));
}

#[test]
fn a_kernel_that_views_an_array_is_left_out_by_name_when_the_texture_is_plain() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = volumes_host(directory.path(), vec![array_writer(), array_gatherer(0)]);
    let mut world = world();
    let plain = target(&mut world, RGBA8);
    let out = target(&mut world, RGBA8);
    let shader = compute_shader_of(&mut world, &["Write", "Gather0"]);
    bind(&mut world, shader, 1, "_Source", plain);
    bind(&mut world, shader, 1, "_Out", out);
    world.dispatch_compute(shader, 1, [2, 2, 1]).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert_eq!(failures(&outcome), [("dispatch:TextureDimension", 1)]);
    assert!(!outcome.report.census.contains_key("dispatches ran"));
}
