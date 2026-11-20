//! A command buffer's scissor on a real adapter (lavapipe in CI): a mesh over the whole target is
//! drawn after `EnableScissor`, so only the rectangle changes, and after `DisableScissor` or a
//! new binding of the target the whole target does.
//!
//! The rectangle counts texels from the bottom left, and `Run::at` counts rows from the bottom.

use super::{
    BLUE, GREEN, MESH, Mat4, RED, Rectangle, SIDE, bind_and_clear, draw_mesh, flat, flush,
    identity_view, mesh_of, record, scene, stored,
};
use worldrunner_world_simulation::graphics::OffscreenWork;

const WHOLE: Rectangle = Rectangle {
    x: (-1.0, 1.0),
    y: (-1.0, 1.0),
};

/// The colour each texel has, by column and row from the bottom.
fn expect_rectangle(run: &super::Run, (left, bottom, width, height): (u32, u32, u32, u32)) {
    for row in 0..SIDE {
        for column in 0..SIDE {
            let inside =
                (left..left + width).contains(&column) && (bottom..bottom + height).contains(&row);
            let expected = if inside { RED } else { GREEN };
            assert_eq!(run.at(column, row), expected, "column {column} row {row}");
        }
    }
}

#[test]
fn a_scissor_limits_a_mesh_draw_to_its_rectangle_of_the_target() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    record(
        &mut world,
        OffscreenWork::EnableScissor {
            rect: [1.0, 2.0, 2.0, 1.0],
        },
    );
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, mesh_of(&[WHOLE]))]));

    expect_rectangle(&run, (1, 2, 2, 1));
    assert!(run.outcome.report.failures.is_empty());
    assert!(run.outcome.report.skipped.is_empty());
}

#[test]
fn after_a_disable_the_next_draw_covers_the_whole_target_again() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let blue = flat(&mut world, [0.0, 0.0, 1.0, 1.0]);
    record(
        &mut world,
        OffscreenWork::EnableScissor {
            rect: [1.0, 1.0, 2.0, 2.0],
        },
    );
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));
    record(&mut world, OffscreenWork::DisableScissor);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), blue, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, mesh_of(&[WHOLE]))]));

    assert_eq!(run.columns(), [BLUE; 4]);
}

#[test]
fn binding_the_target_again_ends_the_scissor() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    record(
        &mut world,
        OffscreenWork::EnableScissor {
            rect: [0.0, 0.0, 1.0, 1.0],
        },
    );
    bind_and_clear(&mut world, drawn);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, mesh_of(&[WHOLE]))]));

    assert_eq!(run.columns(), [RED; 4]);
}

#[test]
fn a_scissor_with_no_texel_in_it_draws_nothing() {
    let (mut world, drawn) = scene();
    identity_view(&mut world);
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    record(
        &mut world,
        OffscreenWork::EnableScissor {
            rect: [2.0, 2.0, 0.0, 3.0],
        },
    );
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), red, (0, -1));

    let run = flush(&mut world, drawn, &stored([(MESH, mesh_of(&[WHOLE]))]));

    assert_eq!(run.columns(), [GREEN; 4]);
}
