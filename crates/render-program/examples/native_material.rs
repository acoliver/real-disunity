//! Registers a WGSL program, makes a material from it, sets a colour and draws the material into
//! an offscreen target through a frame of the caller's submission authority. It then reads the
//! target back and checks the pixel against the colour that was set.
//!
//! ```sh
//! cargo run -p worldrunner-render-program --example native_material
//! ```
//!
//! `docs/render-program-native.md` describes each call.

use std::error::Error;

use wgpu::{ShaderStages, TextureFormat};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, PropertyId, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::constants::{
    BoundConstantLayout, ConstantLayout, Member, ScalarKind,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::module::{StageDeclarations, StageKind};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::registry::{ProgramDesc, StageDesc};
use worldrunner_render_program::runtime::{
    Geometry, MaterialDraw, MaterialHandle, ProgramRuntime, VertexInput,
};
use worldrunner_render_program::state::RenderState;

const SHADER: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
@vertex fn vs(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return tint.color; }";

/// A uniform buffer at group 0 binding 0 that holds the `color` property as a `vec4<f32>`.
fn fragment_declarations() -> StageDeclarations {
    let at = BindingAddress { set: 0, binding: 0 };
    StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at,
            source: BindingSource::Property(PropertyId::new("tint")),
            visibility: ShaderStages::FRAGMENT,
            kind: ResourceKind::Uniform { min_size: 16 },
        }]),
        constants: vec![BoundConstantLayout {
            at,
            layout: ConstantLayout {
                name: "Tint".to_owned(),
                byte_size: 16,
                members: vec![Member {
                    source: BindingSource::Property(PropertyId::new("color")),
                    kind: ScalarKind::F32,
                    rows: 1,
                    cols: 4,
                    array: None,
                    offset: 0,
                    matrix: None,
                }],
            },
        }],
    }
}

/// One counter-clockwise triangle that covers all of clip space.
fn triangle(runtime: &mut ProgramRuntime) -> Result<VertexInput, ProgramError> {
    let corners: [[f32; 3]; 3] = [[-1.0, -1.0, 0.5], [3.0, -1.0, 0.5], [-1.0, 3.0, 0.5]];
    let bytes: Vec<u8> = corners
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    Ok(VertexInput {
        buffer: runtime.create_buffer("triangle", &bytes)?,
        stride: 12,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
    })
}

/// Registers the shader and makes a material of it with the colour `color`.
fn tint_material(
    runtime: &mut ProgramRuntime,
    color: [f32; 4],
) -> Result<MaterialHandle, ProgramError> {
    let program = runtime.register_program(ProgramDesc::single(
        "tint",
        RenderState::opaque_depth(),
        StageDesc::wgsl(
            SHADER,
            "vs",
            StageKind::Vertex,
            StageDeclarations::default(),
        ),
        StageDesc::wgsl(SHADER, "fs", StageKind::Fragment, fragment_declarations()),
    ))?;
    let mut initial = PropertySet::new();
    initial.set("color", PropertyValue::Vector([1.0, 1.0, 1.0, 1.0]));
    let material = runtime.create_material(program, initial)?;
    runtime.set_property(material, "color", PropertyValue::Vector(color))?;
    Ok(material)
}

fn main() -> Result<(), Box<dyn Error>> {
    let instance = headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue, _) =
        pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))?;
    let authority = SubmissionAuthority::new(queue);
    let mut runtime =
        ProgramRuntime::new(device.clone(), authority.clone(), AllocationBudget(1 << 24));

    let color = [0.25, 0.5, 0.75, 1.0];
    let material = tint_material(&mut runtime, color)?;
    let vertices = triangle(&mut runtime)?;
    let target = runtime.create_render_target(
        "offscreen",
        [4, 4],
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Depth32Float),
    )?;
    let draw = MaterialDraw {
        material,
        vertices: vec![vertices],
        geometry: Geometry::Vertices { first: 0, count: 3 },
        instances: 0..1,
        overrides: PropertySet::new(),
    };

    let mut frame = authority.begin(&device)?;
    runtime.clear_target(&mut frame, target, [0.0; 4], 1.0)?;
    runtime.draw_material(&mut frame, target, &draw)?;
    let pending = runtime.read_target(&mut frame, target)?;
    let submission = frame.finish().submit()?;
    let image = pending.resolve(submission)?;

    let pixel = image.pixel(2, 2);
    println!("pixel (2, 2) = {pixel:?}");
    for (code, value) in pixel.iter().zip(color) {
        let want = (value * 255.0).round();
        assert!(
            (f32::from(*code) - want).abs() <= 1.0,
            "got {code}, want {want}"
        );
    }
    Ok(())
}
