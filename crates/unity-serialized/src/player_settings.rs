//! The identity a built player reports through `Application`: the leading fields of its
//! `PlayerSettings`, through `bundleVersion`.
//!
//! `PlayerSettings` holds every platform's settings in one object whose layout changes with
//! each Unity release, and built players strip its type tree. [`read_player_identity`] reads
//! the fields before `bundleVersion` with the layout of Unity 6000.3; [`FIELDS`] lists them in
//! order, each with whether Unity aligns the stream after it.

use crate::{SerializedError, object::ObjectReader};

/// The Unity release whose `PlayerSettings` layout [`FIELDS`] describes.
pub const LAYOUT_RELEASE: &str = "6000.3.";

/// What `Application` reports about the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerIdentity {
    pub company_name: String,
    pub product_name: String,
    /// `Application.version`, the project's bundle version.
    pub version: String,
}

/// How a `PlayerSettings` field is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Bool,
    I32,
    U32,
    F32,
    Str,
    PPtr,
    /// This many `float`s.
    Floats(usize),
    /// This many bytes, such as a GUID or a `Hash128`.
    Bytes(usize),
    /// Splash logos: a sprite reference and a duration each.
    SplashLogos,
    /// A map from string to `int`.
    StringIntMap,
    I32s,
}

use Field::{Bool, Bytes, F32, Floats, I32, I32s, PPtr, SplashLogos, Str, StringIntMap, U32};

