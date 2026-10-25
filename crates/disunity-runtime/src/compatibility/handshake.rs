use serde_json::{Value, json};

use crate::HostError;

pub(super) struct Negotiation {
    pub(super) native: Value,
    bridge: Value,
}

fn invalid(message: impl std::fmt::Display) -> HostError {
    HostError::Compatibility(format!("compatibility handshake: {message}"))
}

fn equal(value: &Value, key: &str, expected: Value) -> Result<(), HostError> {
    if value.get(key) != Some(&expected) {
        return Err(invalid(format!("incompatible {key}")));
    }
    Ok(())
}

fn contains(value: &Value, key: &str, expected: Value) -> Result<(), HostError> {
    if !value[key].as_array().is_some_and(|a| a.contains(&expected)) {
        return Err(invalid(format!("unsupported {key}: {expected}")));
    }
    Ok(())
}

pub(super) fn validate_native(native: &Value) -> Result<(), HostError> {
    common(native)?;
    equal(native, "native_abi_version", json!(2))?;
    contains(native, "native_protocol_versions", json!(1))?;
    contains(native, "bridge_revisions", json!(1))?;
    contains(native, "capabilities", json!("describe"))?;
    contains(native, "capabilities", json!("owned_responses"))?;
    limits(native, &["max_live_response_bytes", "max_responses"])
}

fn common(value: &Value) -> Result<(), HostError> {
    equal(value, "descriptor_version", json!(1))?;
    if usize::BITS != 64 {
        return Err(invalid("compatibility requires 64-bit pointers"));
    }
    equal(value, "pointer_width", json!(64))?;
    equal(value, "endianness", json!(endianness()))?;
    equal(value, "clock_policy", json!("float_accumulated"))?;
    contains(value, "manifest_schema_versions", json!(1))?;
    limits(value, &["max_request_bytes", "max_response_bytes"])
}

fn limits(value: &Value, names: &[&str]) -> Result<(), HostError> {
    for name in names {
        if !value["limits"][name].as_u64().is_some_and(|n| n > 0) {
            return Err(invalid(format!("invalid limit {name}")));
        }
    }
    Ok(())
}

fn endianness() -> &'static str {
    if cfg!(target_endian = "little") {
        "little"
    } else {
        "big"
    }
}

impl Negotiation {
    pub(super) fn new(native: Value, bridge: Value) -> Result<Self, HostError> {
        validate_native(&native)?;
        common(&bridge)?;
        equal(&bridge, "bridge_revision", json!(1))?;
        contains(&bridge, "native_abi_versions", json!(2))?;
        contains(&bridge, "native_protocol_versions", json!(1))?;
        contains(&bridge, "capabilities", json!("describe_bridge"))?;
        contains(&bridge, "capabilities", json!("fixture_manifest_v1"))?;
        Ok(Self { native, bridge })
    }

    pub(super) fn validate_world(&self, description: &Value) -> Result<(), HostError> {
        if description != &self.native {
            return Err(invalid("world descriptor differs from boot negotiation"));
        }
        Ok(())
    }

    pub(super) fn manifest(&self, fixture: &Value) -> Result<u64, HostError> {
        let fields = fixture
            .as_object()
            .ok_or_else(|| invalid("manifest must be an object"))?;
        for name in ["version", "schema_version"] {
            if fields.contains_key(name) {
                equal(fixture, name, json!(1))?;
            }
        }
        let world = fixture["world"]
            .as_u64()
            .filter(|w| *w != 0)
            .ok_or_else(|| invalid("manifest requires a nonzero host-owned world"))?;
        if let Some(required) = fields.get("requirements") {
            self.requirements(required)?;
        }
        super::terminal::required(fixture)?;
        Ok(world)
    }

    fn requirements(&self, required: &Value) -> Result<(), HostError> {
        let fields = required
            .as_object()
            .ok_or_else(|| invalid("requirements must be an object"))?;
        for (key, value) in fields {
            match key.as_str() {
                "native_abi_version" => equal(required, key, json!(2))?,
                "native_protocol_version" | "bridge_revision" | "manifest_schema_version" => {
                    equal(required, key, json!(1))?
                }
                "pointer_width" => equal(required, key, json!(64))?,
                "endianness" => equal(required, key, json!(endianness()))?,
                "clock_policy" => equal(required, key, json!("float_accumulated"))?,
                "execution_profile" => {
                    equal(required, key, json!(super::phase::PROFILE))?;
                    contains(
                        &self.native,
                        "capabilities",
                        json!("native_phase_context_native_v1"),
                    )?;
                    contains(&self.bridge, "capabilities", json!(super::phase::PROFILE))?;
                }
                "transport_profile" => {
                    equal(
                        required,
                        key,
                        json!(super::transport::byte_profile::PROFILE),
                    )?;
                    equal(required, "execution_profile", json!(super::phase::PROFILE))?;
                    contains(
                        &self.native,
                        "capabilities",
                        json!(super::transport::byte_profile::PROFILE),
                    )?;
                    contains(
                        &self.bridge,
                        "capabilities",
                        json!("managed_transport_profile_consumer_v1"),
                    )?;
                }
                "host_mutation_profile" => {
                    super::idle_mutation::validate(&self.native, &self.bridge, required)?;
                }
                "scheduler_profile" => {
                    super::readiness::validate(&self.native, &self.bridge, required)?;
                }
                "native_capabilities" => capabilities(value, &self.native)?,
                "bridge_capabilities" => capabilities(value, &self.bridge)?,
                _ => return Err(invalid(format!("unsupported requirement {key}"))),
            }
        }
        Ok(())
    }
}

