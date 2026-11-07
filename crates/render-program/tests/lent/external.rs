//! Programs over a set the pass owner binds, and the counts of what the runtime built to
//! prepare them.

use wgpu::util::DeviceExt;
use worldrunner_render_program::binding::{
    BindingSource, EngineSemantic, ResourceBinding, ResourceKind, ResourceLayout, StorageAccess,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::layout::LayoutError;
use worldrunner_render_program::module::StageDeclarations;
use worldrunner_render_program::runtime::{BoundResource, ExternalSet, Import, RuntimeStats};
use worldrunner_render_program::state::RenderState;

use crate::common::{
    CLEAR, FRAME, Inputs, OWN, PRODUCT, RGBA8, added, assert_halves, assert_pixel, bytes,
    fragment_uniform, frame_semantic, lent_group, lent_layout, lent_program, lent_set, no_cull,
    own_declarations, plain_declarations, plain_program, prepare, render, uniform_layout,
};
use crate::support::Gpu;

#[test]
fn an_external_set_keeps_the_pass_bound_group_across_two_programs() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let shared = [lent_set(&layout)];
    let (group, _frame) = lent_group(&gpu, &layout, FRAME);
    // `left` reads only the lent set. `right` reads it and a set of its own.
    let left = lent_program(
        &runtime,
        ("vs_left", "fs_frame"),
        StageDeclarations::default(),
        &shared,
    )
    .unwrap();
    let right = lent_program(
        &runtime,
        ("vs_right", "fs_scaled"),
        own_declarations(),
        &shared,
    )
    .unwrap();
    let own = runtime.create_buffer("own", &bytes(OWN)).unwrap();

    let mark = runtime.stats();
    let left_draw = prepare(&mut runtime, &left, &Inputs::bound(Vec::new())).unwrap();
    assert_eq!(added(mark, runtime.stats()), [2, 1, 0, 0, 1]);
    let mark = runtime.stats();
    let bindings = vec![BoundResource::buffer(1, 0, own)];
    let right_draw = prepare(&mut runtime, &right, &Inputs::bound(bindings)).unwrap();
    // Two modules, `vs_right` and `fs_scaled`, and one group, for the program's own set 1. Set 0
    // builds none.
    assert_eq!(added(mark, runtime.stats()), [2, 1, 1, 0, 1]);
    assert_eq!(left_draw.external_sets(), [0]);
    assert_eq!(right_draw.external_sets(), [0]);

    // The pass binds set 0 once; neither draw rebinds it, and both read the same colour.
    let image = render(&gpu, &[(0, &group)], &[&left_draw, &right_draw]);
    assert_halves(&image, FRAME, PRODUCT);
}

#[test]
fn a_binding_the_declaration_misses_is_refused_by_name() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    // The same shader and layout are accepted when the declaration lists the binding.
    let accepted = lent_program(
        &runtime,
        ("vs_left", "fs_frame"),
        StageDeclarations::default(),
        &[lent_set(&layout)],
    );
    assert!(accepted.is_ok());
    let declares_nothing = ExternalSet {
        bindings: ResourceLayout::default(),
        ..lent_set(&layout)
    };
    let declares_another = ExternalSet {
        bindings: ResourceLayout::new(vec![ResourceBinding {
            at: worldrunner_render_program::binding::BindingAddress { set: 0, binding: 1 },
            ..fragment_uniform(0, BindingSource::Engine(frame_semantic()))
        }]),
        ..lent_set(&layout)
    };
    for external in [declares_nothing, declares_another] {
        let refused = lent_program(
            &runtime,
            ("vs_left", "fs_frame"),
            StageDeclarations::default(),
            &[external],
        );
        assert_eq!(
            refused.err(),
            Some(ProgramError::Layout(LayoutError::UndeclaredResource {
                set: 0,
                binding: 0,
                name: "frame".to_owned(),
            }))
        );
    }
}

