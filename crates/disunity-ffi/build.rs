//! Generates bindings in Cargo's output directory and rejects checked-in ABI drift.

use std::path::PathBuf;
mod build_bindings;
mod build_errors;
mod build_layout;

const ABI_SOURCES: &[&str] = &[
    "src/abi.rs",
    "src/assets.rs",
    "src/camera.rs",
    "src/canvas.rs",
    "src/canvas_group.rs",
    "src/canvas_geometry.rs",
    "src/canvas_renderer.rs",
    "src/mesh_renderer.rs",
    "src/sorting_group.rs",
    "src/camera_layers.rs",
    "src/camera_data.rs",
    "src/light.rs",
    "src/sprites.rs",
    "src/presentation.rs",
    "src/lib.rs",
    "src/scheduling.rs",
    "src/buffers.rs",
    "src/cloning.rs",
    "src/construction.rs",
    "src/hierarchy.rs",
    "src/callbacks.rs",
    "src/input.rs",
    "src/input_axes.rs",
    "src/physics.rs",
    "src/physics_materials.rs",
    "src/ui.rs",
    "src/object_api.rs",
    "src/text_style.rs",
    "src/tilemap.rs",
    "src/textures.rs",
    "src/meshes.rs",
    "src/mesh_channels.rs",
    "src/faces.rs",
    "src/compute.rs",
    "src/compute_bindings.rs",
    "src/gpu_flush.rs",
    "src/graphics.rs",
    "src/graphics_commands.rs",
    "src/graphics_context.rs",
    "src/graphics_offscreen.rs",
    "src/graphics_properties.rs",
    "src/graphics_readback.rs",
    "src/graphics_renderer_lists.rs",
    "src/graphics_snapshot.rs",
    "src/script_messages.rs",
    "src/presentation_ui.rs",
    "src/presentation_lights.rs",
];

fn main() {
    for input in ABI_SOURCES.iter().copied().chain([
        "build.rs",
        "build_bindings.rs",
        "build_layout.rs",
        "build_errors.rs",
        "src/errors.rs",
    ]) {
        println!("cargo:rerun-if-changed={input}");
    }
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    build_bindings::generate(&output);
    build_errors::generate(&output);
    generate_native_records();
    build_layout::generate(&output);
}

fn generate_native_records() {
    let mut parts = vec![NATIVE_HEADER.to_owned()];
    for file in ABI_SOURCES {
        let source = std::fs::read_to_string(file).expect("read ABI definitions");
        for record in source.split("#[repr(C)]").skip(1) {
            let end = record.find("\n}").expect("flat C record ends") + 2;
            let record = format!("#[repr(C)]{}\n", &record[..end]);
            if parts.last().expect("first part").lines().count() + record.lines().count() > 650 {
                parts.push(NATIVE_HEADER.to_owned());
            }
            parts.last_mut().expect("record part").push_str(&record);
        }
    }
    let output = &mut parts[0];
    output.push_str(&format!(
        "/// The ABI revision these records belong to.
pub const DISUNITY_ABI_VERSION: u32 = {};

",
        abi_version()
    ));
    let source = std::fs::read_to_string("src/abi.rs").expect("ABI reasons");
    let reason = source.split("#[repr(u64)]").nth(1).expect("reason enum");
    let end = reason.find("\n}").expect("enum end") + 2;
    output.push_str("#[repr(u64)]");
    output.push_str(&reason[..end]);
    output.push('\n');
    for index in 1..parts.len() {
        let number = index + 1;
        parts[0].push_str(&format!(
            "#[path = \"native.{number}.g.rs\"]\nmod records_{number};\npub use records_{number}::*;\n"
        ));
    }
    write_native_parts(parts);
}

const NATIVE_HEADER: &str = "// Generated from disunity-ffi ABI definitions. Do not edit.\n\n";

fn write_native_parts(parts: Vec<String>) {
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let checked = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest directory"),
    )
    .join("../disunity-runtime/src");
    let mut files = Vec::new();
    for (index, contents) in parts.into_iter().enumerate() {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!(".{}", index + 1)
        };
        let path = out.join(format!("NativeBindings{suffix}.g.rs"));
        let checked = checked.join(format!("native{suffix}.g.rs"));
        assert!(
            contents.lines().count() < 750,
            "split generated native records"
        );
        std::fs::write(&path, &contents).expect("write native records");
        files.push((path, checked, contents));
    }
    for (path, checked, contents) in files {
        println!("cargo:rerun-if-changed={}", checked.display());
        assert_eq!(
            std::fs::read(&checked).unwrap_or_default(),
            contents.as_bytes(),
            "native ABI bindings drifted; copy {} to {}",
            path.display(),
            checked.display()
        );
    }
}

/// The revision declared by `DISUNITY_ABI_VERSION` in `src/abi.rs`, so generated consumers never
/// repeat the number.
pub(crate) fn abi_version() -> u32 {
    let source = std::fs::read_to_string("src/abi.rs").expect("ABI version source");
    let line = source
        .lines()
        .find_map(|line| line.strip_prefix("pub const DISUNITY_ABI_VERSION: u32 = "))
        .expect("ABI version constant");
    line.trim_end_matches(';')
        .parse()
        .expect("numeric ABI version")
}
