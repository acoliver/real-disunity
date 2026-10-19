//! Behavioral contract tests for the universal semantic world model.
//!
//! All names are synthetic and arbitrary; no dataset, save, or corpus naming
//! conventions are exercised.

use std::collections::BTreeMap;

use worldrunner_world_model::{
    ContentDigest, DOMAIN_GENERATED_STATE, DOMAIN_SOURCE_IDENTITY, DecodedSave, DigestError,
    ResourceBinding, SHA256_LEN, SchemaDescriptor, SemanticComponent, SemanticMap, SemanticValue,
    SemanticWorldDocument, SourceIdentity, ValidationLimits, WorldModelError,
};

const CORPUS_A: [u8; SHA256_LEN] = [1; SHA256_LEN];
const CORPUS_B: [u8; SHA256_LEN] = [2; SHA256_LEN];
const POLICY_A: [u8; SHA256_LEN] = [3; SHA256_LEN];
const POLICY_B: [u8; SHA256_LEN] = [4; SHA256_LEN];

fn limits() -> ValidationLimits {
    ValidationLimits {
        max_depth: 8,
        max_nodes: 64,
        max_text_bytes: 256,
    }
}

fn map(entries: &[(&str, SemanticValue)]) -> SemanticMap {
    let collected: BTreeMap<String, SemanticValue> = entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect();
    SemanticMap::new(collected)
}

fn component(component_id: &str, entries: &[(&str, SemanticValue)]) -> SemanticComponent {
    SemanticComponent {
        component_id: component_id.to_owned(),
        schema_uri: "synthetic:schema/alpha".to_owned(),
        payload: map(entries),
    }
}

fn save_of(components: &[SemanticComponent]) -> DecodedSave {
    DecodedSave::new(components.to_vec(), &limits()).expect("valid decoded save")
}

fn single_save(entries: &[(&str, SemanticValue)]) -> DecodedSave {
    save_of(&[component("unit-alpha", entries)])
}

fn identity_of(save: &DecodedSave) -> SourceIdentity {
    SourceIdentity::of(save).expect("identity derivation")
}

fn single_identity(entries: &[(&str, SemanticValue)]) -> SourceIdentity {
    identity_of(&single_save(entries))
}

fn binding(binding_id: &str, byte_length: u64) -> ResourceBinding {
    ResourceBinding {
        binding_id: binding_id.to_owned(),
        capability_uri: "synthetic:capability/bytes".to_owned(),
        content: ContentDigest::from_bytes([7; SHA256_LEN]).expect("nonzero digest"),
        byte_length,
        element_uri: None,
    }
}

fn document_with(save: &DecodedSave, bindings: Vec<ResourceBinding>) -> SemanticWorldDocument {
    SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: "synthetic:world".to_owned(),
            version: 1,
        },
        identity_of(save),
        save.components().to_vec(),
        bindings,
        &limits(),
    )
    .expect("valid document")
}

fn generated(
    document: &SemanticWorldDocument,
    corpus: [u8; SHA256_LEN],
    policy: [u8; SHA256_LEN],
) -> String {
    let corpus = ContentDigest::from_bytes(corpus).expect("nonzero digest");
    let policy = ContentDigest::from_bytes(policy).expect("nonzero digest");
    document
        .generated_state_digest(corpus, policy)
        .expect("generated state digest")
        .to_hex()
}

#[test]
fn value_type_tag_and_length_distinctness() {
    let unsigned = single_identity(&[("v", SemanticValue::Unsigned(1))]);
    let signed = single_identity(&[("v", SemanticValue::Signed(1))]);
    let boolean = single_identity(&[("v", SemanticValue::Bool(true))]);
    assert_ne!(unsigned, signed);
    assert_ne!(unsigned, boolean);
    assert_ne!(signed, boolean);

    let text = single_identity(&[("v", SemanticValue::Text("a".to_owned()))]);
    let sequence = single_identity(&[("v", SemanticValue::Sequence(vec![SemanticValue::Null]))]);
    let null = single_identity(&[("v", SemanticValue::Null)]);
    let empty_text = single_identity(&[("v", SemanticValue::Text(String::new()))]);
    let empty_sequence = single_identity(&[("v", SemanticValue::Sequence(Vec::new()))]);
    let empty_map = single_identity(&[("v", SemanticValue::Map(SemanticMap::empty()))]);
    assert_ne!(text, sequence);
    assert_ne!(null, empty_text);
    assert_ne!(empty_text, empty_sequence);
    assert_ne!(empty_sequence, empty_map);

    let long_text = single_identity(&[("v", SemanticValue::Text("ab".to_owned()))]);
    assert_ne!(text, long_text);
    let split = single_identity(&[(
        "v",
        SemanticValue::Sequence(vec![
            SemanticValue::Text("a".to_owned()),
            SemanticValue::Text("b".to_owned()),
        ]),
    )]);
    assert_ne!(long_text, split);

    let small = single_identity(&[("v", SemanticValue::Unsigned(1))]);
    let large = single_identity(&[("v", SemanticValue::Unsigned(256))]);
    assert_ne!(small, large);
    assert_ne!(small, single_identity(&[("v", SemanticValue::Unsigned(2))]));
}

