//! Textures and buffers bound through properties that hold resource handles.

use wgpu::ShaderStages;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, PropertyId, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{StageDeclarations, StageKind};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::registry::{ProgramDesc, StageDesc};
use worldrunner_render_program::runtime::Resource;
use worldrunner_render_program::state::RenderState;

use crate::common::{
    Gpu, RGBA8, Rig, WGSL, assert_all, draw_of, full_target_triangle, vertex_stage,
};

fn binding(set: u32, binding: u32, source: BindingSource, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        at: BindingAddress { set, binding },
        source,
        visibility: ShaderStages::FRAGMENT,
        kind,
    }
}

fn property(name: &str) -> BindingSource {
    BindingSource::Property(PropertyId::new(name))
}

fn fragment(entry: &str, bindings: Vec<ResourceBinding>) -> StageDesc {
    StageDesc::wgsl(
        WGSL,
        entry,
        StageKind::Fragment,
        StageDeclarations {
            resources: ResourceLayout::new(bindings),
            constants: Vec::new(),
        },
    )
}

/// A rig whose program samples the texture property `albedo` and writes what it reads.
fn textured(gpu: &Gpu) -> Rig {
    let mut rig = Rig::new(gpu);
    let texture = binding(
        1,
        0,
        property("albedo"),
        ResourceKind::Texture {
            dimension: wgpu::TextureViewDimension::D2,
            sample: wgpu::TextureSampleType::Float { filterable: true },
            multisampled: false,
        },
    );
    let sampler = binding(
        1,
        1,
        BindingSource::Unnamed,
        ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering),
    );
    rig.program = rig
        .runtime
        .register_program(ProgramDesc::single(
            "textured",
            RenderState::opaque_depth(),
            vertex_stage("vs"),
            fragment("fs_texture", vec![texture, sampler]),
        ))
        .unwrap();
    rig.material = rig
        .runtime
        .create_material(rig.program, PropertySet::new())
        .unwrap();
    rig
}

fn uniform_texture(rig: &mut Rig, texel: [u8; 4]) -> Resource {
    let bytes: Vec<u8> = texel.iter().copied().cycle().take(16).collect();
    Resource::Texture(
        rig.runtime
            .create_texture_2d("albedo", [2, 2], RGBA8, &bytes)
            .unwrap(),
    )
}

fn codes(texel: [u8; 4]) -> [f32; 4] {
    texel.map(|code| f32::from(code) / 255.0)
}

#[test]
fn a_texture_property_binds_the_texture_its_handle_names() {
    let gpu = Gpu::new();
    let mut rig = textured(&gpu);
    let (first, second) = ([200, 100, 50, 255], [10, 20, 30, 255]);
    let (one, two) = (
        uniform_texture(&mut rig, first),
        uniform_texture(&mut rig, second),
    );
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(rig.material, triangle, 3);

    rig.runtime
        .set_property(rig.material, "albedo", PropertyValue::Resource(one))
        .unwrap();
    assert_all(&rig.render(&draw).unwrap(), codes(first));
    rig.runtime
        .set_property(rig.material, "albedo", PropertyValue::Resource(two))
        .unwrap();
    assert_all(&rig.render(&draw).unwrap(), codes(second));
}

#[test]
fn a_texture_property_refuses_other_values_a_released_handle_and_a_missing_value() {
    let gpu = Gpu::new();
    let mut rig = textured(&gpu);
    let texel = [200, 100, 50, 255];
    let Resource::Texture(handle) = uniform_texture(&mut rig, texel) else {
        unreachable!()
    };
    let buffer = rig.runtime.create_buffer("bytes", &[0; 16]).unwrap();
    let material = rig.material;

    for wrong in [
        PropertyValue::Vector([1.0; 4]),
        PropertyValue::Resource(Resource::Buffer(buffer)),
    ] {
        let error = rig
            .runtime
            .set_property(material, "albedo", wrong)
            .unwrap_err();
        assert!(
            matches!(&error, ProgramError::WrongPropertyKind { property, .. } if property == "albedo"),
            "{error:?}"
        );
    }

    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(material, triangle, 3);
    assert_eq!(
        rig.render(&draw).unwrap_err(),
        ProgramError::MissingProperty {
            property: "albedo".to_owned(),
            set: 1,
            binding: 0
        }
    );

    rig.runtime
        .set_property(
            material,
            "albedo",
            PropertyValue::Resource(Resource::Texture(handle)),
        )
        .unwrap();
    assert_all(&rig.render(&draw).unwrap(), codes(texel));
    assert!(rig.runtime.release_texture(handle).unwrap());
    assert_eq!(
        rig.runtime
            .set_property(
                material,
                "albedo",
                PropertyValue::Resource(Resource::Texture(handle))
            )
            .unwrap_err(),
        ProgramError::UnknownHandle { kind: "texture" }
    );
}

#[test]
fn a_buffer_property_binds_a_uniform_buffer_the_caller_owns() {
    let gpu = Gpu::new();
    let mut rig = Rig::new(&gpu);
    let block = binding(
        1,
        2,
        property("block"),
        ResourceKind::Uniform { min_size: 16 },
    );
    let program = rig
        .runtime
        .register_program(ProgramDesc::single(
            "block",
            RenderState::opaque_depth(),
            vertex_stage("vs"),
            fragment("fs_block", vec![block]),
        ))
        .unwrap();
    let material = rig
        .runtime
        .create_material(program, PropertySet::new())
        .unwrap();
    rig.material = material;
    let color = [0.5_f32, 0.25, 0.125, 1.0];
    let bytes: Vec<u8> = color.iter().flat_map(|c| c.to_le_bytes()).collect();
    let buffer = rig.runtime.create_buffer("block", &bytes).unwrap();
    let texture = uniform_texture(&mut rig, [1, 2, 3, 4]);
    let triangle = full_target_triangle(&mut rig.runtime);
    let draw = draw_of(material, triangle, 3);

    let error = rig
        .runtime
        .set_property(material, "block", PropertyValue::Resource(texture))
        .unwrap_err();
    assert!(
        matches!(error, ProgramError::WrongPropertyKind { .. }),
        "{error:?}"
    );
    rig.runtime
        .set_property(
            material,
            "block",
            PropertyValue::Resource(Resource::Buffer(buffer)),
        )
        .unwrap();
    assert_all(&rig.render(&draw).unwrap(), color);
}
