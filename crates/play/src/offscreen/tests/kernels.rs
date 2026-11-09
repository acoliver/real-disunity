//! Loading compute kernels from a synthetic player data directory: serialized compute shader
//! objects written by the layout writer of the unity-serialized tests, with SPIR-V compiled from
//! WGSL.

use std::path::Path;

use worldrunner_render_program::module::StageKind;
use worldrunner_render_program::unity::synthetic::compute_spirv;
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    write::{FileContents, ObjectData},
};
use worldrunner_world_simulation::graphics::{ComputeKernel, ShaderSource, WorldComputeShader};

use super::super::compute::{ComputeLoadError, ComputeSupply};
use super::super::kernels::PlayerKernels;

#[path = "../../../../unity-serialized/tests/support/compute_bytes.rs"]
pub(super) mod compute_bytes;
use compute_bytes::{
    Binding, BufferLayout, GLSL, Kernel, Member, Platform, SPIRV, Variant, compute_shader_bytes,
};

/// The shape of the terrain read-back: it reads an R16 float render texture and writes a storage
/// buffer, and a vector in a constant buffer scales the values.
pub(super) const KERNEL: &str = "
struct Params { scale: vec4<f32> }
@group(0) @binding(0) var heights: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    output[id.y * 4u + id.x] = textureLoad(heights, vec2<i32>(id.xy), 0).x * params.scale.x;
}";

/// A kernel storing into a storage texture.
const PAINT: &str = "
@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
@compute @workgroup_size(2, 2, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(image, vec2<i32>(id.xy), vec4<f32>(1.0));
}";

fn code(wgsl: &str) -> Vec<u8> {
    compute_spirv(wgsl, "main")
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
}

/// The copy kernel's one variant, which binds what the kernel declares.
fn copy_variant() -> Variant {
    let mut variant = Variant::new(&code(KERNEL), [2, 2, 1]);
    variant.constant_buffers = vec![Binding::constant_buffer("CGlobals", 0, 2)];
    variant.textures = vec![Binding::texture("_Heights", 0, None, 2)];
    variant.writable = vec![Binding::writable("_Output", 1)];
    variant
}

/// The paint kernel's one variant, which binds the storage texture it writes.
pub(super) fn paint_variant() -> Variant {
    let mut variant = Variant::new(&code(PAINT), [2, 2, 1]);
    variant.writable = vec![Binding::image("_Image", 0, 2)];
    variant
}

fn globals() -> BufferLayout {
    BufferLayout {
        name: "CGlobals".to_owned(),
        size: 16,
        members: vec![Member {
            name: "_Scale".to_owned(),
            words: [0, 0, 0, 1, 4],
        }],
    }
}

/// A compute shader with a GLSL platform and a SPIR-V one, each with the kernels `kernels`.
pub(super) fn shader_bytes(name: &str, kernels: Vec<Kernel>) -> Vec<u8> {
    let glsl = Platform::new(
        GLSL,
        10,
        kernels
            .iter()
            .map(|kernel| Kernel::single(&kernel.name, Variant::new(b"#version 430", [2, 2, 1])))
            .collect(),
    );
    let mut vulkan = Platform::new(SPIRV, 0, kernels);
    vulkan.buffers = vec![globals()];
    compute_shader_bytes(name, &[glsl, vulkan], false)
}

/// The compute shader of the terrain read-back's shape, with the kernel `Copy`.
pub(super) fn copy_object() -> Vec<u8> {
    shader_bytes("Heights", vec![Kernel::single("Copy", copy_variant())])
}

/// A compute shader with a GLSL platform only, with the kernel `Copy`.
pub(super) fn text_object() -> Vec<u8> {
    let variant = Variant::new(b"#version 430", [2, 2, 1]);
    let glsl = Platform::new(GLSL, 10, vec![Kernel::single("Copy", variant)]);
    compute_shader_bytes("Text", &[glsl], false)
}

