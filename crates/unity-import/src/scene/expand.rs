//! Expansion of a scene file, its prefab instances and the assets its objects reference.
//!
//! Each file is first *prepared*: its objects get ids and its prefab instances are prepared
//! recursively, so every fileID in the file maps to an object. Then it is *finished*: fields are
//! converted with references resolved and instance overrides are applied. Referenced templates
//! and assets are registered between the two steps, so files may reference each other.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use super::{
    Object, ObjectId, Origin, Reference, Scene, SceneError, Source, Value, modify, value,
    views::{RECT_TRANSFORM, TRANSFORM},
};
use crate::{
    guid::{Guid, ObjectRef},
    package,
    project::Project,
    yaml::{Document, Mapping, Node, Scalar},
};

/// Fields linking an object to its prefab source. Expansion consumes them.
const BOOKKEEPING: [&str; 5] = [
    "m_PrefabParentObject",
    "m_PrefabInternal",
    "m_CorrespondingSourceObject",
    "m_PrefabInstance",
    "m_PrefabAsset",
];
const PREFAB: i32 = 1001;
const SCENE_ROOTS: i32 = 1_660_057_539;

/// fileIDs of one expanded file, including aliases for its stripped documents and instances.
#[derive(Clone, Default)]
struct FileMap {
    ids: HashMap<i64, ObjectId>,
    /// The single root transform of a prefab.
    root: Option<ObjectId>,
}

struct Prepared {
    path: PathBuf,
    guid: Option<Guid>,
    documents: Vec<Document>,
    map: FileMap,
    regular: Vec<(usize, ObjectId)>,
    instances: Vec<(usize, Prepared)>,
}

enum Kind {
    Regular,
    Instance(ObjectRef),
    Stripped { instance: i64, source: i64 },
    PrefabRoot,
}

struct Context<'a> {
    path: &'a Path,
    guid: Option<Guid>,
    map: &'a FileMap,
}

impl Context<'_> {
    fn local(&self, file_id: i64) -> Reference {
        self.map.ids.get(&file_id).map_or(
            Reference::Missing {
                guid: self.guid,
                file_id,
            },
            |id| Reference::Object(*id),
        )
    }
}

pub(super) struct Importer<'p> {
    project: &'p Project,
    objects: Vec<Object>,
    loaded: HashMap<Guid, FileMap>,
    templates: BTreeMap<Guid, ObjectId>,
    nesting: Vec<Guid>,
}

impl<'p> Importer<'p> {
    pub(super) fn new(project: &'p Project) -> Self {
        Self {
            project,
            objects: Vec::new(),
            loaded: HashMap::new(),
            templates: BTreeMap::new(),
            nesting: Vec::new(),
        }
    }

    pub(super) fn scene(mut self, path: &Path) -> Result<Scene, SceneError> {
        let prepared = self.prepare(path, None, Origin::Scene)?;
        self.finish(prepared)?;
        let roots = self.roots();
        Ok(Scene {
            path: path.to_owned(),
            objects: self.objects,
            roots,
            templates: self.templates,
        })
    }

    fn prepare(
        &mut self,
        path: &Path,
        guid: Option<Guid>,
        origin: Origin,
    ) -> Result<Prepared, SceneError> {
        if let Some(guid) = guid {
            if self.nesting.contains(&guid) {
                return Err(SceneError::Cycle(path.to_owned()));
            }
            self.nesting.push(guid);
        }
        let prepared = self.prepare_file(path, guid, origin);
        if guid.is_some() {
            self.nesting.pop();
        }
        prepared
    }

    fn prepare_file(
        &mut self,
        path: &Path,
        guid: Option<Guid>,
        origin: Origin,
    ) -> Result<Prepared, SceneError> {
        let documents = self.project.read_documents(path)?;
        let mut prepared = Prepared {
            path: path.to_owned(),
            guid,
            documents: Vec::new(),
            map: FileMap::default(),
            regular: Vec::new(),
            instances: Vec::new(),
        };
        let mut stripped = Vec::new();
        let mut prefab_roots = Vec::new();
        for (index, document) in documents.iter().enumerate() {
            let error = |message: String| SceneError::object(path, document.file_id, message);
            match classify(document).map_err(error)? {
                Kind::Regular => {
                    let id = self.allocate(document, origin, guid, path)?;
                    prepared.map.ids.insert(document.file_id, id);
                    prepared.regular.push((index, id));
                }
                Kind::Instance(source) => {
                    let inner = self.instance(path, document.file_id, source, origin)?;
                    prepared.instances.push((index, inner));
                }
                Kind::Stripped { instance, source } => {
                    stripped.push((document.file_id, instance, source));
                }
                Kind::PrefabRoot => prefab_roots.push(document.file_id),
            }
        }
        prepared.documents = documents;
        prepared.alias_instances()?;
        prepared.alias_stripped(&stripped)?;
        // Prefab files have one root transform; scenes and serialized assets have none.
        if guid.is_some() && !matches!(origin, Origin::Asset(_)) {
            prepared.map.root = prepared.root_transform(&self.objects)?;
        }
        if let Some(root) = prepared.map.root {
            for file_id in prefab_roots {
                prepared.map.ids.insert(file_id, root);
            }
        }
        Ok(prepared)
    }