#[test]
fn pipelines_with_different_lent_layouts_are_distinct_cache_entries() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    // Two layouts with equal entries are still two layouts: groups built against one are not
    // groups of the other.
    let first = lent_layout(&gpu);
    let second = lent_layout(&gpu);
    let program = |layout| {
        lent_program(
            &runtime,
            ("vs_left", "fs_frame"),
            StageDeclarations::default(),
            &[lent_set(layout)],
        )
        .unwrap()
    };
    let (on_first, on_second, on_first_again) =
        (program(&first), program(&second), program(&first));
    let inputs = Inputs::bound(Vec::new());

    let first_draw = prepare(&mut runtime, &on_first, &inputs).unwrap();
    assert_eq!(runtime.cached_pipelines(), 1);
    // The same layout again is a cache hit.
    let again = prepare(&mut runtime, &on_first_again, &inputs).unwrap();
    assert_eq!(runtime.cached_pipelines(), 1);
    assert_eq!(runtime.stats().pipelines_built, 1);
    // A different layout with equal stages and state is a new pipeline.
    let second_draw = prepare(&mut runtime, &on_second, &inputs).unwrap();
    assert_eq!(runtime.cached_pipelines(), 2);
    assert_eq!(runtime.stats().pipelines_built, 2);

    // Each pipeline draws with a group of its own layout.
    let (first_group, _first) = lent_group(&gpu, &first, FRAME);
    let (second_group, _second) = lent_group(&gpu, &second, PRODUCT);
    let image = render(&gpu, &[(0, &first_group)], &[&first_draw, &again]);
    assert_halves(&image, FRAME, CLEAR);
    let image = render(&gpu, &[(0, &second_group)], &[&second_draw]);
    assert_halves(&image, PRODUCT, CLEAR);
}

#[test]
fn stats_count_pipelines_modules_groups_and_prepared_draws() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    assert_eq!(runtime.stats(), RuntimeStats::default());

    let layout = lent_layout(&gpu);
    let shared = [lent_set(&layout)];
    let (group, frame) = lent_group(&gpu, &layout, FRAME);
    let program = |vertex| {
        lent_program(&runtime, (vertex, "fs_scaled"), own_declarations(), &shared).unwrap()
    };
    let (left, right) = (program("vs_left"), program("vs_right"));
    let own = runtime.create_buffer("own", &bytes(OWN)).unwrap();
    let inputs = |state| Inputs {
        imports: Vec::new(),
        bindings: vec![BoundResource::buffer(1, 0, own)],
        state,
    };
    // Building programs compiles nothing: layouts only.
    assert_eq!(runtime.stats(), RuntimeStats::default());

    // Each step adds `[modules, pipelines, bind groups, sets, draws]`. A draw builds one group,
    // for set 1: set 0 is lent.
    let first = prepare(&mut runtime, &left, &inputs(no_cull())).unwrap();
    assert_eq!(
        added(RuntimeStats::default(), runtime.stats()),
        [2, 1, 1, 0, 1]
    );

    let mark = runtime.stats();
    let second = prepare(&mut runtime, &left, &inputs(no_cull())).unwrap();
    assert_eq!(added(mark, runtime.stats()), [0, 0, 1, 0, 1]);

    let mark = runtime.stats();
    let culled = prepare(&mut runtime, &left, &inputs(RenderState::opaque_depth())).unwrap();
    assert_eq!(added(mark, runtime.stats()), [0, 1, 1, 0, 1]);

    let mark = runtime.stats();
    let other = prepare(&mut runtime, &right, &inputs(no_cull())).unwrap();
    assert_eq!(added(mark, runtime.stats()), [1, 1, 1, 0, 1]);

    // A draw refused before anything is built prepares nothing and builds nothing.
    let mark = runtime.stats();
    let mut refused = inputs(no_cull());
    refused.imports.push(Import::buffer(
        EngineSemantic::named("nothing.takes.this"),
        frame,
    ));
    assert!(prepare(&mut runtime, &left, &refused).is_err());
    assert_eq!(added(mark, runtime.stats()), [0, 0, 0, 0, 0]);

    let total = runtime.stats();
    assert_eq!(total.modules_built, runtime.cached_modules() as u64);
    assert_eq!(total.pipelines_built, runtime.cached_pipelines() as u64);

    let image = render(&gpu, &[(0, &group)], &[&first, &second, &culled, &other]);
    assert_halves(&image, PRODUCT, PRODUCT);
}

