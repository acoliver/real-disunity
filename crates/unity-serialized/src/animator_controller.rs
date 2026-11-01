//! The layout of `AnimatorController` assets.
//!
//! A controller stores its layers, state machines, transitions, blend trees and parameter
//! values before the lists the engine reads here: its name and the clips it plays. The decoder
//! steps over those structures without keeping them, so that it finds where the lists start and
//! still consumes the object exactly; a layout it does not know fails with a truncation or a
//! trailing-bytes error instead of misreading the clips. The structures were read from Old
//! World's 17 controllers, which all decode to their last byte.
//!
//! The lists are, in order: the layers, the state machines, the parameter values and the seven
//! default value arrays (positions, rotations, scales, floats, integers, booleans, then one
//! more array that is empty in every Old World controller and fails to decode when it is not),
//! the table of names, the clips, and the state machine behaviours.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

/// What the engine keeps of a controller: its name and the clips it lists, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimatorController {
    pub name: String,
    pub clips: Vec<PPtr>,
}

/// Steps over an array of fixed-size entries.
fn skip<const N: usize>(
    r: &mut ObjectReader<'_>,
    what: &'static str,
) -> Result<(), SerializedError> {
    r.array(what, N, |r| r.bytes::<N>().map(drop)).map(drop)
}

fn layer(r: &mut ObjectReader<'_>) -> Result<(), SerializedError> {
    // The state machine indexes and the body mask, then the skeleton mask's elements, then the
    // binding, blending mode, default weight and flags.
    r.bytes::<20>()?;
    skip::<8>(r, "skeleton mask element")?;
    r.bytes::<16>().map(drop)
}

fn transition(r: &mut ObjectReader<'_>) -> Result<(), SerializedError> {
    skip::<16>(r, "condition")?;
    // Destination, ids, durations, exit time and flags.
    r.bytes::<40>().map(drop)
}

fn selector_transition(r: &mut ObjectReader<'_>) -> Result<(), SerializedError> {
    r.bytes::<4>()?;
    skip::<16>(r, "condition")
}

fn selector(r: &mut ObjectReader<'_>) -> Result<(), SerializedError> {
    r.array("selector transition", 4, selector_transition)?;
    r.bytes::<8>().map(drop)
}

fn blend_tree_node(r: &mut ObjectReader<'_>) -> Result<(), SerializedError> {
    r.bytes::<12>()?;
    skip::<4>(r, "child index")?;
    skip::<4>(r, "threshold")?;
    skip::<8>(r, "child position")?;
    skip::<4>(r, "child magnitude")?;
    skip::<8>(r, "child pair vector")?;
    skip::<4>(r, "child pair magnitude")?;
    r.array("neighbor list", 4, |r| skip::<4>(r, "neighbor"))?;
    skip::<4>(r, "blend event")?;
    // The normalized flag, the clip index, the duration, the cycle offset and the mirror flag.
    r.bytes::<4>()?;
    r.bytes::<16>().map(drop)
}

fn state(r: &mut ObjectReader<'_>) -> Result<(), SerializedError> {
    r.array("transition", 4, transition)?;
    skip::<4>(r, "blend tree index")?;
    r.array("blend tree", 4, |r| {
        r.array("blend tree node", 4, blend_tree_node).map(drop)
    })?;
    // Eight ids, the speed and cycle offset, and four flags.
    r.bytes::<44>().map(drop)
}

fn state_machine(r: &mut ObjectReader<'_>) -> Result<(), SerializedError> {
    r.array("state", 4, state)?;
    r.array("any state transition", 4, transition)?;
    r.array("selector", 4, selector)?;
    r.bytes::<8>().map(drop)
}

fn values(r: &mut ObjectReader<'_>) -> Result<(), SerializedError> {
    skip::<12>(r, "value")?;
    skip::<12>(r, "default position")?;
    skip::<16>(r, "default rotation")?;
    skip::<12>(r, "default scale")?;
    skip::<4>(r, "default float")?;
    skip::<4>(r, "default integer")?;
    skip::<1>(r, "default boolean")?;
    let offset = r.position();
    let count = r.i32()?;
    if count == 0 {
        return Ok(());
    }
    Err(SerializedError::InvalidCount {
        what: "reserved array entry",
        count,
        offset,
        remaining: r.remaining(),
    })
}

impl Layout for AnimatorController {
    const CLASS: ClassId = ClassId::ANIMATOR_CONTROLLER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        r.bytes::<4>()?;
        r.array("layer", 4, layer)?;
        r.array("state machine", 4, state_machine)?;
        values(r)?;
        r.array("name table entry", 8, |r| {
            r.bytes::<4>()?;
            r.string().map(drop)
        })?;
        let clips = r.pptrs("clip")?;
        r.array("state machine behaviour range", 16, |r| {
            r.bytes::<16>().map(drop)
        })?;
        skip::<4>(r, "state machine behaviour index")?;
        r.pptrs("state machine behaviour")?;
        r.bool()?;
        r.align()?;
        Ok(Self { name, clips })
    }
}
