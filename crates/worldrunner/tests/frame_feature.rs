#![cfg(feature = "scene")]

#[path = "../examples/frame-feature.rs"]
mod example;

#[test]
fn public_feature_example_runs_headless() {
    example::main().unwrap();
}