#[test]
fn map_ordering_determinism() {
    let ascending = map(&[
        ("alpha", SemanticValue::Unsigned(1)),
        ("beta", SemanticValue::Unsigned(2)),
    ]);
    let descending_inputs: BTreeMap<String, SemanticValue> = vec![
        ("beta".to_owned(), SemanticValue::Unsigned(2)),
        ("alpha".to_owned(), SemanticValue::Unsigned(1)),
    ]
    .into_iter()
    .collect();
    let descending = SemanticMap::new(descending_inputs);
    assert_eq!(ascending, descending);

    let ascending_save = save_of(&[component(
        "unit-alpha",
        &[
            ("alpha", SemanticValue::Unsigned(1)),
            ("beta", SemanticValue::Unsigned(2)),
        ],
    )]);
    let descending_save = save_of(&[component(
        "unit-alpha",
        &[
            ("beta", SemanticValue::Unsigned(2)),
            ("alpha", SemanticValue::Unsigned(1)),
        ],
    )]);
    assert_eq!(identity_of(&ascending_save), identity_of(&descending_save));
}

#[test]
fn depth_bounds_are_enforced() {
    let shallow = ValidationLimits {
        max_depth: 3,
        max_nodes: 64,
        max_text_bytes: 256,
    };
    let exactly_three = component(
        "unit-alpha",
        &[("v", SemanticValue::Sequence(vec![SemanticValue::Null]))],
    );
    assert!(DecodedSave::new(vec![exactly_three], &shallow).is_ok());

    let too_deep = component(
        "unit-alpha",
        &[(
            "v",
            SemanticValue::Sequence(vec![SemanticValue::Sequence(vec![SemanticValue::Null])]),
        )],
    );
    assert_eq!(
        DecodedSave::new(vec![too_deep], &shallow).unwrap_err(),
        WorldModelError::DepthLimitExceeded { max_depth: 3 }
    );
}

#[test]
fn node_bounds_are_aggregate_and_maps_count_once() {
    let tiny_nodes = ValidationLimits {
        max_depth: 8,
        max_nodes: 3,
        max_text_bytes: 256,
    };
    let two_entries = component(
        "unit-alpha",
        &[("a", SemanticValue::Null), ("b", SemanticValue::Null)],
    );
    assert!(DecodedSave::new(vec![two_entries], &tiny_nodes).is_ok());
    let three_entries = component(
        "unit-alpha",
        &[
            ("a", SemanticValue::Null),
            ("b", SemanticValue::Null),
            ("c", SemanticValue::Null),
        ],
    );
    assert!(DecodedSave::new(vec![three_entries], &tiny_nodes).is_err());

    let aggregate_nodes = vec![
        component("unit-alpha", &[("v", SemanticValue::Null)]),
        component("unit-beta", &[("v", SemanticValue::Null)]),
    ];
    assert_eq!(
        DecodedSave::new(aggregate_nodes, &tiny_nodes).unwrap_err(),
        WorldModelError::NodeLimitExceeded { max_nodes: 3 }
    );

    let nested_map = component(
        "unit-alpha",
        &[(
            "v",
            SemanticValue::Map(map(&[("nested", SemanticValue::Null)])),
        )],
    );
    assert!(DecodedSave::new(vec![nested_map], &tiny_nodes).is_ok());
}

