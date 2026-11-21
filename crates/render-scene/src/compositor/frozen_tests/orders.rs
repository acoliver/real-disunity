//! A frozen plan runs in the order its author listed, whatever order the stock frame would
//! have given its roles, and needs no stock geometry.
use super::support::{Counting, Feature, Rig, declaring};
use crate::compositor::plan_trace;
use worldrunner_render_post::{BorrowedFeature, PassEntry, PassSlot};

fn entries(plans: &[plan_trace::SubmittedPlan]) -> Vec<String> {
    let [plan] = plans else {
        panic!("one plan was submitted, not {plans:?}")
    };
    plan.passes
        .iter()
        .map(|pass| pass.split_whitespace().next().unwrap().to_owned())
        .collect()
}

#[test]
fn a_caller_pass_between_two_opaque_entries_runs_between_them() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    // The stock order puts the prepass before every Opaque entry. The author lists it between two,
    // with a distinct target for each appearance of the role.
    let (mut first, first_view) = rig.colour("first target");
    let (mut middle, middle_view) = rig.colour("middle target");
    let (mut last, last_view) = rig.colour("last target");
    first.slot = PassSlot::Opaque;
    middle.slot = PassSlot::DepthNormalPrepass;
    last.slot = PassSlot::Opaque;
    last.occurrence = 1;
    let order = [
        PassEntry::new(PassSlot::Opaque),
        PassEntry::new(PassSlot::DepthNormalPrepass),
        PassEntry::nth(PassSlot::Opaque, 1),
    ];
    let counting = Counting::default();
    let (prepared, draws) = (counting.prepared.clone(), counting.draws.clone());
    let mut feature = Feature(counting);
    let mut features = [BorrowedFeature {
        declaration: declaring(&order),
        prepared: &mut feature,
    }];
    plan_trace::take();
    rig.encode(
        (
            &[first, middle, last],
            &[first_view, middle_view, last_view],
        ),
        &mut features,
    )
    .unwrap();
    assert_eq!(*draws.borrow(), order, "each entry drew once, as listed");
    assert_eq!(prepared.get(), 1, "the feature prepared once");
    assert_eq!(
        entries(&plan_trace::take()),
        ["Opaque#0", "DepthNormalPrepass#0", "Opaque#1"],
        "the executor was handed the entries unsorted"
    );
}

#[test]
fn a_caller_only_plan_encodes_on_a_renderer_with_no_stock_geometry() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut rig = Rig::new();
    let (pass, view) = rig.colour("caller target");
    let entry = pass.entry();
    let counting = Counting::default();
    let (prepared, draws) = (counting.prepared.clone(), counting.draws.clone());
    let mut feature = Feature(counting);
    let mut features = [BorrowedFeature {
        declaration: declaring(&[entry]),
        prepared: &mut feature,
    }];
    rig.encode((&[pass], &[view]), &mut features).unwrap();
    assert_eq!(*draws.borrow(), [entry]);
    assert_eq!(prepared.get(), 1);
}
