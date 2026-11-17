use super::ReceiverLayout;

#[test]
fn render_releases_payload_backing_and_keeps_canonical_normals() {
    use crate::gbuffer::{Gbuffer, GbufferUse};
    use crate::scene_metadata::{SceneMetadata, receiver};
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = crate::device::GpuContext::shared_for_test().unwrap();
    let device = context.device();
    for usage in [None, Some(GbufferUse::Observe), Some(GbufferUse::Render)] {
        let mut frame = crate::compositor::FrameRenderer::new(device);
        let mut metadata = SceneMetadata::new(device, [2, 2], crate::ssr::neutral_ao()).unwrap();
        metadata.receiver_backing = Some(receiver::ReceiverBacking::new(
            device,
            [2, 2],
            ReceiverLayout::Front,
        ));
        metadata.receiver = Some(receiver::ReceiverPayload::new(
            device,
            &metadata,
            ReceiverLayout::Front,
            0.0,
            receiver::ReceiverCapture::Position,
            false,
        ));
        let normals = metadata.normals.clone();
        frame.ssao = Some(metadata);
        let renders = usage == Some(GbufferUse::Render);
        frame.gbuffer = usage.map(|usage| {
            Gbuffer::new(
                device,
                [2, 2],
                (wgpu::TextureFormat::Rgba16Float, false),
                usage,
            )
        });
        assert_eq!(frame.prepare_reconstructed_front_receiver(), renders);
        let metadata = frame.ssao.as_ref().unwrap();
        assert_eq!(metadata.receiver.is_none(), renders);
        assert_eq!(metadata.receiver_backing.is_none(), renders);
        assert_eq!(metadata.normals, normals);
    }
}
