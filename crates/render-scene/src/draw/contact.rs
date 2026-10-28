use wgpu::{Buffer, BufferUsages, Device, Queue};
use worldrunner_render_plan::{RenderScene, SceneTerrain, TerrainLodPolicy};

use super::DrawError;

pub(super) fn prepare(scene: &RenderScene) -> Result<Vec<u8>, DrawError> {
    let terrain = scene
        .resources()
        .materials()
        .iter()
        .any(|material| material.terrain_contact().is_some())
        .then(|| scene.terrain().expect("contact admission requires terrain"));
    let mut bytes = height_bytes(
        terrain,
        &wgpu::Limits {
            max_buffer_size: u64::MAX,
            max_storage_buffer_binding_size: u64::MAX,
            ..Default::default()
        },
    )?;
    super::fade::append(scene, &mut bytes);
    Ok(bytes)
}

pub(super) fn height_buffer(
    device: &Device,
    queue: &Queue,
    bytes: &[u8],
) -> Result<Buffer, DrawError> {
    validate_size(bytes.len() as u64, &device.limits())?;
    Ok(crate::upload::upload_buffer(
        device,
        queue,
        "contact heights",
        BufferUsages::STORAGE,
        bytes,
    ))
}

fn stride(terrain: &SceneTerrain) -> u32 {
    if terrain.streaming_policy().is_some() {
        return 1;
    }
    match terrain.lod_policy() {
        TerrainLodPolicy::FullResolution => 1,
        TerrainLodPolicy::UniformSteps { steps } => 1u32 << steps,
    }
}

fn height_bytes(
    terrain: Option<&SceneTerrain>,
    limits: &wgpu::Limits,
) -> Result<Vec<u8>, DrawError> {
    let Some(terrain) = terrain else {
        return Ok(vec![0; 48]);
    };
    let stride = stride(terrain);
    let columns = (terrain.columns() - 1).div_ceil(stride) + 1;
    let rows = (terrain.rows() - 1).div_ceil(stride) + 1;
    let size = u64::from(columns)
        .checked_mul(u64::from(rows))
        .and_then(|samples| samples.checked_mul(4))
        .and_then(|bytes| bytes.checked_add(32))
        .ok_or(DrawError::ContactHeightSizeOverflow)?;
    validate_size(size, limits)?;
    let mut bytes = Vec::with_capacity(usize::try_from(size).expect("admitted buffer fits host"));
    for value in [
        crate::geometry::millimeter_to_meter(terrain.origin_east_millimeters()),
        crate::geometry::millimeter_to_meter(terrain.origin_north_millimeters()),
        crate::geometry::millimeter_to_meter(i64::from(terrain.sample_spacing_millimeters())),
        dimension(terrain.columns()),
        dimension(terrain.rows()),
        dimension(stride),
        dimension(columns),
        0.0,
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    append_heights(&mut bytes, terrain, columns, rows, stride);
    Ok(bytes)
}

fn append_heights(
    bytes: &mut Vec<u8>,
    terrain: &SceneTerrain,
    columns: u32,
    rows: u32,
    stride: u32,
) {
    for row in 0..rows {
        let y = (u64::from(row) * u64::from(stride)).min(u64::from(terrain.rows() - 1));
        for column in 0..columns {
            let x = (u64::from(column) * u64::from(stride)).min(u64::from(terrain.columns() - 1));
            let index =
                usize::try_from(y * u64::from(terrain.columns()) + x).expect("source grid index");
            let height = crate::geometry::millimeter_to_meter(i64::from(
                terrain.heights_millimeters()[index],
            ));
            bytes.extend_from_slice(&height.to_le_bytes());
        }
    }
}

fn validate_size(size: u64, limits: &wgpu::Limits) -> Result<(), DrawError> {
    let limit = limits
        .max_buffer_size
        .min(limits.max_storage_buffer_binding_size);
    if size > limit {
        return Err(DrawError::ContactHeightLimit {
            required: size,
            limit,
        });
    }
    Ok(())
}

#[expect(
    clippy::cast_precision_loss,
    reason = "GPU grid dimensions use float coordinates"
)]
fn dimension(value: u32) -> f32 {
    value as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_checks_buffer_and_binding_limits_independently() {
        for limits in [
            wgpu::Limits {
                max_buffer_size: 63,
                ..Default::default()
            },
            wgpu::Limits {
                max_storage_buffer_binding_size: 63,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                validate_size(64, &limits),
                Err(DrawError::ContactHeightLimit {
                    required: 64,
                    limit: 63
                })
            ));
        }
    }
    #[test]
    fn contact_keeps_selected_samples_and_shortened_last_cells() {
        use worldrunner_render_plan::{
            MaterialHandle, SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface,
        };
        let terrain = SceneTerrain::new(SceneTerrainParts {
            columns: 6,
            rows: 4,
            origin_east_millimeters: 0,
            origin_north_millimeters: 0,
            sample_spacing_millimeters: 1000,
            heights_millimeters: (0..24).map(|index| index * 1000).collect(),
            surface_indices: vec![0; 24],
            surfaces: vec![SceneTerrainSurface::new(
                MaterialHandle::from_raw(0),
                SceneTerrainAppearance::NEUTRAL,
            )],
            lod_policy: TerrainLodPolicy::UniformSteps { steps: 2 },
        })
        .unwrap();
        let bytes = height_bytes(Some(&terrain), &wgpu::Limits::default()).unwrap();
        let heights: Vec<f32> = bytes[32..]
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(heights, [0.0, 4.0, 5.0, 18.0, 22.0, 23.0]);
        assert_eq!(bytes.len(), 56);
        let streamed = terrain.with_streaming_policy(Some(
            worldrunner_render_plan::TerrainStreamingPolicy::new(1, 9, 4, 1000).unwrap(),
        ));
        let bytes = height_bytes(Some(&streamed), &wgpu::Limits::default()).unwrap();
        let heights: Vec<_> = bytes[32..]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(heights, (0_u16..24).map(f32::from).collect::<Vec<_>>());
    }
}
