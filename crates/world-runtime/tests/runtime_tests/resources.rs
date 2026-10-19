//! Streaming verification contract tests with arbitrary synthetic names.
//!
//! Every capability, handler identity, and identifier is invented for these
//! tests alone; content addresses and chunk boundaries are exercised through
//! the caller-supplied provider and sessions only.

use worldrunner_world_model::{SemanticMap, SemanticWorldDocument};
use worldrunner_world_runtime::RuntimePlanError;

use super::support::*;

fn single_binding_document(source: &[u8]) -> (SemanticWorldDocument, MapProvider) {
    let provider = MapProvider::new().with_source(source);
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding("binding-alpha", BYTES_CAPABILITY, source, None)],
    );
    (document, provider)
}

#[test]
fn zero_byte_resource_is_accepted() {
    let registry = standard_registry();
    let (document, provider) = single_binding_document(b"");

    let prepared = prepare(&registry, &document, &provider).expect("empty content verifies");

    assert_eq!(prepared.resource_contributions().len(), 1);
    let contribution = &prepared.resource_contributions()[0];
    assert_eq!(contribution.claimant, "binding-alpha");
    assert_eq!(
        contribution
            .payload
            .downcast_ref::<SemanticMap>()
            .unwrap()
            .get("bytes"),
        Some(&unsigned(0))
    );
    assert_eq!(provider.open_count(), 1);
}

#[test]
fn provider_open_failure_is_typed() {
    let registry = standard_registry();
    let provider =
        MapProvider::new().with_open_failure(b"alpha-bytes", "synthetic provider refusal");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::ResourceOpen { binding_id, capability_uri, source }
        if binding_id == "binding-alpha"
            && capability_uri == BYTES_CAPABILITY
            && source.message == "synthetic provider refusal"
    ));
}

#[test]
fn provider_read_failure_is_typed() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_read_failure(b"alpha-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::ResourceRead { binding_id, .. } if binding_id == "binding-alpha"
    ));
}

#[test]
fn truncated_content_fails_length_verification() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_source_at(b"alpha-bytes", b"alpha");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::ResourceLengthMismatch { binding_id, expected, observed }
        if binding_id == "binding-alpha" && *expected == 11 && *observed == 5
    ));
}

#[test]
fn extended_content_fails_length_verification_before_crossing_chunk_delivery() {
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(
            BYTES_CAPABILITY,
            bytes_handler().with_chunk_byte_rejection(b'-'),
        )],
    );
    let provider = MapProvider::new().with_source_at(b"alpha", b"alpha-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding("binding-alpha", BYTES_CAPABILITY, b"alpha", None)],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::ResourceLengthMismatch { expected, observed, .. }
        if *expected == 5 && *observed == 11
    ));
}

#[test]
fn tampered_content_fails_digest_verification() {
    let registry = standard_registry();
    let provider = MapProvider::new().with_source_at(b"alpha-bytes", b"alpha-bytES");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::ResourceDigestMismatch { binding_id, expected, observed }
        if binding_id == "binding-alpha"
            && *expected == digest_of(b"alpha-bytes")
            && *observed == digest_of(b"alpha-bytES")
    ));
}

#[test]
fn session_rejection_mid_stream_is_typed() {
    let content = [0x00, 0xff, 0x00];
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(
            BYTES_CAPABILITY,
            bytes_handler().with_chunk_byte_rejection(0xff),
        )],
    );
    let provider = MapProvider::new().with_source(&content);
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding("binding-alpha", BYTES_CAPABILITY, &content, None)],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::SessionRejected { binding_id, capability_uri, handler, reason }
        if binding_id == "binding-alpha"
            && capability_uri == BYTES_CAPABILITY
            && handler.handler_id == "synthetic-handler/bytes"
            && reason.contains("0xff")
    ));
}

