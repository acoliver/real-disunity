use super::{
    extraction::{Extraction, Stamp, hash},
    native::{Fixture, Session},
    view::{POLICY, Target},
};
use serde_json::{Value, json};
use worldrunner::scene::{camera::ResolvedView, gpu::wgpu};

pub struct ViewReceipt {
    pub stamp: Stamp,
    pub captured_snapshot: Value,
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
        let fixture_hash =
            hash(&serde_json::to_vec(&self.captured_snapshot).map_err(|error| error.to_string())?);
        if self.stamp != source.stamp
            || self.texture_revision != source.stamp.content_revision
            || fixture_hash != self.stamp.snapshot_sha256
        {
            return Err("capture belongs to a different native extraction".into());
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
        json!({
            "native_stamp":self.stamp,
            "canonical_snapshot_fixture":self.captured_snapshot,
            "extraction_digest":self.stamp.snapshot_sha256,
            "resolved_view_projection":self.resolved_view.view_projection_matrix().to_cols_array(),
            "view_policy":POLICY, "viewport":self.target.viewport,
            "resize_sequence":self.target.resize_sequence,
            "format":format!("{:?}", self.captured_format),
            "layout":"tightly-packed RGBA8; top-left origin; sRGB color; straight alpha",
            "texture_revision":self.texture_revision,
            "gpu_completion":"Readback::read_encoded completed submitted encoder",
            "rgba_sha256":self.gpu_completion_hash, "bytes":self.pixels.len(),
            "provenance":self.provenance,
            "scope":"native-only exact-coordinate codec fixture; offscreen completion; no native pixel attestation"
        })
    }

    pub fn retain(&self, name: &str) {
        // The managed consumer shares SNAPSHOT_ARTIFACTS and uses the same frame names.
        let directory = std::path::PathBuf::from(
            std::env::var_os("SNAPSHOT_ARTIFACTS").expect("unique owned artifact directory"),
        )
        .join("native-consumer");
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
        write(
            "snapshot-fixture.json",
            &serde_json::to_vec(&self.captured_snapshot).unwrap(),
        );
        write(
            "json",
            &serde_json::to_vec_pretty(&self.metadata()).unwrap(),
        );
    }
}

pub fn provenance() -> Value {
    let provider = std::path::PathBuf::from(std::env::var_os("SNAPSHOT_PROVIDER").unwrap());
    let bridge = std::path::PathBuf::from(std::env::var_os("SNAPSHOT_BRIDGE_DIR").unwrap());
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
    let executable = std::env::current_exe().unwrap();
    json!({"provider":identity(&provider), "bridge_files":bridge_files,
        "test_binary":identity(&executable),
        "features":["managed","scene","gfx","world"], "test":"native_snapshot_drives_canonical_gpu_capture",
        "source_manifest_sha256":std::env::var("SNAPSHOT_SOURCE_SHA256").expect("frozen source identity"),
        "rust":"1.97.0", "managed_fixture_loaded":false})
}
