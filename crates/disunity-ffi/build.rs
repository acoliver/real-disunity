//! Generates bindings in Cargo's output directory and rejects checked-in ABI drift.

use std::path::PathBuf;
mod build_abi;
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
    "src/environment.rs",
    "src/gpu_flush.rs",
    "src/graphics.rs",
    "src/graphics_batch.rs",
    "src/graphics_commands.rs",
    "src/graphics_context.rs",
    "src/graphics_offscreen.rs",
    "src/graphics_properties.rs",
    "src/graphics_readback.rs",
    "src/graphics_renderer_lists.rs",
    "src/graphics_snapshot.rs",
    "src/script_messages.rs",
    "src/shadows.rs",
    "src/presentation_ui.rs",
    "src/presentation_lights.rs",
];

fn main() {
    for input in ABI_SOURCES.iter().copied().chain([
        "build.rs",
        "build_abi.rs",
        "build_bindings.rs",
        "build_layout.rs",
        "build_errors.rs",
        "src/errors.rs",
    ]) {
        println!("cargo:rerun-if-changed={input}");
    }
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let sources: Vec<String> = ABI_SOURCES
        .iter()
        .map(|file| std::fs::read_to_string(file).expect("read ABI definitions"))
        .collect();
    build_bindings::generate(&output);
    build_errors::generate(&output);
    generate_surface(&output, &sources);
    generate_native_records(&sources);
    build_layout::generate(&output);
}

/// Writes the ABI surface of `sources` and rejects a checked-in `abi/surface.g.txt` that differs.
/// The library, the runtime and the managed assembly hash that file into the ABI revision.
fn generate_surface(output: &std::path::Path, sources: &[String]) {
    let surface = build_abi::surface(
        ABI_SOURCES
            .iter()
            .copied()
            .zip(sources.iter().map(String::as_str)),
    );
    let generated = output.join("surface.g.txt");
    std::fs::write(&generated, &surface).expect("write the ABI surface");
    let checked = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets manifest directory"),
    )
    .join("abi/surface.g.txt");
    println!("cargo:rerun-if-changed={}", checked.display());
    assert!(
        std::fs::read_to_string(&checked).unwrap_or_default() == surface,
        "the ABI surface changed; review the diff, then copy {} to {}",
        generated.display(),
        checked.display()
    );
}

fn generate_native_records(sources: &[String]) {
    let mut parts = vec![NATIVE_HEADER.to_owned()];
    for (file, source) in ABI_SOURCES.iter().zip(sources) {
        for record in build_abi::record_sources(file, source) {
            let record = format!("{record}\n");
            if parts.last().expect("first part").lines().count() + record.lines().count() > 650 {
                parts.push(NATIVE_HEADER.to_owned());
            }
            parts.last_mut().expect("record part").push_str(&record);
        }
    }
    let output = &mut parts[0];
    let source = std::fs::read_to_string("src/abi.rs").expect("ABI reasons");
    output.push_str(&format!(
        "{}\n\n/// The ABI revision these records belong to: the hash of the ABI surface the library\n/// build checked against `abi/surface.g.txt`.\npub const DISUNITY_ABI_VERSION: u32 =\n    abi_revision(include_bytes!(\"../../disunity-ffi/abi/surface.g.txt\"));\n\n",
        revision_function(&source)
    ));
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

/// The `abi_revision` function of `src/abi.rs` with its documentation, so the runtime derives the
/// revision with the code the library does.
fn revision_function(abi: &str) -> &str {
    let start = abi
        .find("pub const fn abi_revision")
        .expect("revision function");
    let documentation = abi[..start].rfind("\n\n").expect("revision documentation") + 2;
    let end = start + abi[start..].find("\n}\n").expect("function end") + 2;
    &abi[documentation..end]
}