#[test]
fn text_bounds_are_aggregate() {
    let tiny_text = ValidationLimits {
        max_depth: 8,
        max_nodes: 64,
        max_text_bytes: 4,
    };
    let short_text = component(
        "unit-alpha",
        &[("v", SemanticValue::Text("abcd".to_owned()))],
    );
    assert!(DecodedSave::new(vec![short_text], &tiny_text).is_ok());
    let long_text = component(
        "unit-alpha",
        &[("v", SemanticValue::Text("abcde".to_owned()))],
    );
    assert!(DecodedSave::new(vec![long_text], &tiny_text).is_err());

    let aggregate_text = vec![
        component(
            "unit-alpha",
            &[("v", SemanticValue::Text("abc".to_owned()))],
        ),
        component("unit-beta", &[("v", SemanticValue::Text("def".to_owned()))]),
    ];
    assert_eq!(
        DecodedSave::new(aggregate_text, &tiny_text).unwrap_err(),
        WorldModelError::TextByteLimitExceeded { max_text_bytes: 4 }
    );
}

#[test]
fn strict_ordering_and_duplicates_are_rejected() {
    let unsorted = vec![
        component("unit-beta", &[("v", SemanticValue::Null)]),
        component("unit-alpha", &[("v", SemanticValue::Null)]),
    ];
    assert_eq!(
        DecodedSave::new(unsorted, &limits()).unwrap_err(),
        WorldModelError::UnsortedComponents
    );

    let duplicate = vec![
        component("unit-alpha", &[("v", SemanticValue::Null)]),
        component("unit-alpha", &[("v", SemanticValue::Unsigned(1))]),
    ];
    assert_eq!(
        DecodedSave::new(duplicate, &limits()).unwrap_err(),
        WorldModelError::DuplicateComponent {
            identifier: "unit-alpha".to_owned()
        }
    );

    let save = save_of(&[component("unit-alpha", &[("v", SemanticValue::Null)])]);
    let unsorted_bindings = vec![binding("binding-two", 1), binding("binding-one", 1)];
    let result = SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: "synthetic:world".to_owned(),
            version: 1,
        },
        identity_of(&save),
        save.components().to_vec(),
        unsorted_bindings,
        &limits(),
    );
    assert_eq!(result.unwrap_err(), WorldModelError::UnsortedBindings);

    let duplicate_bindings = vec![binding("binding-one", 1), binding("binding-one", 2)];
    let result = SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: "synthetic:world".to_owned(),
            version: 1,
        },
        identity_of(&save),
        save.components().to_vec(),
        duplicate_bindings,
        &limits(),
    );
    assert_eq!(
        result.unwrap_err(),
        WorldModelError::DuplicateBinding {
            identifier: "binding-one".to_owned()
        }
    );
}

#[test]
fn malformed_identifiers_and_map_keys_are_rejected() {
    let empty_id = DecodedSave::new(
        vec![component("", &[("v", SemanticValue::Null)])],
        &limits(),
    );
    assert_eq!(
        empty_id.unwrap_err(),
        WorldModelError::InvalidIdentifier {
            identifier: String::new()
        }
    );

    let control_id = DecodedSave::new(
        vec![component("unit\u{7}alpha", &[("v", SemanticValue::Null)])],
        &limits(),
    );
    assert!(matches!(
        control_id.unwrap_err(),
        WorldModelError::InvalidIdentifier { .. }
    ));

    let empty_key = DecodedSave::new(
        vec![component("unit-alpha", &[("", SemanticValue::Null)])],
        &limits(),
    );
    assert_eq!(empty_key.unwrap_err(), WorldModelError::EmptyMapKey);

    let control_key = DecodedSave::new(
        vec![component(
            "unit-alpha",
            &[("key\u{1}", SemanticValue::Null)],
        )],
        &limits(),
    );
    assert_eq!(
        control_key.unwrap_err(),
        WorldModelError::ControlCharacterInMapKey
    );
}

