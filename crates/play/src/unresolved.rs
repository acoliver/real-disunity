//! Sprites whose texture the frame cannot draw.
use std::collections::BTreeSet;

use worldrunner_render_plan::simulation::WorldRenderPlan;

/// Leaves out the quads whose texture the plan does not hold, which the scene cannot draw, and
/// remembers which textures it has named. A player's textures take identities the renderer holds
/// no texels for yet, so a sprite showing one would otherwise fail the whole frame.
#[derive(Debug, Default)]
pub struct UnresolvedTextures {
    named: BTreeSet<u64>,
}

impl UnresolvedTextures {
    /// Removes the quads of `plan` that name a texture it does not hold. Returns the identities of
    /// those textures that no earlier call returned, in increasing order.
    pub fn drop_unresolved(&mut self, plan: &mut WorldRenderPlan) -> Vec<u64> {
        let textures = &plan.textures;
        let mut missing = BTreeSet::new();
        plan.quads.retain(|quad| {
            let Some(id) = quad.texture.filter(|id| !textures.contains_key(id)) else {
                return true;
            };
            missing.insert(id);
            false
        });
        missing
            .into_iter()
            .filter(|&id| self.named.insert(id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::UnresolvedTextures;
    use std::collections::BTreeMap;
    use worldrunner_render_plan::{
        BuiltinMaterial, SceneTexture, TexelFormat, TextureColorSpace,
        simulation::{SpriteQuad, WorldRenderPlan},
    };

    fn quad(texture: Option<u64>) -> SpriteQuad {
        SpriteQuad {
            material: BuiltinMaterial::SpriteUnlit,
            overlay: false,
            corners: [[0.; 3]; 4],
            uv_rect: [0., 0., 1., 1.],
            color: [1.; 4],
            texture,
            sorting_layer: 0,
            sorting_order: 0,
        }
    }

    fn plan(textures: &[Option<u64>]) -> WorldRenderPlan {
        let one = SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Srgb,
            width: 1,
            height: 1,
            mips: 1,
            bytes: Box::new([255; 4]),
        };
        WorldRenderPlan {
            camera: None,
            camera_state: None,
            clear: [0.; 4],
            quads: textures.iter().map(|&t| quad(t)).collect(),
            textures: BTreeMap::from([(1, one)]),
        }
    }

    #[test]
    fn leaves_out_sprites_of_unknown_textures_and_reports_each_texture_once() {
        let shown = [
            Some(1),
            Some(1 << 40),
            None,
            Some(1 << 40),
            Some((1 << 40) + 1),
        ];
        let mut first = plan(&shown);
        let mut unresolved = UnresolvedTextures::default();
        assert_eq!(
            unresolved.drop_unresolved(&mut first),
            [1 << 40, (1 << 40) + 1]
        );
        assert_eq!(
            first.quads.iter().map(|q| q.texture).collect::<Vec<_>>(),
            [Some(1), None],
            "quads of known textures and untextured quads stay"
        );
        let mut next = plan(&shown);
        assert!(
            unresolved.drop_unresolved(&mut next).is_empty(),
            "a texture already reported is not reported again"
        );
        assert_eq!(next.quads.len(), 2, "its sprites are still left out");
    }

    #[test]
    fn keeps_a_plan_in_which_every_texture_is_known() {
        let mut known = plan(&[Some(1), Some(1), None]);
        assert!(
            UnresolvedTextures::default()
                .drop_unresolved(&mut known)
                .is_empty()
        );
        assert_eq!(known.quads.len(), 3);
    }
}
