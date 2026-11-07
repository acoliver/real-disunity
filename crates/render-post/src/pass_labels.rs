//! Encoder diagnostics share the timer's typed identities, independently of query support.
use std::{borrow::Cow, fmt};

use crate::{PassId, PassSlot, StageId};

impl PassId {
    /// The spelling shared by encoder diagnostics and timing reports. Authored labels
    /// with levels or occurrences allocate and must be retained when building a schedule.
    /// Indexed geometry uses a bounded stack buffer.
    pub fn encoder_label(self) -> impl AsRef<str> {
        match self {
            Self::Geometry(slot) => geometry_label(slot),
            Self::Resolve => EncoderLabel::Static("Resolve"),
            Self::Stage(stage) => EncoderLabel::Authored(stage.diagnostic_label()),
            Self::Operation(_) => EncoderLabel::Authored(Cow::Owned(format!("{self:?}"))),
        }
    }
}

impl fmt::Display for PassId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.encoder_label().as_ref())
    }
}

impl StageId {
    pub(crate) fn diagnostic_label(self) -> Cow<'static, str> {
        if self.level == 0 {
            Cow::Borrowed(self.name)
        } else {
            Cow::Owned(format!("{} level {}", self.name, self.level))
        }
    }
}

enum EncoderLabel {
    Static(&'static str),
    Authored(Cow<'static, str>),
    Indexed { bytes: [u8; 64], length: usize },
}

impl EncoderLabel {
    fn indexed(name: &'static str, index: u32) -> Self {
        use std::io::Write;
        // The longest geometry name plus a decimal u32 and delimiters fits in 64 bytes.
        let mut bytes = [0; 64];
        let mut remaining = bytes.as_mut_slice();
        write!(remaining, "Geometry({name}({index}))").expect("bounded geometry label");
        let unused = remaining.len();
        let length = bytes.len() - unused;
        Self::Indexed { bytes, length }
    }
}

impl AsRef<str> for EncoderLabel {
    fn as_ref(&self) -> &str {
        match self {
            Self::Static(name) => name,
            Self::Authored(name) => name.as_ref(),
            Self::Indexed { bytes, length } => {
                std::str::from_utf8(&bytes[..*length]).expect("ASCII geometry label")
            }
        }
    }
}

fn geometry_label(slot: PassSlot) -> EncoderLabel {
    use EncoderLabel::Static;
    match slot {
        PassSlot::ShadowCasters(index) => EncoderLabel::indexed("ShadowCasters", index),
        PassSlot::DepthNormalPrepass => Static("Geometry(DepthNormalPrepass)"),
        PassSlot::DepthPyramid(index) => EncoderLabel::indexed("DepthPyramid", index),
        PassSlot::OcclusionMetadata(index) => EncoderLabel::indexed("OcclusionMetadata", index),
        PassSlot::Ssao => Static("Geometry(Ssao)"),
        PassSlot::SsaoBlurHorizontal => Static("Geometry(SsaoBlurHorizontal)"),
        PassSlot::SsaoBlur => Static("Geometry(SsaoBlur)"),
        PassSlot::Celestial => Static("Geometry(Celestial)"),
        PassSlot::Opaque => Static("Geometry(Opaque)"),
        PassSlot::AfterOpaqueSnapshot => Static("Geometry(AfterOpaqueSnapshot)"),
        PassSlot::AfterOpaqueDepth => Static("Geometry(AfterOpaqueDepth)"),
        PassSlot::TransparentWater => Static("Geometry(TransparentWater)"),
        PassSlot::TransparentWaterMask => Static("Geometry(TransparentWaterMask)"),
        PassSlot::TransparentAlpha => Static("Geometry(TransparentAlpha)"),
        PassSlot::AfterWaterDepth => Static("Geometry(AfterWaterDepth)"),
        PassSlot::ReflectionDepthPyramid(index) => {
            EncoderLabel::indexed("ReflectionDepthPyramid", index)
        }
        PassSlot::ReflectionTrace => Static("Geometry(ReflectionTrace)"),
        PassSlot::ReflectionMip(index) => EncoderLabel::indexed("ReflectionMip", index),
        PassSlot::ScreenSpaceAfterTransparent => Static("Geometry(ScreenSpaceAfterTransparent)"),
    }
}
