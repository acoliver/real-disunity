use super::*;
use serde_json::Value;

fn hash(digit: char) -> String {
    digit.to_string().repeat(64)
}

fn known<T>(value: T) -> Observed<T> {
    Observed::Known { value }
}

fn pose(frame: &str, x: f64) -> CameraPose {
    CameraPose {
        frame_id: frame.to_owned(),
        position_meters: [x, 20.0, 30.0],
        orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
    }
}

fn projection(fov: f64) -> ProjectionRecord {
    ProjectionRecord {
        projection_uri: "urn:example:projection:perspective".to_owned(),
        vertical_fov_degrees: fov,
        near_plane_meters: 0.1,
        far_plane_meters: 2000.0,
    }
}

fn full_state(turn: &str, weather: &str) -> CaptureState {
    let values = [
        ("animation_frame", "settled-3"),
        ("exposure", "auto"),
        ("graphics", "high"),
        ("layers", "terrain+vegetation"),
        ("time_of_day", "0830"),
        ("turn", turn),
        ("ui", "hidden"),
        ("weather", weather),
        ("world", "canonical-overview"),
    ];
    values
        .into_iter()
        .map(|(key, value)| (key.to_owned(), known(value.to_owned())))
        .collect()
}

fn side(role: ObservationRole, observation_id: &str, output_digit: char) -> CaptureSide {
    CaptureSide {
        role,
        observation_id: observation_id.to_owned(),
        input_sha256: hash('5'),
        build: BuildIdentity {
            product_uri: "urn:example:product:arbitrary".to_owned(),
            version: known("1.2.3".to_owned()),
            build_digest: known(hash('6')),
        },
        pose: PoseRecord {
            requested: pose("world-metres", 10.0),
            actual: known(pose("world-metres", 10.5)),
        },
        viewport: ViewportRecord {
            width: 1280,
            height: 720,
        },
        projection: known(projection(50.0)),
        state: full_state("12", "clear"),
        output: OutputArtifact {
            format_uri: "urn:example:image:png".to_owned(),
            sha256: hash(output_digit),
            bytes: 4096,
        },
    }
}

fn paired_manifest() -> PairedCaptureManifest {
    PairedCaptureManifest {
        schema: PAIRED_CAPTURE_MANIFEST_SCHEMA.to_owned(),
        schema_version: PAIRED_CAPTURE_MANIFEST_VERSION,
        manifest_id: "canonical-overview-001".to_owned(),
        source: side(ObservationRole::Source, "source-observation-001", '7'),
        replay: side(ObservationRole::Replay, "replay-observation-001", '8'),
        comparability: ComparabilityClaim {
            claimed_grade: ComparabilityGrade::AcceptanceEligible,
            pose_tolerance: PoseTolerance {
                position_meters: 1.0,
                orientation_degrees: 1.0,
                vertical_fov_degrees: 0.5,
            },
        },
    }
}

#[test]
fn synthetic_pair_round_trips_deterministically() {
    let manifest = paired_manifest();
    let bytes = manifest.to_json_bytes().unwrap();
    assert_eq!(manifest.to_json_bytes().unwrap(), bytes);
    assert_eq!(PairedCaptureManifest::from_json(&bytes).unwrap(), manifest);
    assert!(manifest.to_json_pretty().unwrap().starts_with(b"{\n  "));
    assert_eq!(
        manifest.derived_grade(),
        ComparabilityGrade::AcceptanceEligible
    );

    let digest = Sha256::digest(&bytes);
    let mut expected = String::new();
    for byte in digest {
        let _ = write!(expected, "{byte:02x}");
    }
    assert_eq!(manifest.content_digest().unwrap(), expected);
    assert_eq!(
        manifest.content_digest().unwrap(),
        manifest.content_digest().unwrap()
    );
}

