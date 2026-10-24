//! Finite executable source contract for the trusted activation counter fixture.
use crate::image_lease::SealedImage;
use std::{collections::BTreeMap, path::Path, sync::OnceLock};
use worldrunner::compat_content::{
    ApiSurface, AssemblyApiUse, CapabilityId, CapabilityRequirement, ContentReceipt,
    GameSourceQualification, ImportProfile, ResourceKind, ScriptSource, TrustedGameSourceProof,
    TrustedProvider, script_census,
};

pub const COUNTER_PROFILE: &str = "trusted-counter-v1";
const SOURCE: &str = "public sealed class TrustedCounter : UnityEngine.MonoBehaviour\n{\n    public int Updates;\n    void Update() { Updates++; }\n}\n";
const PE: &str = "2bb2c6f56acfaff71172de0fa8c0e4b258c63a9cd2405f44104437bd18ac7d25";
const PDB: &str = "7d9a9144d20a0495c7d5e9829519cb20f765a05caec77bdcfa4b33573a1300e9";
const IMAGE: &str = ".work/resumed-finish-trusted/managed/bin/Counter/debug/TrustedCounter.dll";

static COUNTER_IMAGES: OnceLock<CounterImages> = OnceLock::new();

struct CounterImages {
    image: SealedImage,
    _pdb: SealedImage,
}

pub(crate) struct CounterProgram {
    images: &'static CounterImages,
    qualification: GameSourceQualification,
    assembly: Vec<u8>,
}
impl CounterProgram {
    pub(crate) fn capture(
        project: &Path,
        receipt: &ContentReceipt,
        profile: &str,
    ) -> Result<Self, String> {
        if profile != COUNTER_PROFILE {
            return Err("counter source profile must be trusted-counter-v1".into());
        }
        let sources = script_census(
            project,
            Path::new("Assets/Scripts"),
            &ImportProfile::default(),
        )
        .map_err(|e| e.to_string())?;
        if sources.len() != 1
            || sources[0].path != "Assets/Scripts/TrustedCounter.cs"
            || receipt.sources().get(&sources[0].path).map(Vec::as_slice) != Some(SOURCE.as_bytes())
        {
            return Err("counter requires its exact complete source census".into());
        }
        if !receipt.graph().components.iter().any(|c| {
            c.script
                .as_ref()
                .is_some_and(|s| s.type_name == "TrustedCounter" && s.path == sources[0].path)
        }) {
            return Err(
                "counter receipt must bind the scene script to TrustedCounter source".into(),
            );
        }
        let workspace = project
            .ancestors()
            .filter(|root| root.join(IMAGE).is_file())
            .last()
            .ok_or("counter PE is outside its workspace")?;
        let actual = compat_kit::files::hash(&workspace.join(IMAGE))?;
        if actual != PE {
            return Err(format!(
                "counter PE source digest differs: {actual} at {}",
                workspace.display()
            ));
        }
        let image = SealedImage::capture(workspace, Path::new(IMAGE), PE, 65536)
            .map_err(|error| format!("counter PE: {error}"))?;
        let pdb = SealedImage::capture(
            workspace,
            &Path::new(IMAGE).with_extension("pdb"),
            PDB,
            65536,
        )
        .map_err(|error| format!("counter PDB: {error}"))?;
        let bytes = std::fs::read(image.load_path().as_path()).map_err(|e| e.to_string())?;
        let proof = CounterProof {
            closure: receipt.closure_sha256().into(),
            sources,
            bytes,
        };
        let qualification = GameSourceQualification::from_host(&proof)?;
        // Collectible CLR image teardown is asynchronous; pin this fixed pair once per process.
        let images = COUNTER_IMAGES.get_or_init(|| CounterImages { image, _pdb: pdb });
        Ok(Self {
            images,
            qualification,
            assembly: proof.bytes,
        })
    }
    pub(crate) fn qualification(&self) -> GameSourceQualification {
        self.qualification.clone()
    }
    pub(crate) fn load_path(&self) -> crate::image_lease::ImageLoadPath<'_> {
        self.images.image.load_path()
    }
}
struct CounterProof {
    closure: String,
    sources: Vec<ScriptSource>,
    bytes: Vec<u8>,
}
impl TrustedGameSourceProof for CounterProof {
    fn profile_id(&self) -> &str {
        COUNTER_PROFILE
    }
    fn content_closure_sha256(&self) -> &str {
        &self.closure
    }
    fn source_directory(&self) -> &Path {
        Path::new("Assets/Scripts")
    }
    fn sources(&self) -> Vec<ScriptSource> {
        self.sources.clone()
    }
    fn assembly_bytes(&self) -> &[u8] {
        &self.bytes
    }
    fn verification_evidence_bytes(&self) -> &[u8] {
        b"trusted-counter-v1: exact source/PE/PDB; sealed MonoBehaviour constructor and Update integer increment contract; no resources, delegates, generics or external game calls"
    }
    fn defined_types(&self) -> BTreeMap<String, String> {
        BTreeMap::from([(
            "TrustedCounter".into(),
            "Assets/Scripts/TrustedCounter.cs".into(),
        )])
    }
    fn api_uses(&self) -> Vec<AssemblyApiUse> {
        vec![AssemblyApiUse {
            assembly: "UnityCompat".into(),
            type_name: "UnityEngine.MonoBehaviour".into(),
            member: ".ctor".into(),
            surface: ApiSurface::CanonicalFacade,
        }]
    }
}

impl TrustedProvider for CounterProgram {
    fn id(&self) -> &'static str {
        "trusted.counter-script-map.v1"
    }
    fn resource_kind(&self) -> ResourceKind {
        ResourceKind::Script
    }
    fn capabilities(&self) -> Vec<CapabilityRequirement> {
        vec![CapabilityRequirement {
            id: CapabilityId::MappedTileType,
            contract_version: 1,
            resource_kind: ResourceKind::Script,
        }]
    }
    fn implementation_bytes(&self) -> &[u8] {
        &self.assembly
    }
    fn contract_evidence_bytes(&self) -> &[u8] {
        b"trusted-counter-v1 maps only TrustedCounter to its exact receipt-bound source"
    }
}
