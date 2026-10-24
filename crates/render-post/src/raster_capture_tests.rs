use crate::{AttachmentLoad, FrameConfig, Readback, StageId, contracts::*};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use worldrunner_render_gpu::submission::{
    SubmissionAuthority,
    capture::{CaptureError, CaptureWrites, WriteError, WriteKey},
};

struct Fixture {
    device: wgpu::Device,
    authority: SubmissionAuthority,
    registry: ResourceRegistry,
    texture: wgpu::Texture,
    key: AllocationKey,
}
impl Fixture {
    fn new() -> Self {
        let (device, queue) = crate::gpu_tests::gpu();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("capture raster target"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut registry = ResourceRegistry::default();
        let key = registry
            .import(
                &texture,
                ResourceLifetime::PersistentMutable,
                Semantic::LinearColor,
                None,
            )
            .unwrap();
        Self {
            device,
            authority: SubmissionAuthority::new(queue),
            registry,
            texture,
            key,
        }
    }
    fn clear(&self, occurrence: u32, color: wgpu::Color) -> CompiledRaster {
        let pass = RasterPass {
            id: RasterId::new(StageId::new("capture clear", 91), occurrence),
            colors: vec![Some(ColorAttachment {
                view: self
                    .registry
                    .view(
                        self.key,
                        TextureRange::single(0, 0, wgpu::TextureAspect::All),
                        occurrence + 1,
                    )
                    .unwrap(),
                before: None,
                load: AttachmentLoad::ClearColor(color),
                store: wgpu::StoreOp::Store,
                resolve: None,
            })],
            depth: None,
            uses: vec![],
            bindings: vec![],
        };
        self.registry.compile(&self.device, &[pass]).unwrap()
    }
}

fn encode_suffix(
    f: &Fixture,
    continuation: worldrunner_render_gpu::submission::capture::CaptureContinuation<()>,
    suffix: &CompiledRaster,
    owned: bool,
    published: Arc<AtomicUsize>,
) {
    let key = WriteKey::Resource {
        id: f.key.id(),
        generation: f.key.generation(),
    };
    let result = continuation.resume(
        &f.device,
        |_, _| Ok::<_, ContractError>(()),
        |encoder, edits, (), ()| {
            edits
                .stage_write(WriteKey::Resident(99), move || {
                    published.store(1, Ordering::SeqCst);
                })
                .unwrap();
            let encoded = suffix.encode(&f.registry, encoder, |_, _| {});
            if owned {
                encoded.unwrap();
            } else {
                assert_eq!(
                    encoded,
                    Err(ContractError::CaptureWrite(WriteError::Undeclared(key)))
                );
            }
            Ok(())
        },
    );
    if owned {
        result
            .unwrap()
            .submit()
            .unwrap()
            .finish(&f.device, |_, _| Ok::<_, ()>(()))
            .unwrap();
    } else {
        assert!(
            matches!(result, Err(CaptureError::Write(WriteError::Undeclared(rejected))) if rejected == key)
        );
        f.authority.drain_captures(&f.device).unwrap();
    }
}

#[test]
fn checked_raster_capture_requires_suffix_ownership_even_if_error_is_ignored() {
    for owned in [false, true] {
        let f = Fixture::new();
        let prefix = f.clear(0, wgpu::Color::GREEN);
        let suffix = f.clear(1, wgpu::Color::RED);
        assert_eq!(prefix.ids().next().unwrap().occurrence, 0);
        assert_eq!(suffix.ids().next().unwrap().occurrence, 1);
        let key = WriteKey::Resource {
            id: f.key.id(),
            generation: f.key.generation(),
        };
        let publication = WriteKey::Resident(99);
        let writes = CaptureWrites::new(
            [key],
            if owned {
                vec![key, publication]
            } else {
                vec![publication]
            },
            if owned { vec![key] } else { vec![] },
        )
        .unwrap();
        let published = Arc::new(AtomicUsize::new(0));
        let continuation = f
            .authority
            .begin_capture(&f.device, writes, (), |encoder, _, ()| {
                prefix.encode(&f.registry, encoder, |_, _| {})
            })
            .unwrap()
            .submit()
            .unwrap();
        encode_suffix(&f, continuation, &suffix, owned, published.clone());
        assert_eq!(f.authority.submitted_serial(), if owned { 2 } else { 1 });
        assert_eq!(published.load(Ordering::SeqCst), usize::from(owned));
        let mut readback = Readback::new(
            &f.device,
            FrameConfig::new([1, 1], wgpu::TextureFormat::Rgba8Unorm, 1),
            256,
        )
        .unwrap();
        let bytes = readback.read(&f.device, &f.authority, &f.texture).unwrap();
        assert_eq!(
            bytes,
            if owned {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            }
        );
    }
}

fn reject_stock_suffix(
    f: &Fixture,
    chain: &mut crate::PostChain<crate::CopyOutput>,
    encoder: &mut wgpu::CommandEncoder,
) -> WriteKey {
    let resource = crate::ResourceRef::new("capture color");
    let views = [crate::AttachmentView::new(resource, &f.texture).unwrap()];
    let mut pass = crate::PassDeclaration::new(crate::PassSlot::Opaque);
    pass.color = Some(crate::AttachmentDeclaration {
        resource,
        format: wgpu::TextureFormat::Rgba8Unorm,
        size: [1, 1],
        samples: 1,
        load: AttachmentLoad::ClearColor(wgpu::Color::RED),
        store: wgpu::StoreOp::Store,
    });
    let error = chain
        .encode_geometry(
            &f.device,
            encoder,
            &crate::FramePlan {
                passes: &[pass],
                views: &views,
                requested: &[],
            },
            &mut [],
        )
        .unwrap_err();
    let crate::FrameError::ResourceContract(ContractError::CaptureWrite(WriteError::Undeclared(
        key @ WriteKey::Resource { .. },
    ))) = error
    else {
        panic!("expected typed capture ownership error, got {error:?}");
    };
    key
}

#[test]
fn stock_geometry_capture_latches_unowned_attachment_write() {
    let f = Fixture::new();
    let config = FrameConfig::new([1, 1], wgpu::TextureFormat::Rgba8Unorm, 1);
    let mut chain = crate::PostChain::new(
        &f.device,
        config,
        crate::Resources::default(),
        crate::CopyOutput::new(&f.device, config.format),
    )
    .unwrap();
    let prefix = f.clear(0, wgpu::Color::GREEN);
    let publication = WriteKey::Resident(99);
    let writes = CaptureWrites::new(
        [WriteKey::Resource {
            id: f.key.id(),
            generation: f.key.generation(),
        }],
        [publication],
        [],
    )
    .unwrap();
    let continuation = f
        .authority
        .begin_capture(&f.device, writes, (), |encoder, _, ()| {
            prefix.encode(&f.registry, encoder, |_, _| {})
        })
        .unwrap()
        .submit()
        .unwrap();
    let published = Arc::new(AtomicUsize::new(0));
    let edited = published.clone();
    let mut rejected = None;
    let result = continuation.resume(
        &f.device,
        |_, _| Ok::<_, crate::FrameError>(()),
        |encoder, edits, (), ()| {
            edits
                .stage_write(publication, move || edited.store(1, Ordering::SeqCst))
                .unwrap();
            rejected = Some(reject_stock_suffix(&f, &mut chain, encoder));
            Ok(())
        },
    );
    assert!(
        matches!(result, Err(CaptureError::Write(WriteError::Undeclared(key))) if Some(key) == rejected)
    );
    f.authority.drain_captures(&f.device).unwrap();
    assert_eq!(f.authority.submitted_serial(), 1);
    assert_eq!(published.load(Ordering::SeqCst), 0);
    let bytes = Readback::new(&f.device, config, 256)
        .unwrap()
        .read(&f.device, &f.authority, &f.texture)
        .unwrap();
    assert_eq!(bytes, [0, 255, 0, 255]);
}
