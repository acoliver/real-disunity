//! A plan's textures as scene textures, under the identities the managed loader gives them.
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace, TextureFilter, TextureSampler, TextureWrap,
    simulation::SpriteTextures,
};
use worldrunner_unity_import::{Filter, PlanTexture, Project, Wrap, import_texture};

use crate::PlayError;

/// Decodes texture `i` of the plan into identity `first + i`, with its import sampling.
pub fn scene_textures(
    project: &Project,
    textures: &[PlanTexture],
    first: u64,
) -> Result<SpriteTextures, PlayError> {
    let mut result = SpriteTextures::default();
    for (identity, planned) in (first..).zip(textures) {
        let guid = planned
            .guid
            .parse()
            .map_err(|_| PlayError::Invalid(format!("{}: invalid GUID", planned.path)))?;
        let image = import_texture(project, guid)?.image().clone();
        let texels = SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: if planned.srgb {
                TextureColorSpace::Srgb
            } else {
                TextureColorSpace::Linear
            },
            width: image.width,
            height: image.height,
            mips: 1,
            bytes: image.rgba8.into_boxed_slice(),
        };
        let sampler = TextureSampler {
            filter: match planned.filter {
                Filter::Point => TextureFilter::Point,
                Filter::Bilinear => TextureFilter::Bilinear,
                Filter::Trilinear => TextureFilter::Trilinear,
            },
            wrap_u: wrap(planned, planned.wrap[0])?,
            wrap_v: wrap(planned, planned.wrap[1])?,
        };
        result
            .textures
            .insert(identity, texels.with_sampler(sampler));
    }
    Ok(result)
}

fn wrap(texture: &PlanTexture, wrap: Wrap) -> Result<TextureWrap, PlayError> {
    match wrap {
        Wrap::Repeat => Ok(TextureWrap::Repeat),
        Wrap::Clamp => Ok(TextureWrap::Clamp),
        Wrap::Mirror => Ok(TextureWrap::Mirror),
        Wrap::MirrorOnce => Err(PlayError::Invalid(format!(
            "{}: mirror-once wrapping is not supported",
            texture.path
        ))),
    }
}
