//! Object order and the indices that plan references resolve through.

use std::collections::BTreeMap;

use crate::{
    Guid, ObjectId, Origin,
    components::{ComponentError, MONO_BEHAVIOUR, PHYSICS_MATERIAL_2D},
    package::PackageFont,
    project::Project,
    scene::{Reference, Scene},
    texture::Textures,
};

use super::{PlanAsset, PlanError, PlanObject, PlanSprite, PlanTexture};

const GAME_OBJECT: i32 = 1;
const TRANSFORM: i32 = 4;
const RECT_TRANSFORM: i32 = 224;

/// Converts a scene into plan records, assigning indices as objects and resources are first
/// reached.
pub(super) struct Planner<'a> {
    pub(super) project: &'a Project,
    pub(super) scene: &'a Scene,
    /// Game objects, parents first: each with whether it belongs to a template.
    order: Vec<(ObjectId, bool)>,
    pub(super) objects: BTreeMap<ObjectId, usize>,
    /// Each component's object index and position in that object's component list.
    pub(super) components: BTreeMap<ObjectId, [usize; 2]>,
    pub(super) assets: BTreeMap<ObjectId, usize>,
    asset_order: Vec<ObjectId>,
    pub(super) textures: Textures,
    texture_indices: BTreeMap<Guid, usize>,
    pub(super) plan_textures: Vec<PlanTexture>,
    sprite_indices: BTreeMap<(Guid, i64), usize>,
    pub(super) sprites: Vec<PlanSprite>,
    pub(super) fonts: Vec<PackageFont>,
}

impl<'a> Planner<'a> {
    pub(super) fn new(project: &'a Project, scene: &'a Scene) -> Result<Self, PlanError> {
        if let Some((id, reference)) = scene.missing_references().into_iter().next() {
            let message = format!("references a missing asset: {reference:?}");
            return Err(scene.component_error(id, message).into());
        }
        let mut planner = Self {
            project,
            scene,
            order: Vec::new(),
            objects: BTreeMap::new(),
            components: BTreeMap::new(),
            assets: BTreeMap::new(),
            asset_order: Vec::new(),
            textures: Textures::default(),
            texture_indices: BTreeMap::new(),
            plan_textures: Vec::new(),
            sprite_indices: BTreeMap::new(),
            sprites: Vec::new(),
            fonts: Vec::new(),
        };
        planner.order_objects()?;
        Ok(planner)
    }

    /// Orders scene objects then template objects, each tree depth first, and indexes every
    /// object's components.
    fn order_objects(&mut self) -> Result<(), PlanError> {
        let scene = self.scene;
        let roots = scene.roots().iter().map(|root| (*root, false));
        let templates = scene.templates().map(|(_, root)| (root, true));
        for (root, template) in roots.chain(templates).collect::<Vec<_>>() {
            let mut pending = vec![root];
            while let Some(transform) = pending.pop() {
                let view = scene
                    .transform(transform)
                    .ok_or_else(|| scene.component_error(transform, "is not a transform"))?;
                let owner = view
                    .game_object
                    .ok_or_else(|| scene.component_error(transform, "has no game object"))?;
                self.index_object(owner, template)?;
                pending.extend(view.children.iter().rev());
            }
        }
        self.check_reached()
    }

    fn index_object(&mut self, id: ObjectId, template: bool) -> Result<(), PlanError> {
        let scene = self.scene;
        let object = scene
            .game_object(id)
            .ok_or_else(|| scene.component_error(id, "is not a game object"))?;
        let index = self.order.len();
        self.order.push((id, template));
        self.objects.insert(id, index);
        let mut position = 0;
        for component in object.components {
            if planned(scene, component) {
                self.components.insert(component, [index, position]);
                position += 1;
            }
        }
        Ok(())
    }

    /// Fails for game objects outside every planned hierarchy, which the scene would lose.
    fn check_reached(&self) -> Result<(), PlanError> {
        let scene = self.scene;
        let lost = scene.objects().find(|(id, object)| {
            object.class_id == GAME_OBJECT && !object.removed && !self.objects.contains_key(id)
        });
        match lost {
            Some((id, _)) => Err(scene
                .component_error(id, "is outside every scene and prefab hierarchy")
                .into()),
            None => Ok(()),
        }
    }

    pub(super) fn objects(&mut self) -> Result<Vec<PlanObject>, PlanError> {
        let order = self.order.clone();
        order
            .into_iter()
            .map(|(id, template)| self.object(id, template))
            .collect()
    }

    fn object(&mut self, id: ObjectId, template: bool) -> Result<PlanObject, PlanError> {
        let scene = self.scene;
        let error = |message: &str| scene.component_error(id, message);
        let object = scene
            .game_object(id)
            .ok_or_else(|| error("is not a game object"))?;
        let transform_id = scene
            .transform_of(id)
            .ok_or_else(|| error("has no transform"))?;
        let transform = scene
            .transform(transform_id)
            .ok_or_else(|| error("has no transform"))?;
        let parent = match transform.parent {
            Some(parent) => Some(self.owner_index(parent)?),
            None => None,
        };
        let rect = match scene.object(transform_id).map(|o| o.class_id) {
            Some(RECT_TRANSFORM) => Some(scene.rect_transform(transform_id)?),
            _ => None,
        };
        let mut components = Vec::new();
        for component in object.components.iter().copied() {
            if planned(scene, component) {
                components.push(self.component(component)?);
            }
        }
        Ok(PlanObject {
            name: object.name,
            tag: object.tag,
            layer: object.layer,
            active: object.active,
            parent,
            template,
            position: transform.local_position,
            rotation: transform.local_rotation,
            scale: transform.local_scale,
            rect,
            components,
        })
    }

