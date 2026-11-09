//! The counts `player-compute-report` prints, on a synthetic player whose compute shaders are
//! written by the layout writer and compiled from WGSL.

#[path = "../../unity-serialized/tests/support/compute_bytes.rs"]
mod layout;

use layout::{
    Binding, BufferLayout, GLSL, Kernel, Member, Platform, SPIRV, Variant, compute_shader_bytes,
};
use naga::ShaderStage;
use worldrunner_render_program::unity::synthetic::stage_spirv;
use worldrunner_unity_import::{
    player_compute_report::{ComputeReport, compute_report},
    shader_prep::Backend,
};
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    write::{FileContents, ObjectData},
};

/// A kernel that reads a texture and a constant buffer in set 1 and writes a storage buffer.
/// naga wraps a uniform in a struct of one member at offset 0, so the constant buffer has one
/// member for its offsets to be the module's block's.
const COPY: &str = "
struct Globals { width: u32 }
@group(1) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(0) var heights: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;
@compute @workgroup_size(8, 8, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    output[id.y * globals.width + id.x] = textureLoad(heights, vec2<i32>(id.xy), 0).x;
}";

/// A kernel that writes a storage texture in a format every device binds.
const IMAGE: &str = "
@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
@compute @workgroup_size(4, 4, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(image, vec2<i32>(id.xy), vec4<f32>(1.0));
}";

/// A kernel that writes a storage texture in a format only a device with a feature binds.
const HALF_IMAGE: &str = "
@group(0) @binding(0) var image: texture_storage_2d<r16float, write>;
@compute @workgroup_size(4, 4, 1) fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    textureStore(image, vec2<i32>(id.xy), vec4<f32>(1.0));
}";

fn code(wgsl: &str) -> Vec<u8> {
    stage_spirv(wgsl, "main", ShaderStage::Compute)
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect()
}

/// The copy kernel's variant with its constant buffer named `buffer`.
fn copy_kernel(name: &str, buffer: &str) -> Kernel {
    let mut variant = Variant::new(&code(COPY), [8, 8, 1]);
    variant.constant_buffers = vec![Binding::constant_buffer(buffer, 1, 0)];
    variant.textures = vec![Binding::texture("_Heights", 0, None, 2)];
    variant.writable = vec![Binding::writable("_Output", 2)];
    Kernel::single(name, variant)
}

fn image_kernel(name: &str, wgsl: &str) -> Kernel {
    let mut variant = Variant::new(&code(wgsl), [4, 4, 1]);
    variant.writable = vec![Binding::image("_Image", 0, 2)];
    Kernel::single(name, variant)
}

/// A constant buffer of one integer at `offset`.
fn width_at(name: &str, offset: i32) -> BufferLayout {
    BufferLayout {
        name: name.to_owned(),
        size: 16,
        members: vec![Member {
            name: "_Width".to_owned(),
            words: [1, offset, 0, 1, 1],
        }],
    }
}

fn glsl_only() -> Vec<u8> {
    let variant = Variant::new(b"#version 430", [1, 1, 1]);
    let platform = Platform::new(GLSL, 10, vec![Kernel::single("Text", variant)]);
    compute_shader_bytes("GlslOnly", &[platform], false)
}

fn both_platforms() -> Vec<u8> {
    let glsl = Platform::new(
        GLSL,
        10,
        vec![
            Kernel::single("Copy", Variant::new(b"#version 430", [8, 8, 1])),
            Kernel::single("Paint", Variant::new(b"#version 430", [4, 4, 1])),
            Kernel::single("Skew", Variant::new(b"#version 430", [8, 8, 1])),
            Kernel::single("Smooth", Variant::new(b"#version 430", [4, 4, 1])),
        ],
    );
    // The skewed kernel is the copy kernel, whose constant buffer states another offset.
    let kernels = vec![
        copy_kernel("Copy", "CGlobals"),
        image_kernel("Paint", IMAGE),
        copy_kernel("Skew", "CSkewed"),
        image_kernel("Smooth", HALF_IMAGE),
    ];
    let mut vulkan = Platform::new(SPIRV, 0, kernels);
    vulkan.buffers = vec![width_at("CGlobals", 0), width_at("CSkewed", 8)];
    compute_shader_bytes("Both", &[glsl, vulkan], false)
}

fn write_player(dir: &std::path::Path) -> std::path::PathBuf {
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
        objects: [both_platforms(), glsl_only(), vec![1, 2, 3]]
            .into_iter()
            .zip(1..)
            .map(|(data, path_id)| ObjectData {
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
    let path = dir.join("resources.assets");
    std::fs::write(&path, contents.write().unwrap()).unwrap();
    path
}

fn assert_tables_agree(report: &ComputeReport, variants: usize) {
    let all = format!("{variants} of {variants}");
    assert_eq!(report.bindings.to_string(), all);
    assert_eq!(report.slot_flags.to_string(), all);
    assert_eq!(report.group_sizes.to_string(), all);
}

fn assert_written_by_every_backend(report: &ComputeReport, modules: usize) {
    for backend in Backend::ALL {
        assert_eq!(
            report.written.get(&backend),
            Some(&modules),
            "{}",
            backend.name()
        );
    }
}

#[test]
fn counts_the_kernels_variants_modules_and_what_becomes_of_them() {
    let dir = tempfile::tempdir().unwrap();
    let report = compute_report(&[write_player(dir.path())]).unwrap();

    // Three objects: two decode and one is three bytes of nothing.
    assert_eq!(report.shaders, 2);
    assert_eq!(report.unreadable, 1);
    assert_eq!(report.platforms.get(&GLSL), Some(&2));
    assert_eq!(report.platforms.get(&SPIRV), Some(&1));
    assert_eq!((report.kernels, report.variants), (4, 4));
    assert_eq!(report.default_variants.to_string(), "4 of 4");
    // The copy and skewed kernels share one module.
    assert_eq!((report.modules, report.distinct_modules), (4, 3));
    assert_eq!((report.prepared, report.ready), (4, 4));
    assert_written_by_every_backend(&report, 4);
    assert_tables_agree(&report, 4);
    // The skewed kernel's buffer puts its member at 8, where the block has it at 0.
    assert_eq!(report.constant_buffers.to_string(), "1 of 2");
    // Every kernel becomes a program with a layout and targets, the image kernels too.
    assert_eq!((report.programs, report.layouts, report.targets), (4, 4, 4));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    // The images are counted by what the modules declare, and the one in a format that is not
    // a storage format of every device is counted as needing the device's help.
    assert_eq!(
        report.images.iter().collect::<Vec<_>>(),
        [
            (&"WriteOnly R16Float D2".to_owned(), &1),
            (&"WriteOnly Rgba8Unorm D2".to_owned(), &1)
        ]
    );
    assert_eq!(
        report.beyond_baseline.iter().collect::<Vec<_>>(),
        [(&"WriteOnly R16Float D2".to_owned(), &1)]
    );
}

#[test]
fn a_player_without_compute_shaders_reports_zero() {
    let dir = tempfile::tempdir().unwrap();
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: Vec::new(),
        objects: Vec::new(),
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    let path = dir.path().join("level0");
    std::fs::write(&path, contents.write().unwrap()).unwrap();
    let report = compute_report(&[path]).unwrap();
    assert_eq!((report.shaders, report.kernels, report.variants), (0, 0, 0));
}
