use super::*;

struct ChangedProvider {
    kind: ResourceKind,
}
impl TrustedProvider for ChangedProvider {
    fn id(&self) -> &'static str {
        "boundary-unlit"
    }
    fn resource_kind(&self) -> ResourceKind {
        self.kind
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        if self.kind == ResourceKind::Unknown {
            vec![]
        } else {
            vec![capability()]
        }
    }
    fn implementation_bytes(&self) -> &[u8] {
        b"changed boundary implementation"
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        b"changed boundary qualification"
    }
}
struct ChangedHost {
    base: SourceProof,
    pe: Vec<u8>,
    evidence: Vec<u8>,
}
impl TrustedGameSourceProof for ChangedHost {
    fn profile_id(&self) -> &str {
        self.base.profile_id()
    }
    fn content_closure_sha256(&self) -> &str {
        &self.base.closure
    }
    fn source_directory(&self) -> &Path {
        self.base.source_directory()
    }
    fn sources(&self) -> Vec<ScriptSource> {
        vec![]
    }
    fn assembly_bytes(&self) -> &[u8] {
        &self.pe
    }
    fn verification_evidence_bytes(&self) -> &[u8] {
        &self.evidence
    }
    fn defined_types(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
    fn api_uses(&self) -> Vec<AssemblyApiUse> {
        vec![]
    }
}
#[test]
fn generic_unknown_is_never_a_provider_kind() {
    assert!(
        TrustedProviderInventory::new()
            .register(&ChangedProvider {
                kind: ResourceKind::Unknown
            })
            .is_err()
    );
}
#[test]
fn changed_qualified_provider_invalidates_the_translated_proof() {
    let (root, receipt, policy) = setup();
    let translated = selection(&receipt, &policy);
    let source = qualification(&receipt);
    let admitted = translated
        .admit(root.path(), &inventory(), &source)
        .unwrap();
    let mut changed = TrustedProviderInventory::new();
    changed
        .register(&ChangedProvider {
            kind: ResourceKind::RenderSettings,
        })
        .unwrap();
    assert!(translated.admit(root.path(), &changed, &source).is_ok());
    assert!(
        admitted
            .verify_inputs(root.path(), &policy, &changed, &source)
            .is_err()
    );
}
#[test]
fn current_host_pe_and_verification_evidence_are_proof_bound() {
    let (root, receipt, policy) = setup();
    let base = SourceProof {
        closure: receipt.closure_sha256().into(),
    };
    let mut host = ChangedHost {
        pe: base.assembly_bytes().to_vec(),
        evidence: base.verification_evidence_bytes().to_vec(),
        base,
    };
    let translated = selection(&receipt, &policy);
    let providers = inventory();
    let source = GameSourceQualification::from_host(&host).unwrap();
    let admitted = translated.admit(root.path(), &providers, &source).unwrap();
    host.pe.push(1);
    let changed = GameSourceQualification::from_host(&host).unwrap();
    assert!(
        admitted
            .verify_inputs(root.path(), &policy, &providers, &changed)
            .is_err()
    );
    host.pe.pop();
    host.evidence.push(1);
    let changed = GameSourceQualification::from_host(&host).unwrap();
    assert!(
        admitted
            .verify_inputs(root.path(), &policy, &providers, &changed)
            .is_err()
    );
    host.pe = br#"{"ready":true,"types":["Control"]}"#.to_vec();
    assert!(GameSourceQualification::from_host(&host).is_err());
}
#[test]
fn forged_resealed_effective_observation_removal_is_rejected() {
    let (root, receipt, policy) = setup();
    let translated = selection(&receipt, &policy);
    let providers = inventory();
    let source = qualification(&receipt);
    let admitted = translated.admit(root.path(), &providers, &source).unwrap();
    let mut value = serde_json::to_value(admitted.proof()).unwrap();
    let observations = value["effective_observations"].as_array_mut().unwrap();
    assert!(!observations.is_empty());
    observations.pop();
    let forged = reseal(value);
    assert!(
        translated
            .verify_admission(root.path(), &forged, &providers, &source)
            .is_err()
    );
}
#[test]
fn ledger_retains_each_original_observation_and_each_effective_projection() {
    let (root, receipt, policy) = setup();
    let translated = selection(&receipt, &policy);
    let admitted = translated
        .admit(root.path(), &inventory(), &qualification(&receipt))
        .unwrap();
    let actual: Vec<_> = admitted
        .proof()
        .ledger
        .iter()
        .map(|d| d.observation.clone())
        .collect();
    assert_eq!(actual, receipt.dependency_references());
    println!(
        "TRANSLATED_BOUNDARY_PROOF {}",
        serde_json::to_string(admitted.proof()).unwrap()
    );
    let fields: Vec<_> = admitted
        .proof()
        .effective_observations
        .iter()
        .map(|o| (&o.record, &o.field))
        .collect();
    assert!(fields.iter().any(|(r, f)| matches!(
        r,
        EffectiveRecord::Execution {
            record: ExecutionRecord::Document { .. }
        }
    ) && f.contains(&FieldSegment::Key("m_GameObject".into()))));
    assert!(fields.iter().any(|(r, f)| matches!(
        r,
        EffectiveRecord::Execution {
            record: ExecutionRecord::Component {
                projection: ComponentProjection::Fields,
                ..
            }
        }
    ) && f.contains(&FieldSegment::Key("m_GameObject".into()))));
}
