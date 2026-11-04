use super::{Arc, AtomicBool, Mutex, Ordering, Rc};

type Waker = Arc<dyn Fn() + Send + Sync>;
pub(super) trait Service {
    fn shutdown(&self);
}
#[derive(Default)]
pub(super) struct Wake {
    pending: AtomicBool,
    waker: Mutex<Option<Waker>>,
}
impl Wake {
    pub fn notify(&self) {
        self.pending.store(true, Ordering::Release);
        if let Some(waker) = &*self.waker.lock().unwrap() {
            waker();
        }
    }
}
#[derive(Default)]
pub(in crate::retained) struct WorkServices {
    pub(super) wake: Arc<Wake>,
    services: Vec<std::rc::Weak<dyn Service>>,
    pub(super) closed: bool,
}
impl WorkServices {
    pub(super) fn register(&mut self, service: &Rc<dyn Service>) {
        self.services.retain(|service| service.strong_count() > 0);
        self.services.push(Rc::downgrade(service));
    }
    pub(in crate::retained) fn set_waker(&self, waker: Waker) {
        let mut slot = self.wake.waker.lock().unwrap();
        if self.wake.pending.load(Ordering::Acquire) {
            waker();
        }
        *slot = Some(waker);
    }
    pub(in crate::retained) fn take_ready(&self) -> bool {
        self.wake.pending.swap(false, Ordering::AcqRel)
    }
    pub(in crate::retained) fn shutdown(&mut self) {
        self.closed = true;
        for service in self
            .services
            .drain(..)
            .filter_map(|service| service.upgrade())
        {
            service.shutdown();
        }
        *self.wake.waker.lock().unwrap() = None;
        self.wake.pending.store(false, Ordering::Release);
    }
}
impl Drop for WorkServices {
    fn drop(&mut self) {
        self.shutdown();
    }
}
