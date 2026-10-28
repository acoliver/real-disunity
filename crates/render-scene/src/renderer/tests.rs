//! Tests for pipeline and depth attachment creation.
//!
//! These exist to make the driver arbitrate a disagreement it is uniquely able
//! to see. Every other module in this crate states what the GPU state should
//! be; only pipeline creation compares those statements against the compiled
//! shader. So the assertions here are almost all "wgpu raised no validation
//! error", which is not a weak assertion in this one place: it means the bind
//! group layouts, the vertex attribute locations, the buffer strides, the
//! entry point names and the WGSL all describe the same program.

use pollster;
use wgpu::{BufferDescriptor, BufferUsages, Device, ErrorFilter, TextureFormat, TextureUsages};

use super::{DEPTH_FORMAT, ScenePipelines, depth_texture};
use crate::device::GpuContext;
use crate::frame::FRAME_UNIFORM_BYTES;
use crate::pipeline::PipelineKey;
use crate::view::Viewport;

/// The colour format the governed capture target uses.
const COLOR_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request())
        .expect("GPU acceptance requires a real adapter with BC support")
}

/// Runs `body` inside a validation error scope and returns what wgpu raised.
fn validation_error<T>(device: &Device, body: impl FnOnce() -> T) -> (T, Option<wgpu::Error>) {
    let scope = device.push_error_scope(ErrorFilter::Validation);
    let value = body();
    (value, pollster::block_on(scope.pop()))
}

#[test]
fn every_pipeline_variant_compiles_against_the_declared_layout() {
    let context = context();
    let device = context.device();

    let (pipelines, error) = validation_error(device, || ScenePipelines::new(device, COLOR_FORMAT));

    assert!(
        error.is_none(),
        "the declared bind group layouts, vertex attributes and entry points \
         must match the shader: {error:?}"
    );
    for key in PipelineKey::ALL {
        let _pipeline = pipelines.get(key);
    }
    // The terrain pipeline is built by the same call and is the one whose
    // layout is not shared, so it is the one a divergence would surface in.
    let _terrain = pipelines.terrain();
}

#[test]
fn the_terrain_pipeline_compiles_against_its_declared_layout() {
    // This is the check the CPU-side naga tests cannot make. They confirm the
    // shader is valid WGSL; only a real driver confirms that the bindings it
    // declares are the bindings `terrain_bind_group_layout` declares, in the
    // same slots, with the same sample types. A mismatch there is invisible
    // until draw time, where it appears as a failed bind rather than as
    // anything naming the binding that disagreed.
    let context = context();
    let device = context.device();

    let (pipelines, error) = validation_error(device, || ScenePipelines::new(device, COLOR_FORMAT));

    assert!(
        error.is_none(),
        "the terrain bind group layout, vertex attributes and entry points \
         must match the terrain shader: {error:?}"
    );
    let _terrain = pipelines.terrain();
}

#[test]
fn each_state_selects_a_pipeline_of_its_own() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);

    // Four distinct states must not collapse onto fewer pipelines. If indexing
    // folded two together, the surviving one would be wrong for the state it
    // absorbed -- a masked material drawn opaque keeps the texels it should
    // have discarded, so every cutout leaf becomes a visible rectangle.
    for (position, key) in PipelineKey::ALL.iter().enumerate() {
        assert_eq!(key.index(), position, "{key:?} indexes its own slot");
        for other in PipelineKey::ALL.iter().skip(position + 1) {
            assert_ne!(
                pipelines.get(*key),
                pipelines.get(*other),
                "{key:?} and {other:?} must not share a pipeline"
            );
        }
    }
}

#[test]
fn the_frame_layout_the_pipelines_expose_accepts_the_frame_uniform() {
    let context = context();
    let device = context.device();
    let pipelines = ScenePipelines::new(device, COLOR_FORMAT);

    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("frame"),
        size: FRAME_UNIFORM_BYTES as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // The pipelines were built from these layouts, so a bind group the layout
    // accepts is one the pipeline will accept at draw time. Checking it here
    // ties the layout the upload module declares to the one the shader was
    // actually compiled against, rather than to a second copy that happens to
    // look the same.
    let (_group, error) = validation_error(device, || {
        crate::draw::decals::empty_group(device, pipelines.frame_layout(), &buffer)
    });

    assert!(
        error.is_none(),
        "the frame uniform must satisfy the layout the pipelines were built \
         from: {error:?}"
    );
}

#[test]
fn a_depth_attachment_matches_the_viewport_it_was_sized_for() {
    let context = context();
    let viewport = Viewport {
        width: 1280,
        height: 720,
    };

    let depth = depth_texture(context.device(), viewport);

    // A depth attachment whose extent differs from the colour target is a
    // validation error at pass creation, so the only useful guarantee is that
    // this function reproduces the viewport exactly rather than rounding or
    // clamping it.
    assert_eq!(depth.width(), viewport.width);
    assert_eq!(depth.height(), viewport.height);
    assert_eq!(depth.format(), DEPTH_FORMAT);
    assert!(
        depth.usage().contains(TextureUsages::RENDER_ATTACHMENT),
        "a depth buffer that cannot be attached is useless"
    );
}
