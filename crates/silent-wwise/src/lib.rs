//! A Wwise sound engine that plays nothing and reports zero for every call.
//!
//! Games built on Audiokinetic's Wwise (the binding version here is 2021.1) call the sound
//! engine through P/Invoke entry points in a native library named `AkSoundEngine`. On a
//! platform where the game ships no such library, the first call fails and the game stops
//! during audio start-up. This library exports every entry point of the 2021.1 binding,
//! `exports.txt` lists them, and each one returns zero (`false`, null, `0.0`), so the game
//! runs without sound. Real audio is later work.
//!
//! Every function takes no parameters. The binding declares arguments, but on every C calling
//! convention this engine supports the caller removes them from the stack or registers, so a
//! function that ignores its arguments is safe to call with any of them.
//!
//! `build.rs` generates the functions from `exports.txt` into `OUT_DIR`. To regenerate the
//! list from a game's `AK.Wwise.Unity.API.dll`, see the crate README.

mod entries {
    include!(concat!(env!("OUT_DIR"), "/entries.rs"));
}

#[cfg(test)]
mod list;

#[cfg(test)]
mod tests {
    use super::entries::{self, ENTRY_POINTS};
    use std::collections::BTreeSet;

    fn returns(name: &str) -> &'static str {
        ENTRY_POINTS
            .iter()
            .find(|(entry, _)| *entry == name)
            .unwrap_or_else(|| panic!("{name} is not exported"))
            .1
    }

    #[test]
    fn every_entry_point_of_the_binding_is_exported_once() {
        assert_eq!(ENTRY_POINTS.len(), 1170);
        let names: BTreeSet<_> = ENTRY_POINTS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names.len(), ENTRY_POINTS.len(), "duplicate names");
        assert!(names.iter().all(|name| name.starts_with("CSharp_")));
    }

    #[test]
    fn return_types_follow_the_binding() {
        assert_eq!(returns("CSharp_IsInitialized"), "bool");
        assert_eq!(returns("CSharp_AK_INVALID_AUDIO_OBJECT_ID_get"), "ulong");
        assert_eq!(returns("CSharp_AK_INVALID_PIPELINE_ID_get"), "uint");
        assert_eq!(
            returns("CSharp_AkAudioSettings_uNumSamplesPerFrame_set"),
            "void"
        );
        let types: BTreeSet<_> = ENTRY_POINTS.iter().map(|(_, kind)| *kind).collect();
        let expected: BTreeSet<_> = [
            "void", "int", "uint", "IntPtr", "float", "byte", "bool", "ushort", "ulong", "Vector3",
        ]
        .into_iter()
        .collect();
        assert_eq!(types, expected);
    }

    #[test]
    fn calls_report_zero_whatever_the_type() {
        assert_eq!(entries::c_sharp_is_initialized(), 0);
        assert_eq!(entries::c_sharp_ak_invalid_audio_object_id_get(), 0);
        assert_eq!(entries::c_sharp_ak_invalid_pipeline_id_get(), 0);
        entries::c_sharp_ak_audio_settings_u_num_samples_per_frame_set();
    }
}
