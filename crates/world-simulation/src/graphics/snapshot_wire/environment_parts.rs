//! The encoding of the environment lighting a snapshot copies.
use super::WireError;
use super::codec::{Reader, Wire, Writer, wire_struct};
use crate::graphics::environment::{Ambient, AmbientMode, Environment, Reflection, ReflectionMode};

impl Wire for AmbientMode {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        (*self as u32).write(out)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let value = u32::read(input)?;
        Self::from_value(value).ok_or(WireError::UnknownTag(value))
    }
}

impl Wire for ReflectionMode {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        (*self as u32).write(out)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        let value = u32::read(input)?;
        Self::from_value(value).ok_or(WireError::UnknownTag(value))
    }
}

wire_struct!(Ambient {
    mode,
    sky,
    equator,
    ground,
    flat,
    intensity,
    probe
});

wire_struct!(Reflection {
    mode,
    intensity,
    custom,
    skybox
});

wire_struct!(Environment {
    ambient,
    reflection
});
