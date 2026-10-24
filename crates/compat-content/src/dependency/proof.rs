use super::{
    Deserialize, EvidenceRef, Fidelity, LocatedReference, ProviderBinding, Serialize, identity,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceDecision {
    pub observation: LocatedReference,
    pub classification: DependencyClassification,
    pub rule_id: Option<String>,
    pub reviewed_disposition: Option<super::DependencyDisposition>,
    pub reason: Option<String>,
    pub provider_binding: Option<ProviderBinding>,
    pub evidence: Vec<EvidenceRef>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionProof {
    pub schema_version: u32,
    pub dependency_engine_version: String,
    pub importer_identity: String,
    pub component_omissions: std::collections::BTreeMap<String, String>,
    pub content_closure_sha256: String,
    pub dependency_policy_sha256: String,
    pub component_policy_sha256: String,
    pub provider_inventory_sha256: String,
    pub profile_id: String,
    pub fidelity: Fidelity,
    pub reference_decisions: Vec<ReferenceDecision>,
    pub unsupported_capabilities: Vec<String>,
    pub blockers: Vec<String>,
    pub outcome: AdmissionOutcome,
    pub admission_sha256: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DependencyClassification {
    RequiredRuntime,
    EditorConfiguration,
    RenderSettingsTranslated,
    ProvidedBuiltin,
    Blocked,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum AdmissionOutcome {
    Blocked,
    AdmittedOriginal,
    AdmittedTranslated,
}
impl AdmissionProof {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let proof: Self =
            serde_json::from_value(super::wire::parse(bytes)?).map_err(|e| e.to_string())?;
        if !proof.valid_seal() {
            return Err("invalid admission proof seal or interpreter version".into());
        }
        Ok(proof)
    }
    pub(super) fn valid_seal(&self) -> bool {
        let mut unsigned = self.clone();
        unsigned.admission_sha256.clear();
        self.schema_version == 1
            && self.dependency_engine_version == "dependency-admission-v1"
            && identity(&unsigned) == self.admission_sha256
    }
}