    /// The object index of the game object that owns `transform`.
    pub(super) fn owner_index(&self, transform: ObjectId) -> Result<usize, ComponentError> {
        let scene = self.scene;
        scene
            .transform(transform)
            .and_then(|view| view.game_object)
            .and_then(|owner| self.objects.get(&owner).copied())
            .ok_or_else(|| scene.component_error(transform, "is not part of the plan"))
    }

    /// The index of serialized asset `id`, planning it when first reached.
    pub(super) fn asset(&mut self, id: ObjectId) -> usize {
        if let Some(index) = self.assets.get(&id) {
            return *index;
        }
        let index = self.asset_order.len();
        self.asset_order.push(id);
        self.assets.insert(id, index);
        index
    }

    /// Plans every asset reached so far, including those reached while planning assets.
    pub(super) fn assets(&mut self) -> Result<Vec<PlanAsset>, PlanError> {
        let mut planned = Vec::new();
        while planned.len() < self.asset_order.len() {
            let id = self.asset_order[planned.len()];
            planned.push(self.plan_asset(id)?);
        }
        Ok(planned)
    }

    fn plan_asset(&mut self, id: ObjectId) -> Result<PlanAsset, PlanError> {
        let scene = self.scene;
        let object = scene
            .object(id)
            .ok_or_else(|| scene.component_error(id, "is not an object"))?;
        let name = crate::scene::read::text(&object.fields, "m_Name").unwrap_or_default();
        match object.class_id {
            PHYSICS_MATERIAL_2D => Ok(PlanAsset::PhysicsMaterial2D {
                name,
                material: scene.physics_material(id)?,
            }),
            MONO_BEHAVIOUR => {
                let (script, class) = self.script_identity(id)?;
                let fields = self.fields(id, &object.fields)?;
                Ok(PlanAsset::Script {
                    script,
                    class,
                    name,
                    fields,
                })
            }
            _ => Err(scene
                .component_error(id, "is an asset type scripts cannot reference")
                .into()),
        }
    }

    /// The script file and class of a project `MonoBehaviour` or `ScriptableObject`.
    pub(super) fn script_identity(&self, id: ObjectId) -> Result<(String, String), ComponentError> {
        let scene = self.scene;
        let error = |message: &str| scene.component_error(id, message);
        let Some(Reference::External { guid, .. }) = scene.script(id) else {
            return Err(error("has no project script"));
        };
        let asset = self
            .project
            .asset(guid)
            .ok_or_else(|| error("has no project script"))?;
        let class = asset
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| error("has a script file without a class name"))?;
        Ok((asset.path.display().to_string(), class.to_owned()))
    }

    /// The index of the sprite `{fileID, guid}` names, importing its texture when first reached.
    pub(super) fn sprite(
        &mut self,
        owner: ObjectId,
        guid: Guid,
        file_id: i64,
    ) -> Result<usize, PlanError> {
        if let Some(index) = self.sprite_indices.get(&(guid, file_id)) {
            return Ok(*index);
        }
        let texture = self.texture(guid)?;
        let (_, sprite) = self
            .textures
            .sprite(self.project, guid, file_id)
            .map_err(|e| self.scene.component_error(owner, e.to_string()))?;
        let index = self.sprites.len();
        self.sprites.push(PlanSprite {
            texture,
            name: sprite.name.clone(),
            rect: sprite.rect,
            pivot: sprite.pivot,
            pixels_per_unit: sprite.pixels_per_unit,
            border: sprite.border,
            physics_shape: None,
        });
        self.sprite_indices.insert((guid, file_id), index);
        Ok(index)
    }

    /// Adds sprite `index`'s physics shape, for a tile that collides with its sprite.
    pub(super) fn require_shape(&mut self, guid: Guid, file_id: i64) -> Result<(), PlanError> {
        let index = self.sprite_indices[&(guid, file_id)];
        if self.sprites[index].physics_shape.is_none() {
            let texture = self.textures.get(self.project, guid)?;
            self.sprites[index].physics_shape = Some(texture.physics_shape(file_id)?);
        }
        Ok(())
    }

    pub(super) fn texture(&mut self, guid: Guid) -> Result<usize, PlanError> {
        if let Some(index) = self.texture_indices.get(&guid) {
            return Ok(*index);
        }
        let texture = self.textures.get(self.project, guid)?;
        let settings = texture.settings();
        let image = texture.image();
        let index = self.plan_textures.len();
        self.plan_textures.push(PlanTexture {
            guid: guid.to_string(),
            path: texture.path().display().to_string(),
            width: image.width,
            height: image.height,
            filter: settings.filter,
            wrap: settings.wrap,
            srgb: settings.srgb,
        });
        self.texture_indices.insert(guid, index);
        Ok(index)
    }

    pub(super) fn font(&mut self, font: PackageFont) {
        if !self.fonts.contains(&font) {
            self.fonts.push(font);
        }
    }

    pub(super) fn finish(self) -> (Vec<PlanSprite>, Vec<PlanTexture>, Vec<PackageFont>) {
        (self.sprites, self.plan_textures, self.fonts)
    }
}

/// Whether a component becomes a plan record. Transforms are part of their object; Grid, the
/// tilemap renderer and the canvas renderer are folded into the components that need them.
fn planned(scene: &Scene, component: ObjectId) -> bool {
    scene.object(component).is_some_and(|object| {
        !object.removed
            && !matches!(
                object.type_name.as_str(),
                "Transform" | "RectTransform" | "Grid" | "TilemapRenderer" | "CanvasRenderer"
            )
            && !matches!(object.class_id, TRANSFORM | RECT_TRANSFORM)
            && !matches!(object.origin, Origin::Asset(_))
    })
}