#[test]
fn a_plain_programs_draws_count_one_group_per_set() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let left = plain_program(&runtime, "vs_left");
    let right = plain_program(&runtime, "vs_right");
    let frame = gpu.floats(wgpu::BufferUsages::UNIFORM, &FRAME);
    let own = runtime.create_buffer("own", &bytes(OWN)).unwrap();
    let inputs = |state| Inputs {
        imports: vec![Import::buffer(frame_semantic(), frame.clone())],
        bindings: vec![BoundResource::buffer(1, 0, own)],
        state,
    };

    let first = prepare(&mut runtime, &left, &inputs(no_cull())).unwrap();
    assert_eq!(
        added(RuntimeStats::default(), runtime.stats()),
        [2, 1, 2, 0, 1]
    );
    let mark = runtime.stats();
    let other = prepare(&mut runtime, &right, &inputs(no_cull())).unwrap();
    assert_eq!(added(mark, runtime.stats()), [1, 1, 2, 0, 1]);
    assert!(first.external_sets().is_empty());

    let image = render(&gpu, &[], &[&first, &other]);
    assert_halves(&image, PRODUCT, PRODUCT);
}

#[test]
fn an_import_for_an_external_set_is_refused() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let program = lent_program(
        &runtime,
        ("vs_right", "fs_scaled"),
        own_declarations(),
        &[lent_set(&layout)],
    )
    .unwrap();
    let own = runtime.create_buffer("own", &bytes(OWN)).unwrap();
    let frame = gpu.floats(wgpu::BufferUsages::UNIFORM, &FRAME);
    let inputs = Inputs {
        imports: vec![Import::buffer(frame_semantic(), frame)],
        bindings: vec![BoundResource::buffer(1, 0, own)],
        state: no_cull(),
    };
    let mark = runtime.stats();
    let refused = prepare(&mut runtime, &program, &inputs);
    assert_eq!(
        refused.err(),
        Some(ProgramError::ExternalSetConflict { set: 0 })
    );
    // Nothing was built for the refused draw.
    let after = runtime.stats();
    assert_eq!(after.bind_groups_built, mark.bind_groups_built);
    assert_eq!(after.draws_prepared, mark.draws_prepared);
}

#[test]
fn a_bound_resource_for_an_external_set_is_refused() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let program = lent_program(
        &runtime,
        ("vs_right", "fs_scaled"),
        own_declarations(),
        &[lent_set(&layout)],
    )
    .unwrap();
    let own = runtime.create_buffer("own", &bytes(OWN)).unwrap();
    let inputs = Inputs::bound(vec![
        BoundResource::buffer(1, 0, own),
        BoundResource::buffer(0, 0, own),
    ]);
    assert_eq!(
        prepare(&mut runtime, &program, &inputs).err(),
        Some(ProgramError::ExternalSetConflict { set: 0 })
    );
}

#[test]
fn a_set_lent_twice_is_refused() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let refused = lent_program(
        &runtime,
        ("vs_left", "fs_frame"),
        StageDeclarations::default(),
        &[lent_set(&layout), lent_set(&layout)],
    );
    assert_eq!(
        refused.err(),
        Some(ProgramError::ExternalSetConflict { set: 0 })
    );
}

#[test]
fn a_lent_declaration_that_names_another_set_is_refused() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let mut external = lent_set(&layout);
    external.bindings.bindings[0].at.set = 1;
    let refused = lent_program(
        &runtime,
        ("vs_left", "fs_frame"),
        StageDeclarations::default(),
        &[external],
    );
    assert!(matches!(
        refused.err(),
        Some(ProgramError::Layout(LayoutError::Declaration { .. }))
    ));
}

#[test]
fn a_declared_class_that_differs_from_the_shader_is_refused() {
    let gpu = Gpu::new();
    let runtime = gpu.runtime();
    let layout = lent_layout(&gpu);
    let mut external = lent_set(&layout);
    external.bindings.bindings[0].kind = ResourceKind::Storage {
        access: StorageAccess::ReadOnly,
        min_size: 16,
    };
    let refused = lent_program(
        &runtime,
        ("vs_left", "fs_frame"),
        StageDeclarations::default(),
        &[external],
    );
    assert!(matches!(
        refused.err(),
        Some(ProgramError::Layout(LayoutError::Mismatch {
            set: 0,
            binding: 0,
            ..
        }))
    ));
}

