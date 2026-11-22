//! What a game's targets cost the allocation budget: a backing a replacement keeps is charged
//! once, and a backing a replacement makes anew is charged beside the old one.
use super::fixture::{FORMAT, SCENE_FORMAT, SIZE};
use std::sync::Arc;
use worldrunner_render_gpu::configuration::{AllocationBudget, ConfigurationError};
use worldrunner_render_post::{
    AttachmentView, BuildContext, DrawContext, FeatureDeclaration, FrameConfig, FrameError,
    FrameFeature, PassDeclaration, PrepareContext, PreparedFeature,
};
use worldrunner_render_scene::{
    compositor::{
        ConfiguredFrameRenderer, GameBuild, GameFeatureFactory, GamePasses, RendererConfiguration,
        TargetDeclaration, TargetExtent,
    },
    device::GpuContext,
    wgpu,
};

const FIXED: [u32; 2] = [256, 256];
/// The bytes of the fixed backing: 256 by 256 texels of four bytes.
const FIXED_BYTES: u64 = 256 * 256 * 4;
const LARGER: [u32; 2] = [96, 72];
/// The bytes of the fixed backing in BC4: a 64 by 64 grid of eight-byte blocks.
const FIXED_BLOCK_BYTES: u64 = 64 * 64 * 8;
/// The bytes of the fixed backing in BC4 with three mips: 64, 32 and 16 blocks a side.
const FIXED_CHAIN_BYTES: u64 = (64 * 64 + 32 * 32 + 16 * 16) * 8;
/// The bytes of the fixed backing with three mips: 256, 128 and 64 texels a side of four bytes.
const FIXED_RENDERABLE_CHAIN_BYTES: u64 = (256 * 256 + 128 * 128 + 64 * 64) * 4;

/// A game that owns targets and draws nothing.
#[derive(Clone, Copy, Debug)]
struct Ledger {
    frame_sized: bool,
    persistent: bool,
    format: wgpu::TextureFormat,
    fixed: [u32; 2],
    mips: u32,
}

impl Ledger {
    const FIXED_ONLY: Self = Self {
        frame_sized: false,
        persistent: true,
        format: wgpu::TextureFormat::Rgba8Unorm,
        fixed: FIXED,
        mips: 1,
    };

    fn passes(self) -> GamePasses {
        GamePasses::new().with(Arc::new(self))
    }
}

struct Nothing;

impl FrameFeature for Nothing {
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
        Ok(self.declare())
    }
    fn build(&self, _context: &BuildContext<'_>) -> Result<Box<dyn PreparedFeature>, FrameError> {
        Ok(Box::new(Nothing))
    }
}

impl PreparedFeature for Nothing {
    fn prepare(&mut self, _context: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        Ok(())
    }
    fn draw(&self, _context: &DrawContext, _pass: &mut wgpu::RenderPass<'_>) {}
}

impl GameFeatureFactory for Ledger {
    fn name(&self) -> &'static str {
        "ledger"
    }

    fn targets(&self) -> Vec<TargetDeclaration> {
        let mut targets = vec![
            declaration(
                "ledger",
                self.format,
                TargetExtent::Fixed(self.fixed),
                self.persistent,
            )
            .mips(self.mips),
        ];
        if self.frame_sized {
            targets.push(declaration(
                "trail",
                wgpu::TextureFormat::Rgba8Unorm,
                TargetExtent::Frame,
                true,
            ));
        }
        targets
    }

    fn build(&self, _context: &GameBuild<'_>) -> Result<Box<dyn FrameFeature>, FrameError> {
        Ok(Box::new(Nothing))
    }
}

fn declaration(
    name: &'static str,
    format: wgpu::TextureFormat,
    extent: TargetExtent,
    persistent: bool,
) -> TargetDeclaration {
    let target = TargetDeclaration::new(name, format, extent);
    if persistent {
        target.persistent()
    } else {
        target
    }
}

fn context() -> GpuContext {
    GpuContext::shared_for_test_with_features(wgpu::Features::empty())
        .expect("game pass acceptance requires a real adapter; never skip")
}

fn settings(size: [u32; 2], passes: GamePasses) -> RendererConfiguration {
    let mut settings =
        RendererConfiguration::new(size, FORMAT, worldrunner_render_post::ToneMap::Clamp);
    settings.game_passes = passes;
    settings
}

/// The bytes one generation of `settings` is charged.
fn charged(context: &GpuContext, settings: &RendererConfiguration) -> u64 {
    settings
        .allocations()
        .validate(&context.device().limits())
        .unwrap()
}

fn renderer(
    context: &GpuContext,
    settings: RendererConfiguration,
    budget: u64,
) -> ConfiguredFrameRenderer {
    ConfiguredFrameRenderer::new(context, settings, AllocationBudget(budget)).unwrap()
}

/// A replacement that changes nothing the allocation inventory describes.
fn recolored(settings: &RendererConfiguration) -> RendererConfiguration {
    let mut next = settings.clone();
    next.clear_color = wgpu::Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    next
}