/// The fields of Unity 6000.3's `PlayerSettings` through `bundleVersion`: name, storage, and
/// whether the stream aligns to four bytes after the field.
#[rustfmt::skip]
pub const FIELDS: &[(&str, Field, bool)] = &[
    ("productGUID", Bytes(16), false),
    ("AndroidProfiler", Bool, false),
    ("AndroidFilterTouchesWhenObscured", Bool, false),
    ("AndroidEnableSustainedPerformanceMode", Bool, true),
    ("defaultScreenOrientation", I32, false),
    ("targetDevice", I32, false),
    ("useOnDemandResources", Bool, true),
    ("accelerometerFrequency", I32, true),
    ("companyName", Str, false),
    ("productName", Str, false),
    ("defaultCursor", PPtr, false),
    ("cursorHotspot", Floats(2), false),
    ("m_SplashScreenBackgroundColor", Floats(4), false),
    ("m_ShowUnitySplashScreen", Bool, false),
    ("m_ShowUnitySplashLogo", Bool, true),
    ("m_SplashScreenOverlayOpacity", F32, false),
    ("m_SplashScreenAnimation", I32, false),
    ("m_SplashScreenLogoStyle", I32, false),
    ("m_SplashScreenDrawMode", I32, true),
    ("m_SplashScreenBackgroundAnimationZoom", F32, false),
    ("m_SplashScreenLogoAnimationZoom", F32, false),
    ("m_SplashScreenBackgroundLandscapeAspect", F32, false),
    ("m_SplashScreenBackgroundPortraitAspect", F32, false),
    ("m_SplashScreenBackgroundLandscapeUvs", Floats(4), false),
    ("m_SplashScreenBackgroundPortraitUvs", Floats(4), false),
    ("m_SplashScreenLogos", SplashLogos, false),
    ("m_SplashScreenBackgroundLandscape", PPtr, false),
    ("m_SplashScreenBackgroundPortrait", PPtr, false),
    ("m_UnitySplashLogo", PPtr, false),
    ("m_VirtualRealitySplashScreen", PPtr, true),
    ("m_HolographicTrackingLossScreen", PPtr, false),
    ("defaultScreenWidth", I32, false),
    ("defaultScreenHeight", I32, false),
    ("defaultScreenWidthWeb", I32, false),
    ("defaultScreenHeightWeb", I32, false),
    ("m_StereoRenderingPath", I32, false),
    ("m_ActiveColorSpace", I32, false),
    ("unsupportedMSAAFallback", I32, false),
    ("m_SpriteBatchMaxVertexCount", I32, false),
    ("m_SpriteBatchVertexThreshold", I32, false),
    ("m_MTRendering", Bool, false),
    ("mobileMTRenderingBaked", Bool, false),
    ("AID", Bytes(16), true),
    ("playerMinOpenGLESVersion", I32, false),
    ("mipStripping", Bool, true),
    ("numberOfMipsStripped", I32, false),
    ("numberOfMipsStrippedPerMipmapLimitGroup", StringIntMap, false),
    ("m_StackTraceTypes", I32s, false),
    ("iosShowActivityIndicatorOnLoading", I32, false),
    ("androidShowActivityIndicatorOnLoading", I32, false),
    ("iosUseCustomAppBackgroundBehavior", Bool, false),
    ("allowedAutorotateToPortrait", Bool, false),
    ("allowedAutorotateToPortraitUpsideDown", Bool, false),
    ("allowedAutorotateToLandscapeRight", Bool, false),
    ("allowedAutorotateToLandscapeLeft", Bool, false),
    ("useOSAutorotation", Bool, false),
    ("use32BitDisplayBuffer", Bool, false),
    ("preserveFramebufferAlpha", Bool, false),
    ("disableDepthAndStencilBuffers", Bool, false),
    ("androidStartInFullscreen", Bool, false),
    ("androidRenderOutsideSafeArea", Bool, false),
    ("androidUseSwappy", Bool, true),
    ("androidDisplayOptions", I32, false),
    ("androidBlitType", I32, true),
    ("androidResizeableActivity", Bool, true),
    ("androidDefaultWindowWidth", I32, false),
    ("androidDefaultWindowHeight", I32, false),
    ("androidMinimumWindowWidth", I32, false),
    ("androidMinimumWindowHeight", I32, false),
    ("androidFullscreenMode", I32, false),
    ("androidAutoRotationBehavior", I32, false),
    ("androidPredictiveBackSupport", Bool, true),
    ("androidApplicationEntry", I32, false),
    ("defaultIsNativeResolution", Bool, false),
    ("macRetinaSupport", Bool, false),
    ("runInBackground", Bool, false),
    ("muteOtherAudioSources", Bool, false),
    ("Prepare IOS For Recording", Bool, false),
    ("Force IOS Speakers When Recording", Bool, true),
    ("audioSpatialExperience", I32, false),
    ("deferSystemGesturesMode", I32, false),
    ("hideHomeButton", Bool, false),
    ("submitAnalytics", Bool, false),
    ("usePlayerLog", Bool, false),
    ("dedicatedServerOptimizations", Bool, false),
    ("bakeCollisionMeshes", Bool, false),
    ("forceSingleInstance", Bool, false),
    ("useFlipModelSwapchain", Bool, false),
    ("resizableWindow", Bool, false),
    ("useMacAppStoreValidation", Bool, true),
    ("macAppStoreCategory", Str, false),
    ("gpuSkinning", Bool, true),
    ("meshDeformation", I32, false),
    ("xboxPIXTextureCapture", Bool, false),
    ("xboxEnableAvatar", Bool, false),
    ("xboxEnableKinect", Bool, false),
    ("xboxEnableKinectAutoTracking", Bool, false),
    ("xboxEnableFitness", Bool, false),
    ("visibleInBackground", Bool, false),
    ("allowFullscreenSwitch", Bool, true),
    ("fullscreenMode", I32, true),
    ("xboxSpeechDB", U32, false),
    ("xboxEnableHeadOrientation", Bool, true),
    ("xboxEnableGuest", Bool, true),
    ("xboxEnablePIXSampling", Bool, true),
    ("metalFramebufferOnly", Bool, true),
    ("metalUseMetalDisplayLink", Bool, true),
    ("xboxOneResolution", I32, false),
    ("xboxOneSResolution", I32, false),
    ("xboxOneXResolution", I32, false),
    ("xboxOneMonoLoggingLevel", I32, false),
    ("xboxOneLoggingLevel", I32, false),
    ("xboxOneDisableEsram", Bool, true),
    ("xboxOneEnableTypeOptimization", Bool, true),
    ("xboxOnePresentImmediateThreshold", U32, false),
    ("switchQueueCommandMemory", I32, false),
    ("switchQueueControlMemory", I32, false),
    ("switchQueueComputeMemory", I32, false),
    ("switchNVNShaderPoolsGranularity", I32, false),
    ("switchNVNDefaultPoolsGranularity", I32, false),
    ("switchNVNOtherPoolsGranularity", I32, false),
    ("switchGpuScratchPoolGranularity", I32, false),
    ("switchAllowGpuScratchShrinking", Bool, true),
    ("switchNVNMaxPublicTextureIDCount", I32, false),
    ("switchNVNMaxPublicSamplerIDCount", I32, false),
    ("switchMaxWorkerMultiple", I32, false),
    ("switchNVNGraphicsFirmwareMemory", I32, false),
    ("switchGraphicsJobsSyncAfterKick", Bool, true),
    ("vulkanNumSwapchainBuffers", U32, false),
    ("vulkanEnableSetSRGBWrite", Bool, false),
    ("vulkanEnablePreTransform", Bool, false),
    ("vulkanEnableLateAcquireNextImage", Bool, false),
    ("vulkanEnableCommandBufferRecycling", Bool, true),
    ("loadStoreDebugModeEnabled", Bool, true),
    ("invalidatedPatternTexture", PPtr, true),
    ("visionOSBundleVersion", Str, false),
    ("tvOSBundleVersion", Str, false),
    ("bundleVersion", Str, false),
];

