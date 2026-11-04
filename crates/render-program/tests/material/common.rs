//! A GPU, a one-colour WGSL program and the helpers that draw it and read the pixels back.
//!
//! Colours are predicted in closed form from the property a material holds. The real naga,
//! wgpu and `SubmissionAuthority` run every draw.

use wgpu::{ShaderStages, TextureFormat};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, headless_instance, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_gpu::tolerance_test_support::assert_code_within;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, PropertyId, ResourceBinding, ResourceKind, ResourceLayout,
};
use worldrunner_render_program::constants::{
    BoundConstantLayout, ConstantLayout, Member, ScalarKind,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::material::{MaterialDraw, MaterialHandle};
use worldrunner_render_program::module::{StageDeclarations, StageKind};
use worldrunner_render_program::properties::{PropertySet, PropertyValue};
use worldrunner_render_program::registry::{ProgramDesc, ProgramHandle, StageDesc, VariantDesc};
use worldrunner_render_program::runtime::{
    Geometry, ProgramRuntime, TargetHandle, TargetImage, VertexInput,
};
use worldrunner_render_program::state::RenderState;
use worldrunner_render_program::variant::{FeatureId, FeatureSet};

pub const WGSL: &str = "
struct Tint { color: vec4<f32> }
@group(0) @binding(0) var<uniform> tint: Tint;
@vertex fn vs(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position, 1.0);
}
@vertex fn vs_instanced(
    @location(0) position: vec3<f32>,
    @location(1) shift: vec2<f32>,
) -> @builtin(position) vec4<f32> {
    return vec4<f32>(position.xy + shift, position.z, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return tint.color; }
@fragment fn fs_double() -> @location(0) vec4<f32> {
    return vec4<f32>(tint.color.r * 2.0, tint.color.g, tint.color.b, tint.color.a);
}";

pub const COLOR: [f32; 4] = [0.25, 0.5, 0.75, 1.0];
pub const SIZE: u32 = 4;
pub const RGBA8: TextureFormat = TextureFormat::Rgba8Unorm;
pub const DEPTH: TextureFormat = TextureFormat::Depth32Float;

pub struct Gpu {
    pub device: wgpu::Device,
    pub authority: SubmissionAuthority,
}

impl Gpu {
    pub fn new() -> Self {
        let instance = headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("the native material tests need a GPU adapter");
        eprintln!(
            "render-program material tests adapter: {}",
            adapter.get_info().name
        );
        let (device, queue, _) =
            pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))
                .expect("the adapter gives a device");
        Self {
            device,
            authority: SubmissionAuthority::new(queue),
        }
    }

    pub fn runtime(&self) -> ProgramRuntime {
        self.runtime_with(AllocationBudget(1 << 24))
    }

    pub fn runtime_with(&self, budget: AllocationBudget) -> ProgramRuntime {
        ProgramRuntime::new(self.device.clone(), self.authority.clone(), budget)
    }
}

pub fn features(names: &[&str]) -> FeatureSet {
    names.iter().map(|name| FeatureId::new(*name)).collect()
}

fn at0() -> BindingAddress {
    BindingAddress { set: 0, binding: 0 }
}