#[test]
fn malformed_uris_are_rejected() {
    let schemeless = DecodedSave::new(
        vec![SemanticComponent {
            component_id: "unit-alpha".to_owned(),
            schema_uri: "no-scheme-here".to_owned(),
            payload: SemanticMap::empty(),
        }],
        &limits(),
    );
    assert_eq!(
        schemeless.unwrap_err(),
        WorldModelError::InvalidUri {
            uri: "no-scheme-here".to_owned()
        }
    );

    let spaced = DecodedSave::new(
        vec![SemanticComponent {
            component_id: "unit-alpha".to_owned(),
            schema_uri: "bad scheme:x".to_owned(),
            payload: SemanticMap::empty(),
        }],
        &limits(),
    );
    assert!(matches!(
        spaced.unwrap_err(),
        WorldModelError::InvalidUri { .. }
    ));

    let numeric_scheme = DecodedSave::new(
        vec![SemanticComponent {
            component_id: "unit-alpha".to_owned(),
            schema_uri: "1invalid:schema".to_owned(),
            payload: SemanticMap::empty(),
        }],
        &limits(),
    );
    assert!(matches!(
        numeric_scheme.unwrap_err(),
        WorldModelError::InvalidUri { .. }
    ));
}

#[test]
fn malformed_digests_are_rejected() {
    assert_eq!(
        ContentDigest::from_hex("0".repeat(63).as_str()),
        Err(DigestError::InvalidHexLength)
    );
    assert_eq!(
        ContentDigest::from_hex(&"g".repeat(64)),
        Err(DigestError::InvalidHexCharacter)
    );
    assert_eq!(
        ContentDigest::from_bytes([0; SHA256_LEN]),
        Err(DigestError::ZeroDigest)
    );
}

#[test]
fn zero_byte_bindings_are_accepted() {
    let save = single_save(&[("v", SemanticValue::BindingRef("binding-empty".to_owned()))]);
    let document = document_with(&save, vec![binding("binding-empty", 0)]);
    assert_eq!(document.bindings()[0].byte_length, 0);
    let digest = generated(&document, CORPUS_A, POLICY_A);
    assert_eq!(digest.len(), SHA256_LEN * 2);
}

#[test]
fn dangling_binding_refs_are_rejected() {
    let save = single_save(&[("v", SemanticValue::BindingRef("binding-missing".to_owned()))]);
    let result = SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: "synthetic:world".to_owned(),
            version: 1,
        },
        identity_of(&save),
        save.components().to_vec(),
        vec![binding("binding-other", 1)],
        &limits(),
    );
    assert_eq!(
        result.unwrap_err(),
        WorldModelError::UnresolvedBindingRef {
            identifier: "binding-missing".to_owned()
        }
    );

    let nested = single_save(&[(
        "v",
        SemanticValue::Sequence(vec![SemanticValue::Map(SemanticMap::new(
            vec![(
                "ref".to_owned(),
                SemanticValue::BindingRef("binding-missing".to_owned()),
            )]
            .into_iter()
            .collect(),
        ))]),
    )]);
    let result = SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: "synthetic:world".to_owned(),
            version: 1,
        },
        identity_of(&nested),
        nested.components().to_vec(),
        Vec::new(),
        &limits(),
    );
    assert!(matches!(
        result.unwrap_err(),
        WorldModelError::UnresolvedBindingRef { .. }
    ));

    let resolved = single_save(&[("v", SemanticValue::BindingRef("binding-one".to_owned()))]);
    assert!(
        document_with(&resolved, vec![binding("binding-one", 1)])
            .validate(&limits())
            .is_ok()
    );
}

#[test]
fn source_identity_is_deterministic_and_corpus_policy_independent() {
    let save = single_save(&[("v", SemanticValue::Text("synthetic".to_owned()))]);

    // The derivation signature accepts only the canonical decoded save, so no
    // corpus, policy, timestamp, or provenance can influence the identity.
    let first = SourceIdentity::of(&save).expect("identity derivation");
    let second = SourceIdentity::of(&save).expect("identity derivation");
    assert_eq!(first, second);

    let rebuilt = DecodedSave::new(save.components().to_vec(), &limits()).expect("rebuilt save");
    assert_eq!(
        first,
        SourceIdentity::of(&rebuilt).expect("identity derivation")
    );

    let document = document_with(&save, Vec::new());
    assert_eq!(*first.digest(), *document.source().digest());
    assert_ne!(
        generated(&document, CORPUS_A, POLICY_A),
        generated(&document, CORPUS_B, POLICY_B)
    );
    assert_eq!(&first, document.source());
}

