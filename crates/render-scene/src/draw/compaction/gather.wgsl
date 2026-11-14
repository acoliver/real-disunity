// Gathers a view's selected instance records from the scene mirror into the view buffer.
// One workgroup copies one run of whole records as 32-bit words, so the result is
// bit-for-bit the bytes a buffer copy of the same records would write.

// 32-bit words in one instance record; render-scene's INSTANCE_BYTES / 4.
const RECORD_WORDS: u32 = 20u;
const LANES: u32 = 64u;

struct Run {
    // First record in the mirror.
    source: u32,
    // First record in the view buffer.
    destination: u32,
    // Records in the run.
    count: u32,
}

struct Runs {
    count: u32,
    runs: array<Run>,
}

@group(0) @binding(0) var<storage, read> runs: Runs;
@group(0) @binding(1) var<storage, read> mirror: array<u32>;
@group(0) @binding(2) var<storage, read_write> view: array<u32>;
@group(0) @binding(3) var<storage, read> previous_mirror: array<u32>;
@group(0) @binding(4) var<storage, read_write> previous_view: array<u32>;

// Workgroups are folded into rows of the per-dimension dispatch limit.
fn run_index(group: vec3<u32>, groups: vec3<u32>) -> u32 {
    return group.y * groups.x + group.x;
}

@compute @workgroup_size(64)
fn gather(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(num_workgroups) groups: vec3<u32>,
    @builtin(local_invocation_index) lane: u32,
) {
    let index = run_index(group, groups);
    if index >= runs.count {
        return;
    }
    let run = runs.runs[index];
    let source = run.source * RECORD_WORDS;
    let destination = run.destination * RECORD_WORDS;
    for (var word = lane; word < run.count * RECORD_WORDS; word += LANES) {
        view[destination + word] = mirror[source + word];
    }
}

@compute @workgroup_size(64)
fn gather_with_previous(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(num_workgroups) groups: vec3<u32>,
    @builtin(local_invocation_index) lane: u32,
) {
    let index = run_index(group, groups);
    if index >= runs.count {
        return;
    }
    let run = runs.runs[index];
    let source = run.source * RECORD_WORDS;
    let destination = run.destination * RECORD_WORDS;
    for (var word = lane; word < run.count * RECORD_WORDS; word += LANES) {
        view[destination + word] = mirror[source + word];
        previous_view[destination + word] = previous_mirror[source + word];
    }
}
