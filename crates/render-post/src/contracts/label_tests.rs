//! CPU-only coverage of labels retained across compiled capture segments.
use crate::contracts::{RasterId, ResourceRegistry, TypedOperation, TypedWork};
use crate::{PassId, StageId};

#[test]
fn capture_segments_retain_their_compute_labels_and_occurrences() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl("@compute @workgroup_size(1) fn main() {}".into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let operations = [0, 1].map(|occurrence| {
        TypedOperation::new(
            RasterId::new(StageId::new("compute", 2), occurrence),
            TypedWork::Compute {
                pipeline: pipeline.clone(),
                bindings: Vec::new(),
                workgroups: [1; 3],
            },
        )
    });
    let registry = ResourceRegistry::default();
    let capture = registry
        .compile_capture(&device, &operations[..1], &operations[1..], &[])
        .unwrap();
    for (segment, operation) in [capture.prefix(), capture.suffix()]
        .into_iter()
        .zip(&operations)
    {
        let id = PassId::Operation(operation.id);
        assert_eq!(segment.labels, [format!("{id:?}")]);
        assert_eq!(segment.labels[0], id.to_string());
        assert_eq!(segment.labels[0], id.encoder_label().as_ref());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        segment
            .encode_checked(&registry, &mut encoder, None)
            .unwrap();
        encoder.finish();
    }
    assert_ne!(capture.prefix().labels, capture.suffix().labels);
}
