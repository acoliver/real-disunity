//! A kernel that writes a render texture as a storage image, in the synchronous flush on a real
//! adapter (lavapipe in CI): the kernel's writes are what a later pixel read and a later blit
//! see, and a texture the kernel cannot be given is refused by the name of the cause.
//!
//! The kernel writes texel (x, y) of a four texel side as (16x + 1, 16y + 2, x + 4y, 255) in 8-bit
//! channels, so every expected float comes from that formula and the 8-bit rounding, not from the
//! output.

use std::path::Path;

use worldrunner_render_program::unity::synthetic::compute_spirv;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{
    ComputeKernel, ComputeShaderId, OffscreenWork, PropertyValue, RenderTextureId, ShaderSource,
    TextureRef, WorldComputeShader,
};

use super::super::flush::{FlushOutcome, OffscreenHost};
use super::flush::host;
use super::gpu::{Quad, SIDE, shader, target, vector};
use super::kernels::compute_bytes::{Binding, Kernel, Variant};
use super::kernels::{shader_bytes, write_compute};
use super::support::{snapshot, texture_slot, world};

/// `GraphicsFormat` 8, four 8-bit channels; 48, four half floats; 45, one half float.
const RGBA8: u32 = 8;
const RGBA16_FLOAT: u32 = 48;
const R16_FLOAT: u32 = 45;

const PATTERN: &str = "
@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
@compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(image, vec2<i32>(id.xy), vec4<f32>(
        f32(16u * id.x + 1u) / 255.0,
        f32(16u * id.y + 2u) / 255.0,
        f32(id.x + 4u * id.y) / 255.0,
        1.0));
}";

fn code(wgsl: &str) -> Vec<u8> {
    compute_spirv(wgsl, "main")
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
}

/// A host whose kernels are read from a data directory with the `Paint` kernel in `paint`.
fn painting_host(directory: &Path) -> OffscreenHost {
    let mut variant = Variant::new(&code(PATTERN), [2, 2, 1]);
    variant.writable = vec![Binding::image("_Image", 0, 2)];
    let object = shader_bytes("Paint", vec![Kernel::single("Paint", variant)]);
    write_compute(directory, "paint", vec![(1, object)]);
    host(directory)
}

/// A dispatch of the `Paint` kernel that writes `texture`.
fn paint(world: &mut World, texture: RenderTextureId) -> ComputeShaderId {
    let shader = world
        .create_compute_shader(WorldComputeShader {
            name: "paint".into(),
            source: ShaderSource {
                file: "paint".into(),
                path_id: 1,
            },
            kernels: vec![ComputeKernel {
                name: "Paint".into(),
                thread_group_size: Some([2, 2, 1]),
            }],
            generation: 0,
        })
        .unwrap();
    world
        .set_compute_property(
            shader,
            Some(0),
            PropertyValue::Texture("_Image".into(), texture_slot(TextureRef::Render(texture))),
        )
        .unwrap();
    world.dispatch_compute(shader, 0, [2, 2, 1]).unwrap();
    shader
}

fn stored(byte: u32) -> f32 {
    f32::from(u8::try_from(byte).unwrap()) / 255.0
}

/// The texels the kernel writes, row by row from row 0.
fn written() -> Vec<f32> {
    (0..SIDE)
        .flat_map(|y| (0..SIDE).flat_map(move |x| [16 * x + 1, 16 * y + 2, x + 4 * y, 255]))
        .map(stored)
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

#[test]
fn a_kernel_writes_a_render_texture_and_a_later_pixel_read_answers_what_it_wrote() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = painting_host(directory.path());
    let mut world = world();
    let painted = target(&mut world, RGBA8);
    paint(&mut world, painted);
    let request = world.request_texture_pixels(painted, 0, 0, 4, 4).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(outcome.report.traces[0].outcome, "ran");
    assert_eq!(world.take_texture_pixels(request), Some(written()));
}

#[test]
fn a_blit_after_the_dispatch_samples_what_the_kernel_wrote() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = painting_host(directory.path());
    let mut world = world();
    let (painted, copied) = (target(&mut world, RGBA8), target(&mut world, RGBA8));
    paint(&mut world, painted);
    let blit = shader(&mut world, "blit");
    let material = world.create_material(blit).unwrap();
    vector(&mut world, material, "_Tint", [1.0; 4]);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(painted),
            target: copied,
            material: Some(material),
            pass: 0,
        })
        .unwrap();
    let request = world.request_texture_pixels(copied, 0, 0, 4, 4).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(world.take_texture_pixels(request), Some(written()));
}

