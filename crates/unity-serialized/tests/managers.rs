#[path = "support/decode.rs"]
mod decode;
mod support;

use decode::decode_one;
use worldrunner_unity_serialized::{
    ClassId, SerializedError,
    managers::{
        BuildFlag, BuildFlags, BuildSettings, PreloadData, ResourceDependency, ResourceManager,
        SortingLayer, TagManager,
    },
    object::{Layout, PPtr},
    render_settings::{AmbientLight, HaloAndFlare, RenderSettings, SceneFog, SceneReflections},
    write::ObjectWriter,
};

const fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

fn strings(w: &mut ObjectWriter, values: &[&str]) {
    w.i32(i32::try_from(values.len()).unwrap());
    for value in values {
        w.string(value).unwrap();
    }
}

fn owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|&v| v.to_owned()).collect()
}

fn build_bytes(big_endian: bool, flags: [bool; 14]) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    strings(&mut w, &["Assets/Start.unity", "Assets/Main.unity"]);
    strings(&mut w, &["steam_api"]);
    strings(&mut w, &["OpenVR"]);
    strings(&mut w, &["release"]);
    w.bytes(&flags.map(u8::from)).align();
    w.string("6000.3.15f1").unwrap().string("token").unwrap();
    w.i32(2).i32(21).i32(2);
    w.finish()
}

fn expected_build(flags: [bool; 14]) -> BuildSettings {
    BuildSettings {
        scenes: owned(&["Assets/Start.unity", "Assets/Main.unity"]),
        preloaded_plugins: owned(&["steam_api"]),
        enabled_vr_devices: owned(&["OpenVR"]),
        build_tags: owned(&["release"]),
        flags: BuildFlags::new(flags),
        version: "6000.3.15f1".into(),
        auth_token: "token".into(),
        graphics_apis: vec![21, 2],
    }
}

#[test]
fn decodes_build_settings_with_each_flag_alone() {
    for big_endian in [false, true] {
        for hot in 0..15 {
            let flags: [bool; 14] = std::array::from_fn(|i| i == hot);
            let decoded: BuildSettings = decode_one(
                ClassId::BUILD_SETTINGS,
                build_bytes(big_endian, flags),
                big_endian,
            )
            .unwrap();
            assert_eq!(decoded, expected_build(flags), "flag {hot}");
        }
    }
    let mut flags = [false; 14];
    flags[7] = true;
    let decoded: BuildSettings =
        decode_one(ClassId::BUILD_SETTINGS, build_bytes(false, flags), false).unwrap();
    assert!(decoded.flags.get(BuildFlag::HasShadows));
    assert!(!decoded.flags.get(BuildFlag::HasPublishingRights));
    assert!(!decoded.flags.get(BuildFlag::HasSoftShadows));
}

fn tag_bytes(big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    strings(&mut w, &["Bullet", "Unit"]);
    strings(&mut w, &["Default", "", "Water"]);
    w.i32(2).string("Background").unwrap().u32(0);
    w.string("Front").unwrap().u32(0xdead_beef);
    strings(&mut w, &["Light Layer"]);
    w.finish()
}

#[test]
fn decodes_tag_managers() {
    for big_endian in [false, true] {
        let decoded: TagManager =
            decode_one(ClassId::TAG_MANAGER, tag_bytes(big_endian), big_endian).unwrap();
        let expected = TagManager {
            tags: owned(&["Bullet", "Unit"]),
            layers: owned(&["Default", "", "Water"]),
            sorting_layers: vec![
                SortingLayer {
                    name: "Background".into(),
                    unique_id: 0,
                },
                SortingLayer {
                    name: "Front".into(),
                    unique_id: 0xdead_beef,
                },
            ],
            rendering_layers: owned(&["Light Layer"]),
        };
        assert_eq!(decoded, expected);
    }
}

fn resource_bytes(big_endian: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.i32(3).string("ui/icon").unwrap().pptr(ptr(1));
    w.string("ui/icon").unwrap().pptr(ptr(2));
    w.string("fonts/body").unwrap().pptr(PPtr {
        file_id: 3,
        path_id: -4,
    });
    w.i32(1).pptr(ptr(1)).pptrs(&[ptr(5), ptr(6)]).unwrap();
    w.finish()
}

#[test]
fn decodes_resource_managers_with_repeated_paths() {
    for big_endian in [false, true] {
        let decoded: ResourceManager = decode_one(
            ClassId::RESOURCE_MANAGER,
            resource_bytes(big_endian),
            big_endian,
        )
        .unwrap();
        let expected = ResourceManager {
            container: vec![
                ("ui/icon".into(), ptr(1)),
                ("ui/icon".into(), ptr(2)),
                (
                    "fonts/body".into(),
                    PPtr {
                        file_id: 3,
                        path_id: -4,
                    },
                ),
            ],
            dependent_assets: vec![ResourceDependency {
                object: ptr(1),
                dependencies: vec![ptr(5), ptr(6)],
            }],
        };
        assert_eq!(decoded, expected);
    }
}

fn preload_bytes(big_endian: bool, explicit: bool) -> Vec<u8> {
    let mut w = ObjectWriter::new(big_endian);
    w.string("").unwrap().pptrs(&[ptr(7), ptr(8)]).unwrap();
    strings(&mut w, &["sharedassets1.assets"]);
    w.u8(u8::from(explicit));
    w.finish()
}