#[test]
fn canonical_serialization_is_insertion_order_independent() {
    let mut manifest = paired_manifest();
    let reversed: CaptureState = manifest
        .source
        .state
        .iter()
        .rev()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let original_bytes = manifest.to_json_bytes().unwrap();
    manifest.source.state = reversed;
    assert_eq!(manifest.to_json_bytes().unwrap(), original_bytes);
}

#[test]
fn explicit_unknowns_survive_round_trip_and_weaken_the_grade() {
    let mut manifest = paired_manifest();
    manifest.comparability.claimed_grade = ComparabilityGrade::ApproximatePose;
    manifest.source.pose.actual = Observed::Unknown {
        reason: "camera pose was not recorded".to_owned(),
    };
    manifest.source.state.insert(
        "exposure".to_owned(),
        Observed::Unknown {
            reason: "not recorded".to_owned(),
        },
    );
    let bytes = manifest.to_json_bytes().unwrap();
    assert_eq!(PairedCaptureManifest::from_json(&bytes).unwrap(), manifest);
    assert_eq!(
        manifest.derived_grade(),
        ComparabilityGrade::ApproximatePose
    );

    let mut overclaiming = manifest.clone();
    overclaiming.comparability.claimed_grade = ComparabilityGrade::AcceptanceEligible;
    assert!(matches!(
        overclaiming.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "comparability.claimed_grade",
            ..
        })
    ));
    assert!(manifest.validate().is_ok());
}

#[test]
fn mismatched_private_inputs_pair_only_as_unpaired_observations() {
    let mut manifest = paired_manifest();
    manifest.replay.input_sha256 = hash('9');
    assert_eq!(
        manifest.derived_grade(),
        ComparabilityGrade::UnpairedObservation
    );

    let mut overclaiming = manifest.clone();
    overclaiming.comparability.claimed_grade = ComparabilityGrade::ApproximatePose;
    assert!(matches!(
        overclaiming.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "comparability.claimed_grade",
            expected,
            observed,
        }) if expected == "unpaired_observation" && observed == "approximate_pose"
    ));

    manifest.comparability.claimed_grade = ComparabilityGrade::UnpairedObservation;
    assert!(manifest.validate().is_ok());
}

#[test]
fn unregistered_or_distant_poses_cap_at_approximate() {
    let mut frames_differ = paired_manifest();
    frames_differ.replay.pose.actual = known(pose("source-native", 10.5));
    assert_eq!(
        frames_differ.derived_grade(),
        ComparabilityGrade::ApproximatePose
    );

    let mut distant = paired_manifest();
    distant.replay.pose.actual = known(pose("world-metres", 40.0));
    assert_eq!(distant.derived_grade(), ComparabilityGrade::ApproximatePose);

    for manifest in [&mut frames_differ, &mut distant] {
        manifest.comparability.claimed_grade = ComparabilityGrade::PoseEquivalent;
        assert!(matches!(
            manifest.validate(),
            Err(PairedCaptureError::FieldMismatch {
                field: "comparability.claimed_grade",
                ..
            })
        ));
        manifest.comparability.claimed_grade = ComparabilityGrade::ApproximatePose;
        assert!(manifest.validate().is_ok());
    }
}

#[test]
fn projection_differences_and_unknowns_cap_at_pose_equivalent() {
    let mut unknown_projection = paired_manifest();
    unknown_projection.source.projection = Observed::Unknown {
        reason: "projection readback unavailable".to_owned(),
    };
    assert_eq!(
        unknown_projection.derived_grade(),
        ComparabilityGrade::PoseEquivalent
    );

    let mut wide_fov = paired_manifest();
    wide_fov.replay.projection = known(projection(53.0));
    assert_eq!(wide_fov.derived_grade(), ComparabilityGrade::PoseEquivalent);

    let mut state_mismatch = paired_manifest();
    state_mismatch.replay.state = full_state("13", "clear");
    assert_eq!(
        state_mismatch.derived_grade(),
        ComparabilityGrade::PoseEquivalent
    );

    let mut unknown_state = paired_manifest();
    unknown_state.replay.state.insert(
        "weather".to_owned(),
        Observed::Unknown {
            reason: "not observable".to_owned(),
        },
    );
    assert_eq!(
        unknown_state.derived_grade(),
        ComparabilityGrade::PoseEquivalent
    );

    for manifest in [
        &mut unknown_projection,
        &mut wide_fov,
        &mut state_mismatch,
        &mut unknown_state,
    ] {
        manifest.comparability.claimed_grade = ComparabilityGrade::StateEquivalent;
        assert!(matches!(
            manifest.validate(),
            Err(PairedCaptureError::FieldMismatch {
                field: "comparability.claimed_grade",
                ..
            })
        ));
        manifest.comparability.claimed_grade = ComparabilityGrade::PoseEquivalent;
        assert!(manifest.validate().is_ok());
    }
}

