//! Explicit opt-in translated-content preparation over a qualified managed host.
//!
//! Original-game activation remains fail-closed while its source, provider, loader,
//! and lifecycle contracts remain unresolved. The counter API is a finite fixture
//! contract and does not grant original-game, input, rendering, or resource access.

#[path = "trusted/counter.rs"]
mod counter;
#[path = "trusted/protocol.rs"]
pub(crate) mod protocol;

use crate::providers::{GameSource, QualifiedProviders};
use counter::CounterProgram;
use std::{convert::Infallible, fmt, path::Path};
use worldrunner::{
    compat_content::{
        CapabilityId, CapabilityRequirement, ResourceKind, SelectionPolicy,
        TranslatedAdmissionProof, TranslatedContentReceipt, TrustedProviderInventory,
    },
    managed::CompatibilityHost,
};

/// A borrowed receipt and exclusive borrow of the exact qualified host.
/// There is deliberately no JSON constructor, `Clone`, or ready-token conversion.
pub struct Prepared<'a> {
    boundary: protocol::Boundary<'a>,
    _providers: &'a mut QualifiedProviders,
}

/// Prepare through host-owned provider registration and the actual Q2 attestor.
pub fn prepare<'a>(
    project_root: &Path,
    receipt: &'a TranslatedContentReceipt<'a>,
    providers: &'a mut QualifiedProviders,
    game: GameSource,
) -> Result<Prepared<'a>, String> {
    let mut inventory = TrustedProviderInventory::new();
    providers.register(&mut inventory)?;
    let source = providers.game_source(
        game,
        receipt.original(),
        &receipt.selection_policy().profile_id,
    )?;
    let boundary = protocol::Boundary::prepare(project_root, receipt, inventory, source)?;
    Ok(Prepared {
        boundary,
        _providers: providers,
    })
}

impl Prepared<'_> {
    /// Diagnostic evidence only. Serializing this proof does not preserve readiness.
    pub fn proof(&self) -> &TranslatedAdmissionProof {
        self.boundary.proof()
    }

    /// Original-game activation remains unavailable until all actual contracts qualify.
    pub fn load(self, current_policy: &SelectionPolicy) -> Result<Infallible, LoadError> {
        load_boundary(&self.boundary, current_policy)
    }
}

/// Prepare the exact counter fixture against a retained opaque translated receipt.
/// The source-bound counter proof is constructed by the host, never supplied by JSON.
pub fn prepare_counter<'a>(
    project_root: &Path,
    receipt: &'a TranslatedContentReceipt<'a>,
    host: &'a mut CompatibilityHost,
) -> Result<CounterPrepared<'a>, String> {
    validate_counter_scope(receipt)?;
    require_idle_counter_host(host)?;
    let program = CounterProgram::capture(
        project_root,
        receipt.original(),
        &receipt.selection_policy().profile_id,
    )?;
    let source = program.qualification();
    let mut inventory = TrustedProviderInventory::new();
    inventory.register(&program)?;
    let boundary = protocol::Boundary::prepare(project_root, receipt, inventory, source)?;
    Ok(CounterPrepared {
        boundary,
        program: Some(program),
        host,
    })
}

fn require_idle_counter_host(host: &CompatibilityHost) -> Result<(), String> {
    let statistics = host
        .observe(&serde_json::json!({"op":"managed.statistics"}))
        .map_err(|error| error.to_string())?;
    for (field, expected) in [
        ("loaded", serde_json::json!(false)),
        ("cleanup_pending", serde_json::json!(false)),
        ("scripts", serde_json::json!(0)),
        ("coroutines", serde_json::json!(0)),
        ("bindings", serde_json::json!(0)),
        ("world", serde_json::Value::Null),
    ] {
        if statistics["data"][field] != expected {
            return Err(format!("counter requires an idle managed host: {field}"));
        }
    }
    Ok(())
}

