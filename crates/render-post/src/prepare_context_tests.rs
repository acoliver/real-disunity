use crate::{BufferReadback, PrepareContext};

fn buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("region copy fixture"),
        size,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bytes(device: &wgpu::Device, queue: &wgpu::Queue, source: &wgpu::Buffer) -> Vec<u8> {
    let mut capture = BufferReadback::new(device, source.size(), 4096).unwrap();
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    capture.encode(&mut encoder, source).unwrap();
    let submission = queue.submit([encoder.finish()]);
    capture.read_encoded(device, Some(submission)).unwrap()
}

#[test]
fn a_region_copy_reads_bytes_uploaded_earlier_in_the_same_context() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let source = buffer(&device, 32);
    let destination = buffer(&device, 32);
    let pattern: Vec<u8> = (0..32).collect();
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.upload_buffer(&destination, 0, &[0xEE; 32]);
            context.upload_buffer(&source, 0, &pattern);
            context.copy_buffer_region((&source, 8), (&destination, 20), 12);
            context.copy_buffer_region((&source, 0), (&destination, 0), 4);
            context.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let mut expected = vec![0xEE; 32];
    expected[20..32].copy_from_slice(&pattern[8..20]);
    expected[..4].copy_from_slice(&pattern[..4]);
    assert_eq!(bytes(&device, &queue, &destination), expected);
}

#[test]
fn a_cancelled_context_leaves_the_destination_unchanged() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let source = buffer(&device, 16);
    let destination = buffer(&device, 16);
    queue.write_buffer(&source, 0, &[7; 16]);
    queue.write_buffer(&destination, 0, &[1; 16]);
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.copy_buffer_region((&source, 0), (&destination, 0), 16);
            context.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap();
    prepared.cancel();
    assert_eq!(bytes(&device, &queue, &destination), vec![1; 16]);
}

const DOUBLE: &str = "
@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<storage, read_write> destination: array<u32>;
@compute @workgroup_size(4)
fn double(@builtin(global_invocation_id) id: vec3<u32>) {
    destination[id.x] = source[id.x] * 2u;
}
";

fn storage(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dispatch fixture"),
        size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[test]
fn a_dispatch_reads_earlier_uploads_and_later_copies_read_its_output() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("double"),
        source: wgpu::ShaderSource::Wgsl(DOUBLE.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("double"),
        layout: None,
        module: &module,
        entry_point: Some("double"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });
    let (source, doubled, copied) = (
        storage(&device, 16),
        storage(&device, 16),
        buffer(&device, 16),
    );
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("double"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: source.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: doubled.as_entire_binding(),
            },
        ],
    });
    let words = [3_u32, 5, 0x4000_0000, 11];
    let pattern: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.upload_buffer(&source, 0, &pattern);
            context.dispatch("double", &pipeline, &group, [1, 1, 1]);
            context.copy_buffer_region((&doubled, 0), (&copied, 0), 16);
            context.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    let expected: Vec<u8> = words
        .iter()
        .flat_map(|word| (word * 2).to_le_bytes())
        .collect();
    assert_eq!(bytes(&device, &queue, &copied), expected);
}

#[test]
fn steady_frames_reuse_upload_staging_and_land_every_upload_on_the_gpu() {
    use crate::upload_staging_tests::{destination, expected, frames, read};
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let target = destination(&device, 40 * 64);
    // Each frame stages 2,560 bytes, one chunk. A submitted chunk is free again only once its
    // mapping callback has run, which depends on when the device is next polled, so with two
    // frames in flight while the next records, whether and when a second or third chunk is
    // created is timing; more than three never is.
    let created = frames(&device, &authority, &target, (32, 40, 3));
    assert_eq!(read(&device, &queue, &target), expected(32, 40));
    assert!(created[31] <= 3, "{created:?}");
    // Once the device is idle every chunk's mapping has completed, so frames that each wait
    // for their own submission reuse a free chunk and never create one.
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let settled = frames(&device, &authority, &target, (16, 40, 1));
    assert_eq!(read(&device, &queue, &target), expected(16, 40));
    assert!(
        settled.iter().all(|&count| count == created[31]),
        "staging chunks created after each frame: in flight {created:?}, settled {settled:?}"
    );
    // No chunk was dropped for a failed mapping and replaced.
    assert_eq!(authority.upload_staging().chunks, created[31]);
}