/// The project's color space, `m_ActiveColorSpace`: how the player converts colors and which
/// variants of its default render formats it picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    Gamma,
    Linear,
}

impl ColorSpace {
    fn from_stored(value: i32) -> Result<Self, SerializedError> {
        match value {
            0 => Ok(Self::Gamma),
            1 => Ok(Self::Linear),
            value => Err(SerializedError::InvalidEnum {
                what: "active color space",
                value,
            }),
        }
    }
}

/// What the leading fields of `PlayerSettings` tell the engine: the identity and the project's
/// color space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerSettings {
    pub identity: PlayerIdentity,
    pub color_space: ColorSpace,
}

/// One stored value a walk over [`FIELDS`] reports.
enum Value {
    Text(String),
    Int(i32),
    Other,
}

/// Reads the player's identity from the bytes of its `PlayerSettings` object, built by Unity
/// 6000.3.
///
/// # Errors
/// Fails when the bytes end before `bundleVersion` or a value is malformed.
pub fn read_player_identity(
    bytes: &[u8],
    big_endian: bool,
) -> Result<PlayerIdentity, SerializedError> {
    let mut identity = blank_identity();
    walk(bytes, big_endian, |name, value| {
        if let Value::Text(text) = value {
            match name {
                "companyName" => identity.company_name = text,
                "productName" => identity.product_name = text,
                "bundleVersion" => identity.version = text,
                _ => {}
            }
        }
    })?;
    Ok(identity)
}

/// Reads the identity and the color space from the bytes of the player's `PlayerSettings`
/// object, built by Unity 6000.3.
///
/// # Errors
/// Fails when the bytes end before `bundleVersion`, a value is malformed, or the color space is
/// neither gamma nor linear.
pub fn read_player_settings(
    bytes: &[u8],
    big_endian: bool,
) -> Result<PlayerSettings, SerializedError> {
    let mut identity = blank_identity();
    let mut stored_space = 0;
    walk(bytes, big_endian, |name, value| match (name, value) {
        ("companyName", Value::Text(text)) => identity.company_name = text,
        ("productName", Value::Text(text)) => identity.product_name = text,
        ("bundleVersion", Value::Text(text)) => identity.version = text,
        ("m_ActiveColorSpace", Value::Int(space)) => stored_space = space,
        _ => {}
    })?;
    Ok(PlayerSettings {
        identity,
        color_space: ColorSpace::from_stored(stored_space)?,
    })
}

fn blank_identity() -> PlayerIdentity {
    PlayerIdentity {
        company_name: String::new(),
        product_name: String::new(),
        version: String::new(),
    }
}

/// Reads every field of [`FIELDS`] in order, reporting each one's value to `visit`.
fn walk(
    bytes: &[u8],
    big_endian: bool,
    mut visit: impl FnMut(&str, Value),
) -> Result<(), SerializedError> {
    let mut r = ObjectReader::new(bytes, big_endian);
    for &(name, field, align) in FIELDS {
        visit(name, read_field(&mut r, field)?);
        if align {
            r.align()?;
        }
    }
    Ok(())
}

/// Reads one field, returning its text when it is a string and its value when it is an `int`.
fn read_field(r: &mut ObjectReader<'_>, field: Field) -> Result<Value, SerializedError> {
    match field {
        Str => return r.string().map(Value::Text),
        I32 => return r.i32().map(Value::Int),
        Bool => {
            r.bool()?;
        }
        U32 | F32 => {
            r.u32()?;
        }
        PPtr => {
            r.pptr()?;
        }
        Floats(count) => {
            for _ in 0..count {
                r.f32()?;
            }
        }
        Bytes(count) => {
            for _ in 0..count {
                r.u8()?;
            }
        }
        SplashLogos => {
            r.array("splash logo", 16, |r| {
                r.pptr()?;
                r.f32()
            })?;
        }
        StringIntMap => {
            r.array("mipmap limit group", 8, |r| {
                r.string()?;
                r.i32()
            })?;
        }
        I32s => {
            r.array("stack trace type", 4, ObjectReader::i32)?;
        }
    }
    Ok(Value::Other)
}
