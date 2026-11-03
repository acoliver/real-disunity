//! Stage modules of one program variant, built from hand-assembled SPIR-V because naga's
//! writer never produces combined image samplers.

use std::collections::BTreeMap;

use worldrunner_unity_serialized::spirv_prep::prepare;

/// The class of a resource a test stage declares.
#[derive(Clone, Copy, Debug)]
enum Kind {
    /// A combined image sampler, which the preparation splits.
    Combined,
    /// A uniform buffer.
    Uniform,
    /// A storage buffer, declared as a buffer block.
    Storage,
    /// A separate sampled image.
    Image,
    /// A separate sampler.
    Sampler,
}

/// A resource of a test stage: the descriptor set and binding Unity's module gives it.
#[derive(Clone, Copy, Debug)]
struct Res {
    set: u32,
    binding: u32,
    kind: Kind,
}

const fn res(binding: u32, kind: Kind) -> Res {
    Res {
        set: 0,
        binding,
        kind,
    }
}

const VERTEX: u32 = 0;
const FRAGMENT: u32 = 4;
/// The ID bound of every test stage; IDs the preparation creates start here.
const BOUND: u32 = 400;
/// The ID of the first resource; the nth resource has ID `FIRST_RESOURCE + n`.
const FIRST_RESOURCE: u32 = 100;

fn op(code: u32, operands: &[u32]) -> Vec<u32> {
    let len = u32::try_from(operands.len() + 1).unwrap();
    [vec![(len << 16) | code], operands.to_vec()].concat()
}

/// The pointer type of a resource and the storage class its variable has.
fn pointer_and_class(kind: Kind) -> (u32, u32) {
    match kind {
        Kind::Combined => (13, 0),
        Kind::Uniform => (35, 2),
        Kind::Storage => (37, 2),
        Kind::Image => (33, 0),
        Kind::Sampler => (32, 0),
    }
}

/// A stage module of `model` (0 vertex, 4 fragment) declaring `resources`, which it does not
/// use except that it samples every combined sampler with an explicit level. IDs: 1 main,
/// 2 output, 4 void, 5 function type, 7 float, 8 vec4, 11 image, 12 sampled image, 13..14
/// pointers, 16 vec2, 17 zero, 18 coordinate, 20 label, 29 colour, 31 sampler, 34 and 36
/// blocks, 32 33 35 37 their pointers; resources from [`FIRST_RESOURCE`], then pairs of
/// loaded and sampled values from 200.
fn stage(model: u32, resources: &[Res]) -> Vec<u32> {
    let mut words = [op(17, &[1]), op(14, &[0, 1])].concat();
    words.extend(op(15, &[model, 1, 0x6e69_616d, 0, 2]));
    if model == FRAGMENT {
        words.extend(op(16, &[1, 7]));
        words.extend(op(71, &[2, 30, 0]));
    } else {
        words.extend(op(71, &[2, 11, 0]));
    }
    words.extend(
        [
            op(71, &[34, 2]),
            op(72, &[34, 0, 35, 0]),
            op(71, &[36, 3]),
            op(72, &[36, 0, 35, 0]),
        ]
        .concat(),
    );
    for (n, resource) in (0..).zip(resources) {
        let id = FIRST_RESOURCE + n;
        words.extend(op(71, &[id, 34, resource.set]));
        words.extend(op(71, &[id, 33, resource.binding]));
    }
    words.extend(
        [
            op(19, &[4]),
            op(33, &[5, 4]),
            op(22, &[7, 32]),
            op(23, &[8, 7, 4]),
            op(32, &[14, 3, 8]),
            op(59, &[14, 2, 3]),
            op(23, &[16, 7, 2]),
            op(43, &[7, 17, 0]),
            op(44, &[16, 18, 17, 17]),
            op(44, &[8, 29, 17, 17, 17, 17]),
            op(25, &[11, 7, 1, 0, 0, 0, 1, 0]),
            op(27, &[12, 11]),
            op(32, &[13, 0, 12]),
            op(26, &[31]),
            op(32, &[32, 0, 31]),
            op(32, &[33, 0, 11]),
            op(30, &[34, 8]),
            op(32, &[35, 2, 34]),
            op(30, &[36, 8]),
            op(32, &[37, 2, 36]),
        ]
        .concat(),
    );
    for (n, resource) in (0..).zip(resources) {
        let (pointer, class) = pointer_and_class(resource.kind);
        words.extend(op(59, &[pointer, FIRST_RESOURCE + n, class]));
    }
    words.extend([op(54, &[4, 1, 0, 5]), op(248, &[20])].concat());
    let mut colour = 29;
    for (n, resource) in (0..).zip(resources) {
        if matches!(resource.kind, Kind::Combined) {
            let (loaded, sampled) = (200 + 2 * n, 201 + 2 * n);
            words.extend(op(61, &[12, loaded, FIRST_RESOURCE + n]));
            words.extend(op(88, &[8, sampled, loaded, 18, 2, 17]));
            colour = sampled;
        }
    }
    words.extend([op(62, &[2, colour]), op(253, &[]), op(56, &[])].concat());
    [vec![0x0723_0203, 0x0001_0000, 0, BOUND, 0], words].concat()
}

