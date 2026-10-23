//! Deferred surface intent, separate from driver operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceAction {
    None,
    Suspended,
    Wait,
    Configure,
    Recreate,
}

#[derive(Clone, Copy, Debug)]
pub struct SurfaceTransition {
    size: [u32; 2],
    pending: bool,
    recreate: bool,
}
impl SurfaceTransition {
    pub fn new(size: [u32; 2]) -> Self {
        Self {
            size,
            pending: false,
            recreate: false,
        }
    }
    pub fn resize(&mut self, size: [u32; 2]) {
        self.size = size;
        self.pending = true;
    }
    pub fn outdated(&mut self) {
        self.pending = true;
    }
    pub fn lost(&mut self) {
        self.pending = true;
        self.recreate = true;
    }
    pub fn action(&self, outstanding: usize) -> SurfaceAction {
        if self.size.contains(&0) {
            return SurfaceAction::Suspended;
        }
        if !self.pending {
            return SurfaceAction::None;
        }
        if outstanding != 0 {
            return SurfaceAction::Wait;
        }
        if self.recreate {
            SurfaceAction::Recreate
        } else {
            SurfaceAction::Configure
        }
    }
    pub fn configured(&mut self) {
        self.pending = false;
        self.recreate = false;
    }
    pub fn size(&self) -> [u32; 2] {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_completion_coalesces_resize_and_sticky_loss() {
        let mut state = SurfaceTransition::new([800, 600]);
        state.resize([900, 700]);
        state.outdated();
        state.lost();
        state.resize([640, 480]);
        assert_eq!(state.action(2), SurfaceAction::Wait);
        assert_eq!(state.action(1), SurfaceAction::Wait);
        assert_eq!(state.size(), [640, 480]);
        assert_eq!(state.action(0), SurfaceAction::Recreate);
        state.configured();
        assert_eq!(state.action(0), SurfaceAction::None);
    }
    #[test]
    fn zero_size_pauses_without_consuming_pending_loss_and_resumes_at_newest_size() {
        let mut state = SurfaceTransition::new([800, 600]);
        state.lost();
        state.resize([0, 480]);
        for outstanding in 0..=3 {
            assert_eq!(state.action(outstanding), SurfaceAction::Suspended);
        }
        state.resize([640, 480]);
        state.outdated();
        assert_eq!(state.action(1), SurfaceAction::Wait);
        assert_eq!(state.action(0), SurfaceAction::Recreate);
        state.configured();
        state.resize([320, 240]);
        assert_eq!(state.action(0), SurfaceAction::Configure);
    }
}
