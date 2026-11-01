//! The layouts of `Animator` components, `AnimatorOverrideController` assets and the names of
//! `AnimationClip` assets.

use std::ops::Range;

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

/// The flags that steer root motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootMotionFlags {
    pub apply_root_motion: bool,
    pub linear_velocity_blending: bool,
    pub stabilize_feet: bool,
}

/// The flags about the animated hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HierarchyFlags {
    pub has_transform_hierarchy: bool,
    pub allow_constant_clip_sampling: bool,
}

/// What the animator does with its state when it is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisableFlags {
    pub keep_state: bool,
    pub write_defaults: bool,
}

/// An `Animator` component. `avatar` and `controller` are null when it has none.
///
/// Old World's animators are 56 bytes: the game object, the enabled byte padded to four, the
/// avatar and controller references, the culling and update modes, then two groups of four
/// flag bytes. The flags are named for the order Unity's inspector lists them in; the layout
/// shows only that the first group's first byte and the second group's first two bytes vary
/// (the others are zero in all 470 Old World animators), so those three are the ones whose
/// names rest on their observed values.
#[derive(Clone, Debug, PartialEq)]
pub struct Animator {
    pub game_object: PPtr,
    pub enabled: bool,
    pub avatar: PPtr,
    pub controller: PPtr,
    /// An `AnimatorCullingMode` value: 0 always animate, 1 cull update transforms, 2 cull
    /// completely.
    pub culling_mode: i32,
    /// An `AnimatorUpdateMode` value: 0 normal, 1 animate physics, 2 unscaled time.
    pub update_mode: i32,
    pub motion: RootMotionFlags,
    pub hierarchy: HierarchyFlags,
    pub on_disable: DisableFlags,
}

impl Layout for Animator {
    const CLASS: ClassId = ClassId::ANIMATOR;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        r.align()?;
        let [avatar, controller] = [r.pptr()?, r.pptr()?];
        let [culling_mode, update_mode] = [r.i32()?, r.i32()?];
        let [root, linear, stabilize, _] = r.bytes::<4>()?;
        let [hierarchy, constant_clips, keep, defaults] = r.bytes::<4>()?;
        Ok(Self {
            game_object,
            enabled,
            avatar,
            controller,
            culling_mode,
            update_mode,
            motion: RootMotionFlags {
                apply_root_motion: root != 0,
                linear_velocity_blending: linear != 0,
                stabilize_feet: stabilize != 0,
            },
            hierarchy: HierarchyFlags {
                has_transform_hierarchy: hierarchy != 0,
                allow_constant_clip_sampling: constant_clips != 0,
            },
            on_disable: DisableFlags {
                keep_state: keep != 0,
                write_defaults: defaults != 0,
            },
        })
    }
}

/// A clip of a base controller and the clip that replaces it. A null replacement removes the
/// original.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipOverride {
    pub original: PPtr,
    pub replacement: PPtr,
}

/// An `AnimatorOverrideController`: a base controller and the clips it replaces.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimatorOverrideController {
    pub name: String,
    pub base_controller: PPtr,
    pub overrides: Vec<ClipOverride>,
}

/// Two references.
const OVERRIDE_LEN: usize = 24;

impl Layout for AnimatorOverrideController {
    const CLASS: ClassId = ClassId::ANIMATOR_OVERRIDE_CONTROLLER;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name: r.string()?,
            base_controller: r.pptr()?,
            overrides: r.array("clip override", OVERRIDE_LEN, |r| {
                Ok(ClipOverride {
                    original: r.pptr()?,
                    replacement: r.pptr()?,
                })
            })?,
        })
    }
}

/// An `AnimationClip`'s name. The curves, events and settings after it stay in the object
/// bytes at `rest`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimationClip {
    pub name: String,
    pub rest: Range<usize>,
}

impl Layout for AnimationClip {
    const CLASS: ClassId = ClassId::ANIMATION_CLIP;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let name = r.string()?;
        let rest = r.rest()?;
        Ok(Self { name, rest })
    }
}