/// A uniform buffer at set 0 binding 0 holding one `vec4<f32>` that the property `color` fills.
pub fn tint_declarations(visibility: ShaderStages) -> StageDeclarations {
    StageDeclarations {
        resources: ResourceLayout::new(vec![ResourceBinding {
            at: at0(),
            source: BindingSource::Property(PropertyId::new("tint")),
            visibility,
            kind: ResourceKind::Uniform { min_size: 0 },
        }]),
        constants: vec![BoundConstantLayout {
            at: at0(),
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

pub fn vertex_stage(entry: &str) -> StageDesc {
    StageDesc::wgsl(WGSL, entry, StageKind::Vertex, StageDeclarations::default())
}

pub fn fragment_stage(entry: &str) -> StageDesc {
    StageDesc::wgsl(
        WGSL,
        entry,
        StageKind::Fragment,
        tint_declarations(ShaderStages::FRAGMENT),
    )
}

pub fn variant(on: &[&str], vertex: &str, fragment: &str) -> VariantDesc {
    VariantDesc {
        features: features(on),
        vertex: vertex_stage(vertex),
        fragment: fragment_stage(fragment),
    }
}

/// The program every suite starts from: `fs` with no feature, `fs_double` with `DOUBLE`.
pub fn tint_program(runtime: &mut ProgramRuntime) -> ProgramHandle {
    runtime
        .register_program(ProgramDesc {
            label: "tint".to_owned(),
            state: RenderState::opaque_depth(),
            variants: vec![
                variant(&[], "vs", "fs"),
                variant(&["DOUBLE"], "vs", "fs_double"),
            ],
        })
        .unwrap()
}

pub fn colored(color: [f32; 4]) -> PropertySet {
    let mut properties = PropertySet::new();
    properties.set("color", PropertyValue::Vector(color));
    properties
}

/// A runtime with the tint program, a material of [`COLOR`] and a target cleared to
/// transparent black with depth 1.
pub struct Rig {
    pub runtime: ProgramRuntime,
    pub program: ProgramHandle,
    pub material: MaterialHandle,
    pub target: TargetHandle,
}

impl Rig {
    pub fn new(gpu: &Gpu) -> Self {
        let mut runtime = gpu.runtime();
        let program = tint_program(&mut runtime);
        let material = runtime.create_material(program, colored(COLOR)).unwrap();
        let target = runtime
            .create_render_target("native", [SIZE, SIZE], RGBA8, Some(DEPTH))
            .unwrap();
        let mut rig = Self {
            runtime,
            program,
            material,
            target,
        };
        rig.clear();
        rig
    }

    pub fn clear(&mut self) {
        let target = self.target;
        submit(&mut self.runtime, |runtime, frame| {
            runtime.clear_target(frame, target, [0.0; 4], 1.0)
        })
        .unwrap();
    }

    /// Draws `draw` into the target in one frame and reads the target back in the next.
    pub fn render(&mut self, draw: &MaterialDraw) -> Result<TargetImage, ProgramError> {
        let target = self.target;
        submit(&mut self.runtime, |runtime, frame| {
            runtime.draw_material(frame, target, draw)
        })?;
        self.read()
    }

    pub fn read(&mut self) -> Result<TargetImage, ProgramError> {
        let target = self.target;
        let (pending, index) = submit(&mut self.runtime, |runtime, frame| {
            runtime.read_target(frame, target)
        })?;
        pending.resolve(index)
    }

    /// A draw of the rig's material over `area` of clip space at depth `z`.
    pub fn draw(&mut self, area: [f32; 4], z: f32) -> MaterialDraw {
        let vertices = rectangle(&mut self.runtime, area, z);
        draw_of(self.material, vertices, 6)
    }
}

pub fn draw_of(material: MaterialHandle, vertices: VertexInput, count: u32) -> MaterialDraw {
    MaterialDraw {
        material,
        vertices: vec![vertices],
        geometry: Geometry::Vertices { first: 0, count },
        instances: 0..1,
        overrides: PropertySet::new(),
    }
}

pub fn submit<T>(
    runtime: &mut ProgramRuntime,
    record: impl FnOnce(&mut ProgramRuntime, &mut wgpu::CommandEncoder) -> Result<T, ProgramError>,
) -> Result<(T, wgpu::SubmissionIndex), ProgramError> {
    let authority = runtime.authority().clone();
    let device = runtime.device().clone();
    let mut frame = authority.begin(&device)?;
    let value = record(runtime, &mut frame)?;
    let index = frame.finish().submit()?;
    Ok((value, index))
}

fn position_input(runtime: &mut ProgramRuntime, corners: &[[f32; 3]]) -> VertexInput {
    let bytes: Vec<u8> = corners
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    VertexInput {
        buffer: runtime.create_buffer("positions", &bytes).unwrap(),
        stride: 12,
        step: wgpu::VertexStepMode::Vertex,
        attributes: wgpu::vertex_attr_array![0 => Float32x3].to_vec(),
    }
}

/// Two counter-clockwise triangles covering `[x0, y0]..[x1, y1]` of clip space at depth `z`.
pub fn rectangle(runtime: &mut ProgramRuntime, [x0, y0, x1, y1]: [f32; 4], z: f32) -> VertexInput {
    position_input(
        runtime,
        &[
            [x0, y0, z],
            [x1, y0, z],
            [x1, y1, z],
            [x0, y0, z],
            [x1, y1, z],
            [x0, y1, z],
        ],
    )
}

/// One counter-clockwise triangle that covers all of clip space.
pub fn full_target_triangle(runtime: &mut ProgramRuntime) -> VertexInput {
    position_input(
        runtime,
        &[[-1.0, -1.0, 0.5], [3.0, -1.0, 0.5], [-1.0, 3.0, 0.5]],
    )
}

pub fn assert_pixel(image: &TargetImage, (x, y): (u32, u32), want: [f32; 4]) {
    for (channel, (&code, value)) in image.pixel(x, y).iter().zip(want).enumerate() {
        assert_code_within(
            code,
            f64::from(value * 255.0),
            0.0,
            &format!("pixel ({x}, {y}) channel {channel}"),
        );
    }
}

pub fn assert_all(image: &TargetImage, want: [f32; 4]) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            assert_pixel(image, (x, y), want);
        }
    }
}