/// Writes `file` under `directory` with one compute shader object per `(path id, bytes)`.
pub(super) fn write_compute(directory: &Path, file: &str, objects: Vec<(i64, Vec<u8>)>) {
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: vec![SerializedType {
            class_id: ClassId::COMPUTE_SHADER,
            stripped: false,
            script_type_index: -1,
            script_id: None,
            old_type_hash: [0; 16],
            type_tree: None,
            dependencies: Vec::new(),
            script_class: None,
        }],
        objects: objects
            .into_iter()
            .map(|(path_id, data)| ObjectData {
                path_id,
                type_index: 0,
                data,
            })
            .collect(),
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    let path = directory.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents.write().unwrap()).unwrap();
}

/// A world compute shader from `file` and `path_id` that lists the kernels `names`.
pub(super) fn world_shader(file: &str, path_id: i64, names: &[&str]) -> WorldComputeShader {
    WorldComputeShader {
        name: "Heights".into(),
        source: ShaderSource {
            file: file.into(),
            path_id,
        },
        kernels: names
            .iter()
            .map(|name| ComputeKernel {
                name: (*name).into(),
                thread_group_size: Some([2, 2, 1]),
            })
            .collect(),
        generation: 0,
    }
}

fn loaded(directory: &Path, objects: Vec<(i64, Vec<u8>)>) -> PlayerKernels {
    write_compute(directory, "resources.assets", objects);
    PlayerKernels::new(directory)
}

#[test]
fn a_kernel_loads_from_the_file_and_object_its_shader_names() {
    let directory = tempfile::tempdir().unwrap();
    let mut kernels = loaded(directory.path(), vec![(7, copy_object())]);
    let shader = world_shader("resources.assets", 7, &["Copy"]);
    let stage = kernels.load(&shader, 0).unwrap();
    assert_eq!(stage.module().kind, StageKind::Compute);
    assert_eq!(stage.module().entry_point, "main");
    let names = |targets: Vec<(String, u32, u32)>| targets;
    let textures: Vec<_> = stage
        .texture_targets()
        .unwrap()
        .iter()
        .map(|t| (t.name.resolve(&[]).unwrap().to_owned(), t.set, t.binding))
        .collect();
    assert_eq!(names(textures), [("_Heights".to_owned(), 0, 0)]);
    let buffers: Vec<_> = stage
        .buffer_targets()
        .unwrap()
        .iter()
        .map(|t| (t.name.resolve(&[]).unwrap().to_owned(), t.set, t.binding))
        .collect();
    assert_eq!(buffers, [("_Output".to_owned(), 0, 1)]);
    assert_eq!(stage.constant_buffer_targets().unwrap().len(), 1);
}

#[test]
fn the_kernel_is_the_one_at_the_index_in_the_order_the_object_lists_them() {
    let directory = tempfile::tempdir().unwrap();
    let object = shader_bytes(
        "Two",
        vec![
            Kernel::single("Paint", paint_variant()),
            Kernel::single("Copy", copy_variant()),
        ],
    );
    let mut kernels = loaded(directory.path(), vec![(1, object)]);
    let shader = world_shader("resources.assets", 1, &["Paint", "Copy"]);
    let second = kernels.load(&shader, 1).unwrap();
    assert_eq!(second.texture_targets().unwrap().len(), 1);
    // The first writes a storage texture, which it names as the kernel declares it.
    let first = kernels.load(&shader, 0).unwrap();
    let images: Vec<_> = first
        .image_targets()
        .unwrap()
        .iter()
        .map(|t| (t.name.resolve(&[]).unwrap().to_owned(), t.set, t.binding))
        .collect();
    assert_eq!(images, [("_Image".to_owned(), 0, 0)]);
    assert!(first.texture_targets().unwrap().is_empty());
}

