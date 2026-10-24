use super::{Deserialize, ResourceKind, Serialize, hash, identity};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CapabilityId {
    CanonicalUnlitQuads,
    CanonicalKeyboardSnapshot,
    CanonicalUnlitMaterial,
    CanonicalUnlitShader,
    AuthoredSprite,
    MappedFont,
    MappedTileType,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequirement {
    pub id: CapabilityId,
    pub contract_version: u32,
    pub resource_kind: ResourceKind,
}
impl CapabilityRequirement {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let expected = match self.id {
            CapabilityId::CanonicalUnlitQuads => ResourceKind::RenderSettings,
            CapabilityId::CanonicalKeyboardSnapshot => ResourceKind::Input,
            CapabilityId::CanonicalUnlitMaterial => ResourceKind::Material,
            CapabilityId::CanonicalUnlitShader => ResourceKind::Shader,
            CapabilityId::AuthoredSprite => ResourceKind::Texture,
            CapabilityId::MappedFont => ResourceKind::Font,
            CapabilityId::MappedTileType => ResourceKind::Script,
        };
        if self.contract_version != 1 || self.resource_kind != expected {
            return Err("unsupported capability version or resource kind".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBinding {
    pub id: String,
    pub resource_kind: ResourceKind,
    pub capabilities: Vec<CapabilityRequirement>,
    pub implementation_sha256: String,
    pub contract_evidence_sha256: String,
}
/// Implemented by trusted host composition after provider behavior qualification.
/// Fixture/profile JSON cannot implement this boundary. Bytes identify the actual
/// implementation and retained contract-test evidence, rather than asserted hashes.
pub trait TrustedProvider {
    fn id(&self) -> &str;
    fn resource_kind(&self) -> ResourceKind;
    fn capabilities(&self) -> Vec<CapabilityRequirement>;
    fn implementation_bytes(&self) -> &[u8];
    fn contract_evidence_bytes(&self) -> &[u8];
    /// Exact resources covered by this provider's retained qualification evidence.
    fn resource_bytes(&self) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::new()
    }
}
#[derive(Clone, Debug, Default)]
pub struct TrustedProviderInventory {
    bindings: BTreeMap<String, ProviderBinding>,
    resources: BTreeMap<String, BTreeMap<String, Vec<u8>>>,
}
impl TrustedProviderInventory {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, provider: &dyn TrustedProvider) -> Result<(), String> {
        if provider.resource_kind() == ResourceKind::Unknown {
            return Err("generic Unknown cannot be a trusted provider kind".into());
        }
        if provider.id().trim().is_empty()
            || provider.implementation_bytes().is_empty()
            || provider.contract_evidence_bytes().is_empty()
        {
            return Err(
                "provider requires identity, actual implementation and contract evidence".into(),
            );
        }
        if self.bindings.contains_key(provider.id()) {
            return Err("duplicate provider ID".into());
        }
        let resources = provider.resource_bytes();
        if resources
            .iter()
            .any(|(id, bytes)| id.trim().is_empty() || bytes.is_empty())
        {
            return Err("qualified resource requires exact identity and bytes".into());
        }
        let capabilities = provider.capabilities();
        for capability in &capabilities {
            capability.validate()?;
            if capability.resource_kind != provider.resource_kind() {
                return Err("provider/capability resource kind mismatch".into());
            }
        }
        let binding = ProviderBinding {
            id: provider.id().into(),
            resource_kind: provider.resource_kind(),
            capabilities,
            implementation_sha256: hash(provider.implementation_bytes()),
            contract_evidence_sha256: hash(provider.contract_evidence_bytes()),
        };
        if !resources.is_empty() {
            self.resources.insert(binding.id.clone(), resources);
        }
        self.bindings.insert(binding.id.clone(), binding);
        Ok(())
    }
    pub fn sha256(&self) -> String {
        if self.resources.is_empty() {
            identity(&self.bindings)
        } else {
            identity(&(&self.bindings, &self.resources))
        }
    }
    pub fn resource_bytes(&self, provider_id: &str, resource_id: &str) -> Option<&[u8]> {
        self.resources
            .get(provider_id)?
            .get(resource_id)
            .map(Vec::as_slice)
    }
    pub fn bindings(&self) -> &BTreeMap<String, ProviderBinding> {
        &self.bindings
    }
    pub(crate) fn capability(&self, required: &CapabilityRequirement) -> Option<&ProviderBinding> {
        self.bindings
            .values()
            .find(|b| b.capabilities.contains(required))
    }
    pub(crate) fn builtin(&self, id: &str, kind: ResourceKind) -> Option<&ProviderBinding> {
        self.bindings
            .get(id)
            .filter(|b| b.resource_kind == kind && kind != ResourceKind::Unknown)
    }
}
