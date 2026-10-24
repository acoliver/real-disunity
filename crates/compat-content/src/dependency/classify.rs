use super::{
    CapabilityId, DependencyClassification, DependencyDisposition, DependencyPolicy,
    DependencyRule, Fidelity, FieldSegment, LocatedReference, OwnerIdentity, ReferenceDecision,
    ReferenceLocator, ReferenceResolution, ResourceKind, TrustedProviderInventory,
};

pub(super) fn decide(
    observation: &LocatedReference,
    policy: &DependencyPolicy,
    providers: &TrustedProviderInventory,
) -> ReferenceDecision {
    let mut decision = ReferenceDecision {
        observation: observation.clone(),
        classification: DependencyClassification::Blocked,
        rule_id: None,
        reviewed_disposition: None,
        reason: None,
        provider_binding: None,
        evidence: vec![],
        limitations: vec![],
    };
    if let Some(rule) = policy
        .rules
        .iter()
        .find(|r| r.locator == observation.locator)
    {
        decision.rule_id = Some(rule.id.clone());
        decision.reviewed_disposition = Some(rule.disposition.clone());
        decision.reason = Some(rule.reason.clone());
        decision.evidence.clone_from(&rule.evidence);
        if rule.source_sha256 != observation.source_sha256
            || Some(&rule.expected_reference) != observation.reference.as_ref()
            || rule.resource_kind != observation.resource_kind
        {
            decision
                .limitations
                .push("rule source/hash/reference/resource-kind precondition failed".into());
            return decision;
        }
        apply(&mut decision, rule, policy, providers);
    } else if let ReferenceResolution::ResolvedTarget { .. } = &observation.resolution {
        decision.classification = DependencyClassification::RequiredRuntime;
    }
    decision
}
fn apply(
    decision: &mut ReferenceDecision,
    rule: &DependencyRule,
    policy: &DependencyPolicy,
    providers: &TrustedProviderInventory,
) {
    if policy.fidelity == Fidelity::Original {
        decision
            .limitations
            .push("reviewed exceptions require explicit translated fidelity".into());
        return;
    }
    match &rule.disposition {
        DependencyDisposition::EditorConfiguration if editor(&decision.observation) => {
            decision.classification = DependencyClassification::EditorConfiguration;
        }
        DependencyDisposition::RenderSettingsTranslated { capability }
            if render(&decision.observation)
                && capability.id == CapabilityId::CanonicalUnlitQuads =>
        {
            if let Some(binding) = providers.capability(capability) {
                decision.classification = DependencyClassification::RenderSettingsTranslated;
                decision.provider_binding = Some(binding.clone());
            } else {
                decision
                    .limitations
                    .push("trusted unlit provider unavailable".into());
            }
        }
        DependencyDisposition::ImportToolNotExecuted { .. } => {
            decision.limitations.push("classification only: current source census, compiled assembly and bounded keyboard API profile proof required; importer is unavailable".into());
        }
        DependencyDisposition::ProvidedBuiltin {
            provider_id,
            resource_kind,
        } => {
            if *resource_kind == decision.observation.resource_kind
                && matches!(&decision.observation.resolution, ReferenceResolution::BuiltinMapping { provider_id: id } if id == provider_id)
                && let Some(binding) = providers.builtin(provider_id, *resource_kind)
            {
                decision.classification = DependencyClassification::ProvidedBuiltin;
                decision.provider_binding = Some(binding.clone());
            }
        }
        DependencyDisposition::Blocked { code } => decision.limitations.push(code.clone()),
        _ => decision
            .limitations
            .push("disposition is invalid for this original owner/use site".into()),
    }
}
fn editor(observation: &LocatedReference) -> bool {
    let locator = &observation.locator;
    let Some(reference) = &observation.reference else {
        return false;
    };
    let allowed_identity = match locator.source_path.as_str() {
        "ProjectSettings/PackageManagerSettings.asset" => reference.file_id == 13964,
        "ProjectSettings/Packages/com.unity.dedicated-server/MultiplayerRolesSettings.asset" => {
            reference.file_id == 15023
        }
        _ => false,
    };
    allowed_identity
        && reference.guid == "0000000000000000e000000000000000"
        && reference.reference_type == Some(0)
        && matches!(&locator.owner, OwnerIdentity::SettingsDocument { document_index: 0, class_id: 114, file_id: 1, kind } if kind == "MonoBehaviour")
        && locator.field == [FieldSegment::Key("m_Script".into())]
        && observation.resource_kind == ResourceKind::Script
}
fn render(observation: &LocatedReference) -> bool {
    observation.locator.source_path == "ProjectSettings/GraphicsSettings.asset"
        && matches!(&observation.locator.owner, OwnerIdentity::SettingsDocument { document_index: 0, class_id: 30, file_id: 1, kind } if kind == "GraphicsSettings")
        && observation.resource_kind == ResourceKind::Shader
}
pub(super) fn metadata_field(locator: &ReferenceLocator) -> Option<(String, String)> {
    let mut field = match locator.owner {
        OwnerIdentity::SettingsDocument { document_index, .. } => {
            format!("documents[{document_index}]")
        }
        OwnerIdentity::AssetMetadata { .. } => String::new(),
        OwnerIdentity::AssetDocument { .. } | OwnerIdentity::ExpandedDocument { .. } => {
            return None;
        }
    };
    for segment in &locator.field {
        match segment {
            FieldSegment::Key(key) => {
                if !field.is_empty() {
                    field.push('.');
                }
                field.push_str(key);
            }
            FieldSegment::Index(index) => {
                use std::fmt::Write;
                write!(field, "[{index}]").expect("string write");
            }
        }
    }
    Some((locator.source_path.clone(), field))
}