    fn instance(
        &mut self,
        path: &Path,
        file_id: i64,
        source: ObjectRef,
        origin: Origin,
    ) -> Result<Prepared, SceneError> {
        let guid = source.guid.ok_or_else(|| {
            SceneError::object(path, file_id, "prefab instance has no source GUID")
        })?;
        let prefab = self.project.asset(guid).ok_or_else(|| {
            SceneError::object(
                path,
                file_id,
                format!("prefab {guid} is not in the project"),
            )
        })?;
        let prefab = prefab.path.clone();
        self.prepare(&prefab, Some(guid), origin)
    }

    fn allocate(
        &mut self,
        document: &Document,
        origin: Origin,
        guid: Option<Guid>,
        path: &Path,
    ) -> Result<ObjectId, SceneError> {
        let index = u32::try_from(self.objects.len())
            .map_err(|_| SceneError::object(path, document.file_id, "too many objects"))?;
        self.objects.push(Object {
            class_id: document.class_id,
            type_name: document.type_name.clone(),
            origin,
            source: Source {
                asset: guid,
                file_id: document.file_id,
            },
            fields: Value::empty(),
            removed: false,
        });
        Ok(ObjectId(index))
    }

    fn finish(&mut self, prepared: Prepared) -> Result<FileMap, SceneError> {
        let Prepared {
            path,
            guid,
            documents,
            map,
            regular,
            instances,
        } = prepared;
        let context = Context {
            path: &path,
            guid,
            map: &map,
        };
        for (index, inner) in instances {
            let prefab = inner.guid.unwrap_or(Guid::ZERO);
            let inner = self.finish(inner)?;
            self.apply_instance(&context, &documents[index], prefab, &inner)?;
        }
        for (index, id) in regular {
            let document = &documents[index];
            let fields = self.convert(&context, document.file_id, &document.body)?;
            self.objects[id.index()].fields = fields;
        }
        Ok(map)
    }

    fn convert(
        &mut self,
        context: &Context<'_>,
        file_id: i64,
        body: &Mapping,
    ) -> Result<Value, SceneError> {
        let mut fields = Vec::new();
        for (key, node) in body.iter().filter(|(key, _)| !BOOKKEEPING.contains(key)) {
            let value = value::convert(node, &mut |reference| {
                self.resolve(context, file_id, reference)
            })?;
            fields.push((key.to_owned(), value));
        }
        Ok(Value::Record(fields))
    }

    fn resolve(
        &mut self,
        context: &Context<'_>,
        file_id: i64,
        node: &Node,
    ) -> Result<Reference, SceneError> {
        let reference = ObjectRef::from_node(node)
            .map_err(|error| SceneError::object(context.path, file_id, error.to_string()))?;
        if reference.is_null() {
            return Ok(Reference::Null);
        }
        match reference.guid {
            Some(guid) if Some(guid) != context.guid => self.external(guid, reference.file_id),
            _ => Ok(context.local(reference.file_id)),
        }
    }

    /// Resolves a reference into another file, loading prefabs as templates and serialized
    /// assets once.
    fn external(&mut self, guid: Guid, file_id: i64) -> Result<Reference, SceneError> {
        let missing = Reference::Missing {
            guid: Some(guid),
            file_id,
        };
        let Some(asset) = self.project.asset(guid) else {
            // Files in the project win; only absent assets are looked up as package assets.
            return Ok(match package::identify(guid, file_id) {
                Some(known) => Reference::Package(known),
                None if guid.is_builtin() => Reference::External { guid, file_id },
                None => missing,
            });
        };
        let origin = if asset.path.extension().is_some_and(|ext| ext == "prefab") {
            Origin::Template(guid)
        } else if asset.importer == "NativeFormatImporter" {
            Origin::Asset(guid)
        } else {
            return Ok(Reference::External { guid, file_id });
        };
        if !self.loaded.contains_key(&guid) {
            let path = asset.path.clone();
            let prepared = self.prepare(&path, Some(guid), origin)?;
            self.loaded.insert(guid, prepared.map.clone());
            if let (Origin::Template(_), Some(root)) = (origin, prepared.map.root) {
                self.templates.insert(guid, root);
            }
            self.finish(prepared)?;
        }
        Ok(self.loaded[&guid]
            .ids
            .get(&file_id)
            .map_or(missing, |id| Reference::Object(*id)))
    }