#[test]
fn viewport_mismatch_caps_at_state_equivalent() {
    let mut manifest = paired_manifest();
    manifest.replay.viewport = ViewportRecord {
        width: 1920,
        height: 1080,
    };
    assert_eq!(
        manifest.derived_grade(),
        ComparabilityGrade::StateEquivalent
    );

    manifest.comparability.claimed_grade = ComparabilityGrade::AcceptanceEligible;
    assert!(matches!(
        manifest.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "comparability.claimed_grade",
            ..
        })
    ));
    manifest.comparability.claimed_grade = ComparabilityGrade::StateEquivalent;
    assert!(manifest.validate().is_ok());
}

#[test]
fn weaker_claims_than_derived_evidence_are_permitted() {
    let mut manifest = paired_manifest();
    manifest.comparability.claimed_grade = ComparabilityGrade::ApproximatePose;
    assert!(manifest.validate().is_ok());
}

#[test]
fn missing_core_state_keys_are_rejected_rather_than_implied() {
    let mut manifest = paired_manifest();
    manifest.source.state.remove("weather");
    manifest.replay.state.remove("weather");
    assert!(matches!(
        manifest.validate(),
        Err(PairedCaptureError::FieldMismatch { field: "state", .. })
    ));

    let mut extra_key = paired_manifest();
    extra_key
        .source
        .state
        .insert("cloud_cover".to_owned(), known("none".to_owned()));
    assert!(extra_key.validate().is_ok());
}

#[test]
fn malformed_poses_viewports_and_roles_are_rejected() {
    let mut non_finite = paired_manifest();
    non_finite.source.pose.requested.position_meters[1] = f64::NAN;
    assert!(matches!(
        non_finite.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "pose.requested",
            ..
        })
    ));
    assert!(non_finite.to_json_bytes().is_err());

    let mut denormalized = paired_manifest();
    denormalized.source.pose.requested.orientation_xyzw = [0.0; 4];
    assert!(matches!(
        denormalized.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "pose.requested",
            ..
        })
    ));

    let mut unnamed_frame = paired_manifest();
    unnamed_frame.source.pose.requested.frame_id = "not an identifier".to_owned();
    assert!(matches!(
        unnamed_frame.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "pose.requested",
            ..
        })
    ));

    let mut empty_reason = paired_manifest();
    empty_reason.source.pose.actual = Observed::Unknown {
        reason: " ".to_owned(),
    };
    empty_reason.comparability.claimed_grade = ComparabilityGrade::ApproximatePose;
    assert!(matches!(
        empty_reason.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "pose.actual",
            ..
        })
    ));

    let mut zero_viewport = paired_manifest();
    zero_viewport.source.viewport.width = 0;
    assert!(matches!(
        zero_viewport.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "viewport",
            ..
        })
    ));

    let mut swapped_roles = paired_manifest();
    swapped_roles.source.role = ObservationRole::Replay;
    assert!(matches!(
        swapped_roles.validate(),
        Err(PairedCaptureError::FieldMismatch { field: "role", .. })
    ));
}

