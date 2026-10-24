use std::path::Path;
use worldrunner::{
    compat_content::{
        CapabilityId, CapabilityRequirement, ResourceKind, TrustedProvider,
        TrustedProviderInventory,
    },
    scene::gpu::device::GpuContext,
};

pub fn render_failures(
    root: &Path,
    context: &GpuContext,
    qualified: &mut super::providers::QualifiedProviders,
) {
    let mut errors = serde_json::Map::new();
    for (name, control) in [
        (
            "disconnected",
            super::providers::RenderControl::Disconnected,
        ),
        ("wrong_stage", super::providers::RenderControl::WrongStage),
    ] {
        let binding = super::providers::binding::Binding::load(root).unwrap();
        let Err(error) =
            super::providers::render::qualify(qualified.host_mut(), &binding, context, control)
        else {
            panic!("{name} incorrectly qualified");
        };
        let expected = if name == "wrong_stage" {
            "exactly one enabled final output stage"
        } else {
            "sprite rect/pivot/PPU pixels"
        };
        assert!(error.contains(expected), "{name}: {error}");
        errors.insert(name.into(), error.into());
    }
    std::fs::write(
        root.join(".work/resumed-provider-qualification-v2/render-negative-controls.json"),
        serde_json::to_vec_pretty(&errors).unwrap(),
    )
    .unwrap();
}

struct WrongKind<'a> {
    implementation: &'a [u8],
    evidence: &'a [u8],
}
impl TrustedProvider for WrongKind<'_> {
    fn id(&self) -> &'static str {
        "wrong-kind-actual-unlit-evidence"
    }
    fn resource_kind(&self) -> ResourceKind {
        ResourceKind::Material
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        vec![CapabilityRequirement {
            id: CapabilityId::CanonicalUnlitQuads,
            contract_version: 1,
            resource_kind: ResourceKind::Material,
        }]
    }
    fn implementation_bytes(&self) -> &[u8] {
        self.implementation
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        self.evidence
    }
}
pub fn wrong_kind(root: &Path, qualified: &super::providers::QualifiedProviders) {
    let implementation =
        std::fs::read(root.join(".work/resumed-provider-qualification-v2/provider.so")).unwrap();
    let mut inventory = TrustedProviderInventory::new();
    let error = inventory
        .register(&WrongKind {
            implementation: &implementation,
            evidence: qualified.evidence_bytes(),
        })
        .unwrap_err();
    assert!(error.contains("resource kind"), "{error}");
    assert!(inventory.bindings().is_empty());
}