fn validate_counter_scope(receipt: &TranslatedContentReceipt<'_>) -> Result<(), String> {
    let policy = receipt.selection_policy();
    let capability = |value: &CapabilityRequirement| {
        value.id == CapabilityId::MappedTileType
            && value.resource_kind == ResourceKind::Script
            && value.contract_version == 1
    };
    if policy.profile_id != counter::COUNTER_PROFILE
        || policy.keyboard_only
        || !policy.resources.is_empty()
        || policy.required_capabilities.is_empty()
        || policy
            .required_capabilities
            .iter()
            .any(|value| !capability(value))
        || policy.operations.iter().any(|operation| {
            operation
                .effect()
                .capability
                .as_ref()
                .is_none_or(|value| !capability(value))
        })
    {
        return Err("counter admission permits only its source-mapped script capability".into());
    }
    Ok(())
}

/// A verified finite plan that can be loaded only on the host used at preparation.
pub struct CounterPrepared<'a> {
    boundary: protocol::Boundary<'a>,
    program: Option<CounterProgram>,
    host: &'a mut CompatibilityHost,
}
impl<'a> CounterPrepared<'a> {
    pub fn proof(&self) -> &TranslatedAdmissionProof {
        self.boundary.proof()
    }

    /// Materialize once, load the admitted counter PE, and roll back the world on failure.
    pub fn load(mut self) -> Result<CounterLoaded<'a>, LoadError> {
        require_idle_counter_host(self.host).map_err(LoadError::Runtime)?;
        let world = self.host.create_world().map_err(runtime_error)?;
        match self.execute_plan(world) {
            Ok((scene, script_id, script_handle)) => Ok(CounterLoaded {
                host: self.host,
                world,
                script_id,
                script_handle,
                scene,
                program: self.program.take(),
                cleanup_attempted: false,
            }),
            Err(error) => {
                let unload = self.host.unload().map_err(|failure| failure.to_string());
                let destroy = if unload.is_ok() {
                    self.host
                        .destroy_world(world)
                        .map_err(|failure| failure.to_string())
                } else {
                    Err("world retained because managed rollback failed".into())
                };
                Err(LoadError::Runtime(format!(
                    "{error}; rollback unload={unload:?}, destroy={destroy:?}"
                )))
            }
        }
    }

    fn execute_plan(
        &mut self,
        world: u64,
    ) -> Result<(worldrunner::compat_content::MaterializedScene, String, u64), String> {
        let policy = self.boundary.selection_policy();
        let program = self.program.as_ref().expect("prepared counter program");
        self.boundary
            .with_verified_plan(&self.boundary.current(policy), |plan| {
                let scene = plan
                    .execute(|mut request| {
                        request["version"] = serde_json::json!(1);
                        self.host
                            .request(world, &request)
                            .map_err(|e| e.to_string())
                    })
                    .map_err(|e| e.to_string())?;
                load_counter_fixture(self.host, &self.boundary, program, world, scene)
            })
    }
}

fn load_counter_fixture(
    host: &mut CompatibilityHost,
    boundary: &protocol::Boundary<'_>,
    program: &CounterProgram,
    world: u64,
    scene: worldrunner::compat_content::MaterializedScene,
) -> Result<(worldrunner::compat_content::MaterializedScene, String, u64), String> {
    let script = boundary
        .execution_graph()
        .components
        .iter()
        .find(|component| {
            component
                .script
                .as_ref()
                .is_some_and(|script| script.type_name == "TrustedCounter")
        })
        .ok_or("admitted graph has no TrustedCounter script")?;
    let handle = scene
        .component_ids
        .get(&script.id)
        .copied()
        .ok_or("counter component was not materialized")?;
    let manifest = serde_json::json!({
        "world": world,
        "assemblies": [program.load_path().as_path()],
        "graph": boundary.execution_graph(),
        "handles": scene.handles,
    });
    host.load_fixture(&manifest).map_err(|e| e.to_string())?;
    Ok((scene, script.id.clone(), handle))
}

