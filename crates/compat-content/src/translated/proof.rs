use crate::{AdmissionProof, LocatedReference};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalSelectionDecision {
    pub observation: LocatedReference,
    pub disposition: SelectionDisposition,
    pub operation_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SelectionDisposition {
    Retained,
    Replaced,
    ExcludedFromExecution,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranslatedAdmissionProof {
    pub schema_version: u32,
    pub engine_version: String,
    pub original_proof: AdmissionProof,
    pub original_receipt: serde_json::Value,
    pub execution_graph_sha256: String,
    pub selection_policy_sha256: String,
    pub selection_policy: super::SelectionPolicy,
    pub provider_inventory_sha256: String,
    pub game_source_qualification_sha256: String,
    pub ledger: Vec<OriginalSelectionDecision>,
    pub effective_observations: Vec<super::EffectiveSelectionObservation>,
    pub admission_sha256: String,
}
impl TranslatedAdmissionProof {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        let proof: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let mut unsigned = proof.clone();
        unsigned.admission_sha256.clear();
        if proof.schema_version != 1
            || proof.engine_version != "translated-execution-v1"
            || crate::dependency::identity(&unsigned) != proof.admission_sha256
        {
            return Err("invalid translated proof seal/version".into());
        }
        Ok(proof)
    }
}
