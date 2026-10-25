use serde_json::json;
use std::path::Path;
use worldrunner::scene::viewer::{LiveTick, cpu_profile::CpuProfile};

pub(super) fn elapsed_rejected(tick: &LiveTick, error: &str) -> serde_json::Value {
    let dt = tick.dt.as_nanos();
    json!({"stage":"strict_elapsed_rejected", "frame":tick.frame,
        "dt_ns":dt.to_string(), "elapsed_ns":tick.elapsed.as_nanos().to_string(),
        "endpoint_ns":tick.elapsed.checked_add(tick.dt).map(|value| value.as_nanos().to_string()),
        "cap_ns":crate::strict::CAP_NS.to_string(), "chunk_ns":crate::strict::CHUNK_NS.to_string(),
        "max_chunks":crate::strict::MAX_CHUNKS,
        "required_chunks":((dt - 1) / u128::from(crate::strict::CHUNK_NS) + 1).to_string(), "error":error})
}

pub(super) fn begin(strict: bool, request: &compat_kit::RunRequest) -> Option<CpuProfile> {
    if strict && crate::playback::profiling(request, "presentation") {
        CpuProfile::begin()
    } else {
        None
    }
}

pub(super) fn flush(root: &Path, profile: Option<CpuProfile>) -> Result<(), String> {
    use std::io::Write;
    let Some(profile) = profile else {
        return Ok(());
    };
    let (rows, dropped) = profile.finish();
    let mut file = std::fs::File::create(root.join("presentation-profile.ndjson"))
        .map_err(|error| error.to_string())?;
    for row in rows {
        let event = json!({"stage":row.stage,"frame":row.frame,
            "duration_ns":row.duration.as_nanos().to_string(),"detail":row.detail,
            "clock":"cpu_wall","nested_spans_overlap":true});
        writeln!(file, "{event}").map_err(|error| error.to_string())?;
    }
    writeln!(
        file,
        "{}",
        json!({"stage":"profile_summary","limit":4096,
        "dropped":dropped,"overhead_in_strict_dt":true,"flush":"after_live_outcome"})
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejection_records_checked_endpoint_and_full_duration_without_float_conversion() {
        let mut tick = crate::desktop::tests::tick(4, vec![]);
        tick.elapsed = std::time::Duration::MAX;
        tick.dt = std::time::Duration::MAX;
        let row = elapsed_rejected(&tick, "primary");
        assert_eq!(row["endpoint_ns"], serde_json::Value::Null);
        assert_eq!(row["elapsed_ns"], tick.elapsed.as_nanos().to_string());
        assert_eq!(row["dt_ns"], tick.dt.as_nanos().to_string());
        assert_eq!(
            row["required_chunks"],
            ((tick.dt.as_nanos() - 1) / 16_666_666 + 1).to_string()
        );
        assert_eq!(row["error"], "primary");
        assert_eq!(row["frame"], 4);
    }

    #[test]
    fn bounded_spans_flush_as_decimal_nanoseconds_and_unwritable_output_is_explicit() {
        let root = tempfile::tempdir().unwrap();
        let profile = CpuProfile::begin().unwrap();
        CpuProfile::frame(3);
        drop(CpuProfile::span("native_advance"));
        flush(root.path(), Some(profile)).unwrap();
        let text =
            std::fs::read_to_string(root.path().join("presentation-profile.ndjson")).unwrap();
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows[0]["stage"], "native_advance");
        assert_eq!(rows[0]["frame"], 3);
        assert!(
            rows[0]["duration_ns"]
                .as_str()
                .unwrap()
                .parse::<u128>()
                .is_ok()
        );
        assert_eq!(rows[1]["limit"], 4096);
        assert_eq!(rows[1]["dropped"], 0);
        std::fs::remove_file(root.path().join("presentation-profile.ndjson")).unwrap();
        std::fs::create_dir(root.path().join("presentation-profile.ndjson")).unwrap();
        assert!(flush(root.path(), CpuProfile::begin()).is_err());
        assert!(CpuProfile::begin().is_some());
    }
}
