//! `--read-target` through the host's frame path on a real adapter (lavapipe in CI): the script
//! clears one render texture to red and then to green, and a read after each clear gives that
//! clear's result, in files that decode to it.

use std::path::Path;

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::unity::offscreen::TapRead;
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Clear, Record, RenderDimension, RenderTextureSpec,
};

use super::super::adapter::Settling;
use super::super::flush::OffscreenHost;
use super::super::taps::listing;
use super::blit_material_gpu::fill;
use super::flush::host;
use super::gpu::{Quad, SIDE};
use super::support::{render_texture, snapshot, world};
use crate::TargetRead;
use crate::target_read::finish;

/// The record of `report` that the read at `record` found, if its label says it is `kind`.
fn is_kind(labels: &[(usize, String)], record: usize, kind: &str) -> bool {
    labels
        .iter()
        .any(|(index, label)| *index == record && label.starts_with(kind))
}

fn read(record: usize, directory: &Path) -> TargetRead {
    TargetRead {
        frame: 0,
        record,
        file: directory.join(format!("record{record}.png")),
    }
}

/// Runs `operations` as the one submission of a frame and waits for it.
fn submit(
    host: &mut OffscreenHost,
    operations: worldrunner_render_program::unity::offscreen::OffscreenOperations,
) {
    let runtime = host.runtime_mut();
    let (device, authority) = (runtime.device().clone(), runtime.authority().clone());
    let mut encoder = authority.begin(&device).unwrap();
    let admitted = operations.admit(&encoder).unwrap();
    admitted.encode(None, &mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        })
        .unwrap();
}

struct Taken {
    taps: Vec<TapRead>,
    labels: Vec<(usize, String)>,
}

fn frame_with_reads(host: &mut OffscreenHost, reads: &[TargetRead]) -> Taken {
    let mut world = world();
    let drawn = render_texture(&mut world, [SIDE, SIDE]);
    fill(&mut world, drawn, [1.0, 0.0, 0.0, 1.0]);
    fill(&mut world, drawn, [0.0, 1.0, 0.0, 1.0]);
    let (operations, report) = host
        .frame(&snapshot(&mut world), &Quad, Settling::Wait, reads)
        .unwrap();
    let mut operations = operations.unwrap();
    let taps = operations.take_taps();
    submit(host, operations);
    Taken {
        taps,
        labels: report.reads,
    }
}

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];

#[test]
fn a_read_after_each_of_two_clears_gives_that_clears_result() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let reads: Vec<_> = (0..6)
        .map(|record| read(record, directory.path()))
        .collect();
    let taken = frame_with_reads(&mut host, &reads);
    let device = host.runtime_mut().device().clone();
    let clears: Vec<_> = taken
        .taps
        .into_iter()
        .filter(|tap| is_kind(&taken.labels, tap.record, "Clear"))
        .collect();
    assert_eq!(clears.len(), 2, "{:?}", taken.labels);
    let texels = (SIDE * SIDE) as usize;
    for (tap, expected) in clears.into_iter().zip([RED, GREEN]) {
        let image = tap.colour.unwrap().bytes(&device);
        assert!(
            image.chunks_exact(4).all(|texel| texel == expected),
            "record {} holds {:?}",
            tap.record,
            &image[..4]
        );
        assert_eq!(image.len(), texels * 4);
    }
}

#[test]
fn finishing_writes_each_read_and_names_every_refusal() {
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(directory.path());
    let mut reads: Vec<_> = (0..4)
        .map(|record| read(record, directory.path()))
        .collect();
    reads.push(read(99, directory.path()));
    let taken = frame_with_reads(&mut host, &reads);
    let device = host.runtime_mut().device().clone();
    let clear_records: Vec<usize> = (0..4)
        .filter(|record| is_kind(&taken.labels, *record, "Clear"))
        .collect();
    assert_eq!(clear_records.len(), 2, "{:?}", taken.labels);
    let finished = finish(&reads, &taken.labels, taken.taps, &device, "no tap");
    for (record, expected) in clear_records.into_iter().zip([RED, GREEN]) {
        let file = directory.path().join(format!("record{record}.png"));
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&file).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert!(
            rgba.chunks_exact(4).all(|texel| texel == expected),
            "record {record}"
        );
    }
    assert!(
        finished
            .refusals
            .iter()
            .any(|refusal| refusal.contains("record 99") && refusal.contains("past")),
        "{:?}",
        finished.refusals
    );
    assert!(finished.unwritten >= 1);
    assert!(
        finished
            .refusals
            .iter()
            .any(|refusal| refusal.contains("depth") && refusal.contains("no depth buffer")),
        "a colour-only target names its missing depth: {:?}",
        finished.refusals
    );
}

fn spec(id: u64, size: [u32; 2], graphics_format: u32, depth_bits: u8) -> RenderTextureSpec {
    RenderTextureSpec {
        id: TextureId(id),
        size,
        dimension: RenderDimension::D2,
        graphics_format,
        depth_bits,
        mips: 1,
        srgb: false,
        generation: 0,
    }
}

fn clear_of(color: u64, depth: Option<u64>) -> Record {
    let attachment = |texture| Attachment {
        texture: TextureId(texture),
        mip: 0,
    };
    Record::Clear {
        target: Attachments::single(Some(attachment(color)), depth.map(attachment)),
        clear: Clear::default(),
    }
}

#[test]
fn a_listing_names_a_depth_attachment_that_is_another_texture_than_the_target() {
    let textures = [
        spec(0x10, [1280, 720], 48, 0),
        spec(0x11, [1280, 720], 0, 32),
    ];
    let line = listing(119, 8, &clear_of(0x10, Some(0x11)), &textures);
    assert_eq!(
        line,
        "[gpu] frame 119 record 8 Clear target #10 1280x720 format 48 depth 0 \
         depth attachment #11 1280x720 format 0 depth 32"
    );
}

#[test]
fn a_listing_adds_nothing_when_the_depth_is_the_targets_own_or_absent() {
    let textures = [spec(0x10, [1280, 720], 48, 32)];
    let own = listing(119, 8, &clear_of(0x10, Some(0x10)), &textures);
    assert_eq!(
        own,
        "[gpu] frame 119 record 8 Clear target #10 1280x720 format 48 depth 32"
    );
    assert_eq!(listing(119, 8, &clear_of(0x10, None), &textures), own);
}
