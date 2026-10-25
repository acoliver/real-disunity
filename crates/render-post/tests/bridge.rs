#[path = "../examples/bridge.rs"]
mod bridge;

#[test]
fn mixed_providers_resolves_exports_and_capture_preserve_bytes() {
    bridge::run(false);
}

#[test]
fn prepared_consumer_work_can_be_cancelled_without_publication() {
    bridge::run(true);
}

#[test]
fn reversing_providers_changes_the_result_without_splitting_the_pass() {
    bridge::run_order(false, true);
}
