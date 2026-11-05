use super::*;
use crate::{InitialContent, InitialContext, RetainedHost, RetainedInput};

struct TestSurface {
    extent: [u32; 2],
    platform: [u32; 2],
}

impl Surface for TestSurface {
    fn extent(&self) -> [u32; 2] {
        self.extent
    }
    fn window_extent(&self) -> [u32; 2] {
        self.platform
    }
    fn resize(&mut self, size: [u32; 2]) -> Result<(), ViewerError> {
        self.extent = size;
        Ok(())
    }
}

struct TestContent {
    host: RetainedHost,
    notifications: Vec<[u32; 2]>,
    inputs: Vec<RetainedInput>,
}

impl Content for TestContent {
    fn title(&self) -> &'static str {
        "surface handoff"
    }
    fn size(&self) -> [u32; 2] {
        self.host.size()
    }
    fn source(&mut self) -> crate::renderer::SceneSource<'_> {
        crate::renderer::SceneSource::Retained(&mut self.host)
    }
    fn event(
        &mut self,
        _event: &winit::event::WindowEvent,
        _viewport: Option<worldrunner_render_scene::Viewport>,
    ) {
    }
    fn surface_size(&mut self, size: [u32; 2]) {
        self.notifications.push(size);
        self.inputs
            .extend(self.host.input(RetainedInput::Resized(size)));
    }
    fn redraw(&mut self, _renderer: &mut Renderer) -> Result<bool, ViewerError> {
        unreachable!("CPU-only startup handoff")
    }
}

fn supplied_content(extent: [u32; 2]) -> TestContent {
    let mut host = RetainedHost::with_initial_content(crate::tests::options(), |_| {
        Ok(InitialContent {
            scene: crate::tests::scene(false),
            settings: None,
        })
    })
    .unwrap();
    let adapter = crate::AdapterIdentity {
        name: "synthetic".into(),
        class: crate::AdapterClass::Integrated,
        backend: wgpu::Backend::Metal,
    };
    host.resolve_initial_content(InitialContext {
        adapter: &adapter,
        surface_extent: extent,
    })
    .unwrap();
    TestContent {
        host,
        notifications: Vec::new(),
        inputs: Vec::new(),
    }
}

#[test]
fn startup_handoff_uses_renderer_extent_until_a_real_resize() {
    for (creation, later) in [
        ([640, 480], [800, 600]),
        ([0, 480], [800, 600]),
        ([640, 480], [640, 0]),
    ] {
        let mut surface = TestSurface {
            extent: creation,
            platform: later,
        };
        let mut content = supplied_content(creation);
        notify(&mut content, &surface);
        assert_eq!(content.notifications, vec![surface.extent()]);
        assert_eq!(content.host.size(), surface.extent());
        assert!(content.inputs.is_empty());
        resize(&mut content, &mut surface, later).unwrap();
        assert_eq!(surface.extent(), later);
        assert_eq!(content.host.size(), later);
        assert_eq!(content.inputs, vec![RetainedInput::Resized(later)]);
        assert_eq!(content.host.view().unwrap().is_none(), later.contains(&0));
        resize(&mut content, &mut surface, creation).unwrap();
        assert_eq!(content.host.size(), surface.extent());
        assert_eq!(
            content.inputs,
            vec![
                RetainedInput::Resized(later),
                RetainedInput::Resized(creation)
            ]
        );
    }
}

#[test]
fn resume_adopts_the_new_platform_extent_in_both_owners() {
    let mut surface = TestSurface {
        extent: [0, 480],
        platform: [800, 600],
    };
    let mut content = supplied_content(surface.extent());
    resume(&mut content, &mut surface).unwrap();
    assert_eq!(surface.extent(), surface.platform);
    assert_eq!(content.host.size(), surface.extent());
    assert_eq!(
        content.inputs,
        vec![RetainedInput::Resized(surface.extent())]
    );
}
