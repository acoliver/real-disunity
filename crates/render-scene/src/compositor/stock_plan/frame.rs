//! The stock plan of a frame: the contributors collected in order, then ordered once.
use super::LogicalSchedule;
use super::cache::{Key, Lookup, PlanCache, Probe};
use super::shape::{self, Identity, Shape};
use crate::compositor::{
    FrameOutput, forward_order, game::reconcile_loads, overlay::OverlayStage, snapshots,
};
use crate::frame::FrameUniform;
use crate::scene_metadata::SceneMetadata;
use crate::ssao::passes::ScreenFeature;
use crate::ssr::{ReflectionFeature, Ssr};
use crate::water::{
    Water,
    snapshots::{SnapshotFeature, Snapshots},
};
use std::cell::RefCell;
use std::rc::Rc;
use worldrunner_render_plan::{SceneSsao, SceneSsr};
use worldrunner_render_post::{
    AttachmentView, BorrowedFeature, FeatureDeclaration, FrameConfig, FrameError, PassDeclaration,
    PassSlot, PostChain,
};

/// What the caller authored: the base passes with the views they name, and the declarations of
/// the features that draw in them.
pub(crate) struct Authored<'a> {
    pub passes: &'a [PassDeclaration],
    pub views: &'a [AttachmentView],
    pub features: &'a [FeatureDeclaration],
}

/// The renderer state a stock plan reads. It decides the schedule through the settings and
/// the extents and formats of what is allocated, and decides the binding table through the
/// allocations themselves.
pub(crate) struct Environment<'s> {
    pub config: FrameConfig,
    pub frame: FrameUniform,
    pub slot: usize,
    pub ssao: Option<&'s SceneMetadata>,
    pub ssr: Option<&'s Ssr>,
    /// The water the stock passes draw. It is `None` while a game feature draws the water.
    pub water: Option<&'s Water>,
    pub velocity: Option<wgpu::Texture>,
    /// The screen-space settings the renderer was configured with.
    pub requested: (Option<SceneSsao>, Option<SceneSsr>),
    /// Whether depth-tested overlay geometry reads the post-water depth.
    pub overlay_depth: bool,
    /// Whether a shadow binding accompanies the frame.
    pub shadow: bool,
}

/// The state a plan updates: the retained features it plans, the snapshot banks it allocates and
/// the overlay's depth binding.
pub(crate) struct Parts<'a> {
    pub cache: &'a RefCell<PlanCache>,
    pub chain: &'a mut PostChain<FrameOutput>,
    pub snapshots: &'a mut Option<[Snapshots; 3]>,
    pub overlay: &'a mut OverlayStage,
}

/// The per-frame executors of the stock effects. Each holds this frame's uniforms and bind
/// groups, so none of them is part of the cached schedule.
struct Executors<'s> {
    screen: Option<ScreenFeature<'s>>,
    reflection: Option<ReflectionFeature<'s>>,
    snapshot: Option<SnapshotFeature>,
}

/// A frame's plan: its schedule and binding table, and the executors of its stock effects.
pub(crate) struct StockFrame<'s> {
    pub schedule: Rc<LogicalSchedule>,
    pub views: Rc<Vec<AttachmentView>>,
    executors: Executors<'s>,
}

impl StockFrame<'_> {
    /// The features of the frame in the order their declarations were planned: the caller's, the
    /// screen-space passes, reflections and snapshots. Retained features are the chain's.
    pub(crate) fn features<'a>(
        &'a mut self,
        caller: &'a mut [BorrowedFeature<'_>],
    ) -> Vec<BorrowedFeature<'a>> {
        let schedule = &*self.schedule;
        let mut features: Vec<_> = caller
            .iter_mut()
            .zip(&schedule.features)
            .map(|(feature, declaration)| BorrowedFeature {
                declaration: declaration.clone(),
                prepared: &mut *feature.prepared,
            })
            .collect();
        let Executors {
            screen,
            reflection,
            snapshot,
        } = &mut self.executors;
        if let (Some(prepared), Some(declaration)) = (screen, &schedule.screen) {
            features.push(BorrowedFeature {
                declaration: declaration.clone(),
                prepared,
            });
        }
        if let (Some(prepared), Some(declaration)) = (reflection, &schedule.reflection) {
            features.push(BorrowedFeature {
                declaration: declaration.clone(),
                prepared,
            });
        }
        if let (Some(prepared), Some(declaration)) = (snapshot, &schedule.snapshot) {
            features.push(BorrowedFeature {
                declaration: declaration.clone(),
                prepared,
            });
        }
        features
    }
}

struct Derived<'s> {
    schedule: LogicalSchedule,
    views: Vec<AttachmentView>,
    executors: Executors<'s>,
}

