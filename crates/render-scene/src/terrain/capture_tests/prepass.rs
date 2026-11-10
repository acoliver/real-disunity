//! The metadata prepass: the terrain sources that write the view-space normal, alone or with
//! the reflection material, to one or two single-sample attachments, with the camera uniform at
//! set three. The prepass is drawn in the shared, prepass and independent depth modes the
//! frame's depth representation selects.

use wgpu::{BindGroup, TextureFormat};

use super::{Case, Rig, Source, Target, capture, drawable, scenes};
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

/// The prepass pipelines of one metadata output and the camera group they bind.
pub(in crate::terrain) fn pipelines(
    context: &crate::device::GpuContext,
    frame: &FrameUniform,
    output: MetadataOutput,
) -> (ScenePipelines, BindGroup, Vec<TextureFormat>) {
    let uniform = crate::upload::upload_buffer(
        context.device(),
        context.queue(),
        "capture prepass camera",
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

/// Captures `source` through the prepass of both outputs in one depth mode.
fn prepass(source: Source, mode: DepthMode) -> usize {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    if !drawable(&context, source) {
        return 0;
    }
    let frame = scenes::frame(scenes::pose());
    let mut run = 0;
    for (name, output) in [
        ("prepass-normal", MetadataOutput::Normal),
        ("prepass-merged", MetadataOutput::Merged),
    ] {
        let (built, group, formats) = pipelines(&context, &frame, output);
        let rig = Rig::new(&context, &built, &source.scene(), (source, frame));
        capture(
            &rig,
            &Case {
                name,
                source: source.name(),
                target: Target {
                    formats: &formats,
                    samples: 1,
                },
                pipelines: &built.for_depth(mode),
                screen: Some(&group),
                shadow: None,
            },
        );
        run += 1;
    }
    println!(
        "TERRAIN-CAPTURE-SUMMARY set=prepass depth={mode:?} source={} compared={run}",
        source.name()
    );
    run
}

macro_rules! prepass_tests {
    ($($test:ident: $source:expr, $mode:expr;)*) => {$(
        #[test]
        fn $test() {
            let run = prepass($source, $mode);
            if matches!($source, Source::Plain | Source::Pages) {
                assert_eq!(run, 2);
            }
        }
    )*};
}

prepass_tests! {
    plain_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_metadata_prepass:
        Source::Plain, DepthMode::Independent;
    page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_metadata_prepass:
        Source::Pages, DepthMode::Independent;
    layered_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_metadata_prepass:
        Source::Layered, DepthMode::Independent;
    layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_metadata_prepass:
        Source::LayeredPages, DepthMode::Independent;
    plain_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_prepass_with_shared_depth:
        Source::Plain, DepthMode::Shared;
    page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_prepass_with_shared_depth:
        Source::Pages, DepthMode::Shared;
    layered_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_prepass_with_shared_depth:
        Source::Layered, DepthMode::Shared;
    layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_prepass_with_shared_depth:
        Source::LayeredPages, DepthMode::Shared;
    plain_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_prepass_that_marks_depth_winners:
        Source::Plain, DepthMode::Prepass;
    page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_prepass_that_marks_depth_winners:
        Source::Pages, DepthMode::Prepass;
    layered_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_prepass_that_marks_depth_winners:
        Source::Layered, DepthMode::Prepass;
    layered_page_terrain_matches_the_fixed_pipeline_byte_for_byte_in_the_prepass_that_marks_depth_winners:
        Source::LayeredPages, DepthMode::Prepass;
}
