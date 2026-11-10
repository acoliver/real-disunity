//! The shape of a render texture comes from the world's record: its dimension and its layers,
//! depth or cube faces. The host makes the texture that way before any kernel touches it, a kernel
//! binds it through the view of that shape, and a binding of another shape leaves the dispatch out
//! by name. Expected values come from the formulas the kernels write.

use worldrunner_render_program::unity::offscreen::frame::RenderDimension;
use worldrunner_render_program::unity::synthetic::shaders::no_cull;
use worldrunner_render_program::unity::synthetic::shaders::variant;
use worldrunner_render_program::unity::synthetic::{
    FRAGMENT, PassSpec, SyntheticShader, binding, shader as synthetic_shader, slot,
};
use worldrunner_unity_serialized::shader_parameters::{ResourceKind, VariantParameters};
use worldrunner_world_simulation::graphics::{OffscreenWork, TextureRef, dimension};

use super::super::adapter::build;
use super::super::ids;
use super::flush::host;
use super::gpu::{Quad, Rig, SIDE, shader, shaped_target, target};
use super::kernels::compute_bytes::{Binding, Kernel, Variant};
use super::support::{material, snapshot, world};
use super::volumes::{
    array_gatherer, array_writer, bind, code, compute_shader_of, failures, flush, pattern,
    volume_gatherer, volume_writer, volumes_host, writer,
};

const RGBA8: u32 = 8;

/// A blit pass that samples the cube `_MainTex` along +X in the left half of the target and along
/// -Z in the right half. Faces of a cube are in the order +X, -X, +Y, -Y, +Z, -Z.
const CUBE_BLIT: &str = "
@group(0) @binding(0) var main_tex: texture_cube<f32>;
@group(0) @binding(1) var main_sampler: sampler;
struct VOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> VOut {
    var out: VOut;
    out.position = vec4<f32>(position.xy, 0.0, 1.0);
    out.uv = uv;
    return out;
}
@fragment fn fs_cube(in: VOut) -> @location(0) vec4<f32> {
    var direction = vec3<f32>(1.0, 0.0, 0.0);
    if in.uv.x > 0.5 {
        direction = vec3<f32>(0.0, 0.0, -1.0);
    }
    return textureSample(main_tex, main_sampler, direction);
}";

/// The shader's texture is declared with the code 8 a player variant gives a cube.
pub(super) fn cube_blit_passes() -> SyntheticShader {
    let parameters = VariantParameters {
        constant_buffers: Vec::new(),
        bindings: vec![
            binding(
                "_MainTex",
                slot(0, 0, FRAGMENT),
                ResourceKind::Texture {
                    sampler: Some(slot(0, 1, FRAGMENT)),
                    dimension: 8,
                },
            ),
            binding("", slot(0, 1, FRAGMENT), ResourceKind::Sampler { state: 0 }),
        ],
    };
    no_cull(synthetic_shader(
        &[],
        &[PassSpec {
            tags: Vec::new(),
            variants: vec![variant(&[], CUBE_BLIT, "fs_cube", parameters)],
        }],
    ))
}

/// Adapts and builds the frame of a world, the way a flush does before it records anything.
fn made(
    rig: &mut Rig,
    world: &mut worldrunner_world_simulation::World,
) -> super::super::frame::Adapted {
    let snapshot = snapshot(world);
    let mut adapted = rig
        .adapter
        .adapt(&snapshot, &Quad, &mut rig.state, &mut rig.runtime);
    build(&mut rig.state, &mut rig.runtime, &rig.arena, &mut adapted).unwrap();
    adapted
}

#[test]
fn an_array_record_of_four_layers_is_made_with_four_layers_before_a_kernel_touches_it() {
    let mut rig = Rig::new();
    let mut world = world();
    let layers = shaped_target(&mut world, (RGBA8, 1), (dimension::TEX_2D_ARRAY, 4));
    let plain = target(&mut world, RGBA8);
    world
        .request_texture_pixels(layers, 0, 0, SIDE, SIDE)
        .unwrap();
    world
        .request_texture_pixels(plain, 0, 0, SIDE, SIDE)
        .unwrap();

    made(&mut rig, &mut world);

    let dimension_of = |id| rig.state.texture_dimension(ids::render_texture(id));
    assert_eq!(dimension_of(layers), Some(RenderDimension::D2Array(4)));
    assert_eq!(dimension_of(plain), Some(RenderDimension::D2));
}

