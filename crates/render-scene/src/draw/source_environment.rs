//! Source uploads and filtering share the scene's device and submission authority.
use super::DrawError;
use crate::sky::preparation::{self, EnvironmentPreparation};
use worldrunner_render_gpu::{
    configuration::{AllocationBudget, AllocationSet},
    submission::SubmissionAuthority,
};
use worldrunner_render_plan::{RenderScene, SceneTexture};

/// Peak preparation inventory, including source storage, aligned staging and outputs.
/// Prefiltered scenes add no preparation allocations.
pub fn allocations(scene: &RenderScene) -> Result<AllocationSet, DrawError> {
    let mut set = transient_allocations(scene)?;
    for texture in scene.resources().textures() {
        if let SceneTexture::SpecularCube(cube) = texture.payload()
            && cube.preparation().is_some()
        {
            cube_allocation(&mut set, cube);
        }
    }
    Ok(set)
}

/// Pending-generation source upload, staging and disposable integration storage.
pub fn transient_allocations(scene: &RenderScene) -> Result<AllocationSet, DrawError> {
    let mut set = AllocationSet::default();
    for texture in scene.resources().textures() {
        let SceneTexture::SpecularCube(cube) = texture.payload() else {
            continue;
        };
        let Some(request) = cube.preparation() else {
            continue;
        };
        EnvironmentPreparation::from(request).allocations()?;
        set.texture(
            "integrated BRDF",
            [1, 1, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            1,
        );
        set.buffer("environment job uniforms", 32, request.mip_count + 1);
        set.texture(
            "environment source",
            [cube.size(), cube.size(), 6],
            crate::texture::cube_format(cube),
            [cube.mip_count(), 1],
            1,
        );
        let format = crate::texture::cube_format(cube);
        let (bw, bh) = format.block_dimensions();
        let block_bytes = format.block_copy_size(None).expect("cube block size");
        let staging: u64 = (0..cube.mip_count())
            .map(|mip| {
                let side = cube.size() >> mip;
                let row = (side.div_ceil(bw) * block_bytes).div_ceil(256) * 256;
                u64::from(row) * u64::from(side.div_ceil(bh)) * 6
            })
            .sum();
        set.buffer("environment source staging", staging, 1);
        set.cpu("environment source staging", staging);
    }
    Ok(set)
}

/// Cube storage follows the scene texture table, including prefiltered resources.
pub fn resident_allocations(scene: &RenderScene) -> AllocationSet {
    let mut set = AllocationSet::default();
    for texture in scene.resources().textures() {
        if let SceneTexture::SpecularCube(cube) = texture.payload() {
            cube_allocation(&mut set, cube);
        }
    }
    set
}

fn cube_allocation(set: &mut AllocationSet, cube: &worldrunner_render_plan::SceneSpecularCube) {
    let (side, mips, format) = cube.preparation().map_or(
        (
            cube.size(),
            cube.mip_count(),
            crate::texture::cube_format(cube),
        ),
        |request| {
            (
                request.cube_size,
                request.mip_count,
                wgpu::TextureFormat::Rgba16Float,
            )
        },
    );
    set.texture("scene specular cube", [side, side, 6], format, [mips, 1], 1);
}

pub(super) fn filter(
    device: &wgpu::Device,
    authority: &SubmissionAuthority,
    source: &wgpu::Texture,
    request: worldrunner_render_plan::SceneSpecularPreparation,
) -> Result<wgpu::Texture, DrawError> {
    let settings = EnvironmentPreparation::from(request);
    let required = settings
        .allocations()?
        .validate(&device.limits())
        .map_err(|_| preparation::PreparationError::Budget)?;
    let view = source.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::Cube),
        ..Default::default()
    });
    let mut maps = None;
    let frame = authority
        .prepare(device, |encoder, _| {
            maps = Some(preparation::encode(
                device,
                encoder,
                &view,
                settings,
                AllocationBudget(required),
            )?);
            Ok::<_, DrawError>(())
        })
        .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
    frame.submit()?;
    // Submission order makes the result usable without a CPU wait or readback.
    Ok(maps
        .expect("recorded preparation")
        .views()?
        .0
        .texture()
        .clone())
}
