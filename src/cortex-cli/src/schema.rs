//! Shipped JSON Schemas for headless CLI result documents.
//!
//! `cortex run --format json` and `cortex exec --output-format json` both emit a
//! single result document. CI consumers pin that shape, so the schema is a
//! shipped contract: [`cortex schema <name>`](crate::schema_cmd) prints it and
//! `--json-schema` validates the document before it reaches stdout.
//!
//! Documents are validated structurally here rather than through a JSON Schema
//! engine, so the check stays dependency-free and deterministic in CI.

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

/// Schema name for a `cortex run --format json` result document.
pub const RUN_RESULT_SCHEMA: &str = "run-result";

/// Schema name for a `cortex exec --output-format json` result document.
pub const EXEC_RESULT_SCHEMA: &str = "exec-result";

/// Every schema this build ships, in listing order.
pub const SCHEMA_NAMES: &[&str] = &[RUN_RESULT_SCHEMA, EXEC_RESULT_SCHEMA];

/// Return the schema document for `name`, or `None` when it is unknown.
pub fn schema_document(name: &str) -> Option<Value> {
    match name {
        RUN_RESULT_SCHEMA => Some(run_result_schema()),
        EXEC_RESULT_SCHEMA => Some(exec_result_schema()),
        _ => None,
    }
}

fn run_result_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://cortex.foundation/schemas/run-result.json",
        "title": "Cortex run result",
        "description": "One document printed by `cortex run --format json`.",
        "type": "object",
        "required": [
            "type",
            "session_id",
            "message",
            "events",
            "success",
            "interrupted",
            "complete",
            "truncated",
        ],
        "properties": {
            "type": { "const": "result" },
            "session_id": { "type": "string" },
            "message": { "type": "string" },
            "events": { "type": "integer", "minimum": 0 },
            "success": { "type": "boolean" },
            "interrupted": { "type": "boolean" },
            "complete": { "type": "boolean" },
            "truncated": { "type": "boolean" },
            "finish_reason": { "type": ["string", "null"] }
        },
        "additionalProperties": true
    })
}

fn exec_result_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://cortex.foundation/schemas/exec-result.json",
        "title": "Cortex exec result",
        "description": "One document printed by `cortex exec --output-format json`.",
        "type": "object",
        "required": ["type", "subtype", "is_error", "duration_ms", "num_turns", "session_id"],
        "properties": {
            "type": { "const": "result" },
            "subtype": { "enum": ["success", "error"] },
            "is_error": { "type": "boolean" },
            "result": { "type": "string" },
            "error": { "type": "string" },
            "duration_ms": { "type": "integer", "minimum": 0 },
            "num_turns": { "type": "integer", "minimum": 0 },
            "session_id": { "type": "string" }
        },
        "additionalProperties": true
    })
}

/// Property name → JSON type name, for the subset the shipped schemas use.
fn expected_type(schema: &Value, property: &str) -> Option<Vec<String>> {
    let declared = declared_property(schema, property)?;
    if let Some(name) = declared.get("type").and_then(Value::as_str) {
        return Some(vec![name.to_string()]);
    }
    declared.get("type").and_then(Value::as_array).map(|types| {
        types
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// The declared property object for `property`, when the schema names it.
fn declared_property<'a>(schema: &'a Value, property: &str) -> Option<&'a Value> {
    schema.get("properties")?.get(property)
}

fn value_matches(value: &Value, allowed: &[String]) -> bool {
    allowed.iter().any(|name| match name.as_str() {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => true,
    })
}

/// Validate `document` against the shipped schema named `name`.
///
/// Returns every violation, so a failing CI run reports the whole shape rather
/// than the first field it tripped over.
pub fn validate(name: &str, document: &Value) -> Result<()> {
    let Some(schema) = schema_document(name) else {
        bail!("Unknown schema `{name}`.");
    };
    let Some(object) = document.as_object() else {
        bail!("Schema `{name}` expects a JSON object.");
    };

    let mut violations: Vec<String> = Vec::new();

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(field) {
                violations.push(format!("missing required property `{field}`"));
            }
        }
    }

    for (property, value) in object {
        let Some(declared) = declared_property(&schema, property) else {
            continue;
        };
        if let Some(allowed) = expected_type(&schema, property)
            && !value_matches(value, &allowed)
        {
            violations.push(format!(
                "`{property}` must be {} but is {}",
                allowed.join(" or "),
                type_name(value)
            ));
        }
        if let Some(constant) = declared.get("const")
            && value != constant
        {
            violations.push(format!("`{property}` must be {constant}"));
        }
        if let Some(minimum) = declared.get("minimum").and_then(Value::as_i64)
            && value.as_i64().is_some_and(|n| n < minimum)
        {
            violations.push(format!("`{property}` must be at least {minimum}"));
        }
    }

    if violations.is_empty() {
        return Ok(());
    }
    bail!(
        "Result document does not match the `{name}` schema: {}.",
        violations.join("; ")
    )
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Build the `cortex run --format json` result document.
pub fn run_result_document(fields: &RunResultFields<'_>) -> Value {
    let mut document = Map::new();
    document.insert("type".into(), json!("result"));
    document.insert("session_id".into(), json!(fields.session_id));
    document.insert("message".into(), json!(fields.message));
    document.insert("events".into(), json!(fields.events));
    document.insert("success".into(), json!(fields.success));
    document.insert("interrupted".into(), json!(fields.interrupted));
    document.insert("complete".into(), json!(fields.complete));
    document.insert("truncated".into(), json!(fields.truncated));
    if let Some(reason) = fields.finish_reason {
        document.insert("finish_reason".into(), json!(reason));
    }
    Value::Object(document)
}