/// Plans a frame: reuses its schedule and binding table when the cache has them for this key and
/// these allocations, and builds what it lacks.
///
/// # Errors
/// The errors of the contributors, in the order they plan.
pub(crate) fn plan_frame<'s>(
    device: &wgpu::Device,
    parts: Parts<'_>,
    environment: &Environment<'s>,
    authored: &Authored<'_>,
) -> Result<StockFrame<'s>, FrameError> {
    let Parts {
        cache,
        chain,
        snapshots,
        overlay,
    } = parts;
    let probe = probe(environment, authored, snapshots.as_ref());
    let cacheable = chain.registered_feature_count() == usize::from(environment.water.is_some());
    let lookup = if cacheable {
        cache.borrow_mut().frames.lookup(&probe)
    } else {
        Lookup::Absent
    };
    let frame = if let Lookup::Bound {
        schedule,
        views,
        binding,
    } = lookup
    {
        #[cfg(any(test, debug_assertions))]
        verify(
            device,
            chain,
            snapshots,
            environment,
            authored,
            (&schedule, &views),
        );
        chain.adopt_planned_features(&schedule.retained);
        let executors = executors(device, snapshots, environment, (&schedule, &views))?;
        cache.borrow_mut().counters.reused(binding);
        StockFrame {
            schedule,
            views,
            executors,
        }
    } else {
        let built = derive(device, chain, snapshots, environment, authored)?;
        let Derived {
            schedule,
            views,
            executors,
        } = built;
        let mut cache = cache.borrow_mut();
        let (schedule, views) = if cacheable {
            let cache = &mut *cache;
            // Deriving allocates the snapshot banks when the schedule first reads them. The
            // table is resolved against them, so it is stored for the allocations the next
            // frame is bound to rather than the ones this frame was probed with.
            let probe = Probe {
                identity: identity(
                    environment,
                    authored,
                    (&shape::owned(environment), snapshots.as_ref()),
                ),
                key: probe.key,
            };
            cache
                .frames
                .store(&mut cache.counters, probe, (schedule, views))
        } else {
            cache.counters.uncached();
            (Rc::new(schedule), Rc::new(views))
        };
        StockFrame {
            schedule,
            views,
            executors,
        }
    };
    overlay.prepare_depth(
        device,
        snapshots.as_ref().map(|slots| &slots[environment.slot]),
        &environment.frame,
    );
    Ok(frame)
}

pub(super) fn probe(
    environment: &Environment<'_>,
    authored: &Authored<'_>,
    snapshots: Option<&[Snapshots; 3]>,
) -> Probe {
    let owned = shape::owned(environment);
    Probe {
        key: Key {
            shape: Shape::new(environment, &owned),
            passes: authored.passes.to_vec(),
            features: authored.features.to_vec(),
            views: authored
                .views
                .iter()
                .map(AttachmentView::resource)
                .collect(),
            shadow: environment.shadow,
        },
        identity: identity(environment, authored, (&owned, snapshots)),
    }
}

/// The allocations a binding table is resolved against, which include the frame slot's snapshot
/// bank once the renderer has allocated it.
fn identity(
    environment: &Environment<'_>,
    authored: &Authored<'_>,
    allocations: (&shape::Owned<'_>, Option<&[Snapshots; 3]>),
) -> Identity {
    let (owned, snapshots) = allocations;
    let bank = shape::snapshot_bank(environment, snapshots);
    Identity::new(environment, authored, (owned, &bank))
}

/// Plans the frame from the caller's passes in the order the contributors add theirs, then
/// orders it. This is where the stock plan is decided; the cache only remembers its result.
fn derive<'s>(
    device: &wgpu::Device,
    chain: &mut PostChain<FrameOutput>,
    snapshots: &mut Option<[Snapshots; 3]>,
    environment: &Environment<'s>,
    authored: &Authored<'_>,
) -> Result<Derived<'s>, FrameError> {
    let mut passes = authored.passes.to_vec();
    let mut views = authored.views.to_vec();
    let screen = ScreenFeature::installed(environment.ssao, environment.ssr, &environment.frame);
    let mut screen_declaration = screen
        .as_ref()
        .map(|screen| screen.schedule(&mut passes, &mut views))
        .transpose()?;
    let water = environment
        .water
        .map(|water| crate::ssr::water::Inputs::new(water, environment.slot));
    let reflection = environment
        .ssr
        .map(|ssr| {
            ssr.feature(
                &environment.frame,
                (&passes, &views),
                environment.config,
                water,
            )
        })
        .transpose()?;
    let mut reflection_declaration = reflection
        .as_ref()
        .map(|reflection| reflection.schedule(&mut passes, &mut views))
        .transpose()?;
    let retained = chain.plan_features(&mut passes, &mut views)?;
    let declared: Vec<&FeatureDeclaration> = authored
        .features
        .iter()
        .chain(&screen_declaration)
        .chain(&reflection_declaration)
        .collect();
    let (request, snapshot) = snapshots::plan(
        device,
        (snapshots, environment.overlay_depth),
        &(
            environment.config,
            environment.frame,
            environment.slot,
            environment.water.is_some(),
        ),
        (&mut passes, &mut views),
        (&declared, &retained),
    )?;
    let (mut snapshot_declaration, snapshot) = match snapshot {
        Some((declaration, feature)) => (Some(declaration), Some(feature)),
        None => (None, None),
    };
    schedule_screen_reads(
        environment.ssao,
        screen_declaration.as_mut(),
        authored.features,
        [
            reflection_declaration.as_ref(),
            snapshot_declaration.as_ref(),
        ],
        &retained,
        (&mut passes, &mut views),
    )?;
    let mut features = authored.features.to_vec();
    // An Opaque entry may repeat with its own colour layout, so each feature pass takes the
    // formats of the planned pass for its own entry. A pass with no planned entry is left
    // for frame validation to report.
    for declaration in features
        .iter_mut()
        .chain(screen_declaration.as_mut())
        .chain(reflection_declaration.as_mut())
        .chain(snapshot_declaration.as_mut())
    {
        take_planned_colour_layout(declaration, &passes);
    }
    forward_order::forward_order(&mut passes);
    reconcile_loads(&mut passes);
    let first = passes.first().map(PassDeclaration::entry);
    Ok(Derived {
        schedule: LogicalSchedule {
            passes,
            features,
            screen: screen_declaration,
            reflection: reflection_declaration,
            snapshot: snapshot_declaration,
            retained,
            request,
            first,
        },
        views,
        executors: Executors {
            screen,
            reflection,
            snapshot,
        },
    })
}

