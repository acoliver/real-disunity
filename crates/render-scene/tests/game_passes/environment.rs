//! A game's feature reads the specular environment cube the host installed, in the frame it
//! draws: with the scale and roughness convention the stock consumer samples it by, whether or
//! not the stock passes draw the water whose cube it is.
use super::fixture::{Rig, SIZE};
use super::snapshot_water::water_scene;
use std::sync::{Arc, Mutex};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{SceneEnvironmentConsumers, SceneSpecularConvention};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameError, FrameFeature,
    PassDeclaration, PrepareContext, PreparedFeature,
};
use worldrunner_render_scene::{
    compositor::{
        EnvironmentConsumer, FrameEnvironment, FrameEnvironmentHandle, FrameInputError, GameBuild,
        GameFeatureFactory, GamePasses, StockReplacement,
    },
    sky::preparation::{EnvironmentPreparation, PreparedEnvironmentMaps, encode},
    wgpu,
};

type Seen = Arc<Mutex<Vec<Result<Option<FrameEnvironment>, FrameInputError>>>>;

/// A game that plans nothing and records the lent environment as each frame plans.
struct Reader {
    seen: Seen,
    replaces: StockReplacement,
}

impl GameFeatureFactory for Reader {
    fn name(&self) -> &'static str {
        "environment reader"
    }

    fn replaces_stock(&self) -> StockReplacement {
        self.replaces
    }

    fn build(&self, context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(ReaderFeature {
            seen: self.seen.clone(),
            environment: context.environment.clone(),
        }))
    }
}

#[derive(Clone)]
struct ReaderFeature {
    seen: Seen,
    environment: FrameEnvironmentHandle,
}

impl FrameFeature for ReaderFeature {
    fn declare(&self) -> FeatureDeclaration {
        FeatureDeclaration {
            passes: Vec::new(),
            effects: Vec::new(),
        }
    }

    fn plan(
        &self,
        _passes: &mut Vec<PassDeclaration>,
        _views: &mut Vec<AttachmentView>,
    ) -> Result<FeatureDeclaration, FrameError> {
        self.seen.lock().unwrap().push(self.environment.current());
        Ok(self.declare())
    }

    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(self.clone()))
    }
}

impl PreparedFeature for ReaderFeature {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }

    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

/// The specular intensity the host installs the cube with, which the stock consumers multiply
/// sampled radiance by.
const INTENSITY_MICRO: u32 = 2_000_000;

/// Prefilters an empty source into a two-level cube on the rig's device and returns the maps a
/// host installs, with the intensity [`INTENSITY_MICRO`], and the cube texture.
fn environment_maps(rig: &Rig) -> (PreparedEnvironmentMaps, wgpu::Texture) {
    let device = rig.context.device();
    let source = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("empty environment source"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
    let mut encoder = rig.context.submission().begin(device).unwrap();
    let prepared = encode(
        device,
        &mut encoder,
        &source,
        EnvironmentPreparation {
            convention: SceneSpecularConvention::default(),
            cube_size: 2,
            source_lod_tail: 1,
            mip_count: 2,
            specular_intensity_micro: 1_000_000,
            lut_size: 1,
            samples: 8,
        },
        AllocationBudget(4096),
    )
    .unwrap();
    encoder.finish().submit().unwrap();
    let (cube, lut) = prepared.views().unwrap();
    let maps = PreparedEnvironmentMaps::from_uploaded(
        cube.texture(),
        lut.texture(),
        prepared.convention(),
        INTENSITY_MICRO,
    )
    .unwrap();
    (maps, cube.texture().clone())
}

/// One frame of an HDR rig with one water body, drawn by the stock passes or by the game as
/// `replaces` says, with a water-only environment cube installed, and what the game was lent.
fn lent_with_water(replaces: StockReplacement) -> (FrameEnvironment, wgpu::Texture) {
    let seen = Seen::default();
    let inventory = GamePasses::new().with(Arc::new(Reader {
        seen: seen.clone(),
        replaces,
    }));
    let mut rig = Rig::hdr(&water_scene(), SIZE, |settings| {
        settings.water = vec![super::fixture::water_body()];
        settings.game_passes = inventory;
    });
    let (maps, cube) = environment_maps(&rig);
    let device = rig.context.device().clone();
    rig.renderer
        .set_scene_environment(&device, &maps, SceneEnvironmentConsumers::Water)
        .unwrap();
    rig.render().expect("the frame renders");
    let lent = seen
        .lock()
        .unwrap()
        .last()
        .cloned()
        .expect("the game planned")
        .expect("the frame published its environment")
        .expect("the installed water cube is lent");
    (lent, cube)
}

fn assert_water_cube(lent: &FrameEnvironment, cube: &wgpu::Texture, replaces: StockReplacement) {
    assert_eq!(lent.consumer, EnvironmentConsumer::Water, "{replaces:?}");
    assert_eq!(&lent.texture, cube, "{replaces:?}");
    assert_eq!(lent.cube.texture(), cube, "{replaces:?}");
    assert_eq!(lent.mip_count, 2, "{replaces:?}");
    assert_eq!(
        lent.radiance_scale.to_bits(),
        2.0_f32.to_bits(),
        "{replaces:?}: the installed intensity, not baked into the texels"
    );
    assert_eq!(
        lent.convention,
        SceneSpecularConvention::default(),
        "{replaces:?}"
    );
}

#[test]
fn a_frame_with_an_installed_water_cube_lends_it_with_the_scale_water_samples_it_by() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (lent, cube) = lent_with_water(StockReplacement::NONE);
    assert_water_cube(&lent, &cube, StockReplacement::NONE);
}

/// A game that draws the water still has that water's environment installed, for its own water
/// passes to sample, so the frame lends it although no stock pass samples it.
#[test]
fn a_game_that_draws_the_water_is_still_lent_the_water_cube() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (lent, cube) = lent_with_water(StockReplacement::WATER);
    assert_water_cube(&lent, &cube, StockReplacement::WATER);
}