#[test]
fn a_same_shape_replacement_fits_both_stock_sets_and_one_fixed_game_backing() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = context();
    let plain = charged(&context, &settings(SIZE, GamePasses::new()));
    let live = settings(SIZE, Ledger::FIXED_ONLY.passes());
    assert_eq!(charged(&context, &live), plain + FIXED_BYTES);
    let peak = 2 * plain + FIXED_BYTES;

    let mut fits = renderer(&context, live.clone(), peak);
    let backing = fits.game_targets().texture("ledger").unwrap();
    fits.reconfigure(&context, recolored(&live)).unwrap();
    assert_eq!(fits.generation(), 1);
    assert_eq!(fits.game_targets().texture("ledger").unwrap(), backing);

    // One byte less would refuse it: the peak is exact, not slack.
    let mut tight = renderer(&context, live.clone(), peak - 1);
    let error = tight.reconfigure(&context, recolored(&live)).unwrap_err();
    assert!(
        matches!(error, ConfigurationError::Budget { required, .. } if required == peak),
        "{error:?}"
    );
    assert_eq!(tight.generation(), 0);
}

#[test]
fn a_resize_charges_the_old_and_the_new_backing_until_the_old_one_retires() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = context();
    let passes = Ledger {
        frame_sized: true,
        ..Ledger::FIXED_ONLY
    }
    .passes();
    let small = settings(SIZE, passes.clone());
    let mut large = small.clone();
    large.frame = FrameConfig::new(LARGER, SCENE_FORMAT, 1);
    let (before, after) = (charged(&context, &small), charged(&context, &large));
    // The fixed backing is kept; the frame-sized one and the stock targets exist twice.
    let peak = before + after - FIXED_BYTES;

    let mut fits = renderer(&context, small.clone(), peak);
    let trail = fits.game_targets().texture("trail").unwrap();
    let ledger = fits.game_targets().texture("ledger").unwrap();
    fits.reconfigure(&context, large.clone()).unwrap();
    assert_ne!(fits.game_targets().texture("trail").unwrap(), trail);
    assert_eq!(fits.game_targets().texture("ledger").unwrap(), ledger);
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    fits.collect();
    assert_eq!(
        fits.resident_bytes(&context.device().limits()).unwrap(),
        after,
        "only the new generation stays charged once the old one retires"
    );

    let mut tight = renderer(&context, small, peak - 1);
    let error = tight.reconfigure(&context, large).unwrap_err();
    assert!(
        matches!(error, ConfigurationError::Budget { required, .. } if required == peak),
        "{error:?}"
    );
}

#[test]
fn a_backing_is_kept_only_for_an_identical_declaration() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = context();
    let live = settings(SIZE, Ledger::FIXED_ONLY.passes());
    for changed in [
        Ledger {
            persistent: false,
            ..Ledger::FIXED_ONLY
        },
        Ledger {
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            ..Ledger::FIXED_ONLY
        },
    ] {
        let mut renderer = renderer(&context, live.clone(), 1 << 28);
        let kept = renderer.game_targets().texture("ledger").unwrap();
        let mut next = live.clone();
        next.game_passes = changed.passes();
        renderer.reconfigure(&context, next).unwrap();
        assert_ne!(
            renderer.game_targets().texture("ledger").unwrap(),
            kept,
            "a target whose declaration changed is a new backing"
        );
    }
}

fn block_context() -> GpuContext {
    GpuContext::shared_for_test_with_features(wgpu::Features::TEXTURE_COMPRESSION_BC)
        .expect("game pass acceptance requires a real adapter; never skip")
}

const BLOCKS: Ledger = Ledger {
    format: wgpu::TextureFormat::Bc4RUnorm,
    ..Ledger::FIXED_ONLY
};

#[test]
fn a_block_compressed_target_is_charged_its_blocks_and_a_kept_backing_counts_once() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = block_context();
    let plain = charged(&context, &settings(SIZE, GamePasses::new()));
    let live = settings(SIZE, BLOCKS.passes());
    assert_eq!(charged(&context, &live), plain + FIXED_BLOCK_BYTES);
    let peak = 2 * plain + FIXED_BLOCK_BYTES;
    let mut fits = renderer(&context, live.clone(), peak);
    let backing = fits.game_targets().texture("ledger").unwrap();
    assert_eq!(
        backing.usage(),
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        "a block-compressed target is written by copies only"
    );
    assert_eq!(backing.mip_level_count(), 1);
    fits.reconfigure(&context, recolored(&live)).unwrap();
    assert_eq!(fits.game_targets().texture("ledger").unwrap(), backing);
    let mut tight = renderer(&context, live.clone(), peak - 1);
    let error = tight.reconfigure(&context, recolored(&live)).unwrap_err();
    assert!(
        matches!(error, ConfigurationError::Budget { required, .. } if required == peak),
        "{error:?}"
    );
}

