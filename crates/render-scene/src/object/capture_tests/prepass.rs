//! The metadata prepass of the object family: the object sources that write the view-space
//! normal, alone or with the reflection material, with the camera uniform at set three, drawn
//! through `SceneGpu::draw_prepass`, and compared with an untouched render on every attachment
//! and, at one sample, on the depth plane and the stencil plane.
//!
//! One frame holds an opaque mesh, a relief impostor, a masked double-sided card and a foliage
//! card under wind, so more than one of the 32 pipeline keys draws in each capture. The prepass
//! is drawn in the independent, shared and prepass depth modes, as the terrain prepass captures
//! do: the shared and prepass modes carry a stencil plane, which the prepass marks for the
//! forward pass's reuse.

use wgpu::{BindGroup, TextureFormat};

use super::scenes::{self, Alpha, Sampling, Spec};
use super::{Rig, Target, digest, render_with_depth};
use crate::device::GpuContext;
use crate::dynamic::capture_tests::{HDR, context};
use crate::frame::FrameUniform;
use crate::renderer::ScenePipelines;
use crate::renderer::depth::DepthMode;
use crate::scene_metadata::{MetadataOutput, pipelines_from_sources, sources};

/// The bytes of the `ScreenCamera` block the prepass reads: the view, the projection with the
/// view's inverse folded out, its inverse, and zeroed settings.
fn camera(frame: &FrameUniform) -> Vec<u8> {
    let projection = frame.view_projection * frame.view.inverse();
    let mut values = Vec::with_capacity(56);
    for matrix in [frame.view, projection, projection.inverse()] {
        values.extend(matrix.to_cols_array());
    }
    values.extend([0.0; 8]);
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

/// An opaque mesh, a relief impostor, a masked double-sided card and a foliage card under wind.
pub(in crate::object) fn specs() -> Vec<Spec> {
    vec![
        Spec::default(),
        Spec {
            sampling: Sampling::Relief,
            ..Spec::default()
        },
        Spec {
            alpha: Alpha::Cutout,
            double_sided: true,
            ..Spec::default()
        },
        Spec {
            alpha: Alpha::Cutout,
            double_sided: true,
            wind: true,
            ..Spec::default()
        },
    ]
}

/// The prepass pipelines of `output`, the camera group they bind and their colour formats.
pub(in crate::object) fn pipelines(
    context: &crate::device::GpuContext,
    frame: &FrameUniform,
    output: MetadataOutput,
) -> (ScenePipelines, BindGroup, Vec<TextureFormat>) {
    let uniform = worldrunner_render_gpu::resource::upload_buffer(
        context.device(),
        context.queue(),
        "object capture prepass camera",
        wgpu::BufferUsages::UNIFORM,
        &camera(frame),
    );
    let (pipelines, group) = pipelines_from_sources(context.device(), &uniform, &sources(output));
    let formats = match output {
        MetadataOutput::Merged => vec![HDR; 2],
        _ => vec![HDR],
    };
    (pipelines, group, formats)
}

fn plane_name(index: usize, colors: usize) -> String {
    match index.checked_sub(colors) {
        None => format!("attachment {index}"),
        Some(0) => "depth plane".to_owned(),
        Some(_) => "stencil plane".to_owned(),
    }
}

/// Captures the prepass of `output` in `mode` and returns how many planes were compared.
fn capture(output: MetadataOutput, mode: DepthMode) -> usize {
    let context = context();
    let frame = scenes::frame(scenes::pose());
    let (built, group, formats) = pipelines(&context, &frame, output);
    let built = built.for_depth(mode);
    let mut rig = Rig::moving(&context, &built, &scenes::scene(&specs()), false);
    let target = Target {
        formats: &formats,
        samples: 1,
    };
    let name = format!("prepass-{output:?}-{mode:?}");
    let depth = mode.format();
    rig.prime();
    let before = built.object_runtime_stats().pipelines_built;
    let scene = render_with_depth(&context, target, depth, &mut rig.gpu, |pass, gpu| {
        drop(gpu.draw_prepass(pass, &built, &group));
    });
    // The first prepass frame builds the objects' pipelines on the program runtime.
    assert!(
        built.object_runtime_stats().pipelines_built > before,
        "{name}: the prepass built no object pipeline on the program runtime"
    );
    rig.prime();
    let untouched = render_with_depth(&context, target, depth, &mut rig.gpu, |_, _| {});
    let planes = formats.len() + 1 + usize::from(depth.has_stencil_aspect());
    assert_eq!(scene.len(), planes, "{name}: every plane is read");
    compare_planes((&context, &name, mode), formats.len(), [&scene, &untouched]);
    // The foliage card and the masked double-sided card share a key: wind is material state.
    let keys = rig.gpu.pipeline_keys_drawn(0);
    assert_eq!(
        keys.len(),
        3,
        "{name}: the opaque mesh, the relief impostor and the masked double-sided cards each \
         draw with their own key, got {keys:?}"
    );
    planes
}

/// Checks that the objects wrote the planes the mode writes.
fn compare_planes(
    (context, name, mode): (&GpuContext, &str, DepthMode),
    colors: usize,
    [scene, untouched]: [&[Vec<u8>]; 2],
) {
    for (index, (scene, untouched)) in scene.iter().zip(untouched).enumerate() {
        let plane = plane_name(index, colors);
        // Only the prepass mode marks the stencil; the shared mode tests depth alone.
        let written = index <= colors || mode == DepthMode::Prepass;
        assert!(
            !written || scene != untouched,
            "{name}: the objects left {plane} as it was"
        );
        println!(
            "OBJECT-CAPTURE adapter={:?} config={name} scene=prepass mask=0 attachment={plane} bytes={} hash={:016x}",
            context.adapter().get_info().name,
            scene.len(),
            digest(scene)
        );
    }
}

macro_rules! prepass_tests {
    ($($test:ident: $output:expr, $mode:expr;)*) => {$(
        #[test]
        fn $test() {
            let _session = crate::test_support::SceneTestSession::acquire();
            let planes = capture($output, $mode);
            println!(
                "OBJECT-CAPTURE-SUMMARY set=prepass output={:?} depth={:?} compared={planes} skipped=0",
                $output, $mode
            );
        }
    )*};
}

prepass_tests! {
    object_normal_prepass_writes_with_independent_depth:
        MetadataOutput::Normal, DepthMode::Independent;
    object_merged_prepass_writes_with_independent_depth:
        MetadataOutput::Merged, DepthMode::Independent;
    object_normal_prepass_writes_with_shared_depth:
        MetadataOutput::Normal, DepthMode::Shared;
    object_merged_prepass_writes_with_shared_depth:
        MetadataOutput::Merged, DepthMode::Shared;
    object_normal_prepass_writes_marking_depth_winners:
        MetadataOutput::Normal, DepthMode::Prepass;
    object_merged_prepass_writes_marking_depth_winners:
        MetadataOutput::Merged, DepthMode::Prepass;
}