#[test]
fn malformed_projections_hashes_and_outputs_are_rejected() {
    let mut zero_fov = paired_manifest();
    zero_fov.source.projection = known(projection(0.0));
    assert!(matches!(
        zero_fov.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "projection.vertical_fov_degrees",
            ..
        })
    ));

    let mut inverted_planes = paired_manifest();
    inverted_planes.source.projection = known(ProjectionRecord {
        projection_uri: "urn:example:projection:perspective".to_owned(),
        vertical_fov_degrees: 50.0,
        near_plane_meters: 2000.0,
        far_plane_meters: 0.1,
    });
    assert!(matches!(
        inverted_planes.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "projection.far_plane_meters",
            ..
        })
    ));

    let mut bad_input_hash = paired_manifest();
    bad_input_hash.replay.input_sha256 = "not-a-digest".to_owned();
    bad_input_hash.comparability.claimed_grade = ComparabilityGrade::UnpairedObservation;
    assert!(matches!(
        bad_input_hash.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "input_sha256",
            ..
        })
    ));

    let mut empty_output = paired_manifest();
    empty_output.source.output.bytes = 0;
    assert!(matches!(
        empty_output.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "output.bytes",
            ..
        })
    ));

    let mut bad_build_digest = paired_manifest();
    bad_build_digest.source.build.build_digest = known("short".to_owned());
    assert!(matches!(
        bad_build_digest.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "build.build_digest",
            ..
        })
    ));
}

#[test]
fn out_of_range_tolerances_are_rejected() {
    let mut negative = paired_manifest();
    negative.comparability.pose_tolerance.position_meters = -1.0;
    assert!(matches!(
        negative.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "pose_tolerance.position_meters",
            ..
        })
    ));

    let mut overflowing = paired_manifest();
    overflowing.comparability.pose_tolerance.orientation_degrees = 181.0;
    assert!(matches!(
        overflowing.validate(),
        Err(PairedCaptureError::FieldMismatch {
            field: "pose_tolerance.orientation_degrees",
            ..
        })
    ));
}

#[test]
fn unknown_fields_and_future_versions_are_rejected() {
    let mut text = String::from_utf8(paired_manifest().to_json_bytes().unwrap()).unwrap();
    text = text.replace(
        "\"manifest_id\"",
        "\"unexpected_field\": 1, \"manifest_id\"",
    );
    assert!(matches!(
        PairedCaptureManifest::from_json(text.as_bytes()),
        Err(PairedCaptureError::Document(VersionedDocumentError::Json(
            _
        )))
    ));

    let mut unknown_in_observed =
        String::from_utf8(paired_manifest().to_json_bytes().unwrap()).unwrap();
    unknown_in_observed = unknown_in_observed.replace(
        "{\"state\":\"known\",\"value\":",
        "{\"state\":\"known\",\"intruder\":true,\"value\":",
    );
    assert!(matches!(
        PairedCaptureManifest::from_json(unknown_in_observed.as_bytes()),
        Err(PairedCaptureError::Document(VersionedDocumentError::Json(
            _
        )))
    ));

    let future = String::from_utf8(paired_manifest().to_json_bytes().unwrap())
        .unwrap()
        .replace("\"schema_version\":1", "\"schema_version\":2");
    assert!(matches!(
        PairedCaptureManifest::from_json(future.as_bytes()),
        Err(PairedCaptureError::Document(
            VersionedDocumentError::UnsupportedSchemaVersion { .. }
        ))
    ));
}

#[test]
fn manifest_records_no_paths_or_personal_metadata() {
    let value: Value = serde_json::from_slice(&paired_manifest().to_json_bytes().unwrap()).unwrap();
    let text = value.to_string();
    for concept in [
        "path",
        "home",
        "users",
        "username",
        "account",
        "file://",
        "save_name",
    ] {
        assert!(
            !text.to_lowercase().contains(&concept.to_lowercase()),
            "manifest must not mention {concept}"
        );
    }
}
