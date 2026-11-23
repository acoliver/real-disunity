//! Baking a page compiles no shader module: the composition kernel and the page writer are
//! compiled once and every later page reuses them. wgpu's no-op device validates what is
//! created and encoded as a real device does and runs nothing.

use super::*;
use crate::terrain::pages::residency_tests::settings;
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};
use worldrunner_render_plan::*;

const PAGES: u32 = 64;

#[test]
fn baking_pages_compiles_the_kernel_modules_once() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let authority = SubmissionAuthority::new(queue);
    let (terrain, resources) = super::tests::fixture(
        TerrainSplatFilter::Spiral14,
        TerrainColorizationChannels::Rgb,
    );
    let source = LayeredGpuSource::new(&terrain, &resources).unwrap();
    let mut parts = settings().parts();
    parts.upload_bytes_per_frame = 1008;
    let settings = TerrainPageSettings::new(parts).unwrap();
    let cache = ResidentPages::new_layered(
        &device,
        &authority,
        settings,
        2,
        AllocationBudget(100_000),
        &source,
    )
    .unwrap();
    let before = super::super::kernel_modules_compiled();
    let writer = PageWriter::new_layered(&device, settings);
    let start = std::time::Instant::now();
    for _ in 0..PAGES {
        let mut frame = authority.begin(&device).unwrap();
        let pending = cache.reserve(&frame, PageKey::ROOT).unwrap();
        source
            .encode_page(&device, &mut frame, &writer, &cache, pending)
            .unwrap();
        frame.finish().cancel();
    }
    let elapsed = start.elapsed();
    let compiled = super::super::kernel_modules_compiled() - before;
    eprintln!(
        "baked {PAGES} pages: {compiled} kernel modules compiled, {:.1} us per page",
        elapsed.as_secs_f64() * 1e6 / f64::from(PAGES)
    );
    // The page writer's store and reduce kernels and the composition kernel, once.
    assert_eq!(compiled, 3, "kernel modules compiled over {PAGES} pages");
}
