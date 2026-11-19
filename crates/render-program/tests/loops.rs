//! Loops whose `continuing` block uses values the body computed, in the shape DXC gives a loop
//! whose continue block computes one phi's next value from another phi.
//!
//! naga's IR evaluates a body expression where the body emits it, and the `continuing` block may
//! use it. naga 30's Metal writer prints `continuing` at the top of the next iteration, before the
//! body, and prints a body `Load` it has not named yet inline, so a phi read after an earlier phi
//! store in the same block sees the new value. The fixtures under
//! `tests/fixtures/loop_continuing/` have that shape; the stage runtime gives `continuing` a
//! snapshot of each such value, and these tests pin the IR, the Metal text and the result on the
//! GPU.

use std::collections::HashSet;
use std::sync::Arc;

use naga::back::msl;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::{AddressSpace, Block, Expression, Function, Handle, ImageClass, Statement, TypeInner};
use wgpu::ShaderStages;
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{DeviceRequirements, request_device};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::binding::{
    BindingAddress, BindingSource, ResourceBinding, ResourceKind, ResourceLayout, StorageAccess,
};
use worldrunner_render_program::module::{
    StageDeclarations, StageKind, StageModule, StageProgram, StageSource,
};
use worldrunner_render_program::runtime::{BoundResource, DispatchRequest, ProgramRuntime};

/// `out[0] = 1 | 2 | 4 | 8` from phis `i` and `acc`; `acc`'s next value shifts by `i`.
const MINIMAL: &[u8] = include_bytes!("fixtures/loop_continuing/minimal.spv");
/// Ten 3-bit indices from binding 1 packed into `out[0]`, index k at bit 3k.
const PACKED: &[u8] = include_bytes!("fixtures/loop_continuing/packed_indices.spv");
const INDICES: [u32; 10] = [1, 2, 3, 4, 5, 6, 7, 0, 7, 5];

fn words(bytes: &[u8]) -> Arc<[u32]> {
    bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
        .collect()
}

fn storage(binding: u32, access: StorageAccess) -> ResourceBinding {
    ResourceBinding {
        at: BindingAddress { set: 0, binding },
        source: BindingSource::Unnamed,
        visibility: ShaderStages::COMPUTE,
        kind: ResourceKind::Storage {
            access,
            min_size: 0,
        },
    }
}

fn kernel(bytes: &[u8], resources: Vec<ResourceBinding>) -> StageProgram {
    let module = StageModule {
        source: StageSource::SpirV(words(bytes)),
        entry_point: "main".to_owned(),
        kind: StageKind::Compute,
    };
    let declarations = StageDeclarations {
        resources: ResourceLayout::new(resources),
        constants: Vec::new(),
    };
    StageProgram::new(module, declarations).unwrap()
}

fn minimal() -> StageProgram {
    kernel(MINIMAL, vec![storage(0, StorageAccess::ReadWrite)])
}

fn packed() -> StageProgram {
    kernel(
        PACKED,
        vec![
            storage(0, StorageAccess::ReadWrite),
            storage(1, StorageAccess::ReadOnly),
        ],
    )
}

