use super::{ActiveEventLoop, App, Arc, Renderer, ViewerError, window_attributes};

impl App<'_> {
    pub(super) fn create_renderer(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), ViewerError> {
        let window = Arc::new(event_loop.create_window(window_attributes(&*self.content))?);
        if let Some(accessibility) = self.content.accessibility() {
            accessibility.attach(
                event_loop,
                &window,
                self.proxy.as_ref().expect("running event loop").clone(),
            );
        }
        if !self.content.window_on_top() {
            window.set_visible(true);
        }
        self.renderer = Some(pollster::block_on(Renderer::from_source(
            event_loop.owned_display_handle(),
            window,
            self.content.source(),
        ))?);
        Ok(())
    }
    pub(super) fn accessibility_event(&mut self, event: accesskit_winit::Event) {
        let Some(window) = self.renderer.as_ref().and_then(|r| r.window.as_ref()) else {
            return;
        };
        if event.window_id != window.id() {
            return;
        }
        if let accesskit_winit::WindowEvent::ActionRequested(request) = event.window_event {
            let request = self
                .content
                .accessibility()
                .and_then(|a| a.request(request));
            if let Some(request) = request {
                self.content.accessibility_action(request);
            }
        }
    }
}
