//! Layouts of the classes that make up a scene's hierarchy and its scripts.

use std::ops::Range;

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

#[derive(Clone, Debug, PartialEq)]
pub struct GameObject {
    pub components: Vec<PPtr>,
    pub layer: u32,
    pub name: String,
    pub tag: u16,
    pub active: bool,
}

impl Layout for GameObject {
    const CLASS: ClassId = ClassId::GAME_OBJECT;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            components: r.pptrs("component")?,
            layer: r.u32()?,
            name: r.string()?,
            tag: r.u16()?,
            active: r.bool()?,
        })
    }
}

/// The `RectTransform` fields that follow the `Transform` fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectLayout {
    pub anchor_min: [f32; 2],
    pub anchor_max: [f32; 2],
    pub anchored_position: [f32; 2],
    pub size_delta: [f32; 2],
    pub pivot: [f32; 2],
}

/// A `Transform`, or a `RectTransform` when `rect` is present.
#[derive(Clone, Debug, PartialEq)]
pub struct Transform {
    pub game_object: PPtr,
    /// Quaternion as x, y, z, w.
    pub local_rotation: [f32; 4],
    pub local_position: [f32; 3],
    pub local_scale: [f32; 3],
    pub children: Vec<PPtr>,
    pub father: PPtr,
    pub rect: Option<RectLayout>,
}

impl Transform {
    fn read_base(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let local_rotation = r.floats()?;
        let local_position = r.floats()?;
        let local_scale = r.floats()?;
        r.align()?;
        Ok(Self {
            game_object,
            local_rotation,
            local_position,
            local_scale,
            children: r.pptrs("child")?,
            father: r.pptr()?,
            rect: None,
        })
    }
}

impl Layout for Transform {
    const CLASS: ClassId = ClassId::TRANSFORM;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Self::read_base(r)
    }
}

/// A `RectTransform`, decoded as a [`Transform`] with its rect fields.
#[derive(Clone, Debug, PartialEq)]
pub struct RectTransform(pub Transform);

impl Layout for RectTransform {
    const CLASS: ClassId = ClassId::RECT_TRANSFORM;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let mut transform = Transform::read_base(r)?;
        transform.rect = Some(RectLayout {
            anchor_min: r.floats()?,
            anchor_max: r.floats()?,
            anchored_position: r.floats()?,
            size_delta: r.floats()?,
            pivot: r.floats()?,
        });
        Ok(Self(transform))
    }
}

/// The managed class a script component instantiates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonoScript {
    pub name: String,
    pub execution_order: i32,
    pub properties_hash: [u8; 16],
    pub class_name: String,
    pub namespace: String,
    pub assembly: String,
}

impl Layout for MonoScript {
    const CLASS: ClassId = ClassId::MONO_SCRIPT;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            name: r.string()?,
            execution_order: r.i32()?,
            properties_hash: r.bytes()?,
            class_name: r.string()?,
            namespace: r.string()?,
            assembly: r.string()?,
        })
    }
}

/// A script component: the fields every `MonoBehaviour` has, then the script's own fields,
/// whose layout comes from the script class and is decoded separately.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonoBehaviour {
    pub game_object: PPtr,
    pub enabled: bool,
    pub script: PPtr,
    pub name: String,
    /// Byte range of the script's fields within the object.
    pub script_data: Range<usize>,
}

impl Layout for MonoBehaviour {
    const CLASS: ClassId = ClassId::MONO_BEHAVIOUR;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        r.align()?;
        Ok(Self {
            game_object,
            enabled,
            script: r.pptr()?,
            name: r.string()?,
            script_data: r.rest(),
        })
    }
}