#[test]
fn generated_digest_moves_independently_for_world_corpus_and_policy() {
    let world_a = single_save(&[("v", SemanticValue::Unsigned(1))]);
    let world_b = single_save(&[("v", SemanticValue::Unsigned(2))]);

    let document_a = document_with(&world_a, Vec::new());
    let document_b = document_with(&world_b, Vec::new());

    let base = generated(&document_a, CORPUS_A, POLICY_A);
    assert_ne!(base, generated(&document_b, CORPUS_A, POLICY_A));
    assert_ne!(base, generated(&document_a, CORPUS_B, POLICY_A));
    assert_ne!(base, generated(&document_a, CORPUS_A, POLICY_B));

    let mut other_corpus = CORPUS_B;
    other_corpus[0] = 5;
    let corpus_digest = ContentDigest::from_bytes(other_corpus).expect("nonzero digest");
    let policy_digest = ContentDigest::from_bytes(POLICY_A).expect("nonzero digest");
    let digest = document_a
        .generated_state_digest(corpus_digest, policy_digest)
        .expect("generated state digest");
    assert_eq!(digest.as_bytes().len(), SHA256_LEN);
    assert_eq!(digest.to_hex().len(), SHA256_LEN * 2);
    assert_ne!(digest.to_hex(), base);
}

#[test]
fn equivalent_documents_match_without_any_path_context() {
    // Physical source names cannot be represented in the model: DecodedSave
    // exposes only components, and SourceIdentity is derived exclusively from
    // them. Two saves assembled independently therefore match exactly.
    let first = single_save(&[
        ("a", SemanticValue::Unsigned(1)),
        (
            "b",
            SemanticValue::Sequence(vec![SemanticValue::Text("x".to_owned())]),
        ),
    ]);
    let second = DecodedSave::new(
        vec![SemanticComponent {
            component_id: "unit-alpha".to_owned(),
            schema_uri: "synthetic:schema/alpha".to_owned(),
            payload: map(&[
                (
                    "b",
                    SemanticValue::Sequence(vec![SemanticValue::Text("x".to_owned())]),
                ),
                ("a", SemanticValue::Unsigned(1)),
            ]),
        }],
        &limits(),
    )
    .expect("valid decoded save");

    assert_eq!(first, second);
    assert_eq!(identity_of(&first), identity_of(&second));

    let document_first = document_with(&first, vec![binding("binding-one", 3)]);
    let document_second = document_with(&second, vec![binding("binding-one", 3)]);
    assert_eq!(
        generated(&document_first, CORPUS_A, POLICY_A),
        generated(&document_second, CORPUS_A, POLICY_A)
    );
}

#[test]
fn authored_component_rename_changes_identity() {
    let alpha = single_save(&[("v", SemanticValue::Null)]);
    let beta = save_of(&[component("unit-beta", &[("v", SemanticValue::Null)])]);
    assert_ne!(identity_of(&alpha), identity_of(&beta));

    let payload_only_change = single_save(&[("v", SemanticValue::Unsigned(1))]);
    assert_ne!(identity_of(&alpha), identity_of(&payload_only_change));

    let empty = DecodedSave::new(Vec::new(), &limits()).expect("empty save");
    assert_ne!(identity_of(&alpha), identity_of(&empty));
}

#[test]
fn frozen_domain_tags_are_distinct_and_stable() {
    assert_eq!(
        DOMAIN_SOURCE_IDENTITY,
        "real-disunity:world-model:source-identity:v2"
    );
    assert_eq!(
        DOMAIN_GENERATED_STATE,
        "real-disunity:world-model:generated-state:v1"
    );
    assert_ne!(DOMAIN_SOURCE_IDENTITY, DOMAIN_GENERATED_STATE);

    let save = single_save(&[("v", SemanticValue::Null)]);
    let document = document_with(&save, Vec::new());
    let identity = identity_of(&save);
    let source_digest = identity.digest().as_bytes();
    let state_digest = document
        .generated_state_digest(
            ContentDigest::from_bytes(CORPUS_A).expect("nonzero digest"),
            ContentDigest::from_bytes(POLICY_A).expect("nonzero digest"),
        )
        .expect("generated state digest");
    assert_ne!(source_digest, state_digest.as_bytes());
}

#[test]
fn public_types_implement_serde_traits() {
    fn assert_serde<T: serde::Serialize + serde::de::DeserializeOwned>() {}
    assert_serde::<SemanticValue>();
    assert_serde::<SemanticMap>();
    assert_serde::<SemanticComponent>();
    assert_serde::<DecodedSave>();
    assert_serde::<SourceIdentity>();
    assert_serde::<SchemaDescriptor>();
    assert_serde::<ResourceBinding>();
    assert_serde::<SemanticWorldDocument>();
    assert_serde::<ContentDigest>();
    assert_serde::<worldrunner_world_model::GeneratedStateDigest>();
}