fn capabilities(required: &Value, supported: &Value) -> Result<(), HostError> {
    for value in required
        .as_array()
        .ok_or_else(|| invalid("capabilities must be an array"))?
    {
        if !value.is_string() {
            return Err(invalid("capability must be a string"));
        }
        contains(supported, "capabilities", value.clone())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn negotiation() -> Negotiation {
        let common = json!({
            "descriptor_version": 1, "pointer_width": 64, "endianness": endianness(),
            "clock_policy": "float_accumulated", "manifest_schema_versions": [1],
            "limits": {"max_request_bytes": 1048576, "max_response_bytes": 16777216,
                "max_live_response_bytes": 67108864, "max_responses": 1024}
        });
        let mut native = common.clone();
        native["native_abi_version"] = json!(2);
        native["native_protocol_versions"] = json!([1]);
        native["bridge_revisions"] = json!([1]);
        native["capabilities"] = json!(["describe", "owned_responses"]);
        let mut bridge = common;
        bridge["bridge_revision"] = json!(1);
        bridge["native_abi_versions"] = json!([2]);
        bridge["native_protocol_versions"] = json!([1]);
        bridge["capabilities"] = json!(["describe_bridge", "fixture_manifest_v1"]);
        Negotiation::new(native, bridge).unwrap()
    }

    #[test]
    fn legacy_manifest_without_callback_requirements_still_negotiates() {
        let negotiation = negotiation();
        assert_eq!(negotiation.manifest(&json!({"world": 1})).unwrap(), 1);
        assert_eq!(
            negotiation
                .manifest(&json!({"world": 1, "requirements": {}}))
                .unwrap(),
            1
        );
    }

    #[test]
    fn unknown_profiles_and_requirements_reject() {
        let negotiation = negotiation();
        for requirements in [
            json!({"scheduler_profile": "unknown"}),
            json!({"unrecognized_requirement": true}),
        ] {
            assert!(
                negotiation
                    .manifest(&json!({"world": 1, "requirements": requirements}))
                    .is_err()
            );
        }
    }
}

#[cfg(test)]
mod idle_rename_tests {
    use super::*;
    fn paired() -> Negotiation {
        let common = json!({"descriptor_version":1,"pointer_width":64,"endianness":endianness(),"clock_policy":"float_accumulated","manifest_schema_versions":[1],"limits":{"max_request_bytes":1048576,"max_response_bytes":16777216,"max_live_response_bytes":67108864,"max_responses":1024}});
        let mut native = common.clone();
        native["native_abi_version"] = json!(2);
        native["native_protocol_versions"] = json!([1]);
        native["bridge_revisions"] = json!([1]);
        native["capabilities"] = json!([
            "describe",
            "owned_responses",
            "native_phase_context_native_v1",
            "native_host_idle_rename_v1"
        ]);
        let mut bridge = common;
        bridge["bridge_revision"] = json!(1);
        bridge["native_abi_versions"] = json!([2]);
        bridge["native_protocol_versions"] = json!([1]);
        bridge["capabilities"] = json!([
            "describe_bridge",
            "fixture_manifest_v1",
            "native_phase_context_v1",
            "managed_host_idle_rename_consumer_v1"
        ]);
        Negotiation::new(native, bridge).unwrap()
    }
    #[test]
    fn exact_paired_host_idle_requirement_negotiates() {
        assert_eq!(paired().manifest(&json!({"world":1,"requirements":{"execution_profile":"native_phase_context_v1","host_mutation_profile":"native_host_idle_rename_v1"}})).unwrap(), 1);
    }
    #[test]
    fn host_idle_requires_both_consumers_and_rejects_scheduler_composition() {
        let fixture = json!({"world":1,"requirements":{"execution_profile":"native_phase_context_v1","host_mutation_profile":"native_host_idle_rename_v1"}});
        let mut old = paired();
        old.bridge["capabilities"] = json!(["native_phase_context_v1"]);
        assert!(old.manifest(&fixture).is_err());
        let mut composed = fixture;
        composed["requirements"]["scheduler_profile"] = json!("native_readiness_single_v1");
        assert!(paired().manifest(&composed).is_err());
    }
}
