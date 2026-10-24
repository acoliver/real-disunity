use super::{
    AdmissionOutcome, AdmissionProof, DependencyClassification, DependencyPolicy, Fidelity,
    TrustedProviderInventory, hash, identity,
};
use crate::{AdmissionPolicy, ContentReceipt, ImportError, MaterializationPlan};
use std::path::Path;

#[derive(Clone, Debug, thiserror::Error)]
#[error("dependency admission blocked: {message}")]
pub struct DependencyAdmissionError {
    pub message: String,
    pub proof: Box<AdmissionProof>,
}
pub struct AdmittedContent<'a> {
    receipt: &'a ContentReceipt,
    components: AdmissionPolicy,
    dependencies: DependencyPolicy,
    providers: TrustedProviderInventory,
    plan: MaterializationPlan,
    proof: AdmissionProof,
}
impl AdmittedContent<'_> {
    pub fn proof(&self) -> &AdmissionProof {
        &self.proof
    }
    pub fn materialization_plan(&self) -> &MaterializationPlan {
        &self.plan
    }
    pub fn receipt(&self) -> &ContentReceipt {
        self.receipt
    }
    pub fn verify_inputs(
        &self,
        root: &Path,
        components: &AdmissionPolicy,
        dependencies: &DependencyPolicy,
        providers: &TrustedProviderInventory,
    ) -> Result<(), ImportError> {
        self.receipt.verify_current(root)?;
        self.receipt
            .verify_admission(&self.proof, components, dependencies, providers)
            .map_err(|e| crate::source_io::error("admission", e.to_string()))
    }
    pub fn package_current(
        &self,
        root: &Path,
        variant: &worldrunner_asset_id::AssetVariant,
        source_package: &str,
    ) -> Result<Vec<worldrunner_package::AssetInput>, ImportError> {
        self.verify_current(root)?;
        self.receipt.package_current(root, variant, source_package)
    }
    pub fn verify_current(&self, root: &Path) -> Result<(), ImportError> {
        self.receipt.verify_current(root)?;
        let current = self
            .receipt
            .admit(&self.components, &self.dependencies, &self.providers)
            .map_err(|e| crate::source_io::error("admission", e.to_string()))?;
        if current.proof.admission_sha256 != self.proof.admission_sha256 {
            return Err(crate::source_io::error(
                "admission",
                "admission identity changed",
            ));
        }
        Ok(())
    }
}
impl ContentReceipt {
    pub fn admit<'a>(
        &'a self,
        components: &AdmissionPolicy,
        dependencies: &DependencyPolicy,
        providers: &TrustedProviderInventory,
    ) -> Result<AdmittedContent<'a>, DependencyAdmissionError> {
        let mut proof = self.admission_observation(components, dependencies, providers);
        let exemptions = proof
            .reference_decisions
            .iter()
            .filter(|d| {
                matches!(
                    d.classification,
                    DependencyClassification::EditorConfiguration
                        | DependencyClassification::RenderSettingsTranslated
                )
            })
            .filter_map(|d| super::classify::metadata_field(&d.observation.locator))
            .collect();
        let plan = crate::admission::plan_classified(self.graph(), components, &exemptions);
        match &plan {
            Ok(plan) => proof
                .component_omissions
                .clone_from(&plan.skipped_components),
            Err(error) => proof.blockers.push(error.to_string()),
        }
        if proof.blockers.is_empty() {
            proof.outcome = match dependencies.fidelity {
                Fidelity::Original => AdmissionOutcome::AdmittedOriginal,
                Fidelity::TranslatedUnlitKeyboardSubset => AdmissionOutcome::AdmittedTranslated,
            };
        }
        proof.admission_sha256 = identity(&proof);
        if !proof.blockers.is_empty() {
            return Err(blocked(proof));
        }
        Ok(AdmittedContent {
            receipt: self,
            components: components.clone(),
            dependencies: dependencies.clone(),
            providers: providers.clone(),
            plan: plan.expect("validated plan"),
            proof,
        })
    }
    fn admission_observation(
        &self,
        components: &AdmissionPolicy,
        dependencies: &DependencyPolicy,
        providers: &TrustedProviderInventory,
    ) -> AdmissionProof {
        let mut proof = AdmissionProof {
            schema_version: 1,
            dependency_engine_version: "dependency-admission-v1".into(),
            importer_identity: concat!("worldrunner-compat-content@", env!("CARGO_PKG_VERSION"))
                .into(),
            component_omissions: std::collections::BTreeMap::new(),
            content_closure_sha256: self.closure_sha256().into(),
            dependency_policy_sha256: identity(dependencies),
            component_policy_sha256: identity(components),
            provider_inventory_sha256: providers.sha256(),
            profile_id: dependencies.profile_id.clone(),
            fidelity: dependencies.fidelity,
            reference_decisions: self
                .dependency_references()
                .iter()
                .map(|r| super::classify::decide(r, dependencies, providers))
                .collect(),
            unsupported_capabilities: vec![],
            blockers: vec![],
            outcome: AdmissionOutcome::Blocked,
            admission_sha256: String::new(),
        };
        if let Err(error) = dependencies.validate() {
            proof.blockers.push(error);
        }
        self.validate_rules(dependencies, &mut proof);
        for requirement in &dependencies.required_capabilities {
            if providers.capability(requirement).is_none() {
                proof.blockers.push(format!(
                    "trusted capability unavailable: {:?}",
                    requirement.id
                ));
            }
        }
        for decision in &proof.reference_decisions {
            if decision.classification == DependencyClassification::Blocked {
                proof.blockers.push(format!(
                    "unclassified/blocked reference {:?}",
                    decision.observation.locator
                ));
            }
        }
        if dependencies.fidelity == Fidelity::TranslatedUnlitKeyboardSubset {
            proof.unsupported_capabilities = vec![
                "original URP shaders, lighting, shadows and postprocessing".into(),
                "InputActions importer, action evaluation and generated wrappers".into(),
                "editor package-manager and dedicated-server behavior".into(),
            ];
        }
        proof
    }
    fn validate_rules(&self, dependencies: &DependencyPolicy, proof: &mut AdmissionProof) {
        for rule in &dependencies.rules {
            if !self
                .dependency_references()
                .iter()
                .any(|r| r.locator == rule.locator)
            {
                proof
                    .blockers
                    .push(format!("stale/unmatched rule {}", rule.id));
            }
            for evidence in &rule.evidence {
                if let Some(bytes) = self.sources().get(&evidence.path)
                    && hash(bytes) != evidence.sha256
                {
                    proof
                        .blockers
                        .push(format!("evidence hash mismatch {}", evidence.path));
                }
            }
        }
    }
    pub fn verify_admission(
        &self,
        expected: &AdmissionProof,
        components: &AdmissionPolicy,
        dependencies: &DependencyPolicy,
        providers: &TrustedProviderInventory,
    ) -> Result<(), DependencyAdmissionError> {
        let current = self.admit(components, dependencies, providers)?;
        if !expected.valid_seal() || current.proof.admission_sha256 != expected.admission_sha256 {
            let mut proof = current.proof;
            proof
                .blockers
                .push("admission proof changed; qualify current inputs".into());
            proof.outcome = AdmissionOutcome::Blocked;
            proof.admission_sha256.clear();
            proof.admission_sha256 = identity(&proof);
            return Err(blocked(proof));
        }
        Ok(())
    }
}
fn blocked(proof: AdmissionProof) -> DependencyAdmissionError {
    DependencyAdmissionError {
        message: proof.blockers.join("; "),
        proof: Box::new(proof),
    }
}
