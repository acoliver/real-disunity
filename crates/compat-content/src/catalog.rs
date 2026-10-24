use crate::model::{AssetKind, ImportError, ImportProfile, ImportSidecar, ImportedAsset};
use crate::yaml::{UnityDocument, parse_unity_yaml, parse_yaml_value};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Component, Path, PathBuf},
};
use worldrunner_asset_id::{ContentHash, LogicalAssetId};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AssetSource {
    pub path: String,
    pub meta: String,
}
pub(crate) struct Project<'a> {
    source: &'a dyn crate::ContentSource,
    pub root: PathBuf,
    pub index: BTreeMap<String, AssetSource>,
    pub assets: BTreeMap<String, ImportedAsset>,
    pub docs: BTreeMap<String, Vec<UnityDocument>>,
    pub hashes: BTreeMap<String, String>,
    pub scripts: BTreeMap<String, (String, String)>,
    pub settings: BTreeMap<String, Value>,
    pub sources: BTreeMap<String, Vec<u8>>,
    observed_meta: BTreeMap<String, String>,
    bytes: usize,
    document_count: usize,
    duplicates: BTreeSet<String>,
    requested_guids: BTreeSet<String>,
}
fn error(path: &str, message: impl Into<String>) -> ImportError {
    ImportError::Source {
        path: path.into(),
        message: message.into(),
    }
}
pub(crate) fn relative(path: &Path) -> Result<String, ImportError> {
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(error(
            &path.display().to_string(),
            "project-relative path required",
        ));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}