fn runtime_error(error: impl fmt::Display) -> LoadError {
    LoadError::Runtime(error.to_string())
}

/// Loaded counter world; update and unload are bound to this same host and world.
pub struct CounterLoaded<'a> {
    host: &'a mut CompatibilityHost,
    world: u64,
    script_id: String,
    script_handle: u64,
    scene: worldrunner::compat_content::MaterializedScene,
    program: Option<CounterProgram>,
    cleanup_attempted: bool,
}
impl CounterLoaded<'_> {
    pub fn world(&self) -> u64 {
        self.world
    }
    pub fn script_handle(&self) -> u64 {
        self.script_handle
    }
    pub fn materialized(&self) -> &worldrunner::compat_content::MaterializedScene {
        &self.scene
    }

    /// Dispatch exactly one actual managed Update callback on the retained world.
    pub fn update(&mut self) -> Result<u64, LoadError> {
        self.host
            .dispatch_phase(&serde_json::json!({"phase":"Update"}))
            .map_err(|e| LoadError::Runtime(e.to_string()))?;
        let value = self.host.observe(&serde_json::json!({
            "op":"managed.public", "type":"TrustedCounter", "id":self.script_id, "member":"Updates"
        })).map_err(|e| LoadError::Runtime(e.to_string()))?;
        value["data"].as_u64().ok_or_else(|| {
            LoadError::Runtime("counter Updates field is not an unsigned integer".into())
        })
    }

    /// Unload callbacks and destroy the admitted world. The fixed image pair stays pinned.
    /// Cleanup errors are returned without an implicit second callback attempt in Drop.
    pub fn unload(mut self) -> Result<(), LoadError> {
        self.cleanup_once()
    }

    fn cleanup_once(&mut self) -> Result<(), LoadError> {
        self.cleanup_attempted = true;
        self.host.unload().map_err(runtime_error)?;
        self.host.destroy_world(self.world).map_err(runtime_error)?;
        drop(self.program.take());
        Ok(())
    }
}
impl Drop for CounterLoaded<'_> {
    fn drop(&mut self) {
        if !self.cleanup_attempted {
            let _ = self.cleanup_once();
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivationPrerequisite {
    ReleasedQualifiedProviderTuple,
    ImmutableProjectProviderAndGameImageLeases,
    CompleteTransitiveGameApiSignatureProof,
    LoadedGameImageIdentityAndLifecycleCleanup,
}
const REQUIRED: [ActivationPrerequisite; 4] = [
    ActivationPrerequisite::ReleasedQualifiedProviderTuple,
    ActivationPrerequisite::ImmutableProjectProviderAndGameImageLeases,
    ActivationPrerequisite::CompleteTransitiveGameApiSignatureProof,
    ActivationPrerequisite::LoadedGameImageIdentityAndLifecycleCleanup,
];

#[derive(Debug, Eq, PartialEq)]
pub enum LoadError {
    InputsChanged(String),
    Runtime(String),
    ActivationUnavailable(&'static [ActivationPrerequisite]),
}
impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputsChanged(reason) => write!(f, "trusted load input revalidation: {reason}"),
            Self::Runtime(reason) => write!(f, "trusted managed counter runtime: {reason}"),
            Self::ActivationUnavailable(required) => write!(
                f,
                "trusted native activation is unavailable; required contracts: {required:?}"
            ),
        }
    }
}
impl std::error::Error for LoadError {}

pub(crate) fn load_boundary(
    boundary: &protocol::Boundary<'_>,
    current_policy: &SelectionPolicy,
) -> Result<Infallible, LoadError> {
    boundary
        .with_verified_plan(&boundary.current(current_policy), |_| Ok(()))
        .map_err(LoadError::InputsChanged)?;
    Err(LoadError::ActivationUnavailable(&REQUIRED))
}
