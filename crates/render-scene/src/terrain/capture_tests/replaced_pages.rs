//! A terrain a game draws stages no stock material pages: no camera pass draws the stock
//! terrain, so the page cache selects, bakes and uploads nothing while the surface is replaced
//! and resumes on the next frame once it is not. Runs on wgpu's no-op device, which completes
//! every submission, and reads the page counts the scene publishes.

use super::scenes;
use crate::draw::{SceneGpu, TerrainPageStatistics};
use crate::dynamic::capture_tests::{CONFIGS, SIZE, build};
use crate::test_support::upload_scene;
use worldrunner_render_gpu::submission::SubmissionAuthority;

const FRAMES: usize = 4;

struct Rig {
    device: wgpu::Device,
    authority: SubmissionAuthority,
    gpu: SceneGpu,
    frame: crate::frame::FrameUniform,
}

impl Rig {
    /// A terrain scene with material pages enabled and none prepared.
    fn new() -> Self {
        let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
        let authority = SubmissionAuthority::new(queue.clone());
        let built = build(&device, &CONFIGS[0]);
        let mut gpu = upload_scene(&device, &authority, &scenes::plain(), &built.pipelines)
            .expect("the scene uploads");
        gpu.enable_terrain_pages(&device, &authority, scenes::page_source(false))
            .expect("pages enable");
        let frame = scenes::frame(scenes::pose());
        gpu.set_frame(&queue, frame);
        Self {
            device,
            authority,
            gpu,
            frame,
        }
    }

    /// Prepares the pages of one frame and submits it.
    fn frame(&self) {
        let mut transaction = self.authority.begin(&self.device).unwrap();
        self.gpu
            .prepare_terrain_pages(&self.device, &mut transaction, &self.frame, [SIZE; 2])
            .unwrap();
        transaction.finish().submit().unwrap();
        self.device.poll(wgpu::PollType::Poll).unwrap();
    }

    /// The selected, resident and pending pages the last submitted frame published.
    fn pages(&self) -> TerrainPageStatistics {
        self.gpu.statistics().terrain_pages
    }
}

#[test]
fn a_replaced_terrain_surface_bakes_and_uploads_no_pages() {
    let rig = Rig::new();
    rig.gpu.set_terrain_surface_replaced(true);
    for _ in 0..FRAMES {
        rig.frame();
        assert_eq!(rig.pages(), TerrainPageStatistics::default());
        assert!(!rig.gpu.terrain_pages_ready());
    }
    assert!(!rig.gpu.has_terrain_pages());
    let request = rig.gpu.terrain_residency_request(&rig.frame, [SIZE; 2]);
    assert!(request.unwrap().is_none());
}

#[test]
fn a_stock_terrain_surface_bakes_the_same_scene_pages() {
    let rig = Rig::new();
    rig.gpu.set_terrain_surface_replaced(false);
    assert!(rig.gpu.has_terrain_pages());
    for _ in 0..FRAMES {
        rig.frame();
    }
    let pages = rig.pages();
    assert!(pages.requested > 0 && pages.resident > 0, "{pages:?}");
    assert!(rig.gpu.terrain_pages_ready());
}

#[test]
fn pages_prepare_on_the_next_frame_once_the_terrain_is_no_longer_replaced() {
    let rig = Rig::new();
    rig.gpu.set_terrain_surface_replaced(true);
    for _ in 0..FRAMES {
        rig.frame();
    }
    assert_eq!(rig.pages(), TerrainPageStatistics::default());
    rig.gpu.set_terrain_surface_replaced(false);
    rig.frame();
    rig.frame();
    let pages = rig.pages();
    assert!(pages.requested > 0 && pages.resident > 0, "{pages:?}");
    assert!(rig.gpu.terrain_pages_ready());
}