#[test]
fn pathless_source_resources_are_identity_inputs() {
    let components = vec![component(
        "source-component",
        &[(
            "resource",
            SemanticValue::BindingRef("source-resource".to_owned()),
        )],
    )];
    let first = DecodedSave::with_resources(
        components.clone(),
        vec![binding("source-resource", 12)],
        &limits(),
    )
    .expect("valid source resource");
    let second =
        DecodedSave::with_resources(components, vec![binding("source-resource", 13)], &limits())
            .expect("valid changed source resource");
    assert_ne!(identity_of(&first), identity_of(&second));
    assert_eq!(first.resources()[0].byte_length, 12);
    let json = serde_json::to_string(&first).expect("serialize decoded source");
    assert!(json.contains("source-resource"));
    assert!(!json.contains("logical_path"));
}

// Golden vectors pin the exact digests of two fixed synthetic fixtures so any
// accidental change to the frozen canonical encoding is caught.
const GOLDEN_SOURCE_IDENTITY_HEX: &str =
    "a3da99368f689ec425de4c8c10f425ac524e48d53c0780841224693321f603a0";
const GOLDEN_GENERATED_STATE_HEX: &str =
    "ea9ae734ef7c40af73d5708e1e025151af0f7b4bf35f2ad1a2c99f1d58179a84";

fn golden_save() -> DecodedSave {
    save_of(&[
        component("unit-alpha", &[("v", SemanticValue::Unsigned(7))]),
        component(
            "unit-beta",
            &[(
                "payload",
                SemanticValue::Map(SemanticMap::new(
                    vec![
                        ("enabled".to_owned(), SemanticValue::Bool(true)),
                        ("label".to_owned(), SemanticValue::Text("golden".to_owned())),
                        (
                            "asset".to_owned(),
                            SemanticValue::BindingRef("binding-one".to_owned()),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                )),
            )],
        ),
    ])
}

fn golden_document() -> SemanticWorldDocument {
    document_with(&golden_save(), vec![binding("binding-one", 12)])
}

#[test]
fn golden_source_identity_vector() {
    let identity = identity_of(&golden_save());
    println!("golden source identity: {}", identity.digest().to_hex());
    assert_eq!(identity.digest().to_hex(), GOLDEN_SOURCE_IDENTITY_HEX);
}

#[test]
fn golden_generated_state_vector() {
    let digest = golden_document()
        .generated_state_digest(
            ContentDigest::from_bytes(CORPUS_A).expect("nonzero digest"),
            ContentDigest::from_bytes(POLICY_A).expect("nonzero digest"),
        )
        .expect("generated state digest");
    println!("golden generated state: {}", digest.to_hex());
    assert_eq!(digest.to_hex(), GOLDEN_GENERATED_STATE_HEX);
}

#[test]
fn golden_fixtures_are_deterministic_today() {
    let first = identity_of(&golden_save());
    let second = identity_of(&golden_save());
    assert_eq!(first, second);
    assert_eq!(
        generated(&golden_document(), CORPUS_A, POLICY_A),
        generated(&golden_document(), CORPUS_A, POLICY_A)
    );
}

#[test]
fn runtime_components_are_independent_from_decoded_source_identity() {
    let source_save = single_save(&[("source", SemanticValue::Unsigned(1))]);
    let runtime_save = single_save(&[("runtime", SemanticValue::Unsigned(2))]);
    let document = SemanticWorldDocument::new(
        SchemaDescriptor {
            tag: "synthetic:world".to_owned(),
            version: 1,
        },
        identity_of(&source_save),
        runtime_save.components().to_vec(),
        Vec::new(),
        &limits(),
    )
    .expect("runtime components need not duplicate decoded source semantics");

    assert_eq!(document.source(), &identity_of(&source_save));
    assert_ne!(document.source(), &identity_of(&runtime_save));
    let encoded = serde_json::to_vec(&document).expect("serialize document");
    let decoded: SemanticWorldDocument = serde_json::from_slice(&encoded).expect("deserialize");
    assert_eq!(decoded, document);
}
