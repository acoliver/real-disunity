use super::binding::Binding;
use serde::Deserialize;
use std::collections::BTreeMap;
use worldrunner::compat_content::{
    CapabilityId, CapabilityRequirement, FallbackSprite, ResourceKind, SelectedResource,
    TrustedProvider,
};
use worldrunner::scene::gpu::{dynamic::DynamicQuad, shader::ENVIRONMENT_WGSL};

pub(super) const WHITE: &str = "canonical.unlit.material.white.v1";
pub(super) const GREEN: &str = "canonical.unlit.material.green-half.v1";
pub(super) const SHADER: &str = "canonical.unlit.shader.v1";
pub(super) const SPRITE: &str = "fixture.authored-snake-unit-square";
const WHITE_BYTES: &[u8] = include_bytes!("white.json");
const GREEN_BYTES: &[u8] = include_bytes!("green.json");
const SPRITE_BYTES: &[u8] = include_bytes!("sprite.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Material {
    shader: String,
    tint: [f32; 4],
    blend: String,
    depth_write: bool,
    depth_compare: String,
    sorting_layer: i32,
    sorting_order: i32,
}
pub(super) fn apply(bytes: &[u8], quads: &mut [DynamicQuad]) -> Result<(), String> {
    let material: Material = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if material.shader != SHADER
        || material.blend != "straight_alpha"
        || material.depth_write
        || material.depth_compare != "less_equal"
        || material
            .tint
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err("unsupported canonical material semantics".into());
    }
    for quad in quads {
        for (channel, tint) in quad.tint.iter_mut().zip(material.tint) {
            *channel *= tint;
        }
        quad.sorting_layer = material.sorting_layer;
        quad.sorting_order = material.sorting_order;
    }
    Ok(())
}
pub(super) fn material(id: &str) -> Result<&'static [u8], String> {
    match id {
        WHITE => Ok(WHITE_BYTES),
        GREEN => Ok(GREEN_BYTES),
        _ => Err("material resource is outside the qualified exact catalog".into()),
    }
}
pub(super) fn sprite() -> Result<FallbackSprite, String> {
    serde_json::from_slice(SPRITE_BYTES).map_err(|e| e.to_string())
}
struct CatalogEntry {
    id: &'static str,
    kind: ResourceKind,
    capability: CapabilityId,
    bytes: Vec<(&'static str, &'static [u8])>,
}
fn resources() -> Vec<CatalogEntry> {
    vec![
        CatalogEntry {
            id: "host.resource-unlit-material.v1",
            kind: ResourceKind::Material,
            capability: CapabilityId::CanonicalUnlitMaterial,
            bytes: vec![(WHITE, WHITE_BYTES), (GREEN, GREEN_BYTES)],
        },
        CatalogEntry {
            id: "host.resource-unlit-shader.v1",
            kind: ResourceKind::Shader,
            capability: CapabilityId::CanonicalUnlitShader,
            bytes: vec![(SHADER, ENVIRONMENT_WGSL.as_bytes())],
        },
        CatalogEntry {
            id: "host.resource-authored-snake.v1",
            kind: ResourceKind::Texture,
            capability: CapabilityId::AuthoredSprite,
            bytes: vec![(SPRITE, SPRITE_BYTES)],
        },
    ]
}
pub(super) struct Provider {
    id: String,
    kind: ResourceKind,
    capability: CapabilityId,
    implementation: Vec<u8>,
    evidence: Vec<u8>,
    bytes: BTreeMap<String, Vec<u8>>,
}
pub(super) fn providers(binding: &Binding, evidence: &[u8]) -> Vec<Provider> {
    resources()
        .into_iter()
        .map(|entry| Provider {
            id: entry.id.into(),
            kind: entry.kind,
            capability: entry.capability,
            implementation: binding.implementation().to_vec(),
            evidence: evidence.to_vec(),
            bytes: entry
                .bytes
                .into_iter()
                .map(|(id, bytes)| (id.into(), bytes.to_vec()))
                .collect(),
        })
        .collect()
}
pub(super) fn selected() -> Vec<SelectedResource> {
    serde_json::from_slice(include_bytes!("resources.json"))
        .expect("compiled exact resource descriptor table")
}
pub(super) fn verify(
    providers: &[Provider],
    binding: &Binding,
    evidence: &[u8],
) -> Result<(), String> {
    let expected = self::providers(binding, evidence);
    if providers.len() != expected.len()
        || providers.iter().zip(expected).any(|(actual, expected)| {
            actual.id != expected.id
                || actual.kind != expected.kind
                || actual.capability != expected.capability
                || actual.bytes != expected.bytes
                || actual.implementation != expected.implementation
                || actual.evidence != expected.evidence
        })
    {
        return Err("resource provider implementation/evidence/resource tuple changed".into());
    }
    Ok(())
}
impl TrustedProvider for Provider {
    fn id(&self) -> &str {
        &self.id
    }
    fn resource_kind(&self) -> ResourceKind {
        self.kind
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        vec![CapabilityRequirement {
            id: self.capability,
            contract_version: 1,
            resource_kind: self.kind,
        }]
    }
    fn implementation_bytes(&self) -> &[u8] {
        &self.implementation
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        &self.evidence
    }
    fn resource_bytes(&self) -> BTreeMap<String, Vec<u8>> {
        self.bytes.clone()
    }
}

#[cfg(test)]
pub(super) fn corrupt(provider: &mut Provider, control: &str) {
    match control {
        "kind" => provider.kind = ResourceKind::Shader,
        "evidence" => provider.evidence.push(0),
        "implementation" => provider.implementation.push(0),
        "resource-id" => {
            provider.bytes.insert("unqualified".into(), vec![1]);
        }
        "bytes" => {
            provider.bytes.values_mut().next().unwrap().push(0);
        }
        _ => panic!("unknown negative control"),
    }
}