#[test]
fn a_context_reports_the_declaration_of_each_entry_the_frame_opens() {
    use crate::{FrameError, GamePlace, PassDeclaration, PassEntry, PassSlot};
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let opaque = PassDeclaration::new(PassSlot::Opaque);
    let game = PassDeclaration::nth(PassSlot::Game(GamePlace::BeforeOpaque), 2);
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            assert!(
                context.passes().is_empty(),
                "nothing is open before a frame plans"
            );
            context.set_passes(&[game.clone(), opaque.clone()]);
            assert_eq!(context.passes(), [game.clone(), opaque.clone()]);
            assert_eq!(context.pass(game.entry()), Ok(&game));
            let absent = PassEntry::nth(PassSlot::Opaque, 1);
            assert_eq!(context.pass(absent), Err(FrameError::MissingPass(absent)));
            assert_eq!(
                game.entry().to_string(),
                "Game(BeforeOpaque)#2",
                "an entry names its place and occurrence"
            );
            context.finish();
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.cancel();
}

/// Why test commands refuse, as their owner's own error type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
enum Refusal {
    #[error("the owner refused to retain the commands")]
    Retain,
    #[error("the owner refused to record the commands")]
    Record,
}

/// Prepared commands that copy one buffer into another, report `writes` as the frame textures
/// they write, and refuse to be retained or recorded when `refuse` says so.
struct CopyCommands {
    source: wgpu::Buffer,
    destination: wgpu::Buffer,
    writes: Vec<crate::PreparedWrite>,
    refuse: Option<Refusal>,
}

impl CopyCommands {
    fn new(source: &wgpu::Buffer, destination: &wgpu::Buffer, refuse: Option<Refusal>) -> Self {
        Self {
            source: source.clone(),
            destination: destination.clone(),
            writes: Vec::new(),
            refuse,
        }
    }
}

impl crate::PreparedCommands for CopyCommands {
    fn writes(&self) -> Vec<crate::PreparedWrite> {
        self.writes.clone()
    }

    fn retain(&self, _encoder: &wgpu::CommandEncoder) -> Result<(), crate::PreparedRefusal> {
        match self.refuse {
            Some(Refusal::Retain) => Err(crate::PreparedRefusal::new(Refusal::Retain)),
            _ => Ok(()),
        }
    }

    fn record(&self, encoder: &mut wgpu::CommandEncoder) -> Result<(), crate::PreparedRefusal> {
        if self.refuse == Some(Refusal::Record) {
            return Err(crate::PreparedRefusal::new(Refusal::Record));
        }
        encoder.copy_buffer_to_buffer(&self.source, 0, &self.destination, 0, self.source.size());
        Ok(())
    }
}

/// Whether `error` is the owner's `refusal` of the commands staged as `label`, kept typed.
fn refused_as(error: &crate::FrameError, label: &str, refusal: Refusal) -> bool {
    matches!(
        error,
        crate::FrameError::PreparedRefused { label: found, refusal: kept }
            if *found == label && kept.downcast_ref::<Refusal>() == Some(&refusal)
    )
}

#[test]
fn a_prepared_operation_records_after_the_uploads_staged_before_it() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let (source, destination) = (buffer(&device, 32), buffer(&device, 32));
    let pattern: Vec<u8> = (0..32).collect();
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.open_preparation();
            context.upload_buffer(&source, 0, &pattern);
            let copy = CopyCommands::new(&source, &destination, None);
            context.record_prepared("copy", crate::PreparedAccess::default(), Box::new(copy))?;
            context.finish_preparation()?;
            Ok::<_, crate::FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    assert_eq!(bytes(&device, &queue, &destination), pattern);
}

#[test]
fn a_refused_retention_and_a_context_outside_preparation_stage_nothing() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let (source, destination) = (buffer(&device, 16), buffer(&device, 16));
    queue.write_buffer(&source, 0, &[7; 16]);
    queue.write_buffer(&destination, 0, &[1; 16]);
    let copy = |refuse| Box::new(CopyCommands::new(&source, &destination, refuse));
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.open_preparation();
            let refused = context
                .record_prepared(
                    "copy",
                    crate::PreparedAccess::default(),
                    copy(Some(Refusal::Retain)),
                )
                .unwrap_err();
            assert!(refused_as(&refused, "copy", Refusal::Retain), "{refused:?}");
            context.finish_preparation()?;
            let mut after = PrepareContext::new(&device, encoder)?;
            assert_eq!(
                after.record_prepared("copy", crate::PreparedAccess::default(), copy(None)),
                Err(crate::FrameError::PreparedOutsidePreparation)
            );
            after.finish();
            Ok::<_, crate::FrameError>(())
        })
        .unwrap();
    prepared.submit().unwrap();
    assert_eq!(bytes(&device, &queue, &destination), vec![1; 16]);
}

#[test]
fn a_refused_recording_reaches_the_frame_as_the_owners_error() {
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let (source, destination) = (buffer(&device, 16), buffer(&device, 16));
    let refused = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.open_preparation();
            let copy = CopyCommands::new(&source, &destination, Some(Refusal::Record));
            context.record_prepared("copy", crate::PreparedAccess::default(), Box::new(copy))?;
            context.finish_preparation()?;
            Ok::<_, crate::FrameError>(())
        })
        .expect_err("the frame was prepared");
    let worldrunner_render_gpu::submission::PrepareError::Preparation(error) = refused else {
        panic!("expected the preparation's refusal, found {refused:?}");
    };
    assert!(refused_as(&error, "copy", Refusal::Record), "{error:?}");
    assert_eq!(
        error.to_string(),
        "prepared commands copy were refused: the owner refused to record the commands"
    );
}