#[test]
fn each_record_dimension_is_made_with_its_layers() {
    let mut rig = Rig::new();
    let mut world = world();
    let records = [
        (dimension::TEX_2D_ARRAY, 1, RenderDimension::D2Array(1)),
        (dimension::TEX_3D, 8, RenderDimension::D3(8)),
        (dimension::CUBE, 1, RenderDimension::Cube),
        (dimension::CUBE_ARRAY, 12, RenderDimension::CubeArray(2)),
    ];
    let ids: Vec<_> = records
        .iter()
        .map(|&(code, depth, _)| {
            let id = shaped_target(&mut world, (RGBA8, 1), (code, depth));
            world.request_texture_pixels(id, 0, 0, SIDE, SIDE).unwrap();
            id
        })
        .collect();

    made(&mut rig, &mut world);

    for (id, (_, _, expected)) in ids.into_iter().zip(records) {
        let found = rig
            .state
            .texture_dimension(super::super::ids::render_texture(id));
        assert_eq!(found, Some(expected));
    }
}

#[test]
fn a_dimension_code_the_world_does_not_define_is_refused_by_name_and_the_rest_is_made() {
    let mut rig = Rig::new();
    let mut world = world();
    let odd = target(&mut world, RGBA8);
    let plain = target(&mut world, RGBA8);
    world.request_texture_pixels(odd, 0, 0, SIDE, SIDE).unwrap();
    world
        .request_texture_pixels(plain, 0, 0, SIDE, SIDE)
        .unwrap();
    let mut snapshot = snapshot(&mut world);
    snapshot.render_textures.get_mut(&odd).unwrap().dimension = 9;
    let mut adapted = rig
        .adapter
        .adapt(&snapshot, &Quad, &mut rig.state, &mut rig.runtime);

    build(&mut rig.state, &mut rig.runtime, &rig.arena, &mut adapted).unwrap();

    let named: Vec<_> = adapted.report.failures.iter().collect();
    assert_eq!(named, [(&"rendertexture:UnknownDimension".to_owned(), &1)]);
    let example = &adapted.report.examples["rendertexture:UnknownDimension"];
    assert!(example.contains("dimension code 9"), "{example}");
    assert_eq!(rig.state.texture_dimension(ids::render_texture(odd)), None);
    assert_eq!(
        rig.state.texture_dimension(ids::render_texture(plain)),
        Some(RenderDimension::D2)
    );
}

/// The texel (x, y) of slice z of the deep pattern: (16x + 1, 16y + 2, 30z + 3, 255).
fn deep_pattern(z: u32) -> Vec<f32> {
    (0..SIDE)
        .flat_map(|y| (0..SIDE).flat_map(move |x| [16 * x + 1, 16 * y + 2, 30 * z + 3, 255]))
        .map(|byte| f32::from(u8::try_from(byte).unwrap()) / 255.0)
        .collect()
}

#[test]
fn a_one_layer_array_is_written_by_a_kernel_that_views_an_array_and_reads_back_exactly() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = volumes_host(directory.path(), vec![array_writer(), array_gatherer(0)]);
    let mut world = world();
    let one = shaped_target(&mut world, (RGBA8, 1), (dimension::TEX_2D_ARRAY, 1));
    let out = target(&mut world, RGBA8);
    let shader = compute_shader_of(&mut world, &["Write", "Gather0"]);
    bind(&mut world, shader, 0, "_Volume", one);
    world.dispatch_compute(shader, 0, [2, 2, 1]).unwrap();
    bind(&mut world, shader, 1, "_Source", one);
    bind(&mut world, shader, 1, "_Out", out);
    world.dispatch_compute(shader, 1, [2, 2, 1]).unwrap();
    let request = world.request_texture_pixels(out, 0, 0, SIDE, SIDE).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(outcome.report.census["dispatches ran"], 2);
    assert_eq!(world.take_texture_pixels(request), Some(pattern(0)));
}

#[test]
fn a_3d_texture_of_depth_eight_is_written_in_every_slice_and_reads_back_exactly() {
    const DEPTH: u32 = 8;
    let directory = tempfile::tempdir().unwrap();
    let store = "textureStore(volume, vec3<i32>(id), vec4<f32>(
        f32(16u * id.x + 1u) / 255.0, f32(16u * id.y + 2u) / 255.0,
        f32(30u * id.z + 3u) / 255.0, 1.0));";
    let write = writer("texture_storage_3d<rgba8unorm, write>", store, 3);
    let kernels = std::iter::once(write)
        .chain((0..DEPTH).map(volume_gatherer))
        .collect();
    let mut host = volumes_host(directory.path(), kernels);
    let mut world = world();
    let volume = shaped_target(&mut world, (RGBA8, 1), (dimension::TEX_3D, DEPTH));
    let names: Vec<String> = std::iter::once("Write".to_owned())
        .chain((0..DEPTH).map(|at| format!("Gather{at}")))
        .collect();
    let shader = compute_shader_of(
        &mut world,
        &names.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    bind(&mut world, shader, 0, "_Volume", volume);
    // One dispatch covers the whole depth. The record, not this extent, gives the texture its
    // eight slices.
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

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    assert_eq!(outcome.report.census["dispatches ran"], 9);
    for (z, request) in (0..DEPTH).zip(requests) {
        assert_eq!(
            world.take_texture_pixels(request),
            Some(deep_pattern(z)),
            "slice {z}"
        );
    }
}

