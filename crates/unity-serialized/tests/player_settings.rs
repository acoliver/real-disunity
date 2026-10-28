use worldrunner_unity_serialized::{
    SerializedError,
    player_settings::{FIELDS, Field, PlayerIdentity, read_player_identity},
};

/// `PlayerSettings` bytes for [`FIELDS`]: every string named `companyName`, `productName` or
/// `bundleVersion` holds `text(name)`, other strings are empty, and every collection holds
/// one entry, so a wrong element size or alignment shifts the strings that follow.
fn settings(text: impl Fn(&str) -> String) -> Vec<u8> {
    let mut bytes = Vec::new();
    let align = |bytes: &mut Vec<u8>| bytes.resize(bytes.len().next_multiple_of(4), 0);
    let string = |bytes: &mut Vec<u8>, value: &str| {
        bytes.extend(u32::try_from(value.len()).unwrap().to_le_bytes());
        bytes.extend(value.as_bytes());
        align(bytes);
    };
    for &(name, field, aligned) in FIELDS {
        match field {
            Field::Bool => bytes.push(1),
            Field::I32 | Field::U32 | Field::F32 => bytes.extend(7u32.to_le_bytes()),
            Field::Str => string(&mut bytes, &text(name)),
            Field::PPtr => bytes.extend([0; 12]),
            Field::Floats(n) => bytes.extend(vec![0; n * 4]),
            Field::Bytes(n) => bytes.extend(vec![0; n]),
            Field::SplashLogos => bytes.extend([[1, 0, 0, 0].as_slice(), &[0; 16]].concat()),
            Field::StringIntMap => {
                bytes.extend(1u32.to_le_bytes());
                string(&mut bytes, "group");
                bytes.extend(2u32.to_le_bytes());
            }
            Field::I32s => bytes.extend([1u32, 3].iter().flat_map(|v| v.to_le_bytes())),
        }
        if aligned {
            align(&mut bytes);
        }
    }
    bytes.extend([0xAB; 64]);
    bytes
}

fn named(name: &str) -> String {
    match name {
        "companyName" => "Studio".to_owned(),
        "productName" => "Game".to_owned(),
        "bundleVersion" => "1.2.3".to_owned(),
        _ => String::new(),
    }
}

#[test]
fn reads_the_names_and_version_and_ignores_later_settings() {
    assert_eq!(
        read_player_identity(&settings(named), false).unwrap(),
        PlayerIdentity {
            company_name: "Studio".to_owned(),
            product_name: "Game".to_owned(),
            version: "1.2.3".to_owned(),
        }
    );
}

#[test]
fn fails_when_the_settings_end_before_the_version() {
    let bytes = settings(named);
    let end = bytes.len() - 64;
    assert!(matches!(
        read_player_identity(&bytes[..end - 4], false),
        Err(SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. })
    ));
}

#[test]
fn the_table_ends_with_the_version_and_names_the_identity_fields_once() {
    assert_eq!(FIELDS.last().map(|f| f.0), Some("bundleVersion"));
    for name in ["companyName", "productName", "bundleVersion"] {
        assert_eq!(FIELDS.iter().filter(|f| f.0 == name).count(), 1, "{name}");
    }
}

/// The table's shape as Unity 6000.3's `PlayerSettings` type tree gives it: 138 fields through
/// `bundleVersion`, these storage kinds, and alignment after these fields only.
#[test]
fn the_table_matches_the_unity_6000_3_type_tree() {
    let count = |kind: Field| FIELDS.iter().filter(|f| f.1 == kind).count();
    assert_eq!(FIELDS.len(), 138);
    assert_eq!(
        [
            count(Field::Bool),
            count(Field::I32),
            count(Field::U32),
            count(Field::F32),
            count(Field::Str),
            count(Field::PPtr),
            count(Field::I32s),
        ],
        [60, 48, 3, 5, 6, 7, 1]
    );
    let aligned: Vec<_> = FIELDS.iter().filter(|f| f.2).map(|f| f.0).collect();
    assert_eq!(
        aligned,
        [
            "AndroidEnableSustainedPerformanceMode",
            "useOnDemandResources",
            "accelerometerFrequency",
            "m_ShowUnitySplashLogo",
            "m_SplashScreenDrawMode",
            "m_VirtualRealitySplashScreen",
            "AID",
            "mipStripping",
            "androidUseSwappy",
            "androidBlitType",
            "androidResizeableActivity",
            "androidPredictiveBackSupport",
            "Force IOS Speakers When Recording",
            "useMacAppStoreValidation",
            "gpuSkinning",
            "allowFullscreenSwitch",
            "fullscreenMode",
            "xboxEnableHeadOrientation",
            "xboxEnableGuest",
            "xboxEnablePIXSampling",
            "metalFramebufferOnly",
            "metalUseMetalDisplayLink",
            "xboxOneDisableEsram",
            "xboxOneEnableTypeOptimization",
            "switchAllowGpuScratchShrinking",
            "switchGraphicsJobsSyncAfterKick",
            "vulkanEnableCommandBufferRecycling",
            "loadStoreDebugModeEnabled",
            "invalidatedPatternTexture",
        ]
    );
}
