//! Shared helpers for versioned JSON documents.
//!
//! Domain crates own version constants and migration steps. This module only
//! provides the common sequential migration loop and error normalization.

use std::io;

use serde_json::{Map, Value};

/// The field used by every versioned OneTerm JSON document.
pub const SCHEMA_VERSION_FIELD: &str = "schema_version";

/// Read a document version, treating an absent field as the original schema.
pub fn schema_version(value: &Value) -> io::Result<u32> {
    let Some(version) = value.get(SCHEMA_VERSION_FIELD) else {
        return Ok(0);
    };
    let version = version.as_u64().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "schema version must be a non-negative integer",
        )
    })?;
    u32::try_from(version)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "schema version is too large"))
}

/// Set the current schema version on a JSON object.
pub fn set_schema_version(value: &mut Value, version: u32) -> io::Result<()> {
    let Value::Object(fields) = value else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "versioned JSON document must be an object",
        ));
    };
    fields.insert(SCHEMA_VERSION_FIELD.into(), Value::from(version));
    Ok(())
}

/// Create an empty JSON object with a current schema version.
pub fn versioned_object(version: u32) -> Value {
    let mut fields = Map::new();
    fields.insert(SCHEMA_VERSION_FIELD.into(), Value::from(version));
    Value::Object(fields)
}

/// Apply sequential, idempotent migrations to a JSON object.
///
/// The callback receives the source version and must return the next document.
/// A missing version is treated as version zero. Future versions are rejected
/// rather than silently discarded, so a newer application cannot corrupt data
/// written by an older binary.
pub fn migrate_json_value(
    mut value: Value,
    current_version: u32,
    document_name: &str,
    mut migrate_step: impl FnMut(u32, Value) -> io::Result<Value>,
) -> io::Result<Value> {
    let mut version = schema_version(&value)?;
    if version > current_version {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{document_name} schema version {version} is newer than supported version {current_version}"
            ),
        ));
    }
    while version < current_version {
        value = migrate_step(version, value)?;
        version = version
            .checked_add(1)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "schema version overflow"))?;
        set_schema_version(&mut value, version)?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_version_migrates_sequentially_and_is_idempotent() {
        let migrated = migrate_json_value(
            serde_json::json!({"value": 7}),
            2,
            "test",
            |version, mut value| {
                value[format!("step_{version}")] = Value::Bool(true);
                Ok(value)
            },
        )
        .unwrap();
        assert_eq!(migrated[SCHEMA_VERSION_FIELD], 2);
        assert_eq!(migrated["step_0"], true);
        assert_eq!(migrated["step_1"], true);
        let again = migrate_json_value(migrated.clone(), 2, "test", |_, _| {
            panic!("current documents must not migrate")
        })
        .unwrap();
        assert_eq!(again, migrated);
    }

    #[test]
    fn future_versions_are_rejected() {
        let error = migrate_json_value(
            serde_json::json!({"schema_version": 9}),
            1,
            "test",
            |_, value| Ok(value),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("newer than supported"));
    }
}
