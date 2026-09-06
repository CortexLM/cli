//! OpenAPI for the supported local session API. Types come from handler models.

use schemars::JsonSchema;
use serde_json::{Map, Value, json};

use super::types::*;

fn add<T: JsonSchema>(schemas: &mut Map<String, Value>, name: &str) {
    let schema = schemars::schema_for!(T);
    let encoded = serde_json::to_string(&schema)
        .expect("Serializable schema")
        .replace("#/definitions/", "#/components/schemas/");
    let mut value: Value = serde_json::from_str(&encoded).expect("Generated schema JSON");
    let object = value.as_object_mut().expect("Schema object");
    object.remove("$schema");
    if let Some(Value::Object(definitions)) = object.remove("definitions") {
        schemas.extend(definitions);
    }
    schemas.insert(name.to_string(), value);
}

fn operation(summary: &str, response: Value, request: Option<&str>) -> Value {
    let mut operation = json!({
        "summary": summary,
        "responses": {
            "200": {"description": "Success", "content": {"application/json": {"schema": response}}},
            "400": {"description": "Invalid JSON or request"},
            "401": {"description": "Authentication required"},
            "404": {"description": "Resource not found or endpoint disabled"},
            "413": {"description": "Request body too large"},
            "429": {"description": "Rate limited; Retry-After header contains seconds"},
            "503": {"description": "Local server is not ready"}
        }
    });
    if let Some(request) = request {
        operation["requestBody"] = json!({
            "required": true,
            "content": {"application/json": {"schema": reference(request)}}
        });
    }
    operation
}

fn reference(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn array(name: &str) -> Value {
    json!({"type": "array", "items": reference(name)})
}

pub fn document() -> Value {
    let mut schemas = Map::new();
    add::<HealthResponse>(&mut schemas, "HealthResponse");
    add::<crate::state::MetricsSnapshot>(&mut schemas, "MetricsSnapshot");
    add::<CreateSessionRequest>(&mut schemas, "CreateSessionRequest");
    add::<SessionResponse>(&mut schemas, "SessionResponse");
    add::<SessionListItem>(&mut schemas, "SessionListItem");
    add::<SendMessageRequest>(&mut schemas, "SendMessageRequest");
    add::<MessageResponse>(&mut schemas, "MessageResponse");
    let id = json!([{"in":"path", "name":"id", "required":true, "schema":{"type":"string"}}]);
    let mut health = operation(
        "Local readiness, not coding-service availability",
        reference("HealthResponse"),
        None,
    );
    health["security"] = json!([]);
    let mut sessions = operation("List in-memory sessions", array("SessionListItem"), None);
    sessions["parameters"] = json!([
        {"in":"query", "name":"limit", "schema":{"type":"integer", "minimum":0, "default":20}},
        {"in":"query", "name":"offset", "schema":{"type":"integer", "minimum":0, "default":0}}
    ]);
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Cortex local session API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Supported local session/health contract. Message POST stores a message; it does not generate a model response. Other development endpoints are not part of this stable contract."
        },
        "servers": [{"url": "/api/v1"}],
        "security": [{"serverApiKey":[]}, {"bearerAuth":[]}],
        "paths": {
            "/health": {"get": health},
            "/metrics": {"get": operation("Local request counters", reference("MetricsSnapshot"), None)},
            "/sessions": {
                "get": sessions,
                "post": operation("Create an in-memory session", reference("SessionResponse"), Some("CreateSessionRequest"))
            },
            "/sessions/{id}": {
                "parameters": id,
                "get": operation("Get a session", reference("SessionResponse"), None),
                "delete": operation("Delete a session", json!({"type":"object", "required":["deleted"], "properties":{"deleted":{"const":true}}}), None)
            },
            "/sessions/{id}/messages": {
                "parameters": id,
                "get": operation("List stored messages", array("MessageResponse"), None),
                "post": operation("Store a message without invoking a model", reference("MessageResponse"), Some("SendMessageRequest"))
            }
        },
        "components": {
            "schemas": schemas,
            "securitySchemes": {
                "serverApiKey": {"type":"apiKey", "in":"header", "name":"Authorization", "description":"Value: ApiKey followed by the server key"},
                "bearerAuth": {"type":"http", "scheme":"bearer", "bearerFormat":"JWT"}
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_preserves_request_requirements_and_responses() {
        let doc = document();
        assert_eq!(
            doc["components"]["schemas"]["SendMessageRequest"]["required"],
            json!(["content"])
        );
        assert_eq!(
            doc["paths"]["/sessions"]["post"]["requestBody"]["required"],
            true
        );
        assert!(
            doc["components"]["schemas"]
                .get("ToolCallResponse")
                .is_some()
        );
        assert!(!doc.to_string().contains("#/definitions/"));
    }
}
