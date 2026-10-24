use super::{
    extraction::{Extraction, Stamp, hash},
    native::{Fixture, Session},
    view::{POLICY, Target},
};
use serde_json::{Value, json};
use worldrunner::scene::{camera::ResolvedView, gpu::wgpu};

pub struct ViewReceipt {
    pub stamp: Stamp,
    pub raw_snapshot: Vec<u8>,
    pub managed: Value,
    pub resolved_view: ResolvedView,
    pub target: Target,
    pub captured_format: wgpu::TextureFormat,
    pub texture_revision: u64,
    pub gpu_completion_hash: String,
    pub provenance: Value,
    pub pixels: Vec<u8>,
    pub retained_texture: wgpu::Texture,
}

impl ViewReceipt {
    pub fn bind_current(
        &self,
        fixture: &Fixture,
        session: &Session,
        source: &Extraction,
        target: &Target,
    ) -> Result<(), String> {
        source.validate(fixture, session, true)?;
        self.join(source, target)
    }

    pub fn replay_historical(
        &self,
        fixture: &Fixture,
        session: &Session,
        source: &Extraction,
        target: &Target,
    ) -> Result<bool, String> {
        let current = source.validate(fixture, session, false)?;
        self.join(source, target)?;
        Ok(current)
    }

    fn join(&self, source: &Extraction, target: &Target) -> Result<(), String> {
        if self.stamp != source.stamp
            || self.texture_revision != source.stamp.content_revision
            || hash(&self.raw_snapshot) != source.stamp.snapshot_sha256
            || self.raw_snapshot != source.raw()
            || self.managed != source.managed.metadata()
        {
            return Err("capture belongs to a different native/managed extraction".into());
        }
        if self.target != *target
            || self.resolved_view.view_projection_matrix()
                != target.resolve().view_projection_matrix()
        {
            return Err("capture viewport/resize/view identity is stale".into());
        }
        if self.captured_format != super::stage::FORMAT
            || hash(&self.pixels) != self.gpu_completion_hash
        {
            return Err("capture format or completed readback hash differs".into());
        }
        Ok(())
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * self.target.viewport[0] + x) * 4) as usize;
        self.pixels[index..index + 4].try_into().unwrap()
    }

    pub fn metadata(&self) -> Value {
        json!({"native_stamp":self.stamp,"managed_observation":self.managed,
            "extraction_digest":hash(&self.raw_snapshot),
            "resolved_view_projection":self.resolved_view.view_projection_matrix().to_cols_array(),
            "view_policy":POLICY,"viewport":self.target.viewport,"resize_sequence":self.target.resize_sequence,
            "format":format!("{:?}",self.captured_format),"texture_revision":self.texture_revision,
            "layout":"tightly-packed RGBA8; top-left origin; sRGB color; straight alpha",
            "gpu_completion":"Readback::read_encoded completed actual submitted encoder",
            "rgba_sha256":self.gpu_completion_hash,"bytes":self.pixels.len(),"provenance":self.provenance,
            "scope":"generated primitive-field CLR fixture; host-correlated metadata; no native CLR/pixel attestation; offscreen only"})
    }

    pub fn retain(&self, name: &str) {
        let directory = std::path::PathBuf::from(std::env::var_os("SNAPSHOT_ARTIFACTS").unwrap());
        std::fs::create_dir_all(&directory).unwrap();
        let write = |suffix: &str, bytes: &[u8]| {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(format!("{name}.{suffix}")))
                .unwrap();
            file.write_all(bytes).unwrap();
        };
        write("rgba", &self.pixels);
        write("native-snapshot.json", &self.raw_snapshot);
        write(
            "json",
            &serde_json::to_vec_pretty(&self.metadata()).unwrap(),
        );
    }
}

pub fn provenance() -> Value {
    use std::path::PathBuf;
    let provider = PathBuf::from(std::env::var_os("SNAPSHOT_PROVIDER").unwrap());
    let bridge = PathBuf::from(std::env::var_os("SNAPSHOT_BRIDGE_DIR").unwrap());
    let identity =
        |path: &std::path::Path| json!({"path":path,"sha256":hash(&std::fs::read(path).unwrap())});
    let bridge_files: Vec<Value> = [
        "CompatibilityBridge.dll",
        "CompatibilityBridge.pdb",
        "UnityCompat.dll",
        "UnityCompat.pdb",
        "CompatibilityBridge.runtimeconfig.json",
    ]
    .iter()
    .map(|name| identity(&bridge.join(name)))
    .collect();
    let fixture = PathBuf::from(std::env::var_os("SNAPSHOT_FIXTURE").unwrap());
    json!({"provider":identity(&provider),"bridge_files":bridge_files,"fixture":identity(&fixture),
        "fixture_pdb":identity(&fixture.with_extension("pdb")),"test_binary":identity(&std::env::current_exe().unwrap()),
        "source_manifest_sha256":std::env::var("SNAPSHOT_SOURCE_SHA256").unwrap(),
        "qualified_inputs_sha256":std::env::var("SNAPSHOT_INPUTS_SHA256").unwrap(),"rust":"1.97.0",
        "features":["managed","scene","gfx","world"],"managed_fixture_loaded":true,
        "test":"retained_managed_snapshot_drives_canonical_gpu_capture"})
}
