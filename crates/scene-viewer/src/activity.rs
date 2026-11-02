/// Caller-supplied explanation for a retained run's platform activity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityReason(String);

impl Default for ActivityReason {
    fn default() -> Self {
        Self("Retained scene execution".into())
    }
}

impl From<String> for ActivityReason {
    fn from(reason: String) -> Self {
        Self(reason)
    }
}

impl From<&str> for ActivityReason {
    fn from(reason: &str) -> Self {
        Self(reason.into())
    }
}

impl AsRef<str> for ActivityReason {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub(crate) struct ActivityGuard {
    #[cfg(target_os = "macos")]
    _token:
        objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_foundation::NSObjectProtocol>>,
}

impl ActivityGuard {
    pub(crate) fn begin(reason: &str) -> Self {
        #[cfg(target_os = "macos")]
        {
            let token = objc2::rc::autoreleasepool(|_| {
                let reason = objc2_foundation::NSString::from_str(reason);
                objc2_foundation::NSProcessInfo::processInfo()
                    .beginActivityWithOptions_reason(options(), &reason)
            });
            Self { _token: token }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = reason;
            Self {}
        }
    }
}

#[cfg(target_os = "macos")]
fn options() -> objc2_foundation::NSActivityOptions {
    use objc2_foundation::NSActivityOptions;
    NSActivityOptions::UserInitiated | NSActivityOptions::LatencyCritical
}

pub(crate) fn release_after<G, T>(activity: &mut Option<G>, finish: impl FnOnce() -> T) -> T {
    let activity = activity.take();
    let result = finish();
    drop(activity);
    result
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn guard_can_begin_drop_and_begin_again() {
        for _ in 0..2 {
            let _activity = super::ActivityGuard::begin("scene activity test");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_activity_deallocates_before_outer_pool_drains() {
        objc2::rc::autoreleasepool(|_| {
            let activity = super::ActivityGuard::begin("native lifetime test");
            let super::ActivityGuard { _token: token } = &activity;
            let weak = objc2::rc::Weak::from_retained(token);
            assert!(weak.load().is_some());
            drop(activity);
            assert!(weak.load().is_none());
        });
    }

    #[test]
    fn activity_reason_is_validated_during_host_construction() {
        for reason in ["".into(), String::new().into()] {
            let mut options = crate::tests::options();
            options.activity_reason = reason;
            assert!(matches!(
                crate::RetainedHost::new(crate::tests::scene(false), options),
                Err(crate::ViewerError::InvalidOptions(_))
            ));
        }
        for reason in [
            "borrowed".into(),
            String::from("owned").into(),
            super::ActivityReason::default(),
        ] {
            let mut options = crate::tests::options();
            options.activity_reason = reason.clone();
            let host = crate::RetainedHost::new(crate::tests::scene(false), options).unwrap();
            assert_eq!(host.options.activity_reason, reason);
        }
    }

    #[test]
    fn windowless_shutdown_releases_activity_when_cleanup_panics() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let events = Rc::new(RefCell::new(vec!["begin"]));
        let mut activity = Some(Token(events.clone()));
        let result = catch_unwind(AssertUnwindSafe(|| {
            super::release_after(&mut activity, || {
                assert_eq!(*events.borrow(), ["begin"]);
                events.borrow_mut().push("cleanup");
                panic!("injected cleanup panic");
            });
        }));
        assert!(result.is_err());
        assert!(activity.is_none());
        assert_eq!(*events.borrow(), ["begin", "cleanup", "end"]);
        drop(activity);
        assert_eq!(events.borrow().iter().filter(|e| **e == "end").count(), 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn options_are_exactly_user_initiated_and_latency_critical() {
        use objc2_foundation::NSActivityOptions;
        assert_eq!(
            super::options(),
            NSActivityOptions::UserInitiated | NSActivityOptions::LatencyCritical
        );
    }

    struct Token(Rc<RefCell<Vec<&'static str>>>);
    impl Drop for Token {
        fn drop(&mut self) {
            self.0.borrow_mut().push("end");
        }
    }

    #[test]
    fn desktop_activity_surrounds_success_error_and_early_return() {
        for outcome in ["success", "error", "early"] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let mut host =
                crate::RetainedHost::new(crate::tests::scene(false), crate::tests::options())
                    .unwrap();
            host.options.activity_reason = "caller reason".into();
            let result = host.run_with_activity(
                &mut Driver,
                |reason| {
                    assert_eq!(reason, "caller reason");
                    events.borrow_mut().push("begin");
                    Token(events.clone())
                },
                |_, _| {
                    if outcome == "early" {
                        events.borrow_mut().push("early");
                        return Ok(());
                    }
                    events.borrow_mut().push("loop");
                    if outcome == "error" {
                        return Err(crate::ViewerError::Driver("loop error".into()));
                    }
                    Ok(())
                },
            );
            assert_eq!(result.is_ok(), outcome != "error");
            let work = if outcome == "early" { "early" } else { "loop" };
            assert_eq!(*events.borrow(), ["begin", work, "end"]);
        }
    }

    #[test]
    fn windowless_activity_surrounds_startup_and_releases_on_return() {
        for outcome in [Ok(()), Err("pump error"), Ok(())] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let mut host =
                crate::RetainedHost::new(crate::tests::scene(false), crate::tests::options())
                    .unwrap();
            host.options.activity_reason = "capture reason".into();
            let result = host.start_windowless_with_activity(
                &mut Driver,
                |reason| {
                    assert_eq!(reason, "capture reason");
                    events.borrow_mut().push("begin");
                    Token(events.clone())
                },
                |_, _| {
                    events.borrow_mut().push("pump");
                    outcome.map_err(|e| crate::ViewerError::Driver(e.into()))
                },
            );
            assert_eq!(result.is_ok(), outcome.is_ok());
            if result.is_ok() {
                assert_eq!(*events.borrow(), ["begin", "pump"]);
            }
            drop(result);
            assert_eq!(*events.borrow(), ["begin", "pump", "end"]);
        }
    }

    #[test]
    fn windowless_shutdown_releases_activity_after_success_error_or_cancellation() {
        use crate::app::Content;
        for outcome in ["success", "error", "cancel"] {
            let events = Rc::new(RefCell::new(Vec::new()));
            let mut host =
                crate::RetainedHost::new(crate::tests::scene(false), crate::tests::options())
                    .unwrap();
            if outcome == "cancel" {
                host.request_exit();
            }
            let mut driver = Driver;
            let (mut session, token) = host
                .start_windowless_with_activity(
                    &mut driver,
                    |_| {
                        events.borrow_mut().push("begin");
                        Token(events.clone())
                    },
                    |host, driver| {
                        events.borrow_mut().push("start");
                        Ok(crate::retained::Session::new(host, driver))
                    },
                )
                .unwrap();
            assert_eq!(session.exit_requested(), outcome == "cancel");
            let mut activity = Some(token);
            let result = super::release_after(&mut activity, || {
                let shutdown = session.shutdown();
                events.borrow_mut().push("drain");
                shutdown?;
                if outcome == "error" {
                    return Err(crate::ViewerError::Driver("drain error".into()));
                }
                Ok(())
            });
            assert_eq!(result.is_ok(), outcome != "error");
            assert!(activity.is_none());
            assert_eq!(*events.borrow(), ["begin", "start", "drain", "end"]);
            drop(activity);
            assert_eq!(events.borrow().iter().filter(|e| **e == "end").count(), 1);
        }
    }

    struct Driver;
    impl crate::RetainedDriver for Driver {
        fn update(
            &mut self,
            _: &mut crate::RetainedHost,
            _: &crate::RetainedTick,
        ) -> Result<(), String> {
            Ok(())
        }
    }
}
