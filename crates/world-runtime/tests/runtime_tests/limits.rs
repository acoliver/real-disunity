//! Explicit caller-budget preflight tests.

use worldrunner_world_model::ValidationLimits;
use worldrunner_world_runtime::{RuntimePlanError, RuntimePlanLimits};

use super::support::*;

fn plan_limits(
    max_components: usize,
    max_bindings: usize,
    max_binding_bytes: u64,
    max_total_binding_bytes: u64,
) -> RuntimePlanLimits {
    RuntimePlanLimits::new(
        ValidationLimits {
            max_depth: 8,
            max_nodes: 64,
            max_text_bytes: 512,
        },
        max_components,
        max_bindings,
        max_binding_bytes,
        max_total_binding_bytes,
    )
}

fn one_binding_fixture() -> (
    worldrunner_world_model::SemanticWorldDocument,
    MapProvider,
    worldrunner_world_runtime::RuntimeRegistry,
) {
    let bytes = b"bounded-content";
    let world = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding("binding-alpha", BYTES_CAPABILITY, bytes, None)],
    );
    (
        world,
        MapProvider::new().with_source(bytes),
        standard_registry(),
    )
}

#[test]
fn component_count_limit_fails_before_open() {
    let (world, provider, registry) = one_binding_fixture();
    let error = prepare_with_limits(&registry, &world, &provider, &plan_limits(0, 1, 100, 100))
        .unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::ComponentLimitExceeded {
            max: 0,
            observed: 1
        }
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn binding_count_limit_fails_before_open() {
    let (world, provider, registry) = one_binding_fixture();
    let error = prepare_with_limits(&registry, &world, &provider, &plan_limits(1, 0, 100, 100))
        .unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::BindingLimitExceeded {
            max: 0,
            observed: 1
        }
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn per_binding_byte_limit_fails_before_open() {
    let (world, provider, registry) = one_binding_fixture();
    let error =
        prepare_with_limits(&registry, &world, &provider, &plan_limits(1, 1, 4, 100)).unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::BindingByteLimitExceeded {
            binding_id,
            max: 4,
            observed: 15
        } if binding_id == "binding-alpha"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn aggregate_binding_byte_limit_uses_checked_sum_before_open() {
    let first = b"first";
    let second = b"second";
    let world = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_multi_payload(&["binding-alpha", "binding-beta"]),
        )],
        vec![
            binding("binding-alpha", BYTES_CAPABILITY, first, None),
            binding("binding-beta", BYTES_CAPABILITY, second, None),
        ],
    );
    let provider = MapProvider::new().with_source(first).with_source(second);
    let registry = standard_registry();
    let error =
        prepare_with_limits(&registry, &world, &provider, &plan_limits(1, 2, 10, 10)).unwrap_err();
    assert!(matches!(
        error,
        RuntimePlanError::TotalBindingByteLimitExceeded {
            max: 10,
            observed: 11
        }
    ));
    assert_eq!(provider.open_count(), 0);
}