#[test]
fn a_kernel_writes_the_faces_of_a_cube_and_a_blit_samples_the_face_it_looks_along() {
    let directory = tempfile::tempdir().unwrap();
    // Each face holds one color: (40 (face + 1), 10 face + 3, 17, 255).
    let faces = writer(
        "texture_storage_2d_array<rgba8unorm, write>",
        "textureStore(volume, vec2<i32>(id.xy), i32(id.z), vec4<f32>(
            f32(40u * (id.z + 1u)) / 255.0, f32(10u * id.z + 3u) / 255.0, 17.0 / 255.0, 1.0));",
        5,
    );
    let mut host = volumes_host(directory.path(), vec![faces]);
    let mut world = world();
    let cube = shaped_target(&mut world, (RGBA8, 1), (dimension::CUBE, 1));
    let out = target(&mut world, RGBA8);
    let writing = compute_shader_of(&mut world, &["Write"]);
    bind(&mut world, writing, 0, "_Volume", cube);
    world.dispatch_compute(writing, 0, [2, 2, 6]).unwrap();
    let cube_shader = shader(&mut world, "cubeblit");
    let blit = material(&mut world, cube_shader);
    world
        .record_offscreen(OffscreenWork::Blit {
            source: TextureRef::Render(cube),
            target: out,
            material: Some(blit),
            pass: 0,
        })
        .unwrap();
    let request = world.request_texture_pixels(out, 0, 0, SIDE, SIDE).unwrap();

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    let face = |face: u32| {
        [40 * (face + 1), 10 * face + 3, 17, 255]
            .map(|byte| f32::from(u8::try_from(byte).unwrap()) / 255.0)
    };
    let expected: Vec<f32> = (0..SIDE)
        .flat_map(|_| (0..SIDE).flat_map(|x| face(if x < SIDE / 2 { 0 } else { 5 })))
        .collect();
    assert_eq!(world.take_texture_pixels(request), Some(expected));
}

#[test]
fn a_kernel_whose_storage_view_is_not_the_records_shape_is_left_out_by_name_with_both_shapes() {
    let cases = [
        // A 2D-array view on a plain record, a 2D view on an array record and a 3D view on one.
        (array_writer(), (dimension::TEX_2D, 1), "D2Array", "D2"),
        (array_writer(), (dimension::TEX_3D, 4), "D2Array", "D3(4)"),
        (
            volume_writer(),
            (dimension::TEX_2D_ARRAY, 4),
            "D3",
            "D2Array(4)",
        ),
        (
            paint_writer(),
            (dimension::TEX_2D_ARRAY, 1),
            "D2",
            "D2Array(1)",
        ),
    ];
    for (kernel, record, declared, found) in cases {
        let directory = tempfile::tempdir().unwrap();
        let mut host = volumes_host(directory.path(), vec![kernel]);
        let mut world = world();
        let texture = shaped_target(&mut world, (RGBA8, 1), record);
        let shader = compute_shader_of(&mut world, &["Write"]);
        bind(&mut world, shader, 0, "_Volume", texture);
        world.dispatch_compute(shader, 0, [2, 2, 1]).unwrap();

        let outcome = flush(&mut host, &mut world);

        assert_eq!(
            failures(&outcome),
            [("dispatch:StorageDimension", 1)],
            "{record:?}"
        );
        assert!(!outcome.report.census.contains_key("dispatches ran"));
        let reason = &outcome.report.examples["dispatch:StorageDimension"];
        assert!(
            reason.contains(&format!("declares a {declared} view"))
                && reason.contains(&format!("render texture is {found}")),
            "{record:?}: {reason}"
        );
    }
}

/// A kernel that writes a plain 2D storage image `_Volume`.
fn paint_writer() -> Kernel {
    let wgsl = "@group(0) @binding(0) var volume: texture_storage_2d<rgba8unorm, write>;
        @compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
            textureStore(volume, vec2<i32>(id.xy), vec4<f32>(1.0));
        }";
    let mut variant = Variant::new(&code(wgsl), [2, 2, 1]);
    variant.writable = vec![Binding::image("_Volume", 0, 2)];
    Kernel::single("Write", variant)
}

#[test]
fn a_cube_record_no_kernel_binds_is_made_and_reads_as_zeros() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut world = world();
    let cube = shaped_target(&mut world, (RGBA8, 1), (dimension::CUBE, 1));
    let request = world
        .request_texture_pixels(cube, 0, 0, SIDE, SIDE)
        .unwrap();

    let outcome = flush(&mut host, &mut world);

    assert!(failures(&outcome).is_empty(), "{:?}", failures(&outcome));
    let zeros = vec![0.0; (SIDE * SIDE * 4) as usize];
    assert_eq!(world.take_texture_pixels(request), Some(zeros));
}
