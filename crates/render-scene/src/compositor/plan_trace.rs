//! A record of the plans a frame hands to the executor, for the tests that pin the stock
//! schedule. Each plan is described by its entries in submission order, the attachments each
//! entry names with how it loads them, the effects the planned features declare and where the
//! operations group is positioned.
use std::cell::RefCell;
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, BorrowedFeature, OperationsGroup, PassDeclaration,
    ResourceRef,
};

/// One plan as the executor received it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubmittedPlan {
    /// A depth prefix, which the frame's own plan then lists again and skips.
    pub prefix: bool,
    pub passes: Vec<String>,
    pub effects: Vec<&'static str>,
    pub operations_before: Vec<Option<String>>,
}

thread_local! {
    static SUBMITTED: RefCell<Vec<SubmittedPlan>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn record(
    prefix: bool,
    passes: &[PassDeclaration],
    features: &[BorrowedFeature<'_>],
    operations: &[OperationsGroup<'_>],
) {
    let plan = SubmittedPlan {
        prefix,
        passes: passes.iter().map(describe).collect(),
        effects: features
            .iter()
            .flat_map(|feature| feature.declaration.effects.iter().copied())
            .collect(),
        operations_before: operations
            .iter()
            .map(|group| group.before.map(|entry| format!("{entry:?}")))
            .collect(),
    };
    SUBMITTED.with_borrow_mut(|plans| plans.push(plan));
}

/// The plans recorded on this thread since the last call.
pub(crate) fn take() -> Vec<SubmittedPlan> {
    SUBMITTED.with_borrow_mut(std::mem::take)
}

fn resource(resource: ResourceRef) -> String {
    let mut text = resource.name.to_owned();
    for (tag, value) in [
        ('v', resource.version),
        ('m', resource.mip),
        ('l', resource.layer),
    ] {
        if value != 0 {
            text.extend(['@', tag]);
            text.push_str(&value.to_string());
        }
    }
    text
}

fn attachment(attachment: &AttachmentDeclaration) -> String {
    let load = match attachment.load {
        AttachmentLoad::Load => 'L',
        AttachmentLoad::ClearColor(_) | AttachmentLoad::ClearDepth(_) => 'C',
    };
    let read_only = if attachment.read_only { "!ro" } else { "" };
    format!("{}:{load}{read_only}", resource(attachment.resource))
}

/// `Slot#occurrence colour,more... / depth`, naming each attachment and its load.
fn describe(pass: &PassDeclaration) -> String {
    let colours: Vec<_> = pass
        .color
        .iter()
        .chain(&pass.additional_colors)
        .map(attachment)
        .collect();
    let depth = pass.depth.as_ref().map_or_else(String::new, attachment);
    format!(
        "{:?}#{} {} / {}",
        pass.slot,
        pass.occurrence,
        colours.join(","),
        depth
    )
}

/// One plan as a test pins it.
pub(crate) struct Pin {
    pub prefix: bool,
    pub passes: &'static [&'static str],
    pub effects: &'static [&'static str],
    pub operations: &'static [Option<&'static str>],
}

/// The plans as a pin list, for taking a new pin from a frame.
fn source(plans: &[SubmittedPlan]) -> String {
    use std::fmt::Write;
    let mut text = String::from("&[\n");
    for plan in plans {
        let _ = write!(
            text,
            "    Pin {{\n        prefix: {},\n        passes: &[\n",
            plan.prefix
        );
        for pass in &plan.passes {
            let _ = writeln!(text, "            {pass:?},");
        }
        let _ = write!(
            text,
            "        ],\n        effects: &{:?},\n        operations: &{:?},\n    }},\n",
            plan.effects, plan.operations_before
        );
    }
    text.push_str("]\n");
    text
}

/// Asserts that the plans a frame submitted are the pinned ones, or prints the pins it submits.
pub(crate) fn assert_pinned(name: &str, actual: &[SubmittedPlan], pinned: &[Pin]) {
    let matches = actual.len() == pinned.len()
        && actual.iter().zip(pinned).all(|(actual, pin)| {
            actual.prefix == pin.prefix
                && actual.passes == pin.passes
                && actual.effects == pin.effects
                && actual.operations_before.len() == pin.operations.len()
                && actual
                    .operations_before
                    .iter()
                    .zip(pin.operations)
                    .all(|(actual, pin)| actual.as_deref() == *pin)
        });
    assert!(
        matches,
        "{name}: the submitted plans differ from the pins; the frame now submits\n{}",
        source(actual)
    );
}