#[test]
fn session_rejection_at_start_fails_before_open() {
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(
            BYTES_CAPABILITY,
            bytes_handler().with_start_rejection("synthetic binding shape is refused"),
        )],
    );
    let provider = MapProvider::new().with_source(b"alpha-bytes");
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding(
            "binding-alpha",
            BYTES_CAPABILITY,
            b"alpha-bytes",
            None,
        )],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::SessionRejected { reason, .. }
        if reason == "synthetic binding shape is refused"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn every_session_starts_before_the_first_resource_open() {
    let registry = registry_of(
        vec![
            (ALPHA_SCHEMA, alpha_handler()),
            (BETA_SCHEMA, beta_handler()),
        ],
        vec![
            (BYTES_CAPABILITY, bytes_handler()),
            (
                ELEMENT_CAPABILITY,
                element_handler().with_start_rejection("later session refused"),
            ),
        ],
    );
    let alpha = b"alpha";
    let beta = b"beta";
    let provider = MapProvider::new().with_source(alpha).with_source(beta);
    let world = document(
        vec![
            component("unit-alpha", ALPHA_SCHEMA, alpha_payload("binding-alpha")),
            component("unit-beta", BETA_SCHEMA, beta_payload("binding-beta")),
        ],
        vec![
            binding("binding-alpha", BYTES_CAPABILITY, alpha, None),
            binding("binding-beta", ELEMENT_CAPABILITY, beta, Some(ELEMENT_LEAF)),
        ],
    );

    let error = prepare(&registry, &world, &provider).unwrap_err();

    assert!(matches!(
        error,
        RuntimePlanError::SessionRejected { reason, .. } if reason == "later session refused"
    ));
    assert_eq!(provider.open_count(), 0);
}

#[test]
fn session_rejection_at_finish_is_typed() {
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(
            BYTES_CAPABILITY,
            bytes_handler().with_finish_rejection("synthetic accumulated content is refused"),
        )],
    );
    let (document, provider) = single_binding_document(b"alpha-bytes");

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::SessionRejected { reason, .. }
        if reason == "synthetic accumulated content is refused"
    ));
    assert_eq!(provider.open_count(), 1);
}

#[test]
fn session_failure_mid_stream_is_typed() {
    let content = [0x01, 0xfe, 0x02];
    let registry = registry_of(
        vec![(ALPHA_SCHEMA, alpha_handler())],
        vec![(
            BYTES_CAPABILITY,
            bytes_handler().with_chunk_byte_failure(0xfe),
        )],
    );
    let provider = MapProvider::new().with_source(&content);
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding("binding-alpha", BYTES_CAPABILITY, &content, None)],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::SessionFailed { binding_id, message, .. }
        if binding_id == "binding-alpha" && message.contains("0xfe")
    ));
}

#[test]
fn chunks_crossing_the_fixed_buffer_boundary_verify_exactly() {
    let first = patterned_bytes(65_537, 251);
    let second = patterned_bytes(131_074, 241);
    let registry = standard_registry();
    let provider = MapProvider::new().with_source(&first).with_source(&second);
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_multi_payload(&["binding-alpha", "binding-beta"]),
        )],
        vec![
            binding("binding-alpha", BYTES_CAPABILITY, &first, None),
            binding("binding-beta", BYTES_CAPABILITY, &second, None),
        ],
    );

    let prepared = prepare(&registry, &document, &provider)
        .expect("multi-boundary content verifies in one pass");

    let contributions = prepared.resource_contributions();
    assert_eq!(contributions.len(), 2);
    let alpha = contributions
        .iter()
        .find(|contribution| contribution.claimant == "binding-alpha")
        .expect("alpha resource contribution");
    assert_eq!(
        alpha
            .payload
            .downcast_ref::<SemanticMap>()
            .unwrap()
            .get("bytes"),
        Some(&unsigned(65_537))
    );
    let beta = contributions
        .iter()
        .find(|contribution| contribution.claimant == "binding-beta")
        .expect("beta resource contribution");
    assert_eq!(
        beta.payload
            .downcast_ref::<SemanticMap>()
            .unwrap()
            .get("bytes"),
        Some(&unsigned(131_074))
    );
    assert_eq!(
        provider.opened(),
        vec![content_address(&first), content_address(&second)]
    );
}

#[test]
fn boundary_crossing_digest_drift_is_detected() {
    let declared = patterned_bytes(65_537, 251);
    let mut tampered = declared.clone();
    tampered[65_536] ^= 0xff;
    let registry = standard_registry();
    let provider = MapProvider::new().with_source_at(&declared, &tampered);
    let document = document(
        vec![component(
            "unit-alpha",
            ALPHA_SCHEMA,
            alpha_payload("binding-alpha"),
        )],
        vec![binding("binding-alpha", BYTES_CAPABILITY, &declared, None)],
    );

    let error = prepare(&registry, &document, &provider).unwrap_err();

    assert!(matches!(
        &error,
        RuntimePlanError::ResourceDigestMismatch { binding_id, expected, observed }
        if binding_id == "binding-alpha"
            && *expected == digest_of(&declared)
            && *observed == digest_of(&tampered)
    ));
}

fn patterned_bytes(length: usize, period: u8) -> Vec<u8> {
    (0..period).cycle().take(length).collect()
}