/// Parses and validates `words` with naga, which fails the test with naga's reason.
fn assert_naga_accepts(words: &[u32]) {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let options = naga::front::spv::Options::default();
    let module = naga::front::spv::parse_u8_slice(&bytes, &options)
        .unwrap_or_else(|error| panic!("naga cannot parse the module: {error:?}"));
    let flags = naga::valid::ValidationFlags::all();
    naga::valid::Validator::new(flags, naga::valid::Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|error| panic!("naga rejects the module: {error:?}"));
}

/// The descriptor set and binding each decorated ID has in `words`.
fn placements(words: &[u32]) -> BTreeMap<u32, (u32, u32)> {
    let mut sets = BTreeMap::new();
    let mut bindings = BTreeMap::new();
    let mut rest = &words[5..];
    while let Some(&head) = rest.first() {
        let (instruction, tail) = rest.split_at((head >> 16) as usize);
        rest = tail;
        if let (71, &[_, id, decoration, value]) = (head & 0xffff, instruction) {
            match decoration {
                34 => sets.insert(id, value),
                33 => bindings.insert(id, value),
                _ => None,
            };
        }
    }
    sets.into_iter()
        .filter_map(|(id, set)| Some((id, (set, *bindings.get(&id)?))))
        .collect()
}

/// Where a stage's nth declared resource is bound in `words`.
fn place(words: &[u32], n: u32) -> (u32, u32) {
    placements(words)[&(FIRST_RESOURCE + n)]
}

/// The places of the samplers the split created: the variables with IDs from [`BOUND`].
fn sampler_places(words: &[u32]) -> Vec<(u32, u32)> {
    let mut places: Vec<_> = placements(words)
        .into_iter()
        .filter(|&(id, _)| id >= BOUND)
        .map(|(_, place)| place)
        .collect();
    places.sort_unstable();
    places
}

#[test]
fn modules_prepared_one_at_a_time_number_a_shared_binding_differently() {
    // Only the fragment stage declares a combined sampler, at binding 1 between two buffers.
    let vertex = stage(VERTEX, &[res(0, Kind::Uniform), res(2, Kind::Storage)]);
    let fragment = stage(
        FRAGMENT,
        &[
            res(0, Kind::Uniform),
            res(1, Kind::Combined),
            res(2, Kind::Storage),
        ],
    );
    let vertex = prepare(&vertex).unwrap();
    let fragment = prepare(&fragment).unwrap();
    assert_eq!(place(&vertex.words, 1), (0, 2));
    assert_eq!(place(&fragment.words, 2), (0, 3));
    assert_eq!(sampler_places(&fragment.words), [(0, 2)]);
    assert_naga_accepts(&vertex.words);
    assert_naga_accepts(&fragment.words);
}

#[test]
fn the_split_puts_each_sampler_after_its_image_and_keeps_gaps() {
    let module = stage(
        FRAGMENT,
        &[
            res(0, Kind::Combined),
            res(2, Kind::Uniform),
            res(3, Kind::Image),
            res(4, Kind::Sampler),
            res(6, Kind::Combined),
        ],
    );
    let prepared = prepare(&module).unwrap();
    let words = &prepared.words;
    let placed: Vec<_> = (0..5).map(|n| place(words, n)).collect();
    assert_eq!(placed, [(0, 0), (0, 3), (0, 4), (0, 5), (0, 7)]);
    assert_eq!(sampler_places(words), [(0, 1), (0, 8)]);
    assert_naga_accepts(words);
}

#[test]
fn each_descriptor_set_is_numbered_on_its_own() {
    let module = stage(
        FRAGMENT,
        &[
            res(0, Kind::Combined),
            Res {
                set: 1,
                binding: 1,
                kind: Kind::Uniform,
            },
        ],
    );
    let prepared = prepare(&module).unwrap();
    assert_eq!(place(&prepared.words, 1), (1, 1));
    assert_naga_accepts(&prepared.words);
}