/// WGSL cannot say that a float texture is unfilterable. A lent layout can, and the declaration
/// the runtime checks against the shader ignores the flag.
#[test]
fn an_unfilterable_lent_texture_is_read_by_a_shader_that_cannot_declare_it() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let float = wgpu::TextureSampleType::Float { filterable: false };
    let layout = gpu
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ambient"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: float,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
    let external = ExternalSet {
        set: 0,
        layout: layout.clone(),
        bindings: ResourceLayout::new(vec![ResourceBinding {
            at: worldrunner_render_program::binding::BindingAddress { set: 0, binding: 1 },
            source: BindingSource::Engine(EngineSemantic::named("ambient")),
            visibility: wgpu::ShaderStages::FRAGMENT,
            kind: ResourceKind::Texture {
                dimension: wgpu::TextureViewDimension::D2,
                sample: float,
                multisampled: false,
            },
        }]),
    };
    let program = lent_program(
        &runtime,
        ("vs_left", "fs_ambient"),
        StageDeclarations::default(),
        &[external],
    )
    .unwrap();
    let level = 0.25_f32;
    let texture = gpu.device.create_texture_with_data(
        gpu.authority.queue(),
        &wgpu::TextureDescriptor {
            label: Some("ambient"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &level.to_le_bytes(),
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ambient"),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    let draw = prepare(&mut runtime, &program, &Inputs::bound(Vec::new())).unwrap();
    let image = render(&gpu, &[(0, &group)], &[&draw]);
    assert_halves(&image, [level, level, level, 1.0], CLEAR);
}

/// A disagreement the runtime's own check cannot see, here a layout visible to the vertex stage
/// only, is wgpu's to find when the pipeline is created.
#[test]
fn a_lent_layout_the_shader_cannot_use_surfaces_as_the_gpu_error() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let layout = uniform_layout(&gpu, wgpu::ShaderStages::VERTEX);
    let program = lent_program(
        &runtime,
        ("vs_left", "fs_frame"),
        StageDeclarations::default(),
        &[lent_set(&layout)],
    )
    .unwrap();
    let refused = prepare(&mut runtime, &program, &Inputs::bound(Vec::new()));
    assert!(matches!(refused.err(), Some(ProgramError::Gpu(_))));
    assert_eq!(runtime.stats().pipelines_built, 0);
}

#[test]
fn no_external_sets_is_the_plain_program() {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let stages = (
        crate::common::vertex("vs_left"),
        crate::common::fragment("fs_scaled", plain_declarations()),
    );
    let plain = runtime.graphics_program(&stages.0, &stages.1).unwrap();
    let none = runtime
        .graphics_program_in(&stages.0, &stages.1, &[])
        .unwrap();
    let frame = gpu.floats(wgpu::BufferUsages::UNIFORM, &FRAME);
    let own = runtime.create_buffer("own", &bytes(OWN)).unwrap();
    let inputs = Inputs {
        imports: vec![Import::buffer(frame_semantic(), frame)],
        bindings: vec![BoundResource::buffer(1, 0, own)],
        state: no_cull(),
    };
    let first = prepare(&mut runtime, &plain, &inputs).unwrap();
    let second = prepare(&mut runtime, &none, &inputs).unwrap();
    assert_eq!(runtime.stats().pipelines_built, 1);
    assert!(second.external_sets().is_empty());

    let target = gpu.texture(crate::common::SIZE, RGBA8, 1);
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    gpu.pass(
        &[crate::support::Attachment {
            view: &view,
            resolve: None,
            clear: wgpu::Color::TRANSPARENT,
        }],
        &[&first, &second],
    );
    let image = gpu.read(&target);
    assert_pixel(&image, (0, 0), PRODUCT);
    assert_pixel(&image, (1, 3), PRODUCT);
    assert_pixel(&image, (2, 0), CLEAR);
}