impl<'a> Project<'a> {
    pub fn index(root: &Path, profile: &ImportProfile) -> Result<Self, ImportError> {
        Self::index_with_source(root, profile, &crate::FileContentSource)
    }
    pub fn index_with_source(
        root: &Path,
        profile: &ImportProfile,
        source: &'a dyn crate::ContentSource,
    ) -> Result<Self, ImportError> {
        let root = root
            .canonicalize()
            .map_err(|e| error(&root.display().to_string(), e.to_string()))?;
        let mut project = Self {
            source,
            root,
            index: BTreeMap::new(),
            assets: BTreeMap::new(),
            docs: BTreeMap::new(),
            hashes: BTreeMap::new(),
            scripts: BTreeMap::new(),
            settings: BTreeMap::new(),
            sources: BTreeMap::new(),
            observed_meta: BTreeMap::new(),
            bytes: 0,
            document_count: 0,
            duplicates: BTreeSet::new(),
            requested_guids: BTreeSet::new(),
        };
        let mut queue = VecDeque::from([project.root.join("Assets")]);
        let mut entries = 0;
        while let Some(dir) = queue.pop_front() {
            for item in
                fs::read_dir(&dir).map_err(|e| error(&dir.display().to_string(), e.to_string()))?
            {
                let item = item.map_err(|e| error("Assets", e.to_string()))?;
                entries += 1;
                if entries > profile.max_catalog_entries {
                    return Err(ImportError::Limit("catalog entries".into()));
                }
                let kind = item
                    .file_type()
                    .map_err(|e| error("Assets", e.to_string()))?;
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    queue.push_back(item.path());
                    continue;
                }
                let path = item.path();
                if path.extension().is_none_or(|e| e != "meta") {
                    continue;
                }
                project.index_meta(&path, profile)?;
            }
        }
        // Indexing metadata has an independent finite entry/file budget. Reachable input bytes
        // are accounted separately so unused project assets cannot consume the admission budget.
        project.bytes = 0;
        Ok(project)
    }
    fn index_meta(&mut self, path: &Path, profile: &ImportProfile) -> Result<(), ImportError> {
        let rel = path
            .strip_prefix(&self.root)
            .map_err(|e| error("Assets", e.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        let asset = rel.trim_end_matches(".meta").to_owned();
        if !self.root.join(&asset).is_file() {
            return Ok(());
        }
        let text = self.read(&rel, profile, false)?;
        self.observed_meta
            .insert(rel.clone(), format!("{:x}", Sha256::digest(&text)));
        let text = std::str::from_utf8(&text).map_err(|e| error(&rel, e.to_string()))?;
        let Some(guid) = text.lines().find_map(|l| l.strip_prefix("guid: ")) else {
            return Ok(());
        };
        if guid.len() != 32 || !guid.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(error(&rel, "invalid meta GUID"));
        }
        if self
            .index
            .insert(
                guid.into(),
                AssetSource {
                    path: asset,
                    meta: rel.clone(),
                },
            )
            .is_some()
        {
            self.duplicates.insert(guid.into());
        }
        Ok(())
    }
    pub fn read(
        &mut self,
        relative: &str,
        profile: &ImportProfile,
        hash: bool,
    ) -> Result<Vec<u8>, ImportError> {
        let bytes = self
            .source
            .read(&self.root, relative, profile.max_file_bytes)?;
        let digest = format!("{:x}", Sha256::digest(&bytes));
        if self
            .observed_meta
            .get(relative)
            .is_some_and(|old| old != &digest)
            || self.hashes.get(relative).is_some_and(|old| old != &digest)
        {
            return Err(error(
                relative,
                "source changed during import; reimport current sources",
            ));
        }
        if hash && !self.sources.contains_key(relative) {
            self.bytes = self
                .bytes
                .checked_add(bytes.len())
                .ok_or_else(|| ImportError::Limit("input bytes".into()))?;
            if self.bytes > profile.max_total_bytes {
                return Err(ImportError::Limit("reachable input bytes".into()));
            }
            self.hashes.insert(relative.into(), digest);
            self.sources.insert(relative.into(), bytes.clone());
        }
        Ok(bytes)
    }
    pub fn verify_current(&self, profile: &ImportProfile) -> Result<(), ImportError> {
        let current = Self::index_with_source(&self.root, profile, self.source)?;
        for guid in &self.requested_guids {
            if current.index.get(guid) != self.index.get(guid)
                || current.duplicates.contains(guid) != self.duplicates.contains(guid)
            {
                return Err(error(
                    guid,
                    "source GUID catalogue changed during import; reimport current sources",
                ));
            }
        }
        for (path, expected) in &self.hashes {
            let bytes = self.source.read(&self.root, path, profile.max_file_bytes)?;
            if format!("{:x}", Sha256::digest(&bytes)) != *expected {
                return Err(error(
                    path,
                    "source changed during import; reimport current sources",
                ));
            }
        }
        Ok(())
    }
    pub fn load(&mut self, guid: &str, profile: &ImportProfile) -> Result<(), ImportError> {
        if self.duplicates.contains(guid) {
            return Err(error(
                guid,
                "reachable GUID is ambiguous across multiple .meta files",
            ));
        }
        if self.assets.contains_key(guid) {
            return Ok(());
        }
        if self.assets.len() >= profile.max_assets {
            return Err(ImportError::Limit("reachable assets".into()));
        }
        let source = self
            .index
            .get(guid)
            .cloned()
            .ok_or_else(|| error(guid, "GUID absent from catalog"))?;
        let bytes = self.read(&source.path, profile, true)?;
        let meta = self.read(&source.meta, profile, true)?;
        let metadata = parse_yaml_value(
            std::str::from_utf8(&meta).map_err(|e| error(&source.meta, e.to_string()))?,
        )
        .map_err(|e| error(&source.meta, e))?;
        let ext = Path::new(&source.path)
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = asset_kind(&ext);
        self.load_documents(guid, &source.path, &bytes, profile)?;
        if kind == AssetKind::Script {
            self.load_script(guid, &source.path, &bytes)?;
        }
        let texture_size = if kind == AssetKind::Texture && ext == "png" {
            let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
            let reader = decoder
                .read_info()
                .map_err(|e| error(&source.path, e.to_string()))?;
            Some([reader.info().width, reader.info().height])
        } else {
            None
        };
        let sprites = crate::sprites::extract(&metadata, texture_size);
        self.assets.insert(
            guid.into(),
            ImportedAsset {
                guid: guid.into(),
                logical_id: LogicalAssetId::new(format!("unity.{guid}"))
                    .map_err(|e| error(&source.path, e.to_string()))?,
                path: source.path,
                kind,
                content_hash: ContentHash::of(&bytes),
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                byte_length: bytes.len() as u64,
                metadata,
                texture_size,
                sprites,
            },
        );
        Ok(())
    }
    fn load_documents(
        &mut self,
        guid: &str,
        path: &str,
        bytes: &[u8],
        profile: &ImportProfile,
    ) -> Result<(), ImportError> {
        if bytes.starts_with(b"%YAML") || bytes.starts_with(b"--- !u!") {
            let text = std::str::from_utf8(bytes).map_err(|e| error(path, e.to_string()))?;
            let docs = parse_unity_yaml(text, profile.max_documents).map_err(|e| {
                if e == "document count limit exceeded" {
                    ImportError::Limit(e)
                } else {
                    error(path, e)
                }
            })?;
            self.account_documents(docs.len(), profile)?;
            self.docs.insert(guid.into(), docs);
        }
        Ok(())
    }
    pub fn account_documents(
        &mut self,
        count: usize,
        profile: &ImportProfile,
    ) -> Result<(), ImportError> {
        self.document_count = self
            .document_count
            .checked_add(count)
            .ok_or_else(|| ImportError::Limit("reachable document count".into()))?;
        if self.document_count > profile.max_documents {
            return Err(ImportError::Limit("reachable document count".into()));
        }
        Ok(())
    }
    fn load_script(&mut self, guid: &str, path: &str, bytes: &[u8]) -> Result<(), ImportError> {
        let text = std::str::from_utf8(bytes).map_err(|e| error(path, e.to_string()))?;
        let name = Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let tokens: Vec<_> = text
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
            .filter(|s| !s.is_empty())
            .collect();
        let namespace = tokens
            .windows(2)
            .find(|t| t[0] == "namespace")
            .map(|t| t[1]);
        let name = namespace.map_or_else(|| name.to_owned(), |ns| format!("{ns}.{name}"));
        self.scripts.insert(guid.into(), (path.into(), name));
        Ok(())
    }
    pub fn reachable(
        &mut self,
        scene_guid: &str,
        profile: &ImportProfile,
        sidecar: &ImportSidecar,
    ) -> Result<(), ImportError> {
        let mut queue = VecDeque::from([scene_guid.to_owned()]);
        for settings in self.settings.values() {
            collect_guids(settings, &mut queue);
        }
        let mut seen = BTreeSet::new();
        while let Some(guid) = queue.pop_front() {
            if !seen.insert(guid.clone()) || !self.index.contains_key(&guid) {
                continue;
            }
            self.load(&guid, profile)?;
            if let Some(docs) = self.docs.get(&guid) {
                for doc in docs {
                    collect_guids(&doc.fields, &mut queue);
                }
            }
            if let Some(asset) = self.assets.get(&guid) {
                collect_guids(&asset.metadata, &mut queue);
            }
            for patch in &sidecar.field_overrides {
                if patch.guid == guid {
                    collect_guids(&patch.value, &mut queue);
                }
            }
            for patch in &sidecar.reference_substitutions {
                collect_guids(&patch.replacement, &mut queue);
            }
        }
        self.requested_guids = seen;
        Ok(())
    }
}
fn collect_guids(value: &Value, queue: &mut VecDeque<String>) {
    match value {
        Value::Object(map) => {
            if map.contains_key("fileID")
                && let Some(guid) = map.get("guid").and_then(Value::as_str)
            {
                queue.push_back(guid.into());
            }
            for v in map.values() {
                collect_guids(v, queue);
            }
        }
        Value::Array(v) => {
            for v in v {
                collect_guids(v, queue);
            }
        }
        _ => {}
    }
}

fn asset_kind(extension: &str) -> AssetKind {
    match extension {
        "unity" => AssetKind::Scene,
        "prefab" => AssetKind::Prefab,
        "cs" => AssetKind::Script,
        "asset" => AssetKind::ScriptableObject,
        "png" | "jpg" | "jpeg" | "tga" | "psd" => AssetKind::Texture,
        "mat" => AssetKind::Material,
        "fbx" | "obj" => AssetKind::Mesh,
        "ttf" | "otf" => AssetKind::Font,
        "wav" | "mp3" | "ogg" => AssetKind::Audio,
        _ => AssetKind::Data,
    }
}