/// Two layers of one texture, as the frame names them.
const LAYERS: [crate::ResourceRef; 2] = [
    crate::ResourceRef {
        name: "blocks",
        version: 0,
        mip: 0,
        layer: 0,
    },
    crate::ResourceRef {
        name: "blocks",
        version: 0,
        mip: 0,
        layer: 1,
    },
];

fn layered(device: &wgpu::Device, label: &str) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 2,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// The access of commands that read nothing and write `writes`.
fn declared(writes: &[crate::ResourceRef]) -> crate::PreparedAccess<'_> {
    crate::PreparedAccess { reads: &[], writes }
}

#[test]
fn prepared_commands_that_write_other_views_than_they_declare_are_refused_by_name() {
    use crate::{AttachmentView, FrameError, PreparedWrite};
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let blocks = layered(&device, "blocks");
    let scratch = layered(&device, "scratch");
    let (source, destination) = (buffer(&device, 16), buffer(&device, 16));
    let view = |layer: usize| {
        AttachmentView::new(LAYERS[layer], &blocks)
            .unwrap()
            .with_prepared_contents()
    };
    let writing = |writes: Vec<PreparedWrite>| {
        let mut copy = CopyCommands::new(&source, &destination, None);
        copy.writes = writes;
        Box::new(copy)
    };
    let layer = |texture: &wgpu::Texture, layer: u32| PreparedWrite::layer(texture, 0, layer);
    let mismatch = |declared: &[crate::ResourceRef],
                    written: &[crate::ResourceRef]|
     -> Result<(), FrameError> {
        Err(FrameError::PreparedWriteMismatch {
            label: "transfer",
            declared: declared.to_vec(),
            written: written.to_vec(),
        })
    };
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            context.set_views(&[view(0), view(1)]);
            context.open_preparation();
            let into_layer_1 = writing(vec![layer(&blocks, 1)]);
            assert_eq!(
                context.record_prepared("transfer", declared(&LAYERS[..1]), into_layer_1),
                mismatch(&LAYERS[..1], &LAYERS[1..]),
                "a transfer into layer 1 declared as layer 0"
            );
            assert_eq!(
                context.record_prepared("transfer", declared(&LAYERS[..1]), writing(Vec::new())),
                mismatch(&LAYERS[..1], &[]),
                "a declared write the commands do not make"
            );
            let both = writing(vec![PreparedWrite {
                texture: blocks.clone(),
                mips: 0..1,
                layers: 0..2,
            }]);
            assert_eq!(
                context.record_prepared("transfer", declared(&LAYERS[..1]), both),
                mismatch(&LAYERS[..1], &LAYERS),
                "a write of both layers declared as one"
            );
            let runtime_owned = writing(vec![layer(&scratch, 0), layer(&blocks, 0)]);
            context.record_prepared("transfer", declared(&LAYERS[..1]), runtime_owned)?;
            context.record_prepared(
                "transfer",
                declared(&LAYERS[1..]),
                writing(vec![layer(&blocks, 1)]),
            )?;
            context.finish_preparation()?;
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.cancel();
}

#[test]
fn a_prepared_write_of_a_layer_the_frame_has_no_view_of_is_refused_by_the_frames_name() {
    use crate::{AttachmentView, FrameError, PreparedAccess, PreparedWrite};
    let (device, queue) = crate::gpu_tests::gpu();
    let authority = crate::test_authority(&queue);
    let blocks = layered(&device, "blocks");
    let (source, destination) = (buffer(&device, 16), buffer(&device, 16));
    let mut copy = CopyCommands::new(&source, &destination, None);
    copy.writes = vec![PreparedWrite::layer(&blocks, 0, 1)];
    let prepared = authority
        .prepare(&device, |encoder, _| {
            let mut context = PrepareContext::new(&device, encoder)?;
            let planned = AttachmentView::new(LAYERS[0], &blocks)
                .unwrap()
                .with_prepared_contents();
            context.set_views(&[planned]);
            context.open_preparation();
            let access = PreparedAccess {
                reads: &[],
                writes: &LAYERS[..1],
            };
            assert_eq!(
                context.record_prepared("transfer", access, Box::new(copy)),
                Err(FrameError::PreparedWriteMismatch {
                    label: "transfer",
                    declared: LAYERS[..1].to_vec(),
                    written: LAYERS[1..].to_vec(),
                })
            );
            Ok::<_, FrameError>(())
        })
        .unwrap();
    prepared.cancel();
}
