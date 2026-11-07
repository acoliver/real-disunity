//! Compute shaders that script fields and the resource container reference, as plan assets.
//!
//! A compute shader plans as its declared name, the serialized file and path id of its object and
//! its kernels in index order, each with the thread group size its compiled code states or
//! `null` when it states none. The compiled code is not planned: a host that dispatches reads
//! it from that object. A compute shader that does not decode, or whose kernels the reader does
//! not understand, plans as an error entry and a reference to it binds `null`.

use serde::Serialize;
use worldrunner_unity_serialized::{compute_shader::ComputeShader, object::PPtr};

use crate::{
    plan::PlanAsset,
    player_files::PlayerFiles,
    player_resources::{ObjectKey, ResourceAsset},
    player_shader_refs::file_name,
};

/// One kernel of a planned compute shader. `thread_group_size` is `null` when the shader's data
/// states none, and the host then states it when the script first needs it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanKernel {
    pub name: String,
    pub thread_group_size: Option<[u32; 3]>,
}

/// What the plan holds of a compute shader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ComputeDecl {
    name: String,
    /// The serialized file the object lives in, relative to the player's data directory with `/`
    /// separators, and the object's path id there.
    file: String,
    path_id: i64,
    kernels: Vec<PlanKernel>,
}

/// Decodes the compute shader object `key`, or says why it does not.
pub(crate) fn decode_compute_shader(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<ComputeDecl, String> {
    let (file, path_id) = key;
    let shader: ComputeShader = files
        .open_path(file)
        .and_then(|open| open.decode(*path_id))
        .map_err(|error| error.to_string())?;
    if shader.name.is_empty() {
        return Err(format!(
            "compute shader object {path_id} of {} declares no name",
            file.display()
        ));
    }
    let kernels = shader.kernels();
    Ok(ComputeDecl {
        name: shader.name.clone(),
        file: file_name(&files.data_dir, file),
        path_id: *path_id,
        kernels: kernels
            .iter()
            .map(|kernel| PlanKernel {
                name: kernel.name.clone(),
                thread_group_size: kernel.thread_group_size(),
            })
            .collect(),
    })
}

impl ComputeDecl {
    pub(crate) fn into_asset(self) -> PlanAsset {
        PlanAsset::ComputeShader {
            name: self.name,
            file: self.file,
            path_id: self.path_id,
            kernels: self.kernels,
        }
    }
}

/// Why a reference to a compute shader that did not plan is left out.
pub(crate) fn unplanned_reference(pptr: PPtr, cause: &str) -> String {
    format!(
        "references compute shader object {} of file {}, which did not plan: {cause}",
        pptr.path_id, pptr.file_id
    )
}

/// The plan asset for the compute shader `key` as decoded, or an error entry naming the object
/// and the cause. The error is also added to `differences`.
pub(crate) fn plan_compute_shader(
    key: &ObjectKey,
    decoded: Result<ComputeDecl, String>,
    differences: &mut Vec<String>,
) -> ResourceAsset {
    match decoded {
        Ok(decl) => ResourceAsset::ComputeShader {
            name: decl.name,
            file: decl.file,
            path_id: decl.path_id,
            kernels: decl.kernels,
        },
        Err(cause) => {
            let (file, path_id) = key;
            let error = format!(
                "compute shader object {path_id} of {}: {cause}",
                file.display()
            );
            differences.push(format!("compute shader did not plan: {error}"));
            ResourceAsset::UnplannedComputeShader { error }
        }
    }
}
