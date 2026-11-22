//! CPU time of the per-frame object selection for a walking camera and four cascades.
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use super::large_scene::{FrameViews, slice_hull, walk, world};
use super::*;

const FRAMES: u16 = 40;

/// Times of one frame: caster fits, selection preparation, then camera and cascades.
#[derive(Default)]
struct Times {
    fits: Duration,
    prepare: Duration,
    views: [Duration; 5],
    drawn: [usize; 5],
    /// Hash of every view's ranges and levels, so builds can be compared for equal output.
    selected: u64,
}

/// The camera and the four caster-fitted cascades, unselected.
fn frame_selections(
    instances: &Instances,
    meshes: &[MeshSelection],
    views: &FrameViews,
    times: &mut Times,
) -> [(Mat4, Selection); 5] {
    let start = Instant::now();
    let fits = views
        .receivers
        .map(|receiver| caster_matrix(instances, meshes, None, receiver));
    times.fits = start.elapsed();
    std::array::from_fn(|view| {
        if view == 0 {
            return (views.camera, Selection::default());
        }
        let matrix = fits[view - 1];
        let selection = Selection {
            casters_only: true,
            slice: Some(slice_hull(views, view - 1, matrix)),
            ..Selection::default()
        };
        (matrix, selection)
    })
}

/// Selects one frame's views one after another, or all at once on threads when `parallel`.
fn time_frame(
    instances: &Instances,
    meshes: &[MeshSelection],
    views: &FrameViews,
    parallel: bool,
) -> Times {
    let mut times = Times::default();
    let mut selections = frame_selections(instances, meshes, views, &mut times);
    let start = Instant::now();
    with_placements(
        instances,
        meshes,
        (views.eye_millimeters, views.fade_eye),
        &std::cell::RefCell::default(),
        |placements| {
            times.prepare = start.elapsed();
            if parallel {
                let start = Instant::now();
                placements.select_each(
                    &mut selections
                        .iter_mut()
                        .map(|(matrix, selection)| (*matrix, selection))
                        .collect::<Vec<_>>(),
                    4,
                );
                times.views[0] = start.elapsed();
            } else {
                for (time, (matrix, selection)) in times.views.iter_mut().zip(&mut selections) {
                    let start = Instant::now();
                    placements.select_each(&mut [(*matrix, selection)], 1);
                    *time = start.elapsed();
                }
            }
        },
    );
    times.drawn = selections
        .each_ref()
        .map(|(_, selection)| selection.stats.drawn_per_lod.iter().sum());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (matrix, selection) in &selections {
        matrix.to_cols_array().map(f32::to_bits).hash(&mut hasher);
        for range in &selection.ranges {
            (range.key, range.lod, range.slots.clone()).hash(&mut hasher);
        }
        selection.stats.drawn_per_lod.hash(&mut hasher);
        selection.stats.culled.hash(&mut hasher);
    }
    times.selected = hasher.finish();
    times
}

#[test]
#[ignore = "CPU selection timing probe: run explicitly with --release --ignored --nocapture"]
fn moving_camera_selection_cost() {
    let source = world(130_000, 60, 7);
    let instances = Instances::new(&source);
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    println!(
        "{} batches, {} placements",
        instances.batches.len(),
        instances.edits()
    );
    let per_frame = |time: Duration| time.as_secs_f64() * 1000.0 / f64::from(FRAMES);
    for parallel in [false, true] {
        let mut total = Times::default();
        let mut selected = std::collections::hash_map::DefaultHasher::new();
        // Frame zero fills the cached bounds and trees, as the first frame after loading does.
        for frame in 0..=FRAMES {
            let times = time_frame(&instances, &meshes, &walk(frame), parallel);
            times.selected.hash(&mut selected);
            if frame == 0 {
                continue;
            }
            total.fits += times.fits;
            total.prepare += times.prepare;
            for view in 0..5 {
                total.views[view] += times.views[view];
                total.drawn[view] += times.drawn[view];
            }
        }
        let selection: Duration = total.views.iter().sum();
        println!("{}", if parallel { "parallel" } else { "sequential" });
        println!("caster fits {:.2} ms/frame", per_frame(total.fits));
        println!("prepare {:.2} ms/frame", per_frame(total.prepare));
        if parallel {
            println!("five views together {:.2} ms/frame", per_frame(selection));
        } else {
            println!("camera {:.2} ms/frame", per_frame(total.views[0]));
            for cascade in 0..4 {
                let time = per_frame(total.views[cascade + 1]);
                println!("cascade {cascade} {time:.2} ms/frame");
            }
        }
        let all = total.fits + total.prepare + selection;
        println!("total {:.2} ms/frame", per_frame(all));
        println!(
            "drawn per frame: {:?}",
            total.drawn.map(|count| count / usize::from(FRAMES))
        );
        println!("selection hash {:016x}", selected.finish());
    }
}
