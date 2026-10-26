use super::*;
use crate::{RetainedHost, RetainedUpdate};

pub(super) fn streaming_present(event_loop: &EventLoop<()>, window: &Arc<Window>) {
    let scene = super::super::retained_tests::source();
    let mut renderer = pollster::block_on(Renderer::new(
        event_loop.owned_display_handle(),
        window.clone(),
        &scene,
        None,
        crate::FrameProfile::Legacy,
        2,
    ))
    .unwrap();
    let size = renderer.viewport.unwrap();
    let mut host = RetainedHost::with_preparation_limit(
        scene.clone(),
        crate::ViewerOptions {
            size: [size.width, size.height],
            ..crate::tests::options()
        },
        2,
    )
    .unwrap();
    present_retained(&mut renderer, &mut host);
    host.apply(RetainedUpdate::Resources {
        resources: scene.resources().clone(),
        meshes: scene.geometry().meshes().to_vec(),
    })
    .unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    host.readiness.queue.front_mut().unwrap().scene = Some(receive);
    let before_view = host.view().unwrap().unwrap().view_matrix();
    let mut options = crate::tests::options();
    options.pose.eye_millimeters[0] += 10_000;
    host.apply(RetainedUpdate::Camera {
        pose: options.pose,
        lens: options.camera,
    })
    .unwrap();
    assert_eq!(host.view().unwrap().unwrap().view_matrix(), before_view);
    assert!(matches!(
        host.apply(RetainedUpdate::Remove(crate::ObjectId(0))),
        Err(ViewerError::BackPressure { limit: 2 })
    ));
    for _ in 0..4 {
        renderer.poll().unwrap();
        present_retained(&mut renderer, &mut host);
        assert_eq!(host.ready_generation(), 0);
        assert_eq!(host.view().unwrap().unwrap().view_matrix(), before_view);
        assert_eq!(host.preparations_in_flight(), 2);
    }
    let prepared = worldrunner_render_scene::draw::PreparedScene::new(&scene).unwrap();
    send.send(Ok(prepared)).unwrap();
    present_retained(&mut renderer, &mut host);
    assert_eq!(host.ready_generation(), 1);
    assert_eq!(host.view().unwrap().unwrap().view_matrix(), before_view);
    assert_eq!(renderer.streaming.retired.len(), 1);
    present_retained(&mut renderer, &mut host);
    assert_eq!(host.ready_generation(), 2);
    assert_ne!(host.view().unwrap().unwrap().view_matrix(), before_view);
    assert_eq!(host.preparations_in_flight(), 0);
}
