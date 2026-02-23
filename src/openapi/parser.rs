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

            let parameters_schema_raw = serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": required_fields,
            });
            let parameters_schema = resolve_schema_deep(&spec, &parameters_schema_raw, 0);

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

/// Maximum allowed length for tool/function names (Gemini limit is 64).
const MAX_TOOL_NAME_LEN: usize = 64;

/// Allowed JSON Schema keys for LLM function calling compatibility.
/// Gemini only supports: type, description, enum, items, properties, required, nullable.
const ALLOWED_SCHEMA_KEYS: &[&str] = &[
    "type",
    "description",
    "enum",
    "items",
    "properties",
    "required",
    "nullable",
    "additionalProperties",
];

/// Truncate a tool name to fit within the provider's limit.
pub fn sanitize_tool_name(name: &str) -> String {
    if name.len() <= MAX_TOOL_NAME_LEN {
        name.to_string()
    } else {
        name[..MAX_TOOL_NAME_LEN].to_string()
    }
}

/// Recursively strip unsupported JSON Schema fields for broad LLM provider compatibility.
/// Converts `title` to `description` when no `description` is present.
pub fn sanitize_schema(value: &Value) -> Value {
    sanitize_schema_inner(value, false)
}

/// Inner recursive sanitizer.
/// `is_properties_map` indicates whether `value` is the value of a `properties` key,
/// meaning its keys are field names (not schema keywords) and should NOT be filtered.
fn sanitize_schema_inner(value: &Value, is_properties_map: bool) -> Value {
    match value {
        Value::Object(map) => {
            if is_properties_map {
                // This is a properties map: keys are field names, values are schemas
                let mut out = serde_json::Map::new();
                for (k, v) in map {
                    out.insert(k.clone(), sanitize_schema_inner(v, false));
                }
                return Value::Object(out);
            }

            // This is a schema object: filter keys to allowed set
            let mut out = serde_json::Map::new();

            // Promote title → description if description is absent
            let has_description = map.contains_key("description");
            if !has_description {
                if let Some(title) = map.get("title").and_then(|v| v.as_str()) {
                    out.insert("description".to_string(), Value::String(title.to_string()));
                }
            }

            for (k, v) in map {
                if !ALLOWED_SCHEMA_KEYS.contains(&k.as_str()) {
                    continue;
                }
                // properties value is a map of field names → schemas
                let child_is_props = k == "properties";
                out.insert(k.clone(), sanitize_schema_inner(v, child_is_props));
            }

            // Prevent LLM from hallucinating extra fields
            if out.contains_key("properties") && !out.contains_key("additionalProperties") {
                out.insert("additionalProperties".to_string(), Value::Bool(false));
            }

            Value::Object(out)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| sanitize_schema_inner(v, false)).collect())
        }
        other => other.clone(),
    }
}

/// Enrich a tool description by appending a summary of top-level parameter names.
/// This helps LLMs understand available fields without inventing new ones.
pub fn enrich_description(description: &str, schema: &Value) -> String {
    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) if !p.is_empty() => p,
        _ => return description.to_string(),
    };

    let field_summaries: Vec<String> = props
        .iter()
        .map(|(name, prop)| {
            let desc = prop.get("description").and_then(|d| d.as_str()).unwrap_or("");
            if desc.is_empty() {
                name.clone()
            } else {
                format!("{name}({desc})")
            }
        })
        .collect();

    format!(
        "{description}. ONLY use these parameters: {}",
        field_summaries.join(", ")
    )
}

/// Resolve a `$ref` in the OpenAPI spec, returning the referenced value
fn resolve_ref(spec: &Value, value: &Value) -> Value {
    if let Some(ref_path) = value.get("$ref").and_then(|r| r.as_str()) {
        let parts: Vec<&str> = ref_path
            .trim_start_matches("#/")
            .split('/')
            .collect();

        let mut current = spec;
        for part in &parts {
            current = current.get(*part).unwrap_or(&Value::Null);
        }

        resolve_ref(spec, current)
    } else {
        value.clone()
    }
}

/// Recursively resolve all `$ref`, `allOf`, and nested schemas throughout a JSON value.
/// This produces a self-contained schema with no `$ref` or `allOf` remaining.
pub fn resolve_schema_deep(spec: &Value, value: &Value, depth: u8) -> Value {
    if depth > 20 {
        return Value::Null; // guard against infinite recursion
    }

    // 1. Resolve top-level $ref first
    let resolved = resolve_ref(spec, value);

    match &resolved {
        Value::Object(map) => {
            // 2. Handle allOf: merge all sub-schemas into one object
            if let Some(all_of) = map.get("allOf").and_then(|v| v.as_array()) {
                let mut merged = serde_json::Map::new();
                // Copy sibling keys (e.g. "title", "description") excluding "allOf"
                for (k, v) in map {
                    if k != "allOf" {
                        merged.insert(k.clone(), resolve_schema_deep(spec, v, depth + 1));
                    }
                }
                // Merge each schema in allOf
                for item in all_of {
                    let item_resolved = resolve_schema_deep(spec, item, depth + 1);
                    if let Value::Object(item_map) = item_resolved {
                        for (k, v) in item_map {
                            if k == "properties" {
                                // Merge properties
                                let existing = merged
                                    .entry("properties")
                                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                                if let (Value::Object(ep), Value::Object(vp)) = (existing, &v) {
                                    for (pk, pv) in vp {
                                        ep.insert(pk.clone(), pv.clone());
                                    }
                                }
                            } else if k == "required" {
                                // Merge required arrays
                                let existing = merged
                                    .entry("required")
                                    .or_insert_with(|| Value::Array(vec![]));
                                if let (Value::Array(ea), Value::Array(va)) = (existing, &v) {
                                    ea.extend(va.clone());
                                }
                            } else {
                                merged.entry(k).or_insert(v);
                            }
                        }
                    }
                }
                // Ensure merged object has "type": "object" if it has properties
                if merged.contains_key("properties") && !merged.contains_key("type") {
                    merged.insert("type".to_string(), Value::String("object".to_string()));
                }
                return Value::Object(merged);
            }

            // 3. Recurse into all object values
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), resolve_schema_deep(spec, v, depth + 1));
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| resolve_schema_deep(spec, v, depth + 1)).collect())
        }
        other => other.clone(),
    }
}
