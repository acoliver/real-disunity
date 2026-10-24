use crate::{
    FieldOverride, ImportError, catalog::Project, prefab::Record, property::get_property,
    source_io::error,
};

pub(crate) fn validate(
    patch: &FieldOverride,
    record: &Record,
    project: &Project,
) -> Result<(), ImportError> {
    let Some(expected) = &patch.precondition else {
        return Err(error(&record.path, "overlay precondition required"));
    };
    if expected.schema_version != 1 || expected.class_id != record.doc.class_id {
        return Err(error(
            &record.path,
            "overlay precondition schema/class mismatch",
        ));
    }
    if get_property(&record.doc.fields, &patch.property_path) != Some(&expected.expected_value) {
        return Err(error(
            &record.path,
            format!(
                "overlay precondition old value/shape mismatch: {}:{} {}",
                patch.guid, patch.file_id, patch.property_path
            ),
        ));
    }
    if expected
        .source_sha256
        .as_ref()
        .is_some_and(|hash| project.hashes.get(&record.path) != Some(hash))
    {
        return Err(error(
            &record.path,
            "overlay precondition source hash mismatch",
        ));
    }
    Ok(())
}