#[test]
fn a_texture_keeps_what_a_kernel_wrote_into_it_across_flushes() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = painting_host(directory.path());
    let mut world = world();
    let painted = target(&mut world, RGBA8);
    paint(&mut world, painted);
    let first = world.request_texture_pixels(painted, 0, 0, 4, 4).unwrap();
    flush(&mut host, &mut world);
    assert_eq!(world.take_texture_pixels(first), Some(written()));

    // The second flush has no dispatch, so nothing marks the texture again, and the read still
    // finds the texture the first flush made and what the kernel wrote into it.
    let second = world.request_texture_pixels(painted, 0, 0, 4, 4).unwrap();
    let outcome = flush(&mut host, &mut world);
    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(world.take_texture_pixels(second), Some(written()));
}

#[test]
fn a_texture_in_another_format_than_the_kernels_is_left_out_by_name() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = painting_host(directory.path());
    let mut world = world();
    let wrong = target(&mut world, RGBA16_FLOAT);
    paint(&mut world, wrong);

    let outcome = flush(&mut host, &mut world);

    assert_eq!(failures(&outcome), [("dispatch:StorageFormatMismatch", 1)]);
    assert_eq!(outcome.report.traces[0].outcome, "StorageFormatMismatch");
}

#[test]
fn a_texture_whose_format_the_device_cannot_bind_as_storage_is_left_out_by_name() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = painting_host(directory.path());
    let mut world = world();
    let half = target(&mut world, R16_FLOAT);
    paint(&mut world, half);

    let outcome = flush(&mut host, &mut world);

    assert_eq!(failures(&outcome), [("dispatch:StorageFormat", 1)]);
    assert_eq!(outcome.report.traces[0].outcome, "StorageFormat");
}

#[test]
fn a_storage_image_given_no_texture_is_left_out_by_name() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = painting_host(directory.path());
    let mut world = world();
    let painted = target(&mut world, RGBA8);
    paint(&mut world, painted);
    // A dispatch of the same kernel with the texture property unset.
    let bare = world
        .create_compute_shader(WorldComputeShader {
            name: "bare".into(),
            source: ShaderSource {
                file: "paint".into(),
                path_id: 1,
            },
            kernels: vec![ComputeKernel {
                name: "Paint".into(),
                thread_group_size: Some([2, 2, 1]),
            }],
            generation: 0,
        })
        .unwrap();
    world.dispatch_compute(bare, 0, [2, 2, 1]).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert_eq!(failures(&outcome), [("dispatch:MissingTexture", 1)]);
}

#[test]
fn a_kernel_that_binds_more_storage_images_than_the_device_allows_is_left_out_by_name() {
    // The test device has the default limit of four storage images in a stage.
    let images = 5;
    let declarations: String = (0..images)
        .map(|i| {
            format!("@group(0) @binding({i}) var image{i}: texture_storage_2d<r32float, write>;\n")
        })
        .collect();
    let stores: String = (0..images)
        .map(|i| format!("textureStore(image{i}, vec2<i32>(id.xy), vec4<f32>(1.0));\n"))
        .collect();
    let wgsl = format!(
        "{declarations}
         @compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{
             {stores}
         }}"
    );
    let mut variant = Variant::new(&code(&wgsl), [2, 2, 1]);
    variant.writable = (0..images)
        .map(|i| Binding::image(&format!("_Image{i}"), i, 2))
        .collect();
    let directory = tempfile::tempdir().unwrap();
    write_compute(
        directory.path(),
        "paint",
        vec![(
            1,
            shader_bytes("Paint", vec![Kernel::single("Paint", variant)]),
        )],
    );
    let mut host = host(directory.path());
    let mut world = world();
    let painted = target(&mut world, RGBA8);
    paint(&mut world, painted);

    let outcome = flush(&mut host, &mut world);

    assert_eq!(failures(&outcome), [("dispatch:StorageLimit", 1)]);
}
