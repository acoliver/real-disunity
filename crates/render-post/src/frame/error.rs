//! The ways a frame's declarations, preparation or recording are refused.
use super::{PassEntry, PreparedRefusal, ResourceRef};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("streaming terrain already prepared for a different view in this transaction")]
    StreamingSelectionConflict,
    #[error("{feature} preparation failed: {reason}")]
    FeaturePreparation {
        feature: &'static str,
        reason: String,
    },
    #[error(transparent)]
    Transaction(#[from] worldrunner_render_gpu::submission::TransactionError),
    #[error("invalid or unsupported SSR setting: {0}")]
    SsrSettings(&'static str),
    #[error("invalid or unsupported water setting: {0}")]
    WaterSettings(&'static str),
    /// Two transparent features draw in the same entry. Transparent entries run in the order the
    /// frame lists them, but the executor does not sort packets across features inside one entry.
    #[error("{0} has more than one transparent feature; list each in its own entry")]
    TransparentOrdering(PassEntry),
    #[error("scene exposure differs from the compiled fixed-exposure profile")]
    FixedExposureMismatch,
    #[error("invalid or unsupported SSAO setting: {0}")]
    SsaoSettings(&'static str),
    /// The list names one entry twice. A role may repeat; an entry (role and occurrence) may not.
    #[error("pass entry appears more than once in the frame: {0}")]
    DuplicatePass(PassEntry),
    #[error(transparent)]
    ResourceContract(#[from] crate::contracts::ContractError),
    #[error("feature requests an undeclared pass entry {0}")]
    MissingPass(PassEntry),
    #[error("resource has no preceding producer: {0:?}")]
    MissingProducer(ResourceRef),
    #[error("resource is loaded or sampled after discard: {0:?}")]
    LoadAfterDiscard(ResourceRef),
    #[error("resource is sampled while attached or written ambiguously: {0:?}")]
    AttachmentHazard(ResourceRef),
    #[error("attachment or pipeline contract is incompatible in slot {0}")]
    AttachmentContract(PassEntry),
    #[error("frame does not support requested effect {0}")]
    UnsupportedEffect(&'static str),
    #[error("attachment view does not match its declared subresource: {0:?}")]
    AttachmentView(ResourceRef),
    /// A frozen plan lists a pass before one that the stock passes need it to follow.
    #[error("pass {pass} is listed before {needs}, which the stock passes need it to follow")]
    StockOrder { pass: PassEntry, needs: PassEntry },
    /// A pass reads the depth that Opaque draws with before a prepass or Opaque has written it.
    #[error("pass {pass} reads {resource:?} before the opaque depth is complete")]
    IncompleteDepth {
        pass: PassEntry,
        resource: ResourceRef,
    },
    /// A geometry pass attaches a texture that operations groups write.
    #[error("pass {pass} attaches {resource:?}, which operations write; passes may only read it")]
    OffscreenWrite {
        pass: PassEntry,
        resource: ResourceRef,
    },
    /// A preparation staged a write of a view the plan did not declare prepared.
    #[error("prepared write {0:?} was not declared by the plan")]
    UnplannedWrite(ResourceRef),
    /// The plan declared a view prepared and no preparation of the frame staged its write.
    #[error("the plan declared prepared write {0:?} and no preparation staged it")]
    UnstagedWrite(ResourceRef),
    /// A frame's preparations staged a second write of one view, which the frame versions once.
    #[error("prepared write {0:?} is staged more than once in the frame's preparation")]
    RepeatedPreparedWrite(ResourceRef),
    /// Prepared commands were staged outside the features' `prepare`, after the frame checked
    /// what its preparations write.
    #[error("prepared commands may be staged only while features prepare")]
    PreparedOutsidePreparation,
    /// Prepared commands write other views of the frame than their access declares.
    #[error("prepared commands {label} write {written:?} and declare writes of {declared:?}")]
    PreparedWriteMismatch {
        label: &'static str,
        declared: Vec<ResourceRef>,
        written: Vec<ResourceRef>,
    },
    /// The owner of prepared commands refused to retain or record them, with its own error.
    #[error("prepared commands {label} were refused: {refusal}")]
    PreparedRefused {
        label: &'static str,
        refusal: PreparedRefusal,
    },
    /// Prepared commands were staged for an entry that is not a compute pass the frame records:
    /// a render pass, or a compute pass a completed geometry prefix already encoded.
    #[error(
        "prepared commands {label} were staged for {entry}, which is not a compute pass the frame records"
    )]
    NotComputePass {
        label: &'static str,
        entry: PassEntry,
    },
    /// Prepared commands staged for a compute pass have no compute form to record in it.
    #[error("prepared commands {label} have no compute form for the compute pass {entry}")]
    NotComputeWork {
        label: &'static str,
        entry: PassEntry,
    },
}
