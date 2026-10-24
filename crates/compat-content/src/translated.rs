mod api;
mod apply;
mod closure;
mod effective;
mod proof;
mod records;
mod scope;
mod source;
mod types;

pub use effective::{EffectiveRecord, EffectiveSelectionObservation};
pub use proof::{OriginalSelectionDecision, SelectionDisposition, TranslatedAdmissionProof};
pub use source::{ApiSurface, AssemblyApiUse, GameSourceQualification, TrustedGameSourceProof};
pub use types::*;

use crate::{
    AdmissionPolicy, AdmissionProof, ContentReceipt, DependencyPolicy, MaterializationPlan,
    ReferenceResolution, TrustedProviderInventory,
};
use std::path::Path;

pub struct TranslatedContentReceipt<'a> {
    original: &'a ContentReceipt,
    original_components: AdmissionPolicy,
    original_dependencies: DependencyPolicy,
    selection: SelectionPolicy,
    graph: crate::ImportedScene,
    ledger: Vec<OriginalSelectionDecision>,
}
pub struct AdmittedTranslatedContent<'a> {
    receipt: &'a TranslatedContentReceipt<'a>,
    proof: TranslatedAdmissionProof,
    plan: MaterializationPlan,
}
impl ContentReceipt {
    pub fn select_execution<'a>(
        &'a self,
        components: &AdmissionPolicy,
        dependencies: &DependencyPolicy,
        selection: &SelectionPolicy,
    ) -> Result<TranslatedContentReceipt<'a>, String> {
        let (graph, ledger) = apply::select(self, selection)?;
        Ok(TranslatedContentReceipt {
            original: self,
            original_components: components.clone(),
            original_dependencies: dependencies.clone(),
            selection: selection.clone(),
            graph,
            ledger,
        })
    }
}
impl TranslatedContentReceipt<'_> {
    pub fn original(&self) -> &ContentReceipt {
        self.original
    }
    pub fn selection_policy(&self) -> &SelectionPolicy {
        &self.selection
    }
    pub fn execution_graph(&self) -> &crate::ImportedScene {
        &self.graph
    }
    pub fn selection_ledger(&self) -> &[OriginalSelectionDecision] {
        &self.ledger
    }
    pub fn original_proof(&self, providers: &TrustedProviderInventory) -> AdmissionProof {
        match self.original.admit(
            &self.original_components,
            &self.original_dependencies,
            providers,
        ) {
            Ok(admitted) => admitted.proof().clone(),
            Err(error) => *error.proof,
        }
    }
    pub fn admit<'a>(
        &'a self,
        root: &Path,
        providers: &TrustedProviderInventory,
        source: &GameSourceQualification,
    ) -> Result<AdmittedTranslatedContent<'a>, String> {
        self.original
            .verify_current(root)
            .map_err(|e| e.to_string())?;
        source.verify_current(root)?;
        source.validate(
            self.original,
            &self.selection.profile_id,
            self.selection.keyboard_only,
        )?;
        closure::providers(&self.selection, providers)?;
        self.validate_retained(source)?;
        records::validate(self.original, &self.graph)?;
        closure::graph(&self.graph, &self.selection, source)?;
        let plan = crate::admission::plan(&self.graph, &AdmissionPolicy::default())
            .map_err(|e| e.to_string())?;
        let mut proof = TranslatedAdmissionProof {
            schema_version: 1,
            engine_version: "translated-execution-v1".into(),
            original_proof: self.original_proof(providers),
            original_receipt: self.original.proof(),
            execution_graph_sha256: crate::dependency::identity(&self.graph),
            selection_policy_sha256: crate::dependency::identity(&self.selection),
            selection_policy: self.selection.clone(),
            provider_inventory_sha256: providers.sha256(),
            game_source_qualification_sha256: source.sha256(),
            ledger: self.ledger.clone(),
            effective_observations: effective::observe(&self.graph),
            admission_sha256: String::new(),
        };
        proof.admission_sha256 = crate::dependency::identity(&proof);
        Ok(AdmittedTranslatedContent {
            receipt: self,
            proof,
            plan,
        })
    }
    fn validate_retained(&self, source: &GameSourceQualification) -> Result<(), String> {
        for decision in &self.ledger {
            if matches!(decision.disposition, SelectionDisposition::Retained) {
                match &decision.observation.resolution {
                    ReferenceResolution::ResolvedTarget { .. } => {}
                    ReferenceResolution::ManagedTypeMapping { type_name }
                        if source.has_type(type_name) => {}
                    _ => {
                        return Err(format!(
                            "unresolved retained original observation: {:?}",
                            decision.observation.locator
                        ));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn verify_admission(
        &self,
        root: &Path,
        expected: &TranslatedAdmissionProof,
        providers: &TrustedProviderInventory,
        source: &GameSourceQualification,
    ) -> Result<(), String> {
        let current = self.admit(root, providers, source)?;
        if serde_json::to_value(expected).map_err(|e| e.to_string())?
            != serde_json::to_value(current.proof()).map_err(|e| e.to_string())?
        {
            return Err("translated proof differs from current receipt recomputation".into());
        }
        Ok(())
    }
}
impl AdmittedTranslatedContent<'_> {
    pub fn proof(&self) -> &TranslatedAdmissionProof {
        &self.proof
    }
    pub fn materialization_plan(&self) -> &MaterializationPlan {
        &self.plan
    }
    pub fn execution_graph(&self) -> &crate::ImportedScene {
        &self.receipt.graph
    }
    pub fn original(&self) -> &ContentReceipt {
        self.receipt.original
    }
    pub fn verify_inputs(
        &self,
        root: &Path,
        policy: &SelectionPolicy,
        providers: &TrustedProviderInventory,
        source: &GameSourceQualification,
    ) -> Result<(), String> {
        if crate::dependency::identity(policy) != self.proof.selection_policy_sha256 {
            return Err("current selection policy changed".into());
        }
        self.receipt
            .verify_admission(root, &self.proof, providers, source)
    }
}