#[test]
fn a_kernel_the_object_does_not_list_is_named() {
    let directory = tempfile::tempdir().unwrap();
    let mut kernels = loaded(directory.path(), vec![(7, copy_object())]);
    let shader = world_shader("resources.assets", 7, &["Copy", "Extra"]);
    let error = kernels.load(&shader, 1).unwrap_err();
    assert!(
        matches!(error, ComputeLoadError::NoSuchKernel(1)),
        "{error}"
    );
}

#[test]
fn a_kernel_that_the_world_names_differently_is_not_the_one_in_the_object() {
    let directory = tempfile::tempdir().unwrap();
    let mut kernels = loaded(directory.path(), vec![(7, copy_object())]);
    let shader = world_shader("resources.assets", 7, &["Other"]);
    let error = kernels.load(&shader, 0).unwrap_err();
    assert!(matches!(error, ComputeLoadError::Unreadable(_)), "{error}");
}

#[test]
fn a_shader_with_no_spirv_platform_has_no_compiled_code() {
    let directory = tempfile::tempdir().unwrap();
    let mut kernels = loaded(directory.path(), vec![(1, text_object())]);
    let error = kernels
        .load(&world_shader("resources.assets", 1, &["Copy"]), 0)
        .unwrap_err();
    assert!(matches!(error, ComputeLoadError::NoCompiledCode), "{error}");
}

#[test]
fn a_kernel_with_no_variant_for_no_keywords_is_named() {
    let directory = tempfile::tempdir().unwrap();
    let mut kernel = Kernel::single("Copy", copy_variant());
    kernel.keyword_variants = vec![("TILE_8".to_owned(), 0)];
    kernel.keyword_names = vec!["TILE_8".to_owned()];
    let mut kernels = loaded(
        directory.path(),
        vec![(1, shader_bytes("Tiled", vec![kernel]))],
    );
    let error = kernels
        .load(&world_shader("resources.assets", 1, &["Copy"]), 0)
        .unwrap_err();
    assert!(matches!(error, ComputeLoadError::NoVariant), "{error}");
    assert_eq!(error.name(), "NoVariant");
}

#[test]
fn a_file_or_object_that_is_not_there_is_unreadable() {
    let directory = tempfile::tempdir().unwrap();
    let mut kernels = loaded(directory.path(), vec![(7, copy_object())]);
    for (file, path_id) in [("missing.assets", 7), ("resources.assets", 8)] {
        let error = kernels
            .load(&world_shader(file, path_id, &["Copy"]), 0)
            .unwrap_err();
        assert!(
            matches!(error, ComputeLoadError::Unreadable(_)),
            "{file}: {error}"
        );
    }
}

#[test]
fn a_path_that_leaves_the_data_directory_is_refused_before_it_is_opened() {
    let directory = tempfile::tempdir().unwrap();
    let mut kernels = PlayerKernels::new(directory.path().join("data"));
    for file in ["../level0", "/etc/passwd", "a/../../b", "..", "a\\b", ""] {
        let error = kernels
            .load(&world_shader(file, 1, &["Copy"]), 0)
            .unwrap_err();
        assert!(
            matches!(error, ComputeLoadError::Unreadable(_)),
            "{file:?}: {error}"
        );
    }
}

#[test]
fn an_object_is_decoded_once_however_many_kernels_are_loaded_from_it() {
    let directory = tempfile::tempdir().unwrap();
    let object = shader_bytes(
        "Two",
        vec![
            Kernel::single("Copy", copy_variant()),
            Kernel::single("Again", copy_variant()),
        ],
    );
    let mut kernels = loaded(directory.path(), vec![(1, object)]);
    let shader = world_shader("resources.assets", 1, &["Copy", "Again"]);
    kernels.load(&shader, 0).unwrap();
    // The file is gone, so a second decode would fail; the cached one serves.
    std::fs::remove_file(directory.path().join("resources.assets")).unwrap();
    assert!(kernels.load(&shader, 1).is_ok());
}
