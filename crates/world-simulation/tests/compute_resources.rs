//! Compute shaders and compute buffers: creation, limits, data ranges, edits and generations.
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, ComputeKernel, ComputeShaderId, GraphicsError, ShaderSource,
    WorldComputeShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn kernel(name: &str, size: Option<[u32; 3]>) -> ComputeKernel {
    ComputeKernel {
        name: name.into(),
        thread_group_size: size,
    }
}

fn shader(kernels: Vec<ComputeKernel>) -> WorldComputeShader {
    WorldComputeShader {
        name: "TerrainReadBackShader".into(),
        source: ShaderSource {
            file: "resources.assets".into(),
            path_id: 8167,
        },
        kernels,
        generation: 0,
    }
}

fn graphics_error<T: std::fmt::Debug>(result: Result<T, WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn two_kernels(world: &mut World) -> ComputeShaderId {
    world
        .create_compute_shader(shader(vec![
            kernel("First", Some([8, 8, 1])),
            kernel("Second", None),
        ]))
        .unwrap()
}

#[test]
fn a_created_compute_shader_reads_back_with_generation_one() {
    let mut world = world();
    let id = two_kernels(&mut world);
    let held = world.compute_shader(id).unwrap();
    assert_eq!(held.name, "TerrainReadBackShader");
    assert_eq!(held.source.file, "resources.assets");
    assert_eq!(held.source.path_id, 8167);
    assert_eq!(held.generation, 1);
    assert_eq!(held.kernels.len(), 2);
    assert_eq!(held.kernels[0].thread_group_size, Some([8, 8, 1]));
    assert_eq!(held.kernels[1].thread_group_size, None);
    assert_eq!(world.compute_shaders().count(), 1);
}

#[test]
fn a_host_filling_in_a_group_size_raises_the_generation() {
    let mut world = world();
    let id = two_kernels(&mut world);
    world
        .set_kernel_thread_group_size(id, 1, [16, 4, 2])
        .unwrap();
    let held = world.compute_shader(id).unwrap();
    assert_eq!(held.kernels[1].thread_group_size, Some([16, 4, 2]));
    assert_eq!(held.kernels[0].thread_group_size, Some([8, 8, 1]));
    assert_eq!(held.generation, 2);
}

#[test]
fn group_sizes_follow_the_hardware_limits() {
    let mut world = world();
    let id = two_kernels(&mut world);
    for size in [
        [0, 1, 1],
        [1, 0, 1],
        [1, 1, 0],
        [1025, 1, 1],
        [1, 1025, 1],
        [1, 1, 65],
        [32, 32, 2],
    ] {
        let result = world.set_kernel_thread_group_size(id, 1, size);
        assert_eq!(
            graphics_error(result),
            GraphicsError::InvalidComputeShader,
            "{size:?}"
        );
    }
    world
        .set_kernel_thread_group_size(id, 1, [1024, 1, 1])
        .unwrap();
    world
        .set_kernel_thread_group_size(id, 1, [16, 16, 4])
        .unwrap();
    assert_eq!(world.compute_shader(id).unwrap().generation, 3);
}

#[test]
fn a_kernel_the_shader_lacks_is_refused() {
    let mut world = world();
    let id = two_kernels(&mut world);
    let result = world.set_kernel_thread_group_size(id, 2, [1, 1, 1]);
    assert_eq!(graphics_error(result), GraphicsError::InvalidKernel);
}

#[test]
fn a_compute_shader_needs_a_name_a_source_and_a_bounded_kernel_list() {
    let mut world = world();
    let mut nameless = shader(vec![kernel("A", None)]);
    nameless.name.clear();
    assert_eq!(
        graphics_error(world.create_compute_shader(nameless)),
        GraphicsError::InvalidName
    );
    let mut sourceless = shader(vec![kernel("A", None)]);
    sourceless.source.file.clear();
    assert_eq!(
        graphics_error(world.create_compute_shader(sourceless)),
        GraphicsError::InvalidShaderSource
    );
    assert_eq!(
        graphics_error(world.create_compute_shader(shader(vec![kernel("", None)]))),
        GraphicsError::InvalidName
    );
    assert_eq!(
        graphics_error(world.create_compute_shader(shader(vec![kernel("A", Some([0, 1, 1]))]))),
        GraphicsError::InvalidComputeShader
    );
    let many = (0..257).map(|n| kernel(&format!("K{n}"), None)).collect();
    assert_eq!(
        graphics_error(world.create_compute_shader(shader(many))),
        GraphicsError::InvalidComputeShader
    );
    let fits = (0..256).map(|n| kernel(&format!("K{n}"), None)).collect();
    world.create_compute_shader(shader(fits)).unwrap();
    assert_eq!(world.compute_shaders().count(), 1);
}

#[test]
fn a_destroyed_compute_shader_is_stale_and_its_slot_gets_a_new_generation() {
    let mut world = world();
    let first = two_kernels(&mut world);
    world.destroy_compute_shader(first).unwrap();
    assert_eq!(
        graphics_error(world.compute_shader(first)),
        GraphicsError::StaleComputeShader
    );
    assert_eq!(
        graphics_error(world.destroy_compute_shader(first)),
        GraphicsError::StaleComputeShader
    );
    let second = two_kernels(&mut world);
    assert_ne!(first, second);
    assert!(world.compute_shader(second).is_ok());
    assert!(world.compute_shader(first).is_err());
    assert!(GraphicsError::StaleComputeShader.is_stale());
    assert!(GraphicsError::StaleComputeBuffer.is_stale());
}

#[test]
fn a_new_compute_buffer_is_zeroed_and_sized_by_count_and_stride() {
    let mut world = world();
    let id = world.create_compute_buffer(4, 8, 2).unwrap();
    let held = world.compute_buffer(id).unwrap();
    assert_eq!((held.count, held.stride, held.kind), (4, 8, 2));
    assert_eq!(held.contents, vec![0; 32]);
    assert_eq!(held.generation, 1);
    assert_eq!(world.compute_buffers().count(), 1);
}

#[test]
fn compute_buffer_strides_are_multiples_of_four_up_to_2048() {
    let mut world = world();
    for stride in [0, 1, 2, 3, 6, 2049, 2052, 4096] {
        let result = world.create_compute_buffer(1, stride, 0);
        assert_eq!(
            graphics_error(result),
            GraphicsError::InvalidComputeBuffer,
            "{stride}"
        );
    }
    for stride in [4, 8, 2044, 2048] {
        world.create_compute_buffer(1, stride, 0).unwrap();
    }
}

#[test]
fn compute_buffers_hold_at_most_256_mib() {
    let mut world = world();
    let result = world.create_compute_buffer(0, 4, 0);
    assert_eq!(graphics_error(result), GraphicsError::InvalidComputeBuffer);
    let over = world.create_compute_buffer((256 << 20) / 4 + 1, 4, 0);
    assert_eq!(graphics_error(over), GraphicsError::InvalidComputeBuffer);
    let overflow = world.create_compute_buffer(u32::MAX, 2048, 0);
    assert_eq!(
        graphics_error(overflow),
        GraphicsError::InvalidComputeBuffer
    );
    let exact = world.create_compute_buffer((256 << 20) / 4, 4, 0).unwrap();
    assert_eq!(
        world.compute_buffer(exact).unwrap().contents.len(),
        256 << 20
    );
}

#[test]
fn data_written_at_an_offset_lands_there_and_raises_the_generation() {
    let mut world = world();
    let id = world.create_compute_buffer(4, 4, 0).unwrap();
    world.set_compute_buffer_data(id, 4, &[1, 2, 3, 4]).unwrap();
    let held = world.compute_buffer(id).unwrap();
    assert_eq!(
        held.contents,
        vec![0, 0, 0, 0, 1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(held.generation, 2);
}

#[test]
fn data_ranges_must_fit_the_buffer() {
    let mut world = world();
    let id = world.create_compute_buffer(2, 4, 0).unwrap();
    world.set_compute_buffer_data(id, 0, &[7; 8]).unwrap();
    world.set_compute_buffer_data(id, 8, &[]).unwrap();
    for (offset, length) in [(0, 9), (1, 8), (8, 1), (9, 0), (usize::MAX, 2)] {
        let result = world.set_compute_buffer_data(id, offset, &vec![9; length]);
        assert_eq!(
            graphics_error(result),
            GraphicsError::ComputeRange,
            "{offset} {length}"
        );
    }
    assert_eq!(world.compute_buffer(id).unwrap().contents, vec![7; 8]);
}

#[test]
fn a_destroyed_compute_buffer_is_stale() {
    let mut world = world();
    let id: ComputeBufferId = world.create_compute_buffer(1, 4, 0).unwrap();
    world.destroy_compute_buffer(id).unwrap();
    assert_eq!(
        graphics_error(world.compute_buffer(id)),
        GraphicsError::StaleComputeBuffer
    );
    assert_eq!(
        graphics_error(world.set_compute_buffer_data(id, 0, &[1])),
        GraphicsError::StaleComputeBuffer
    );
    assert_eq!(
        graphics_error(world.destroy_compute_buffer(id)),
        GraphicsError::StaleComputeBuffer
    );
}

#[test]
fn all_compute_buffers_together_stay_within_a_budget() {
    let mut world = world();
    let big = (256 << 20) / 4;
    let ids: Vec<_> = (0..4)
        .map(|_| world.create_compute_buffer(big, 4, 0).unwrap())
        .collect();
    let result = world.create_compute_buffer(1, 4, 0);
    assert_eq!(graphics_error(result), GraphicsError::InvalidComputeBuffer);
    world.destroy_compute_buffer(ids[0]).unwrap();
    world.create_compute_buffer(1, 4, 0).unwrap();
}
