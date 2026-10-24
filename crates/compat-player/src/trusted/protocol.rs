use std::path::{Path, PathBuf};
use worldrunner::compat_content::{
    AdmittedTranslatedContent, ContentReceipt, GameSourceQualification, ImportedScene,
    MaterializationPlan, SelectionPolicy, TranslatedAdmissionProof, TranslatedContentReceipt,
    TrustedProviderInventory,
};

const MAX_CAPTURED_FILES: usize = 4096;
const MAX_CAPTURED_BYTES: usize = 128 * 1024 * 1024;

pub(crate) struct CurrentInputs<'a> {
    root: &'a Path,
    policy: &'a SelectionPolicy,
    inventory: &'a TrustedProviderInventory,
    source: &'a GameSourceQualification,
}
impl<'a> CurrentInputs<'a> {
    pub(crate) fn new(
        root: &'a Path,
        policy: &'a SelectionPolicy,
        inventory: &'a TrustedProviderInventory,
        source: &'a GameSourceQualification,
    ) -> Self {
        Self {
            root,
            policy,
            inventory,
            source,
        }
    }
}

pub(crate) struct Boundary<'a> {
    admitted: AdmittedTranslatedContent<'a>,
    root: PathBuf,
    inventory: TrustedProviderInventory,
    source: GameSourceQualification,
}
impl<'a> Boundary<'a> {
    pub(crate) fn prepare(
        root: &Path,
        receipt: &'a TranslatedContentReceipt<'a>,
        inventory: TrustedProviderInventory,
        source: GameSourceQualification,
    ) -> Result<Self, String> {
        bounded_capture(receipt.original())?;
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let admitted = receipt.admit(&root, &inventory, &source)?;
        Ok(Self {
            admitted,
            root,
            inventory,
            source,
        })
    }
    pub(crate) fn proof(&self) -> &TranslatedAdmissionProof {
        self.admitted.proof()
    }
    pub(crate) fn selection_policy(&self) -> &SelectionPolicy {
        &self.admitted.proof().selection_policy
    }
    pub(crate) fn execution_graph(&self) -> &ImportedScene {
        self.admitted.execution_graph()
    }
    pub(crate) fn current<'b>(&'b self, policy: &'b SelectionPolicy) -> CurrentInputs<'b> {
        CurrentInputs::new(&self.root, policy, &self.inventory, &self.source)
    }
    pub(crate) fn with_verified_plan<T>(
        &self,
        current: &CurrentInputs<'_>,
        operation: impl FnOnce(&MaterializationPlan) -> Result<T, String>,
    ) -> Result<T, String> {
        let root = current.root.canonicalize().map_err(|e| e.to_string())?;
        if root != self.root {
            return Err("prepared project root changed".into());
        }
        self.admitted
            .verify_inputs(&root, current.policy, current.inventory, current.source)?;
        operation(self.admitted.materialization_plan())
    }
}

fn bounded_capture(receipt: &ContentReceipt) -> Result<(), String> {
    let total = receipt
        .sources()
        .values()
        .try_fold(0usize, |sum, bytes| sum.checked_add(bytes.len()));
    if receipt.sources().len() > MAX_CAPTURED_FILES
        || total.is_none_or(|bytes| bytes > MAX_CAPTURED_BYTES)
    {
        return Err("trusted preparation exceeds captured-content budget".into());
    }
    Ok(())
}