/// The `body` and `continuing` blocks of every loop of the module, outermost first.
fn loops(module: &naga::Module) -> Vec<(&Function, &Block, &Block)> {
    fn walk<'a>(
        function: &'a Function,
        block: &'a Block,
        found: &mut Vec<(&'a Function, &'a Block, &'a Block)>,
    ) {
        for statement in block {
            match statement {
                Statement::Loop {
                    body, continuing, ..
                } => {
                    found.push((function, body, continuing));
                    walk(function, body, found);
                    walk(function, continuing, found);
                }
                Statement::Block(inner) => walk(function, inner, found),
                Statement::If { accept, reject, .. } => {
                    walk(function, accept, found);
                    walk(function, reject, found);
                }
                Statement::Switch { cases, .. } => {
                    for case in cases {
                        walk(function, &case.body, found);
                    }
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    for (_, function) in module.functions.iter() {
        walk(function, &function.body, &mut found);
    }
    for entry in &module.entry_points {
        walk(&entry.function, &entry.function.body, &mut found);
    }
    found
}

fn emitted(block: &Block) -> Vec<Handle<Expression>> {
    block
        .iter()
        .filter_map(|statement| match statement {
            Statement::Emit(range) => Some(range.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The operands of the expressions these fixtures' loops are made of.
fn operands(expression: &Expression) -> Vec<Handle<Expression>> {
    match *expression {
        Expression::Binary { left, right, .. } => vec![left, right],
        Expression::Unary { expr, .. } | Expression::As { expr, .. } => vec![expr],
        Expression::Load { pointer } => vec![pointer],
        Expression::Access { base, index } => vec![base, index],
        Expression::AccessIndex { base, .. } => vec![base],
        Expression::Literal(_)
        | Expression::Constant(_)
        | Expression::LocalVariable(_)
        | Expression::GlobalVariable(_) => Vec::new(),
        ref other => panic!("the fixtures have no {other:?}"),
    }
}

/// Checks that the `continuing` block of each loop reads no `Load` the body emitted, and that
/// what it reads instead is a local the body stored that `Load` into.
fn assert_continuing_reads_snapshots(module: &naga::Module) {
    let found = loops(module);
    assert!(!found.is_empty(), "the fixture has a loop");
    for (function, body, continuing) in found {
        let body_loads: HashSet<_> = emitted(body)
            .into_iter()
            .filter(|&h| matches!(function.expressions[h], Expression::Load { .. }))
            .collect();
        let mut uses: Vec<Handle<Expression>> = Vec::new();
        for statement in continuing {
            match *statement {
                Statement::Emit(ref range) => {
                    for h in range.clone() {
                        uses.extend(operands(&function.expressions[h]));
                    }
                }
                Statement::Store { pointer, value } => uses.extend([pointer, value]),
                ref other => panic!("the fixtures' continuing blocks have no {other:?}"),
            }
        }
        for used in &uses {
            assert!(
                !body_loads.contains(used),
                "continuing uses the body's Load [{}] directly",
                used.index()
            );
        }
        let snapshots: Vec<_> = body
            .iter()
            .filter_map(|statement| match *statement {
                Statement::Store { pointer, value } if body_loads.contains(&value) => Some(pointer),
                _ => None,
            })
            .collect();
        let first = emitted(continuing)
            .into_iter()
            .next()
            .expect("continuing emits");
        let Expression::Load { pointer } = function.expressions[first] else {
            panic!("continuing starts with {:?}", function.expressions[first]);
        };
        assert!(
            snapshots.contains(&pointer),
            "continuing's first Load reads a local the body stored a Load into"
        );
    }
}

/// The options wgpu-hal 30's Metal backend writes a compute stage with on macOS 26: MSL 4.0,
/// restricted bounds checks, bounded loops and layout-numbered bindings.
fn hal_options(module: &naga::Module) -> msl::Options {
    let mut resources = msl::BindingMap::default();
    let mut buffers = 0u8;
    let mut globals: Vec<_> = module
        .global_variables
        .iter()
        .filter_map(|(_, global)| global.binding.map(|binding| (binding, global)))
        .collect();
    globals.sort_by_key(|(binding, _)| (binding.group, binding.binding));
    for (binding, global) in globals {
        let mut target = msl::BindTarget::default();
        match global.space {
            AddressSpace::Storage { access } => {
                target.buffer = Some(buffers);
                buffers += 1;
                target.mutable = access.contains(naga::StorageAccess::STORE);
            }
            _ => match module.types[global.ty].inner {
                TypeInner::Image {
                    class: ImageClass::Storage { .. },
                    ..
                } => panic!("the fixtures have no storage image"),
                ref other => panic!("the fixtures have no {other:?} binding"),
            },
        }
        resources.insert(binding, target);
    }
    let restrict = naga::proc::BoundsCheckPolicy::Restrict;
    msl::Options {
        lang_version: (4, 0),
        inline_samplers: Vec::new(),
        spirv_cross_compatibility: false,
        fake_missing_bindings: false,
        per_entry_point_map: msl::EntryPointResourceMap::from([(
            "main".to_owned(),
            msl::EntryPointResources {
                resources,
                immediates_buffer: None,
                sizes_buffer: None,
            },
        )]),
        bounds_check_policies: naga::proc::BoundsCheckPolicies {
            index: restrict,
            buffer: restrict,
            image_load: restrict,
            binding_array: naga::proc::BoundsCheckPolicy::Unchecked,
        },
        zero_initialize_workgroup_memory: true,
        force_loop_bounding: true,
        task_dispatch_limits: None,
        mesh_shader_primitive_indices_clamp: true,
        emit_int_div_checks: true,
        ray_query_initialization_tracking: true,
    }
}

/// The Metal text wgpu-hal writes for the stage's module.
fn metal(stage: &StageProgram) -> String {
    let module = stage.naga_module();
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(module)
        .unwrap();
    let pipeline = msl::PipelineOptions {
        entry_point: Some((naga::ShaderStage::Compute, "main".to_owned())),
        allow_and_force_point_size: false,
        vertex_pulling_transform: true,
        vertex_buffer_mappings: Vec::new(),
        binding_array_length_map: naga::FastHashMap::default(),
    };
    msl::write_string(module, &info, &hal_options(module), &pipeline)
        .unwrap()
        .0
}

/// The statements of each `if (!loop_init) { ... }` block (`loop_init_1` and so on for later
/// loops), which is where the Metal writer puts a loop's `continuing`.
fn continuing_blocks(text: &str) -> Vec<Vec<&str>> {
    let mut blocks = Vec::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if !(line.starts_with("if (!loop_init") && line.ends_with(") {")) {
            continue;
        }
        let mut depth = 1;
        let mut block = Vec::new();
        for line in lines.by_ref() {
            depth += line.matches('{').count();
            depth -= line.matches('}').count();
            if depth == 0 {
                break;
            }
            block.push(line.trim());
        }
        blocks.push(block);
    }
    blocks
}

fn identifiers(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
}

/// Whether `text` is a variable, its members and its elements and nothing else: what the Metal
/// writer declares a `Load` as (`int _e184 = local_7.inner[1];`).
fn is_place(text: &str) -> bool {
    let word = |text: &str| {
        text.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(text.len())
    };
    let mut rest = text.trim_end_matches(';');
    let first = word(rest);
    if first == 0 {
        return false;
    }
    rest = &rest[first..];
    while !rest.is_empty() {
        if let Some(member) = rest.strip_prefix('.') {
            rest = &member[word(member)..];
        } else if rest.starts_with('[') {
            let mut depth = 0;
            let end = rest.char_indices().find_map(|(at, c)| {
                depth += i32::from(c == '[') - i32::from(c == ']');
                (depth == 0).then_some(at)
            });
            let Some(end) = end else { return false };
            rest = &rest[end + 1..];
        } else {
            return false;
        }
    }
    true
}

/// Checks that no expression of a Metal `continuing` block reads a variable an earlier statement
/// of the block assigned, which is how the body's values are lost: the writer declares every
/// `Load` it evaluates (`uint _e5 = phi_15_;`), so a variable read inside an expression is a
/// body `Load` printed inline. A declaration of a load is one `continuing` makes itself, where
/// it stands, and may read what the block assigned.
fn assert_no_read_after_store(block: &[&str]) {
    let mut assigned: Vec<&str> = Vec::new();
    for line in block {
        let Some((left, right)) = line.split_once(" = ") else {
            continue;
        };
        // A declaration (`uint _e9 = ...`) names a new value; an assignment (`phi_15_ = ...`)
        // changes a variable later statements may read.
        let declaration = left.contains(' ');
        if declaration && is_place(right) {
            continue;
        }
        if let Some(read) = identifiers(right).find(|word| assigned.contains(word)) {
            panic!(
                "`{line}` reads {read} after the block assigned it:\n{}",
                block.join("\n")
            );
        }
        if !declaration {
            assigned.extend(identifiers(left).next());
        }
    }
}

#[test]
fn continuing_reads_a_snapshot_of_each_body_load_it_uses() {
    assert_continuing_reads_snapshots(minimal().naga_module());
    assert_continuing_reads_snapshots(packed().naga_module());
}

#[test]
fn the_metal_continuing_block_reads_body_values_before_any_phi_store() {
    for (name, stage) in [("minimal", minimal()), ("packed_indices", packed())] {
        let text = metal(&stage);
        let blocks = continuing_blocks(&text);
        assert_eq!(blocks.len(), 1, "{name}: one loop with continuing:\n{text}");
        let block = &blocks[0];
        assert_no_read_after_store(block);
        let first_store = block
            .iter()
            .position(|line| {
                line.split_once(" = ")
                    .is_some_and(|(left, _)| !left.contains(' '))
            })
            .expect("continuing stores the phis");
        let snapshot_reads: Vec<usize> = (0..block.len())
            .filter(|&at| block[at].contains("= continuing_snapshot"))
            .collect();
        assert!(
            snapshot_reads.first() == Some(&0) && snapshot_reads.iter().all(|&at| at < first_store),
            "{name}: continuing reads the snapshots first, before any store:\n{text}"
        );
    }
}

/// The same shape written in WGSL: `continuing` stores `i` first, then uses the body's `now`,
/// a value computed from it (`bit`) and, in `break if`, `now` again.
const WGSL_KERNEL: &str = "
@group(0) @binding(0) var<storage, read_write> out: array<u32, 2>;
@compute @workgroup_size(1) fn main() {
    var i = 0u;
    var acc = 0u;
    var last = 0u;
    loop {
        let now = i;
        let bit = 1u << now;
        if now >= 4u { break; }
        continuing {
            i = now + 1u;
            acc = acc | bit;
            last = now;
            break if now >= 9u;
        }
    }
    out[0] = acc;
    out[1] = last;
}";

fn wgsl_kernel() -> StageProgram {
    let module = StageModule {
        source: StageSource::Wgsl(Arc::from(WGSL_KERNEL)),
        entry_point: "main".to_owned(),
        kind: StageKind::Compute,
    };
    let declarations = StageDeclarations {
        resources: ResourceLayout::new(vec![storage(0, StorageAccess::ReadWrite)]),
        constants: Vec::new(),
    };
    StageProgram::new(module, declarations).unwrap()
}

#[test]
fn a_wgsl_continuing_block_reads_body_values_and_what_they_compute_before_any_store() {
    let text = metal(&wgsl_kernel());
    let blocks = continuing_blocks(&text);
    assert_eq!(blocks.len(), 1, "one loop with continuing:\n{text}");
    assert_no_read_after_store(&blocks[0]);
}

struct Gpu {
    device: wgpu::Device,
    authority: SubmissionAuthority,
}

impl Gpu {
    fn new() -> Self {
        let instance = worldrunner_render_gpu::test_support::test_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("the loop tests need a GPU adapter");
        eprintln!(
            "render-program loop tests adapter: {}",
            adapter.get_info().name
        );
        let (device, queue, _) =
            pollster::block_on(request_device(&adapter, &DeviceRequirements::default()))
                .expect("the adapter gives a device");
        Self {
            device,
            authority: SubmissionAuthority::new(queue),
        }
    }

    fn runtime(&self) -> ProgramRuntime {
        ProgramRuntime::new(
            self.device.clone(),
            self.authority.clone(),
            AllocationBudget(1 << 20),
        )
    }
}

fn bytes(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// Runs `stage` once with `inputs` at binding 1 when given, and returns the `count` words it
/// writes to binding 0.
fn run(stage: &StageProgram, inputs: Option<&[u32]>, count: usize) -> Vec<u32> {
    let gpu = Gpu::new();
    let mut runtime = gpu.runtime();
    let program = runtime.compute_program(stage).unwrap();
    let output = runtime
        .create_buffer("output", &bytes(&vec![0xdead_beef; count]))
        .unwrap();
    let mut bindings = vec![BoundResource::buffer(0, 0, output)];
    if let Some(inputs) = inputs {
        let input = runtime.create_buffer("indices", &bytes(inputs)).unwrap();
        bindings.push(BoundResource::buffer(0, 1, input));
    }
    let authority = runtime.authority().clone();
    let device = runtime.device().clone();
    let mut frame = authority.begin(&device).unwrap();
    runtime
        .dispatch(
            &mut frame,
            &DispatchRequest {
                program: &program,
                bindings: &bindings,
                groups: [1, 1, 1],
            },
        )
        .unwrap();
    let pending = runtime.read_buffer(&mut frame, output).unwrap();
    let index = frame.finish().submit().unwrap();
    pending
        .resolve(index)
        .unwrap()
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
        .collect()
}

#[test]
fn gpu_a_phi_the_continuing_block_stores_first_keeps_its_body_value() {
    assert_eq!(run(&minimal(), None, 1), [0b1111]);
}

#[test]
fn gpu_packed_indices_land_at_three_bits_each() {
    let expected = INDICES
        .iter()
        .zip(0..)
        .fold(0u32, |word, (&index, k)| word | (index << (3 * k)));
    let word = run(&packed(), Some(&INDICES), 1)[0];
    assert_eq!(
        word, expected,
        "word {word:#010x}, expected {expected:#010x}: each index belongs at bit 3k"
    );
}

#[test]
fn gpu_a_wgsl_continuing_block_keeps_the_body_values() {
    assert_eq!(run(&wgsl_kernel(), None, 2), [0b1111, 3]);
}
