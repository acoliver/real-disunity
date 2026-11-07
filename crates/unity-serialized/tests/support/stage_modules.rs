//! Stage modules of one program variant, built from hand-assembled SPIR-V because naga's
//! writer never produces combined image samplers. Shared by the tests of `unity-serialized`,
//! which checks the numbering, and of `unity-import`, which checks that naga accepts the
//! prepared modules.

#![allow(dead_code)]

use std::collections::BTreeMap;

/// The class of a resource a test stage declares.
#[derive(Clone, Copy, Debug)]
pub enum Kind {
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
pub struct Res {
    pub set: u32,
    pub binding: u32,
    pub kind: Kind,
}

pub const fn res(binding: u32, kind: Kind) -> Res {
    Res {
        set: 0,
        binding,
        kind,
    }
}

pub const VERTEX: u32 = 0;
pub const FRAGMENT: u32 = 4;
/// The ID bound of every test stage; IDs the preparation creates start here.
pub const BOUND: u32 = 400;
/// The ID of the first resource; the nth resource has ID `FIRST_RESOURCE + n`.
pub const FIRST_RESOURCE: u32 = 100;

pub fn op(code: u32, operands: &[u32]) -> Vec<u32> {
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

/// The types and constants every test stage declares, used or not.
fn type_declarations() -> Vec<u32> {
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
    .concat()
}

/// A stage module of `model` (0 vertex, 4 fragment) declaring `resources`, which it does not
/// use except that it samples every combined sampler with an explicit level. IDs: 1 main,
/// 2 output, 4 void, 5 function type, 7 float, 8 vec4, 11 image, 12 sampled image, 13..14
/// pointers, 16 vec2, 17 zero, 18 coordinate, 20 label, 29 colour, 31 sampler, 34 and 36
/// blocks, 32 33 35 37 their pointers; resources from [`FIRST_RESOURCE`], then pairs of
/// loaded and sampled values from 200.
pub fn stage(model: u32, resources: &[Res]) -> Vec<u32> {
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
    words.extend(type_declarations());
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

/// The descriptor set and binding each decorated ID has in `words`.
pub fn placements(words: &[u32]) -> BTreeMap<u32, (u32, u32)> {
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
pub fn place(words: &[u32], n: u32) -> (u32, u32) {
    placements(words)[&(FIRST_RESOURCE + n)]
}

/// The places of the samplers the split created: the variables with IDs from [`BOUND`].
pub fn sampler_places(words: &[u32]) -> Vec<(u32, u32)> {
    let mut places: Vec<_> = placements(words)
        .into_iter()
        .filter(|&(id, _)| id >= BOUND)
        .map(|(_, place)| place)
        .collect();
    places.sort_unstable();
    places
}

/// A variant: the stage modules of one program, vertex first.
pub type Variant = Vec<Vec<u32>>;

/// Only the fragment stage splits a sampler, at binding 1 between two buffers that both stages
/// declare.
pub fn fragment_only_split() -> Variant {
    vec![
        stage(VERTEX, &[res(0, Kind::Uniform), res(2, Kind::Storage)]),
        stage(
            FRAGMENT,
            &[
                res(0, Kind::Uniform),
                res(1, Kind::Combined),
                res(2, Kind::Storage),
            ],
        ),
    ]
}

/// The stages split different samplers, at bindings 0 and 2.
pub fn different_samplers() -> Variant {
    vec![
        stage(
            VERTEX,
            &[
                res(0, Kind::Combined),
                res(1, Kind::Uniform),
                res(3, Kind::Storage),
            ],
        ),
        stage(
            FRAGMENT,
            &[
                res(1, Kind::Uniform),
                res(2, Kind::Combined),
                res(3, Kind::Storage),
            ],
        ),
    ]
}

/// A binding that the vertex stage splits and the fragment stage declares as a plain image.
pub fn split_and_plain_image() -> Variant {
    vec![
        stage(VERTEX, &[res(0, Kind::Combined), res(1, Kind::Uniform)]),
        stage(FRAGMENT, &[res(0, Kind::Image), res(1, Kind::Uniform)]),
    ]
}

/// A resource in each of two descriptor sets, with the split only in set 0.
pub fn two_sets() -> Variant {
    let in_set = |set, binding, kind| Res { set, binding, kind };
    vec![
        stage(
            VERTEX,
            &[in_set(1, 0, Kind::Uniform), in_set(0, 0, Kind::Uniform)],
        ),
        stage(
            FRAGMENT,
            &[in_set(0, 1, Kind::Combined), in_set(1, 1, Kind::Storage)],
        ),
    ]
}

/// One stage with every kind of resource, gaps in its numbering and two splits.
pub fn every_kind() -> Variant {
    vec![stage(
        FRAGMENT,
        &[
            res(0, Kind::Combined),
            res(2, Kind::Uniform),
            res(3, Kind::Image),
            res(4, Kind::Sampler),
            res(6, Kind::Combined),
        ],
    )]
}

/// Every variant that has a shared numbering.
pub fn variants() -> Vec<Variant> {
    vec![
        fragment_only_split(),
        different_samplers(),
        split_and_plain_image(),
        two_sets(),
        every_kind(),
    ]
}
