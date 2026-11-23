//! Clutter that streams around the camera: every frame the oldest pieces go, new ones appear
//! near the eye and other pieces are recycled to a new spot near it, all in one batch of object
//! edits.
use std::collections::VecDeque;

use worldrunner_scene_viewer::{ObjectId, SceneObjectEdit};

use crate::scene::{self, Stream};

/// How far from the eye new and recycled clutter appears, in millimetres.
const REACH_MM: i64 = 40_000;

pub struct Clutter {
    stream: Stream,
    /// The pieces in the order they appeared, oldest first.
    pieces: VecDeque<u64>,
    next_id: u64,
    /// How many pieces each frame removes, adds and recycles.
    each: usize,
}

impl Clutter {
    /// The pieces that the scene's object list ends with, after its `props`, and the edits to
    /// make each frame, rounded up to a multiple of three.
    pub fn new(props: usize, pieces: usize, edits: usize) -> Self {
        let first = u64::try_from(props).expect("small count");
        let end = first + u64::try_from(pieces).expect("small count");
        Self {
            stream: Stream::new(0xed17),
            pieces: (first..end).collect(),
            next_id: end,
            each: edits.div_ceil(3).min(pieces / 2),
        }
    }

    /// The next frame's edits for an eye at `eye_millimeters`, none when edits are off.
    pub fn edits(&mut self, eye_millimeters: [i64; 3]) -> Option<Vec<SceneObjectEdit>> {
        if self.each == 0 {
            return None;
        }
        let near = (eye_millimeters[0], eye_millimeters[1]);
        let mut edits = Vec::with_capacity(3 * self.each);
        for _ in 0..self.each {
            let id = self.pieces.pop_front().expect("more pieces than removals");
            edits.push(SceneObjectEdit::Remove(ObjectId(id)));
        }
        for _ in 0..self.each {
            let id = self.next_id;
            self.next_id += 1;
            self.pieces.push_back(id);
            edits.push(SceneObjectEdit::Add {
                id: ObjectId(id),
                object: scene::clutter_near(&mut self.stream, near, REACH_MM),
            });
        }
        for _ in 0..self.each {
            let last = i64::try_from(self.pieces.len()).expect("small count");
            let at = usize::try_from(self.stream.range(0, last)).expect("below the count");
            edits.push(SceneObjectEdit::Replace {
                id: ObjectId(self.pieces[at]),
                object: scene::clutter_near(&mut self.stream, near, REACH_MM),
            });
        }
        Some(edits)
    }
}
