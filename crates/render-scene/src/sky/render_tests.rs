use super::*;
use crate::{
    frame::FrameUniform,
    view::{CameraPose, SceneView, Viewport},
};
use worldrunner_render_plan::SceneCamera;

fn settings() -> SceneAtmosphere {
    SceneAtmosphere {
        horizon_radiance_micro: [2_000_000; 3],
        zenith_radiance_micro: [2_000_000; 3],
        gradient_exponent_micro: 1_000_000,
        sun_angular_radius_millidegrees: 0,
        fog: None,
        discs: vec![],
        meshes: vec![],
        background_distance_millimeters: 1_000_000,
    }
}
#[test]
fn hdr_sky_has_one_exposure_and_does_not_write_depth() {
    let (color, depth) = render(&settings(), 2.0, 1.0);
    assert_eq!(&color[..8], &[0, 0x44, 0, 0x44, 0, 0x44, 0, 0x3c]);
    assert_eq!(depth.to_bits(), 1.0f32.to_bits());
    let (color, depth) = render(&settings(), 2.0, 0.5);
    assert_eq!(&color[..8], &[0, 0, 0, 0, 0, 0, 0, 0x3c]);
    assert_eq!(depth.to_bits(), 0.5f32.to_bits());
}
#[test]
fn solar_disc_uses_frame_sun_direction() {
    let mut settings = settings();
    settings.sun_angular_radius_millidegrees = 45_000;
    let (color, _) = render(&settings, 1.0, 1.0);
    assert_eq!(&color[..8], &[0, 0x48, 0, 0x48, 0, 0x48, 0, 0x3c]);
}
fn test_frame(exposure: f32) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, 0, 0],
            target_millimeters: [0, 1000, 0],
        },
        SceneCamera {
            vertical_fov_millidegrees: 30_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
        Viewport {
            width: 1,
            height: 1,
        },
    )
    .unwrap();
    let mut frame = FrameUniform::unlit(&view);
    frame.sun_direction = Vec3::Y;
    frame.sun_irradiance = Vec3::splat(8.0);
    frame.exposure = exposure;
    frame
}
struct Targets {
    color: wgpu::Texture,
    depth: wgpu::Texture,
    readback: wgpu::Buffer,
}
impl Targets {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            color: target(device, wgpu::TextureFormat::Rgba16Float),
            depth: target(device, wgpu::TextureFormat::Depth32Float),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 512,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        sky: &Sky,
        frame: &wgpu::BindGroup,
        clear_depth: f32,
    ) {
        let color_view = self
            .color
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth_view = self
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky acceptance"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_depth),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        sky.draw(&mut pass, frame);
    }
    fn copy(&self, encoder: &mut wgpu::CommandEncoder) {
        for (texture, offset, aspect) in [
            (&self.color, 0, wgpu::TextureAspect::All),
            (&self.depth, 256, wgpu::TextureAspect::DepthOnly),
        ] {
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &self.readback,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(1),
                    },
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    fn read(&self, device: &wgpu::Device) -> (Vec<u8>, f32) {
        let (send, recv) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        recv.recv().unwrap().unwrap();
        let data = self.readback.slice(..).get_mapped_range().unwrap();
        let color = data[..8].to_vec();
        let depth = f32::from_le_bytes(data[256..260].try_into().unwrap());
        drop(data);
        self.readback.unmap();
        (color, depth)
    }
}
fn render(settings: &SceneAtmosphere, exposure: f32, clear_depth: f32) -> (Vec<u8>, f32) {
    let mut frame = test_frame(exposure);
    frame.height_fog = settings.fog.map(super::height_fog);
    render_frame(settings, &frame, clear_depth)
}
fn render_frame(
    settings: &SceneAtmosphere,
    camera: &FrameUniform,
    clear_depth: f32,
) -> (Vec<u8>, f32) {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let device = context.device();
    let frame_layout = crate::upload::frame_bind_group_layout(device);
    let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("test frame"),
        contents: &camera.to_bytes(),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let frame = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &frame_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let white = crate::upload::placeholder_texture(device, context.queue());
    let sky = Sky::new(
        device,
        &frame_layout,
        (wgpu::TextureFormat::Rgba16Float, 1),
        settings,
        None,
        (
            &[],
            &white.create_view(&wgpu::TextureViewDescriptor::default()),
        ),
    );
    let targets = Targets::new(device);
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            targets.draw(encoder, &sky, &frame, clear_depth);
            targets.copy(encoder);
            Ok::<_, std::convert::Infallible>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    targets.read(device)
}
fn target(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

#[test]
fn celestial_discs_cover_the_background_only_in_their_angular_extent() {
    use worldrunner_render_plan::{SceneCelestialDisc, SceneDirection};
    let mut sky = settings();
    sky.discs.push(SceneCelestialDisc {
        direction: SceneDirection::new([0, 1_000_000, 0]).unwrap(),
        angular_radius_millidegrees: 45_000,
        rotation_millidegrees: 90_000,
        texture: None,
        radiance_micro: [8_000_000; 3],
    });
    assert_eq!(
        &render(&sky, 1.0, 1.0).0[..8],
        &[0, 0x48, 0, 0x48, 0, 0x48, 0, 0x3c]
    );
    sky.discs[0].direction = SceneDirection::new([0, -1_000_000, 0]).unwrap();
    assert_eq!(
        &render(&sky, 1.0, 1.0).0[..8],
        &[0, 0x40, 0, 0x40, 0, 0x40, 0, 0x3c]
    );
}
#[test]
fn fog_is_linear_hdr_and_uses_the_shared_exposure() {
    let mut sky = settings();
    sky.fog = Some(worldrunner_render_plan::SceneHeightFog {
        density_micro_per_meter: 1_000_000,
        falloff_micro_per_meter: 0,
        base_height_millimeters: 0,
        start_distance_millimeters: 0,
        radiance_micro: [4_000_000; 3],
    });
    assert_eq!(
        &render(&sky, 2.0, 1.0).0[..8],
        &[0, 0x48, 0, 0x48, 0, 0x48, 0, 0x3c]
    );
}

#[test]
fn orthographic_sky_covers_an_off_center_camera_extent() {
    let mut camera = test_frame(1.0);
    camera.view_projection =
        glam::camera::rh::proj::directx::orthographic(5.0, 15.0, 5.0, 15.0, 0.1, 1000.0)
            * glam::camera::rh::view::look_at_mat4(Vec3::ZERO, Vec3::Y, Vec3::Z);
    assert_eq!(
        &render_frame(&settings(), &camera, 1.0).0[..8],
        &[0, 0x40, 0, 0x40, 0, 0x40, 0, 0x3c]
    );
}
