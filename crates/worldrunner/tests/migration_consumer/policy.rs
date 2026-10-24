#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    Streaming,
    Settling,
    Ready,
}

pub fn admit(readiness: Readiness) -> Result<(), &'static str> {
    match readiness {
        Readiness::Streaming => Err("caller streaming incomplete"),
        Readiness::Settling => Err("caller temporal settling incomplete"),
        Readiness::Ready => Ok(()),
    }
}

#[test]
fn caller_policy_blocks_streaming_and_temporal_capture() {
    assert!(admit(Readiness::Streaming).is_err());
    assert!(admit(Readiness::Settling).is_err());
    assert!(admit(Readiness::Ready).is_ok());
}
