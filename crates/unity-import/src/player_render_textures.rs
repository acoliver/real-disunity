//! Render textures that the resource container, materials and script fields reference, as plan
//! assets.
//!
//! A render texture plans once, whoever references it: its size, formats, sample count, mip
//! settings and sampler. The engine has no GPU surface to draw into yet, so the plan carries what
//! scripts read back. A render texture that does not decode is an error entry, and a reference
//! to it binds `null`.

use worldrunner_unity_serialized::{object::PPtr, render_texture::RenderTexture};

use crate::{
    player_files::PlayerFiles,
    player_resources::{ObjectKey, ResourceAsset},
};

/// Decodes the render texture object `key`, or says why it does not.
pub(crate) fn decode_render_texture(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<RenderTexture, String> {
    let (file, path_id) = key;
    files
        .open_path(file)
        .and_then(|open| open.decode(*path_id))
        .map_err(|error| error.to_string())
}

/// Why a reference to a render texture that did not plan is left out.
pub(crate) fn unplanned_reference(pptr: PPtr, cause: &str) -> String {
    format!(
        "references render texture object {} of file {}, which did not plan: {cause}",
        pptr.path_id, pptr.file_id
    )
}

/// The plan asset for the render texture `key` as decoded, or an error entry naming the object
/// and the cause. The error is also added to `differences`.
pub(crate) fn plan_render_texture(
    key: &ObjectKey,
    decoded: Result<RenderTexture, String>,
    differences: &mut Vec<String>,
) -> ResourceAsset {
    match decoded {
        Ok(texture) => ResourceAsset::RenderTexture {
            name: texture.name,
            width: texture.width,
            height: texture.height,
            depth_format: texture.depth_stencil_format,
            color_format: texture.color_format,
            anti_aliasing: texture.anti_aliasing,
            mip_maps: texture.mip_map,
            generate_mips: texture.generate_mips,
            dimension: texture.dimension,
            filter_mode: texture.filter_mode,
            aniso_level: texture.aniso,
            wrap_u: texture.wrap[0],
            wrap_v: texture.wrap[1],
            wrap_w: texture.wrap[2],
        },
        Err(cause) => {
            let (file, path_id) = key;
            let error = format!(
                "render texture object {path_id} of {}: {cause}",
                file.display()
            );
            differences.push(format!("render texture did not plan: {error}"));
            ResourceAsset::UnplannedRenderTexture { error }
        }
    }
}
