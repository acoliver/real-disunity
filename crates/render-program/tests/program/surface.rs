//! A color and depth attachment pair the test owns, for draws recorded into a pass the test
//! opened itself.

pub const SIZE: u32 = 64;
const ROW: u32 = SIZE * 4;

pub struct Surface {
    color: wgpu::Texture,
    depth: wgpu::Texture,
}

fn texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test surface"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

impl Surface {
    /// A 64x64 target of `color_format` and `Depth32Float`.
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        Self {
            color: texture(
                device,
                color_format,
                attachment | wgpu::TextureUsages::COPY_SRC,
            ),
            depth: texture(device, wgpu::TextureFormat::Depth32Float, attachment),
        }
    }

    /// Opens a pass that clears the color to opaque black and the depth to 1, and stores both.
    pub fn open<'e>(&self, encoder: &'e mut wgpu::CommandEncoder) -> wgpu::RenderPass<'e> {
        let color = self
            .color
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test surface pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }

    /// Records a copy of the color texture into a mappable buffer of tightly packed rows.
    pub fn copy_color(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> wgpu::Buffer {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("test surface readback"),
            size: u64::from(ROW * SIZE),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            self.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW),
                    rows_per_image: Some(SIZE),
                },
            },
            wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
        );
        buffer
    }
}

/// The bytes of a buffer from [`Surface::copy_color`], once the work that fills it has run.
pub fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
    buffer.slice(..).map_async(wgpu::MapMode::Read, |result| {
        result.expect("the readback maps");
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .expect("the queue drains");
    let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    bytes
}

/// The RGBA bytes of the pixel at column `x` and row `y` of a 4-byte-per-pixel readback.
pub fn pixel(bytes: &[u8], x: u32, y: u32) -> [u8; 4] {
    let at = (y * SIZE + x) as usize * 4;
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}