    fn apply_instance(
        &mut self,
        context: &Context<'_>,
        document: &Document,
        prefab: Guid,
        inner: &FileMap,
    ) -> Result<(), SceneError> {
        let file_id = document.file_id;
        let error = |message: &str| SceneError::object(context.path, file_id, message);
        let modification = document
            .body
            .get("m_Modification")
            .and_then(Node::as_mapping)
            .ok_or_else(|| error("prefab instance has no m_Modification"))?;
        for entry in items(modification, "m_Modifications") {
            self.apply_override(context, file_id, prefab, inner, entry)?;
        }
        for removed in items(modification, "m_RemovedComponents") {
            let id = target(context, file_id, prefab, inner, removed)?;
            self.objects[id.index()].removed = true;
        }
        if !items(modification, "m_RemovedGameObjects").is_empty() {
            return Err(error(
                "removing GameObjects from a prefab instance is not supported",
            ));
        }
        let parent = match modification.get("m_TransformParent") {
            Some(node) => self.resolve(context, file_id, node)?,
            None => Reference::Null,
        };
        if parent != Reference::Null {
            let root = inner
                .root
                .ok_or_else(|| error("prefab has no root transform"))?;
            let reparent = modify::Override {
                scalar: Scalar::plain(""),
                reference: parent,
            };
            modify::apply(
                &mut self.objects[root.index()].fields,
                "m_Father",
                &reparent,
            )
            .map_err(|message| error(&message))?;
        }
        Ok(())
    }

    fn apply_override(
        &mut self,
        context: &Context<'_>,
        file_id: i64,
        prefab: Guid,
        inner: &FileMap,
        entry: &Node,
    ) -> Result<(), SceneError> {
        let error = |message: String| SceneError::object(context.path, file_id, message);
        let target_node = entry
            .get("target")
            .ok_or_else(|| error("override has no target".into()))?;
        let id = target(context, file_id, prefab, inner, target_node)?;
        let path = entry
            .get("propertyPath")
            .and_then(Node::as_scalar)
            .ok_or_else(|| error("override has no propertyPath".into()))?
            .text()
            .to_owned();
        let scalar = entry
            .get("value")
            .and_then(Node::as_scalar)
            .cloned()
            .unwrap_or_else(|| Scalar::plain(""));
        let reference = match entry.get("objectReference") {
            Some(node) => self.resolve(context, file_id, node)?,
            None => Reference::Null,
        };
        let new = modify::Override { scalar, reference };
        modify::apply(&mut self.objects[id.index()].fields, &path, &new)
            .map_err(|message| error(format!("override `{path}`: {message}")))
    }

    /// Root transforms of the scene: `SceneRoots` order when present, otherwise `m_RootOrder`.
    fn roots(&self) -> Vec<ObjectId> {
        let scene = || {
            (0..)
                .map(ObjectId)
                .zip(&self.objects)
                .filter(|(_, o)| o.origin == Origin::Scene && !o.removed)
        };
        if let Some((_, roots)) = scene().find(|(_, o)| o.class_id == SCENE_ROOTS) {
            return roots
                .fields
                .get("m_Roots")
                .and_then(Value::as_sequence)
                .unwrap_or_default()
                .iter()
                .filter_map(|value| match value.as_reference() {
                    Some(Reference::Object(id)) => Some(id),
                    _ => None,
                })
                .collect();
        }
        let mut roots: Vec<_> = scene()
            .filter(|(_, o)| matches!(o.class_id, TRANSFORM | RECT_TRANSFORM))
            .filter(|(_, o)| {
                o.fields
                    .get("m_Father")
                    .and_then(Value::as_reference)
                    .is_none_or(|father| father == Reference::Null)
            })
            .map(|(id, o)| (root_order(&o.fields), id))
            .collect();
        roots.sort_unstable();
        roots.into_iter().map(|(_, id)| id).collect()
    }
}

impl Prepared {
    /// Maps each instance document to the instance's root transform, and every object of the
    /// instance to Unity's derived fileID `(instance ^ source) & i64::MAX`.
    fn alias_instances(&mut self) -> Result<(), SceneError> {
        for (index, inner) in &self.instances {
            let file_id = self.documents[*index].file_id;
            let root = inner.map.root.ok_or_else(|| {
                SceneError::object(&self.path, file_id, "prefab has no root transform")
            })?;
            self.map.ids.insert(file_id, root);
            for (source, id) in &inner.map.ids {
                self.map
                    .ids
                    .entry((file_id ^ source) & i64::MAX)
                    .or_insert(*id);
            }
        }
        Ok(())
    }