#[test]
fn decodes_preload_data_that_ends_unaligned() {
    for big_endian in [false, true] {
        for explicit in [false, true] {
            let bytes = preload_bytes(big_endian, explicit);
            assert_eq!(bytes.len() % 4, 1);
            let decoded: PreloadData =
                decode_one(ClassId::PRELOAD_DATA, bytes, big_endian).unwrap();
            let expected = PreloadData {
                name: String::new(),
                assets: vec![ptr(7), ptr(8)],
                dependencies: owned(&["sharedassets1.assets"]),
                explicit_data_layout: explicit,
            };
            assert_eq!(decoded, expected);
        }
    }
}

const PROBE: [f32; 27] = [
    0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0, 5.5, 6.0, 6.5, 7.0, 7.5, 8.0, 8.5, 9.0,
    9.5, 10.0, 10.5, 11.0, 11.5, 12.0, 12.5, 13.0,
];

/// `flags` sets fog, then the radiance ambient probe.
fn render_bytes(big_endian: bool, flags: [bool; 2]) -> Vec<u8> {
    let [fog, radiance] = flags.map(u8::from);
    let mut w = ObjectWriter::new(big_endian);
    w.u8(fog).align().floats(&[0.1, 0.2, 0.3, 0.4]).i32(3);
    w.floats(&[0.01, 10.0, 300.0]);
    w.floats(&[1.0, 0.9, 0.8, 0.7])
        .floats(&[0.6, 0.5, 0.4, 0.3]);
    w.floats(&[0.2, 0.1, 0.05, 0.02]).f32(1.25).i32(1);
    w.floats(&[0.42, 0.48, 0.63, 1.0]).pptr(ptr(11));
    w.floats(&[0.5, 1.5, 0.75]).pptr(ptr(12)).pptr(ptr(13));
    w.i32(0).i32(128).i32(2).f32(0.9).pptr(ptr(14));
    w.floats(&PROBE).pptr(ptr(15)).pptr(ptr(16));
    w.floats(&[0.3, 0.35, 0.4, 1.0]).u8(radiance);
    w.finish()
}

fn expected_render(flags: [bool; 2]) -> RenderSettings {
    let [enabled, use_radiance_probe] = flags;
    RenderSettings {
        fog: SceneFog {
            enabled,
            color: [0.1, 0.2, 0.3, 0.4],
            mode: 3,
            density: 0.01,
            linear_start: 10.0,
            linear_end: 300.0,
        },
        ambient: AmbientLight {
            sky_color: [1.0, 0.9, 0.8, 0.7],
            equator_color: [0.6, 0.5, 0.4, 0.3],
            ground_color: [0.2, 0.1, 0.05, 0.02],
            intensity: 1.25,
            mode: 1,
            probe: PROBE,
            use_radiance_probe,
        },
        subtractive_shadow_color: [0.42, 0.48, 0.63, 1.0],
        skybox_material: ptr(11),
        halo_and_flare: HaloAndFlare {
            halo_strength: 0.5,
            flare_strength: 1.5,
            flare_fade_speed: 0.75,
            halo_texture: ptr(12),
        },
        spot_cookie: ptr(13),
        reflections: SceneReflections {
            mode: 0,
            resolution: 128,
            bounces: 2,
            intensity: 0.9,
            custom: ptr(14),
            generated_skybox: ptr(15),
        },
        sun: ptr(16),
        indirect_specular_color: [0.3, 0.35, 0.4, 1.0],
    }
}

#[test]
fn decodes_render_settings_with_each_flag_alone() {
    for big_endian in [false, true] {
        for flags in [[true, false], [false, true]] {
            let bytes = render_bytes(big_endian, flags);
            assert_eq!(bytes.len(), 333);
            let decoded: RenderSettings =
                decode_one(ClassId::RENDER_SETTINGS, bytes, big_endian).unwrap();
            assert_eq!(decoded, expected_render(flags), "flags {flags:?}");
        }
    }
}

fn rejects_bad_lengths<T: Layout + std::fmt::Debug>(class: ClassId, bytes: &[u8]) {
    for end in 0..bytes.len() {
        let error = decode_one::<T>(class, bytes[..end].to_vec(), false).unwrap_err();
        assert!(
            matches!(
                error,
                SerializedError::Truncated { .. } | SerializedError::InvalidCount { .. }
            ),
            "{class} prefix {end}: {error}"
        );
    }
    let mut longer = bytes.to_vec();
    longer.push(0);
    let error = decode_one::<T>(class, longer, false).unwrap_err();
    assert!(
        matches!(error, SerializedError::TrailingBytes { .. }),
        "{class}: {error}"
    );
}

#[test]
fn every_truncated_or_overlong_manager_fails() {
    rejects_bad_lengths::<BuildSettings>(ClassId::BUILD_SETTINGS, &build_bytes(false, [true; 14]));
    rejects_bad_lengths::<TagManager>(ClassId::TAG_MANAGER, &tag_bytes(false));
    rejects_bad_lengths::<ResourceManager>(ClassId::RESOURCE_MANAGER, &resource_bytes(false));
    rejects_bad_lengths::<PreloadData>(ClassId::PRELOAD_DATA, &preload_bytes(false, true));
    rejects_bad_lengths::<RenderSettings>(
        ClassId::RENDER_SETTINGS,
        &render_bytes(false, [true; 2]),
    );
}
