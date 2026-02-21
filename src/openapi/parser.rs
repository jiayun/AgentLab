use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedOperation {
    pub operation_id: String,
    pub method: String,
    pub path: String,
    pub description: String,
    pub parameters_schema: Value,
    pub parameters: Vec<ParsedParameter>,
    pub request_body_content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedParameter {
    pub name: String,
    pub location: String, // path, query, header
    pub required: bool,
    pub schema: Value,
}

/// Parse an OpenAPI 3.x JSON spec into a list of ParsedOperations
pub fn parse_openapi_spec(spec_json: &str) -> Result<Vec<ParsedOperation>> {
    let spec: Value = serde_json::from_str(spec_json).context("Invalid JSON")?;
    let mut operations = Vec::new();

    let paths = spec
        .get("paths")
        .and_then(|p| p.as_object())
        .context("No 'paths' in OpenAPI spec")?;

    for (path, path_item) in paths {
        let path_item = match path_item.as_object() {
            Some(obj) => obj,
            None => continue,
        };

        // Collect path-level parameters
        let path_params: Vec<Value> = path_item
            .get("parameters")
            .and_then(|p| p.as_array())
            .cloned()
            .unwrap_or_default();

        for method in &["get", "post", "put", "patch", "delete"] {
            let operation = match path_item.get(*method) {
                Some(op) => op,
                None => continue,
            };

            let op_id = operation
                .get("operationId")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| {
                    // Generate operation ID from method + path
                    ""
                })
                .to_string();

            let op_id = if op_id.is_empty() {
                format!(
                    "{}_{}",
                    method,
                    path.replace('/', "_").trim_matches('_')
                )
            } else {
                op_id
            };

            let description = operation
                .get("summary")
                .or_else(|| operation.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Merge path-level and operation-level parameters
            let op_params: Vec<Value> = operation
                .get("parameters")
                .and_then(|p| p.as_array())
                .cloned()
                .unwrap_or_default();

            let mut all_params = path_params.clone();
            all_params.extend(op_params);

            // Resolve $ref for parameters
            let resolved_params: Vec<ParsedParameter> = all_params
                .iter()
                .filter_map(|p| {
                    let p = resolve_ref(&spec, p);
                    let name = p.get("name")?.as_str()?.to_string();
                    let location = p.get("in")?.as_str()?.to_string();
                    let required = p.get("required").and_then(|r| r.as_bool()).unwrap_or(false);
                    let schema = resolve_ref(&spec, p.get("schema").unwrap_or(&Value::Null));
                    Some(ParsedParameter {
                        name,
                        location,
                        required,
                        schema,
                    })
                })
                .collect();

            // Build JSON Schema for tool parameters
            let mut properties = serde_json::Map::new();
            let mut required_fields: Vec<String> = Vec::new();

            for param in &resolved_params {
                properties.insert(param.name.clone(), param.schema.clone());
                if param.required {
                    required_fields.push(param.name.clone());
                }
            }

            // Handle request body
            let mut request_body_content_type = None;
            if let Some(body) = operation.get("requestBody") {
                let body = resolve_ref(&spec, body);
                if let Some(content) = body.get("content").and_then(|c| c.as_object()) {
                    // Prefer application/json
                    let (content_type, media_type) = content
                        .get("application/json")
                        .map(|m| ("application/json", m))
                        .or_else(|| content.iter().next().map(|(k, v)| (k.as_str(), v)))
                        .unwrap_or(("application/json", &Value::Null));

                    request_body_content_type = Some(content_type.to_string());

                    if let Some(schema) = media_type.get("schema") {
                        let schema = resolve_ref(&spec, schema);
                        // Merge body schema properties into the tool parameters
                        if let Some(body_props) = schema.get("properties").and_then(|p| p.as_object()) {
                            for (k, v) in body_props {
                                properties.insert(k.clone(), resolve_ref(&spec, v));
                            }
                        }
                        if let Some(body_required) = schema.get("required").and_then(|r| r.as_array()) {
                            for r in body_required {
                                if let Some(s) = r.as_str() {
                                    required_fields.push(s.to_string());
                                }
                            }
                        }
                        // If the body is a simple type (not object), add as "body" parameter
                        if schema.get("properties").is_none() && schema.get("type").is_some() {
                            properties.insert("body".to_string(), schema.clone());
                        }
                    }
                }
            }

            let parameters_schema = serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": required_fields,
            });

            operations.push(ParsedOperation {
                operation_id: op_id,
                method: method.to_uppercase(),
                path: path.clone(),
                description,
                parameters_schema,
                parameters: resolved_params,
                request_body_content_type,
            });
        }
    }

    Ok(operations)
}

/// Resolve a `$ref` in the OpenAPI spec, returning the referenced value
fn resolve_ref<'a>(spec: &'a Value, value: &'a Value) -> Value {
    if let Some(ref_path) = value.get("$ref").and_then(|r| r.as_str()) {
        // "#/components/schemas/Pet" -> ["components", "schemas", "Pet"]
        let parts: Vec<&str> = ref_path
            .trim_start_matches("#/")
            .split('/')
            .collect();

        let mut current = spec;
        for part in &parts {
            current = current.get(*part).unwrap_or(&Value::Null);
        }

        // Recursively resolve nested refs
        if current.get("$ref").is_some() {
            resolve_ref(spec, current)
        } else {
            current.clone()
        }
    } else {
        value.clone()
    }
}