    fn alias_stripped(&mut self, stripped: &[(i64, i64, i64)]) -> Result<(), SceneError> {
        for &(file_id, instance, source) in stripped {
            let error = |message: &str| SceneError::object(&self.path, file_id, message);
            let (_, inner) = self
                .instances
                .iter()
                .find(|(index, _)| self.documents[*index].file_id == instance)
                .ok_or_else(|| error("stripped object names an unknown prefab instance"))?;
            let id =
                inner.map.ids.get(&source).ok_or_else(|| {
                    error("stripped object names an object missing from its prefab")
                })?;
            self.map.ids.insert(file_id, *id);
        }
        Ok(())
    }

    /// The one transform without a parent, among this file's objects and nested instances.
    fn root_transform(&self, objects: &[Object]) -> Result<Option<ObjectId>, SceneError> {
        let regular = self.regular.iter().filter_map(|(index, id)| {
            let class = objects[id.index()].class_id;
            let body = &self.documents[*index].body;
            (matches!(class, TRANSFORM | RECT_TRANSFORM) && unparented(body.get("m_Father")))
                .then_some(*id)
        });
        let nested = self.instances.iter().filter_map(|(index, inner)| {
            let modification = self.documents[*index].body.get("m_Modification");
            unparented(modification.and_then(|m| m.get("m_TransformParent")))
                .then_some(inner.map.root)
                .flatten()
        });
        let roots: Vec<_> = regular.chain(nested).collect();
        match roots.as_slice() {
            [] => Ok(None),
            [root] => Ok(Some(*root)),
            _ => Err(SceneError::object(
                &self.path,
                0,
                "prefab has more than one root transform",
            )),
        }
    }
}

fn unparented(node: Option<&Node>) -> bool {
    node.is_none_or(|node| ObjectRef::from_node(node).is_ok_and(|r| r.is_null()))
}

fn root_order(fields: &Value) -> i64 {
    fields
        .get("m_RootOrder")
        .and_then(Value::as_scalar)
        .and_then(Scalar::as_i64)
        .unwrap_or(0)
}

fn items<'a>(mapping: &'a Mapping, key: &str) -> &'a [Node] {
    mapping
        .get(key)
        .and_then(Node::as_sequence)
        .unwrap_or_default()
}

/// The object an override names. Overrides must name objects of the instance's own prefab.
fn target(
    context: &Context<'_>,
    file_id: i64,
    prefab: Guid,
    inner: &FileMap,
    node: &Node,
) -> Result<ObjectId, SceneError> {
    let error = |message: String| SceneError::object(context.path, file_id, message);
    let reference = ObjectRef::from_node(node).map_err(|e| error(e.to_string()))?;
    if reference.guid != Some(prefab) {
        return Err(error(format!(
            "override names an object outside prefab {prefab}"
        )));
    }
    inner.ids.get(&reference.file_id).copied().ok_or_else(|| {
        error(format!(
            "override names object {} missing from prefab {prefab}",
            reference.file_id
        ))
    })
}

fn classify(document: &Document) -> Result<Kind, String> {
    let body = &document.body;
    if document.stripped {
        let instance = first_reference(body, &["m_PrefabInstance", "m_PrefabInternal"])?;
        let source = first_reference(
            body,
            &["m_CorrespondingSourceObject", "m_PrefabParentObject"],
        )?;
        return Ok(Kind::Stripped {
            instance: instance.file_id,
            source: source.file_id,
        });
    }
    if document.class_id != PREFAB {
        return Ok(Kind::Regular);
    }
    if body.get("m_SourcePrefab").is_some() {
        return first_reference(body, &["m_SourcePrefab"]).map(Kind::Instance);
    }
    let parent = body
        .get("m_IsPrefabParent")
        .and_then(Node::as_scalar)
        .and_then(Scalar::as_bool);
    if parent == Some(true) {
        return Ok(Kind::PrefabRoot);
    }
    first_reference(body, &["m_ParentPrefab"]).map(Kind::Instance)
}

fn first_reference(body: &Mapping, keys: &[&str]) -> Result<ObjectRef, String> {
    let node = keys
        .iter()
        .find_map(|key| body.get(key))
        .ok_or_else(|| format!("missing prefab link {}", keys.join(" or ")))?;
    ObjectRef::from_node(node).map_err(|error| error.to_string())
}
