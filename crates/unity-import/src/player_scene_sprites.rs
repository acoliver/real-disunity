//! Sprites a scene's script fields reference, as scene plan assets.
//!
//! A scene plan holds no textures except the ones its sprites draw. A referenced sprite plans
//! after its texture, which is shared by the sprites that draw it, and the textures name their
//! pixels in place (see [`crate::player_sprites::plan_scene_texture`]). A scene plan has no
//! directory of its own, so a sprite the scene shares with the resources is planned again for
//! each scene.

use std::path::Path;

use worldrunner_unity_serialized::{ClassId, object::PPtr};

use crate::{
    plan::PlanAsset,
    player_shader_refs::{SceneSource, class_of},
    player_sprites::{AssetRef, plan_scene_texture, read_sprite, texture_flaw},
    player_values::not_held,
};

impl SceneSource {
    /// The index of the sprite `pptr` names, planning it and its texture first when they are
    /// new, or why the scene does not hold it.
    pub(crate) fn sprite_asset(
        &mut self,
        key: &(std::path::PathBuf, i64),
        pptr: PPtr,
    ) -> Result<usize, String> {
        if let Some((_, index)) = self.planned.iter().find(|(known, _)| known == key) {
            return Ok(*index);
        }
        let mut parts =
            read_sprite(self.files_mut(), key).map_err(|cause| unplanned(pptr, &cause))?;
        self.differences.append(&mut parts.notes);
        let (owner, texture) = parts.texture();
        let owner = owner.to_owned();
        let texture = self.texture_asset(&owner, texture).map_err(|reason| {
            format!(
                "references sprite object {} of file {}, whose texture is not planned: {reason}",
                pptr.path_id, pptr.file_id
            )
        })?;
        let sprite = parts.into_asset(Some(AssetRef { asset: texture }));
        Ok(self.push_asset(key.clone(), PlanAsset::Sprite { sprite }))
    }

    /// The index of the texture `pptr` names, as seen from the file `owner`, planned when it is new:
    /// the one a sprite draws, or one a script asset such as a font atlas holds.
    pub(crate) fn texture_asset(&mut self, owner: &Path, pptr: PPtr) -> Result<usize, String> {
        if pptr.is_null() {
            return Err("it has none".to_owned());
        }
        let file = self
            .files_mut()
            .referenced_file(owner, pptr)
            .map_err(|_| not_held(pptr))?;
        let key = (file, pptr.path_id);
        if class_of(self.files_mut(), &key) != Some(ClassId::TEXTURE_2D) {
            return Err(not_held(pptr));
        }
        if let Some((_, index)) = self.planned.iter().find(|(known, _)| *known == key) {
            return Ok(*index);
        }
        if let Some(flaw) =
            texture_flaw(self.files_mut(), &key).map_err(|cause| cause.to_string())?
        {
            return Err(flaw);
        }
        let texture =
            plan_scene_texture(self.files_mut(), &key).map_err(|cause| cause.to_string())?;
        Ok(self.push_asset(key, PlanAsset::Texture2D { texture }))
    }
}

/// Why a reference to a sprite that does not decode is left out.
fn unplanned(pptr: PPtr, cause: &crate::PlayerError) -> String {
    format!(
        "references sprite object {} of file {}, which did not plan: {cause}",
        pptr.path_id, pptr.file_id
    )
}