/// Fields of a `cortex run --format json` result document.
pub struct RunResultFields<'a> {
    pub session_id: &'a str,
    pub message: &'a str,
    pub events: u64,
    pub success: bool,
    pub interrupted: bool,
    pub complete: bool,
    pub truncated: bool,
    pub finish_reason: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_document() -> Value {
        run_result_document(&RunResultFields {
            session_id: "sess-1",
            message: "done",
            events: 3,
            success: true,
            interrupted: false,
            complete: true,
            truncated: false,
            finish_reason: Some("stop"),
        })
    }

    #[test]
    fn both_schemas_are_shipped_and_unknown_names_are_rejected() {
        assert_eq!(SCHEMA_NAMES.len(), 2);
        for name in SCHEMA_NAMES {
            let schema = schema_document(name).expect(name);
            assert_eq!(schema["type"], "object");
            assert!(
                schema["$id"]
                    .as_str()
                    .unwrap()
                    .contains("cortex.foundation")
            );
        }
        assert!(schema_document("nope").is_none());
        assert!(validate("nope", &json!({})).is_err());
    }

    #[test]
    fn shipped_run_document_validates() {
        validate(RUN_RESULT_SCHEMA, &run_document()).expect("shipped shape");
    }

    #[test]
    fn missing_required_property_fails_with_the_field_name() {
        let mut document = run_document();
        document.as_object_mut().unwrap().remove("success");
        let error = validate(RUN_RESULT_SCHEMA, &document).expect_err("missing");
        assert!(error.to_string().contains("`success`"), "{error}");
    }

    #[test]
    fn wrong_type_fails_with_the_property_name() {
        let mut document = run_document();
        document["events"] = json!("three");
        let error = validate(RUN_RESULT_SCHEMA, &document).expect_err("type");
        let message = error.to_string();
        assert!(message.contains("`events`"), "{message}");
        assert!(message.contains("integer"), "{message}");
    }

    #[test]
    fn wrong_const_and_negative_minimum_fail() {
        let mut document = run_document();
        document["type"] = json!("summary");
        let error = validate(RUN_RESULT_SCHEMA, &document).expect_err("const");
        assert!(error.to_string().contains("`type`"), "{error}");

        let mut document = run_document();
        document["events"] = json!(-1);
        let error = validate(RUN_RESULT_SCHEMA, &document).expect_err("minimum");
        assert!(error.to_string().contains("at least 0"), "{error}");
    }

    #[test]
    fn non_object_documents_are_rejected() {
        let error = validate(RUN_RESULT_SCHEMA, &json!([1, 2])).expect_err("array");
        assert!(error.to_string().contains("JSON object"), "{error}");
    }

    #[test]
    fn exec_schema_accepts_both_subtypes_and_rejects_others() {
        let ok = json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": "done",
            "duration_ms": 1200,
            "num_turns": 2,
            "session_id": "sess-1",
        });
        validate(EXEC_RESULT_SCHEMA, &ok).expect("success");

        let mut error_document = ok.clone();
        error_document["subtype"] = json!("error");
        error_document["error"] = json!("The coding service is temporarily unavailable");
        validate(EXEC_RESULT_SCHEMA, &error_document).expect("error");

        // `subtype` is an enum in the shipped schema; unknown values still have
        // to be caught by the required/type checks around it.
        let mut unknown = ok;
        unknown["duration_ms"] = json!(-5);
        let error = validate(EXEC_RESULT_SCHEMA, &unknown).expect_err("minimum");
        assert!(error.to_string().contains("`duration_ms`"), "{error}");
    }

    #[test]
    fn additional_properties_stay_allowed_for_forward_compatibility() {
        let mut document = run_document();
        document["future_field"] = json!("added later");
        validate(RUN_RESULT_SCHEMA, &document).expect("forward compatible");
    }
}
