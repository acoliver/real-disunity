//! Content keys for the pipeline cache.

use std::sync::Mutex;

use crate::packet::{PipelineRequest, TargetSignature};
use crate::primitive::Primitive;
use crate::state::RenderState;

/// A hash of everything a pipeline is built from, so equal inputs share one pipeline.
///
/// Plain-data descriptors are hashed through their `Debug` text, which names every field.
pub(crate) struct Key(blake3::Hasher);

impl Key {
    pub(crate) fn new(kind: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(kind.as_bytes());
        Self(hasher)
    }

    pub(crate) fn hash(&mut self, hash: blake3::Hash) {
        self.0.update(hash.as_bytes());
    }

    pub(crate) fn debug(&mut self, value: &impl std::fmt::Debug) {
        self.0.update(format!("{value:?}").as_bytes());
    }

    /// Adds the identity of a wgpu object that compares by identity, such as a bind group layout:
    /// two handles to one object add the same bytes and two objects with equal descriptors add
    /// different ones.
    pub(crate) fn identity(&mut self, object: &impl std::hash::Hash) {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        object.hash(&mut hasher);
        self.0.update(&hasher.finish().to_le_bytes());
    }

    pub(crate) fn finish(&self) -> blake3::Hash {
        self.0.finalize()
    }
}

/// The content keys of the render pipelines a program has been asked for, so a draw prepared
/// again in a later frame finds its key without formatting and hashing its descriptors again.
///
/// A program's stages and layouts never change once it is built, so the key of a request for it
/// depends only on what the request names beyond the program. Clones of a program share these.
#[derive(Default)]
pub(crate) struct RequestKeys(Mutex<Vec<(RequestShape, blake3::Hash)>>);

impl RequestKeys {
    /// The key of `request`, from `compute` the first time a request like it is made.
    pub(crate) fn get_or_compute(
        &self,
        request: &PipelineRequest<'_>,
        compute: impl FnOnce() -> blake3::Hash,
    ) -> blake3::Hash {
        let mut keys = self.0.lock().expect("pipeline key lock");
        if let Some((_, key)) = keys.iter().find(|(shape, _)| shape.matches(request)) {
            return *key;
        }
        let key = compute();
        // A state with a NaN depth bias never equals itself, so its request is keyed again each
        // time; its key is still held once.
        if keys.iter().all(|(_, held)| *held != key) {
            keys.push((RequestShape::of(request), key));
        }
        key
    }
}

/// Everything a pipeline request's key covers beyond its program.
struct RequestShape {
    state: RenderState,
    signature: TargetSignature,
    layouts: Vec<(u64, wgpu::VertexStepMode, Vec<wgpu::VertexAttribute>)>,
    primitive: Primitive,
}

impl RequestShape {
    fn of(request: &PipelineRequest<'_>) -> Self {
        Self {
            state: request.state.clone(),
            signature: request.signature,
            layouts: request
                .layouts
                .iter()
                .map(|layout| (layout.stride, layout.step, layout.attributes.to_vec()))
                .collect(),
            primitive: request.primitive,
        }
    }

    fn matches(&self, request: &PipelineRequest<'_>) -> bool {
        self.state == *request.state
            && self.signature == request.signature
            && self.primitive == request.primitive
            && self.layouts.len() == request.layouts.len()
            && self.layouts.iter().zip(request.layouts).all(
                |((stride, step, attributes), layout)| {
                    *stride == layout.stride
                        && *step == layout.step
                        && attributes.as_slice() == layout.attributes
                },
            )
    }
}
