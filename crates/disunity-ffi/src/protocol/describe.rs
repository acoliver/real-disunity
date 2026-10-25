use serde_json::{Value, json};

pub(super) fn descriptor() -> Value {
    json!({
        "descriptor_version": 1,
        "native_abi_version": crate::DISUNITY_ABI_VERSION,
        "native_protocol_versions": [1],
        "bridge_revisions": [1],
        "manifest_schema_versions": [1],
        "capabilities": ["describe", "owned_responses", "immutable_reads", "deferred_destruction",
            "request_deduplication", "atomic_transactions", "thread_affinity", "lifecycle_scripts",
            "rational_time", "unscaled_time", "timestamped_input", "native_phase_context_native_v1",
            "native_readiness_primitives_v1", "snapshot_primitives_v1", "native_terminal_cleanup_v1",
            "transport_byte_limits_v1", "native_readiness_nested_start_v1", crate::strict::BATCH_PROFILE, crate::strict::IDLE_PROFILE],
        "endianness": if cfg!(target_endian = "little") { "little" } else { "big" },
        "pointer_width": usize::BITS,
        "limits": {
            "max_input_events": crate::input::MAX_INPUT_EVENTS,
            "max_rational_numerator": crate::ratio::MAX_NUMERATOR,
            "max_rational_denominator": crate::ratio::MAX_DENOMINATOR,
            "max_fixed_admissions": 1024,
            "max_phase_claims_per_frame": crate::strict::MAX_CLAIMS,
            "max_request_bytes": crate::response::MAX_REQUEST_BYTES,
            "max_response_bytes": crate::response::MAX_RESPONSE_BYTES,
            "max_live_response_bytes": crate::response::MAX_LIVE_RESPONSE_BYTES,
            "max_responses": crate::response::MAX_RESPONSES,
            "max_commands_per_submit": crate::MAX_COMMANDS_PER_SUBMIT,
            "max_retained_requests": crate::ledger::MAX_RETAINED_REQUESTS,
            "max_retained_request_bytes": crate::ledger::MAX_RETAINED_BYTES,
            "max_request_id_bytes": crate::ledger::MAX_REQUEST_ID_BYTES,
            "max_transaction_operations": super::transaction::MAX_TRANSACTION_OPERATIONS,
            "retention_response_reservation_bytes": crate::response::MAX_RESPONSE_BYTES
        },
        "request_identity_policy": "exact_bytes_until_world_destroy",
        "thread_policy": "owner_mutations_cross_thread_unretained_reads",
        "clock_policy": "float_accumulated",
        "rational_clock_policy": "explicit_bounded_exact",
        "input_timestamp_domain": "session_unscaled_seconds",
        "input_fixed_cutoff": "current_admitted_frame_endpoint",
        "snapshot_primitives": snapshot_descriptor(),
        "transport_byte_limits_v1": crate::transport::descriptor(),
        "native_contextual_lifecycle_batch": {"profile":crate::strict::BATCH_PROFILE,
            "wire_version":1,"native_only":true,"managed_integration":false,
            "callback_batch_attestation":false,"batch_phases":["Cleanup"],
            "serial_phases":["Initialize"],"max_records":4096,"max_envelope_bytes":1048576,
            "max_issuances_per_root":4096,"flags":1,"contacts":false,"coroutines":false,"nesting":false},
        "native_readiness": readiness_descriptor(),
        "native_readiness_nested_start": nested_start_descriptor(),
        "native_terminal_cleanup": {"capability":"native_terminal_cleanup_v1",
            "managed_integration":false,"closed_recovery":false,
            "projection_policy":"freeze_current","recovery_roots":1,
            "readiness_ticket_reconciliation":true,"max_readiness_claims":crate::readiness::LIMIT},
        "native_phase_context": {"profile":crate::strict::PROFILE,
            "qualification":"native_only_bridge_integration_pending",
            "supported_phases":crate::strict::supported_phases(),
            "revision_kind":"schedule_transition",
            "fixed_projection":"before_FixedUpdate_claim",
            "native_scheduler":false}
    })
}

fn readiness_descriptor() -> Value {
    json!({"capability":"native_readiness_primitives_v1",
        "requires_profile":crate::strict::PROFILE,
        "qualification":"native_only_bridge_integration_pending",
        "managed_integration":false,
        "max_claims_per_root":crate::readiness::LIMIT,
        "max_scripts":crate::readiness::LIMIT,"max_routines":crate::readiness::LIMIT,
        "iterator_model":"single_iterator_per_routine",
        "predicates":false,"nested_stacks":false,
        "completion_replay":"last_completion_in_current_root",
        "waits":["next","fixed","end_frame","scaled","unscaled","join"]})
}

fn snapshot_descriptor() -> Value {
    json!({"capability":"snapshot_primitives_v1","qualification":"native_only_adapter_pending",
        "requires_profile":crate::strict::PROFILE,"capture_stage":"initialized_idle",
        "max_receipts":crate::snapshots::MAX_RECEIPTS,
        "max_represented_bytes":crate::snapshots::MAX_SNAPSHOT_BYTES,
        "digest":"sha256_compact_sorted_snapshot_json_utf8",
        "revision_kind":"canonical_content_top_level_commit",
        "managed_readiness":false,"pixel_attestation":false,
        "historical_validation":"retained_identity_only",
        "release":"single_use","storage":"metadata_only_owned_response_bytes"})
}

fn nested_start_descriptor() -> Value {
    json!({"capability":crate::readiness::NESTED_PROFILE,
        "requires_profile":crate::readiness::NESTED_PROFILE,
        "selection":"session.open_before_gameplay",
        "qualification":"native_only_bridge_integration_pending",
        "managed_integration":false,"native_scheduler":false,
        "immediate_child_starts":true,"immediate_lifecycle_children":true,"max_child_depth":crate::readiness::MAX_CHILD_DEPTH,
        "start_inside_lifecycle":true,"start_inside_movenext":true,
        "start_inside_current":false,"start_inside_dispose":false,
        "yielded_ienumerator":false,"predicates":false,
        "max_claims_per_root":crate::readiness::LIMIT,
        "claim_reservation_per_child_scope":3,
        "ordering":"child_suspend_or_dispose_before_parent_completion_or_peer",
        "completion_replay":"last_completion_in_current_root"})
}
