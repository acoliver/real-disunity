use super::*;

#[test]
fn gpu_reducer_rejects_external_unsampleable_and_multisampled_attachments() {
    let context = context();
    let valid = source(&context, [2.0; 4], true);
    for product in [FrameProduct::linear_depth(0), FrameProduct::normals()] {
        let samples = if product == FrameProduct::normals() {
            vec![1, 4]
        } else {
            vec![1]
        };
        for samples in samples {
            let texture = context.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("invalid external reduction attachment"),
                size: wgpu::Extent3d {
                    width: 2,
                    height: 2,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: product.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | if samples == 4 {
                        wgpu::TextureUsages::TEXTURE_BINDING
                    } else {
                        wgpu::TextureUsages::empty()
                    },
                view_formats: &[],
            });
            let invalid = AttachmentView::new(product.resource(), &texture).unwrap();
            let scope = context
                .device()
                .push_error_scope(wgpu::ErrorFilter::Validation);
            let attachments = if product == FrameProduct::normals() {
                (&valid.0, &invalid)
            } else {
                (&invalid, &valid.1)
            };
            let result = DepthReducer::new(
                context.device(),
                (FrameProduct::linear_depth(0), attachments.0),
                (FrameProduct::normals(), attachments.1),
            );
            let gpu_error = pollster::block_on(scope.pop());
            assert!(
                matches!(result, Err(SdsmError::Metadata)),
                "{product:?}, samples={samples}: {result:?}"
            );
            assert!(gpu_error.is_none());
        }
    }
}
