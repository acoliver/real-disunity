//! Storage images: what a module's declaration reflects as, and how a kernel's bindings fit a
//! texture the runtime holds.

use worldrunner_render_program::unity::module::{
    ModuleError, StageKind, StageModule, StageProgram,
};
use worldrunner_unity_serialized::shader_program::ProgramParameters;

use crate::common::{same, spirv};

fn kernel(declaration: &str) -> Result<StageProgram, ModuleError> {
    let wgsl = format!(
        "{declaration}
         @compute @workgroup_size(1) fn main() {{}}"
    );
    let module = StageModule {
        words: spirv(&wgsl),
        entry_point: "main".to_owned(),
        kind: StageKind::Compute,
        bindings: vec![same(0, 0)],
    };
    StageProgram::new(module, ProgramParameters::default())
}

/// A kernel that uses `image` through the call `use_image`, so that the entry point reaches it.
fn using(declaration: &str, use_image: &str) -> StageProgram {
    let wgsl = format!(
        "{declaration}
         @compute @workgroup_size(1) fn main() {{ {use_image} }}"
    );
    let module = StageModule {
        words: spirv(&wgsl),
        entry_point: "main".to_owned(),
        kind: StageKind::Compute,
        bindings: vec![same(0, 0)],
    };
    StageProgram::new(module, ProgramParameters::default()).unwrap()
}

fn storage(
    access: wgpu::StorageTextureAccess,
    format: wgpu::TextureFormat,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindingType {
    wgpu::BindingType::StorageTexture {
        access,
        format,
        view_dimension,
    }
}

#[test]
fn a_storage_image_reflects_with_its_access_format_and_dimension() {
    use wgpu::StorageTextureAccess::{ReadOnly, ReadWrite, WriteOnly};
    use wgpu::TextureFormat as F;
    use wgpu::TextureViewDimension as D;
    let cases = [
        (
            "texture_storage_2d<rgba8unorm, write>",
            "textureStore(image, vec2<i32>(0, 0), vec4<f32>(1.0));",
            storage(WriteOnly, F::Rgba8Unorm, D::D2),
        ),
        (
            "texture_storage_2d<r32float, read>",
            "_ = textureLoad(image, vec2<i32>(0, 0));",
            storage(ReadOnly, F::R32Float, D::D2),
        ),
        (
            "texture_storage_2d<r32uint, read_write>",
            "let t = textureLoad(image, vec2<i32>(0, 0)); textureStore(image, vec2<i32>(0, 0), t);",
            storage(ReadWrite, F::R32Uint, D::D2),
        ),
        (
            "texture_storage_2d_array<rgba16float, write>",
            "textureStore(image, vec2<i32>(0, 0), 1, vec4<f32>(1.0));",
            storage(WriteOnly, F::Rgba16Float, D::D2Array),
        ),
        (
            "texture_storage_3d<rgba32float, write>",
            "textureStore(image, vec3<i32>(0, 0, 0), vec4<f32>(1.0));",
            storage(WriteOnly, F::Rgba32Float, D::D3),
        ),
        (
            "texture_storage_1d<r32sint, write>",
            "textureStore(image, 0, vec4<i32>(1));",
            storage(WriteOnly, F::R32Sint, D::D1),
        ),
    ];
    for (declaration, call, expected) in cases {
        let stage = using(
            &format!("@group(0) @binding(0) var image: {declaration};"),
            call,
        );
        let found = stage.resources();
        assert_eq!(found.len(), 1, "{declaration}");
        assert_eq!(found[0].ty, expected, "{declaration}");
        assert_eq!(found[0].min_size, 0, "{declaration}");
    }
}

#[test]
fn a_storage_image_the_entry_point_does_not_use_is_not_reflected() {
    let stage = kernel("@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;")
        .unwrap();
    assert!(stage.resources().is_empty());
}
