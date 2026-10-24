use super::{ClockRequest, PhysicsRequest, Request, timeline::TimelineRequest};
use crate::{DisunityWorld, strict};

pub(super) fn check(request: &Request, w: &DisunityWorld) -> Result<(), String> {
    if w.strict.is_none()
        || request.is_read()
        || matches!(request, Request::Strict(_) | Request::Snapshot(_))
    {
        return Ok(());
    }
    // Preserve legacy owner-affinity and retention policies for event peeks.
    if matches!(
        request,
        Request::Physics(PhysicsRequest::Events { clear: false })
    ) {
        return Ok(());
    }
    if matches!(
        request,
        Request::Lifetime(_)
            | Request::Physics(PhysicsRequest::Step { .. })
            | Request::Clock(
                ClockRequest::Begin { .. }
                    | ClockRequest::Phase { .. }
                    | ClockRequest::TimeSet { .. }
            )
            | Request::Timeline(TimelineRequest::Begin { .. } | TimelineRequest::Configure { .. })
    ) {
        return Err("strict session requires authenticated native advancement".into());
    }
    if matches!(request, Request::Timeline(TimelineRequest::Events { .. })) {
        let a = w.strict.as_ref().unwrap();
        if matches!(
            a.stage,
            strict::Stage::Idle | strict::Stage::Phase(strict::Phase::Initialize)
        ) && a.active.is_none()
        {
            return Ok(());
        }
        return Err("strict input ingress requires idle session".into());
    }
    if strict::is_terminal(w) {
        terminal_check(request)?;
    }
    strict::allow_mutation(w)
}

fn terminal_check(request: &Request) -> Result<(), String> {
    let forbidden = match request {
        Request::Clock(
            ClockRequest::InputSet { .. }
            | ClockRequest::InputConfigure { .. }
            | ClockRequest::InputFocus { .. },
        )
        | Request::Physics(_)
        | Request::Object(super::ObjectRequest::Clone { .. }) => true,
        Request::Component(super::ComponentRequest::Add { kind, .. }) => kind == "Script",
        _ => false,
    };
    if forbidden {
        return Err("terminal Cleanup forbids new scripts, input and physics operations".into());
    }
    Ok(())
}
