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
    let stage =
        kernel("@group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;").unwrap();
    assert!(stage.resources().is_empty());
}

/// Whether SPIR-V decorates anything `NonReadable`. DXC does not decorate an `RWTexture2D` so,
/// whether or not the shader loads from it, and naga's writer does not decorate a `read_write`
/// image either.
fn decorates_non_readable(words: &[u32]) -> bool {
    const OP_DECORATE: u32 = 71;
    const NON_READABLE: u32 = 25;
    let mut at = 5;
    while at < words.len() {
        let length = usize::try_from(words[at] >> 16).unwrap().max(1);
        if words[at] & 0xffff == OP_DECORATE && words.get(at + 2) == Some(&NON_READABLE) {
            return true;
        }
        at += length;
    }
    false
}

/// A compute stage of `wgsl`'s entry point `entry`, whose SPIR-V marks no image unreadable.
fn undecorated(wgsl: &str, entry: &str) -> StageProgram {
    let words = spirv(wgsl);
    assert!(!decorates_non_readable(&words), "{wgsl}");
    let module = StageModule {
        words,
        entry_point: entry.to_owned(),
        kind: StageKind::Compute,
        bindings: vec![same(0, 0)],
    };
    StageProgram::new(module, ProgramParameters::default()).unwrap()
}

/// The access the module the stage compiles from declares for its image at set 0 binding 0.
fn declared_access(stage: &StageProgram) -> naga::StorageAccess {
    let module = stage.naga_module();
    let address = naga::ResourceBinding {
        group: 0,
        binding: 0,
    };
    let (_, global) = module
        .global_variables
        .iter()
        .find(|(_, global)| global.binding == Some(address))
        .unwrap();
    match module.types[global.ty].inner {
        naga::TypeInner::Image {
            class: naga::ImageClass::Storage { access, .. },
            ..
        } => access,
        ref other => panic!("set 0 binding 0 is not a storage image: {other:?}"),
    }
}

const RG32UINT_READ_WRITE: &str =
    "@group(0) @binding(0) var image: texture_storage_2d<rg32uint, read_write>;";

#[test]
fn a_read_write_storage_image_the_entry_point_only_stores_to_reflects_write_only() {
    use wgpu::StorageTextureAccess::WriteOnly;
    use wgpu::TextureFormat as F;
    use wgpu::TextureViewDimension as D;
    let store = "textureStore(image, vec2<i32>(0, 0), vec4<u32>(1u, 2u, 0u, 0u));";
    let cases = [
        ("", store.to_owned()),
        (
            "fn put() { textureStore(image, vec2<i32>(1, 0), vec4<u32>(3u)); }",
            format!("put(); {store}"),
        ),
        ("", format!("let size = textureDimensions(image); {store}")),
    ];
    for (helpers, body) in cases {
        let wgsl = format!(
            "{RG32UINT_READ_WRITE}
             {helpers}
             @compute @workgroup_size(1) fn main() {{ {body} }}"
        );
        let stage = undecorated(&wgsl, "main");
        let found = stage.resources();
        assert_eq!(found.len(), 1, "{body}");
        assert_eq!(
            found[0].ty,
            storage(WriteOnly, F::Rg32Uint, D::D2),
            "{body}"
        );
        assert_eq!(
            declared_access(&stage),
            naga::StorageAccess::STORE,
            "{body}"
        );
    }
}

#[test]
fn a_read_write_storage_image_the_entry_point_loads_from_stays_read_write() {
    use wgpu::StorageTextureAccess::ReadWrite;
    use wgpu::TextureFormat as F;
    use wgpu::TextureViewDimension as D;
    let cases = [
        (
            "",
            "let t = textureLoad(image, vec2<i32>(0, 0)); \
             textureStore(image, vec2<i32>(1, 0), t);",
        ),
        (
            "fn fetch() -> vec4<u32> { return textureLoad(image, vec2<i32>(0, 0)); }",
            "textureStore(image, vec2<i32>(1, 0), fetch());",
        ),
    ];
    for (helpers, body) in cases {
        let wgsl = format!(
            "{RG32UINT_READ_WRITE}
             {helpers}
             @compute @workgroup_size(1) fn main() {{ {body} }}"
        );
        let stage = undecorated(&wgsl, "main");
        let found = stage.resources();
        assert_eq!(found.len(), 1, "{body}");
        assert_eq!(
            found[0].ty,
            storage(ReadWrite, F::Rg32Uint, D::D2),
            "{body}"
        );
        assert_eq!(
            declared_access(&stage),
            naga::StorageAccess::LOAD | naga::StorageAccess::STORE,
            "{body}"
        );
    }
}

#[test]
fn an_image_one_entry_point_loads_from_stays_read_write_for_every_entry_point() {
    // Every entry point of a module compiles from one shader module, whose declaration the
    // pipeline layout must match, so a load in any entry point keeps the image read-write.
    use wgpu::StorageTextureAccess::ReadWrite;
    use wgpu::TextureFormat as F;
    use wgpu::TextureViewDimension as D;
    let wgsl = format!(
        "{RG32UINT_READ_WRITE}
         @compute @workgroup_size(1) fn stores() {{
             textureStore(image, vec2<i32>(0, 0), vec4<u32>(1u));
         }}
         @compute @workgroup_size(1) fn copies() {{
             textureStore(image, vec2<i32>(1, 0), textureLoad(image, vec2<i32>(0, 0)));
         }}"
    );
    for entry in ["stores", "copies"] {
        let stage = undecorated(&wgsl, entry);
        assert_eq!(
            stage.resources()[0].ty,
            storage(ReadWrite, F::Rg32Uint, D::D2),
            "{entry}"
        );
    }
}
