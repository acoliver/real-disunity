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

/// Reads the player's identity from the bytes of its `PlayerSettings` object, built by Unity
/// 6000.3.
///
/// # Errors
/// Fails when the bytes end before `bundleVersion` or a value is malformed.
pub fn read_player_identity(
    bytes: &[u8],
    big_endian: bool,
) -> Result<PlayerIdentity, SerializedError> {
    let mut r = ObjectReader::new(bytes, big_endian);
    let mut identity = PlayerIdentity {
        company_name: String::new(),
        product_name: String::new(),
        version: String::new(),
    };
    for &(name, field, align) in FIELDS {
        let text = skip(&mut r, field)?;
        match name {
            "companyName" => identity.company_name = text.unwrap_or_default(),
            "productName" => identity.product_name = text.unwrap_or_default(),
            "bundleVersion" => identity.version = text.unwrap_or_default(),
            _ => {}
        }
        if align {
            r.align()?;
        }
    }
    Ok(identity)
}

/// Reads one field, returning its text when it is a string.
fn skip(r: &mut ObjectReader<'_>, field: Field) -> Result<Option<String>, SerializedError> {
    match field {
        Str => return r.string().map(Some),
        Bool => {
            r.bool()?;
        }
        I32 | U32 | F32 => {
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
    Ok(None)
}
