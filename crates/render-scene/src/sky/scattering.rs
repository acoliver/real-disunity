//! Lookup preparation shares the environment transaction and allocation inventory.
use super::preparation::PreparationError;
use glam::Vec3;
use wgpu::util::DeviceExt;
use worldrunner_render_gpu::{
    configuration::{AllocationBudget, AllocationSet},
    submission::Publications,
};
use worldrunner_render_lighting::atmosphere::scattering::ElevationColumnScattering;
use worldrunner_render_plan::{SceneScattering, SceneScatteringFormat, SceneScatteringModel};

#[derive(Debug)]
pub(super) struct Lookup {
    pub view: wgpu::TextureView,
    settings: SceneScattering,
}
impl Lookup {
    pub fn new(device: &wgpu::Device, settings: SceneScattering) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atmospheric column integrals"),
            size: wgpu::Extent3d {
                width: 1,
                height: settings.lookup_rows,
                depth_or_array_layers: 3,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: format(settings.format),
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            }),
            settings,
        }
    }
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        observer: (Vec3, f32),
        budget: AllocationBudget,
    ) -> Result<(), PreparationError> {
        let _transaction = Publications::for_encoder(encoder)?;
        let required = allocations(self.settings)
            .validate(&device.limits())
            .map_err(|_| PreparationError::Budget)?;
        if required > budget.0 {
            return Err(PreparationError::Budget);
        }
        let model = model(self.settings);
        let (sun, camera_altitude) = observer;
        let altitude = observer_altitude(self.settings, camera_altitude);
        if altitude < 0.0
            || altitude >= model.outer_radius - model.planet_radius
            || !altitude.is_finite()
            || !sun.is_finite()
            || (sun.length_squared() - 1.0).abs() > 0.001
        {
            return Err(PreparationError::Settings);
        }
        let bytes = pixels(self.settings, sun.z.clamp(-1.0, 1.0).asin(), altitude);
        let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("atmospheric column upload"),
            contents: &bytes,
            usage: wgpu::BufferUsages::COPY_SRC,
        });
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(self.settings.lookup_rows),
                },
            },
            self.view.texture().as_image_copy(),
            self.view.texture().size(),
        );
        Ok(())
    }
}
pub(super) fn format(format: SceneScatteringFormat) -> wgpu::TextureFormat {
    match format {
        SceneScatteringFormat::Float16 => wgpu::TextureFormat::Rgba16Float,
        SceneScatteringFormat::Float32 => wgpu::TextureFormat::Rgba32Float,
    }
}
pub(super) fn allocations(settings: SceneScattering) -> AllocationSet {
    let mut set = AllocationSet::default();
    set.texture(
        "atmospheric column integrals",
        [1, settings.lookup_rows, 3],
        format(settings.format),
        [1, 1],
        1,
    );
    set.buffer(
        "atmospheric column staging",
        u64::from(settings.lookup_rows) * 256 * 3,
        1,
    );
    set.cpu(
        "atmospheric column pixels",
        u64::from(settings.lookup_rows) * 256 * 3,
    );
    set
}
#[expect(
    clippy::cast_precision_loss,
    reason = "bounded integer atmosphere units"
)]
pub(super) fn model(s: SceneScattering) -> ElevationColumnScattering {
    let SceneScatteringModel::ElevationColumn20 = s.model;
    ElevationColumnScattering {
        planet_radius: s.planet_radius_meters as f32,
        outer_radius: s.atmosphere_radius_meters as f32,
        molecular: Vec3::from_array(
            s.molecular_scattering_pico_per_meter
                .map(|v| v as f32 * 1e-12),
        ),
        aerosol: Vec3::from_array(
            s.aerosol_scattering_pico_per_meter
                .map(|v| v as f32 * 1e-12),
        ),
        scale_heights: [
            s.molecular_height_meters as f32,
            s.aerosol_height_meters as f32,
        ],
        anisotropy: s.anisotropy_micro as f32 / 1e6,
        solar_radiance: (s.solar_radiance_micro as f32 / 1e6) * (s.sky_exposure_micro as f32 / 1e6),
        solar_diameter: s.solar_diameter_microradians as f32 / 1e6,
        peak_radiance: s.peak_radiance_micro as f32 / 1e6,
    }
}
#[expect(clippy::cast_precision_loss, reason = "bounded lookup indices")]
pub(super) fn pixels(settings: SceneScattering, sun: f32, altitude: f32) -> Vec<u8> {
    let mut bytes = vec![0; settings.lookup_rows as usize * 256 * 3];
    let model = model(settings);
    for row in 0..settings.lookup_rows {
        let elevation =
            (1.0 - (row as f32 + 0.5) / settings.lookup_rows as f32) * std::f32::consts::FRAC_PI_2;
        let sample = model.integrate(elevation, sun, altitude);
        for (layer, value) in [sample.molecular, sample.aerosol, sample.transmission]
            .into_iter()
            .enumerate()
        {
            let offset = (layer * settings.lookup_rows as usize + row as usize) * 256;
            for (channel, value) in value.extend(1.0).to_array().into_iter().enumerate() {
                match settings.format {
                    SceneScatteringFormat::Float16 => {
                        let bits = quantize(value);
                        bytes[offset + channel * 2..offset + channel * 2 + 2]
                            .copy_from_slice(&bits.to_le_bytes());
                    }
                    SceneScatteringFormat::Float32 => bytes
                        [offset + channel * 4..offset + channel * 4 + 4]
                        .copy_from_slice(&value.to_le_bytes()),
                }
            }
        }
    }
    bytes
}
/// Finite saturation and rounding toward zero. Relative error is below 2^-10
/// for normal binary16 values; subnormal absolute error is below 2^-24.
fn quantize(value: f32) -> u16 {
    let bounded = value.min(65_504.0);
    let rounded = half::f16::from_f32(bounded);
    rounded.to_bits() - u16::from(rounded.to_f32() > bounded)
}

pub(super) fn columns(device: &wgpu::Device, lookup: Option<&Lookup>) -> wgpu::TextureView {
    if let Some(lookup) = lookup {
        return lookup.view.clone();
    }
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("unused atmospheric columns"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 3,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
}
#[expect(clippy::cast_precision_loss, reason = "bounded lookup rows")]
pub(super) fn parameters(settings: Option<SceneScattering>, lanes: &mut [[f32; 4]]) {
    if let Some(physical) = settings {
        let model = model(physical);
        lanes[6][2] = 1.0;
        lanes[7] = model.molecular.extend(model.anisotropy).to_array();
        lanes[8] = model.aerosol.extend(model.solar_radiance).to_array();
        lanes[9] = [
            physical.lookup_rows as f32,
            model.peak_radiance,
            f32::from(u8::from(physical.compose_celestial)),
            0.0,
        ];
    }
}

#[cfg(test)]
pub(super) mod tests;

pub(super) fn observer_altitude(settings: SceneScattering, camera_altitude: f32) -> f32 {
    settings
        .observer_altitude_millimeters
        .map_or(camera_altitude, |altitude| {
            let high = u16::try_from(altitude >> 16).expect("upper half");
            let low = u16::try_from(altitude & 65535).expect("lower half");
            (f32::from(high) * 65536.0 + f32::from(low)) / 1000.0
        })
}