/// Adds to the screen feature the stages that the caller's, reflection, snapshot and retained
/// declarations read, once every other feature of the frame has planned.
fn schedule_screen_reads(
    ssao: Option<&SceneMetadata>,
    screen: Option<&mut FeatureDeclaration>,
    authored: &[FeatureDeclaration],
    planned: [Option<&FeatureDeclaration>; 2],
    retained: &[FeatureDeclaration],
    plan: (&mut Vec<PassDeclaration>, &mut Vec<AttachmentView>),
) -> Result<(), FrameError> {
    let (Some(ssao), Some(screen)) = (ssao, screen) else {
        return Ok(());
    };
    let others = authored
        .iter()
        .chain(planned.into_iter().flatten())
        .chain(retained);
    crate::ssao::passes::schedule_declared_reads(ssao, plan, screen, others)
}

fn take_planned_colour_layout(declaration: &mut FeatureDeclaration, passes: &[PassDeclaration]) {
    for pass in &mut declaration.passes {
        if pass.slot != PassSlot::Opaque {
            continue;
        }
        if let Some(planned) = passes
            .iter()
            .find(|planned| planned.entry() == pass.entry())
        {
            pass.additional_color_formats = planned
                .additional_colors
                .iter()
                .map(|attachment| attachment.format)
                .collect();
        }
    }
}

/// The executors of a planned frame, from its schedule and binding table.
fn executors<'s>(
    device: &wgpu::Device,
    snapshots: &mut Option<[Snapshots; 3]>,
    environment: &Environment<'s>,
    planned: (&LogicalSchedule, &[AttachmentView]),
) -> Result<Executors<'s>, FrameError> {
    let (schedule, views) = planned;
    let water = environment
        .water
        .map(|water| crate::ssr::water::Inputs::new(water, environment.slot));
    Ok(Executors {
        screen: ScreenFeature::installed(environment.ssao, environment.ssr, &environment.frame),
        reflection: environment
            .ssr
            .map(|ssr| {
                ssr.feature(
                    &environment.frame,
                    (&schedule.passes, views),
                    environment.config,
                    water,
                )
            })
            .transpose()?,
        snapshot: crate::water::planning::snapshot_executor(
            device,
            snapshots,
            (environment.config, &environment.frame, environment.slot),
            (&schedule.passes, views),
            schedule.request,
        )?,
    })
}

/// A cached plan must be the plan a fresh build gives. The key and the identity are meant to
/// cover everything that decides them, and this is how a test or a debug build finds what they
/// miss. A release build reuses a hit without rebuilding it.
#[cfg(any(test, debug_assertions))]
fn verify(
    device: &wgpu::Device,
    chain: &mut PostChain<FrameOutput>,
    snapshots: &mut Option<[Snapshots; 3]>,
    environment: &Environment<'_>,
    authored: &Authored<'_>,
    cached: (&Rc<LogicalSchedule>, &Rc<Vec<AttachmentView>>),
) {
    let fresh = derive(device, chain, snapshots, environment, authored)
        .unwrap_or_else(|error| panic!("a cached plan stands where a fresh build fails: {error}"));
    assert_eq!(
        fresh.schedule, **cached.0,
        "the cached logical schedule differs from a fresh build"
    );
    assert_eq!(
        shape::identities(&fresh.views),
        shape::identities(cached.1),
        "the cached binding table differs from a fresh build"
    );
}