#[test]
fn a_block_compressed_target_needs_the_compression_feature_and_whole_blocks() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let refusal = |context: &GpuContext, ledger: Ledger| {
        let Err(error) = ConfiguredFrameRenderer::new(
            context,
            settings(SIZE, ledger.passes()),
            AllocationBudget(1 << 28),
        ) else {
            panic!("{ledger:?} was allocated");
        };
        error.to_string()
    };
    let missing = refusal(&context(), BLOCKS);
    assert!(
        missing.contains("game target ledger") && missing.contains("TEXTURE_COMPRESSION_BC"),
        "{missing}"
    );
    let partial = refusal(
        &block_context(),
        Ledger {
            fixed: [6, 8],
            ..BLOCKS
        },
    );
    assert!(
        partial.contains("game target ledger") && partial.contains("block"),
        "{partial}"
    );
}

#[test]
fn a_block_target_with_a_mip_chain_is_charged_every_mip() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = block_context();
    let plain = charged(&context, &settings(SIZE, GamePasses::new()));
    let chain = Ledger { mips: 3, ..BLOCKS };
    let live = settings(SIZE, chain.passes());
    assert_eq!(charged(&context, &live), plain + FIXED_CHAIN_BYTES);
    let peak = 2 * plain + FIXED_CHAIN_BYTES;
    let mut fits = renderer(&context, live.clone(), peak);
    let backing = fits.game_targets().texture("ledger").unwrap();
    assert_eq!(backing.mip_level_count(), 3);
    fits.reconfigure(&context, recolored(&live)).unwrap();
    assert_eq!(fits.game_targets().texture("ledger").unwrap(), backing);
    let mut tight = renderer(&context, live.clone(), peak - 1);
    let error = tight.reconfigure(&context, recolored(&live)).unwrap_err();
    assert!(
        matches!(error, ConfigurationError::Budget { required, .. } if required == peak),
        "{error:?}"
    );
    let mut longer = renderer(&context, live.clone(), 1 << 28);
    let mut next = live;
    next.game_passes = Ledger { mips: 2, ..BLOCKS }.passes();
    longer.reconfigure(&context, next).unwrap();
    assert_ne!(
        longer.game_targets().texture("ledger").unwrap(),
        backing,
        "another mip count is another backing"
    );
}

#[test]
fn a_mip_count_outside_the_chain_or_on_a_format_that_cannot_render_is_refused_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let refusal = |context: &GpuContext, ledger: Ledger| {
        let Err(error) = ConfiguredFrameRenderer::new(
            context,
            settings(SIZE, ledger.passes()),
            AllocationBudget(1 << 28),
        ) else {
            panic!("{ledger:?} was allocated");
        };
        error.to_string()
    };
    // 256 texels a side have nine mips, down to one texel.
    for mips in [0, 10] {
        let error = refusal(&block_context(), Ledger { mips, ..BLOCKS });
        assert!(
            error.contains("game target ledger")
                && error.contains(&format!("mip count {mips} is outside 1..=9")),
            "{error}"
        );
    }
    let full = Ledger { mips: 9, ..BLOCKS };
    renderer(&block_context(), settings(SIZE, full.passes()), 1 << 28);
    // Both counts are refused for a renderable colour target the same way.
    for mips in [0, 10] {
        let error = refusal(
            &context(),
            Ledger {
                mips,
                ..Ledger::FIXED_ONLY
            },
        );
        assert!(
            error.contains("game target ledger")
                && error.contains(&format!("mip count {mips} is outside 1..=9")),
            "{error}"
        );
    }
    for format in [
        wgpu::TextureFormat::Rg11b10Ufloat,
        wgpu::TextureFormat::Depth32Float,
    ] {
        let error = refusal(
            &context(),
            Ledger {
                format,
                mips: 2,
                ..Ledger::FIXED_ONLY
            },
        );
        assert!(
            error.contains("game target ledger")
                && error.contains("not a renderable colour format"),
            "{format:?}: {error}"
        );
    }
}

#[test]
fn a_renderable_target_with_a_mip_chain_is_charged_every_mip() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = context();
    let plain = charged(&context, &settings(SIZE, GamePasses::new()));
    let live = settings(
        SIZE,
        Ledger {
            mips: 3,
            ..Ledger::FIXED_ONLY
        }
        .passes(),
    );
    assert_eq!(
        charged(&context, &live),
        plain + FIXED_RENDERABLE_CHAIN_BYTES
    );
    let peak = 2 * plain + FIXED_RENDERABLE_CHAIN_BYTES;
    let mut fits = renderer(&context, live.clone(), peak);
    let backing = fits.game_targets().texture("ledger").unwrap();
    assert_eq!(backing.mip_level_count(), 3);
    assert_eq!(
        backing.usage(),
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT
    );
    fits.reconfigure(&context, recolored(&live)).unwrap();
    assert_eq!(fits.game_targets().texture("ledger").unwrap(), backing);
    let mut tight = renderer(&context, live.clone(), peak - 1);
    let error = tight.reconfigure(&context, recolored(&live)).unwrap_err();
    assert!(
        matches!(error, ConfigurationError::Budget { required, .. } if required == peak),
        "{error:?}"
    );
}
