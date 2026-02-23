use anyhow::{Context, Result};
use serde_json::Value;

use super::parser::ParsedOperation;

/// Execute an OpenAPI operation by building and sending an HTTP request
pub async fn execute_operation(
    client: &reqwest::Client,
    operation: &ParsedOperation,
    base_url: &str,
    arguments: &Value,
    auth_header: Option<&str>,
    auth_value: Option<&str>,
) -> Result<String> {
    let args = arguments.as_object().cloned().unwrap_or_default();

    // Build URL with path parameters
    let mut url_path = operation.path.clone();
    for param in &operation.parameters {
        if param.location == "path" {
            if let Some(val) = args.get(&param.name) {
                let val_str = match val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                url_path = url_path.replace(&format!("{{{}}}", param.name), &val_str);
            }
        }
    }

    let url = format!(
        "{}{}",
        base_url.trim_end_matches('/'),
        url_path
    );

    // Build request
    let method = match operation.method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        other => anyhow::bail!("Unsupported HTTP method: {other}"),
    };

    let mut req = client.request(method, &url);

    // Add query parameters
    for param in &operation.parameters {
        if param.location == "query" {
            if let Some(val) = args.get(&param.name) {
                let val_str = match val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                req = req.query(&[(&param.name, &val_str)]);
            }
        }
    }

    // Add header parameters
    for param in &operation.parameters {
        if param.location == "header" {
            if let Some(val) = args.get(&param.name) {
                let val_str = match val {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                req = req.header(&param.name, &val_str);
            }
        }
    }

    // Add auth header
    if let (Some(header), Some(value)) = (auth_header, auth_value) {
        req = req.header(header, value);
    }

    // Add request body (collect non-parameter fields, filter against schema)
    if operation.request_body_content_type.is_some() {
        let param_names: std::collections::HashSet<&str> =
            operation.parameters.iter().map(|p| p.name.as_str()).collect();

        // Known body properties from the schema
        let schema_props: std::collections::HashSet<String> = operation
            .parameters_schema
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();

        let body_fields: serde_json::Map<String, Value> = args
            .into_iter()
            .filter(|(k, _)| !param_names.contains(k.as_str()))
            .filter(|(k, _)| {
                if schema_props.is_empty() || schema_props.contains(k) {
                    true
                } else {
                    tracing::warn!(field = %k, "Dropping unknown field from request body (not in schema)");
                    false
                }
            })
            .collect();

        if !body_fields.is_empty() {
            let body_value = serde_json::Value::Object(body_fields.clone());
            tracing::info!(method = %operation.method, %url, body = %body_value, "Executing OpenAPI operation");
            req = req.json(&body_fields);
        } else {
            tracing::info!(method = %operation.method, %url, body = "{}", "Executing OpenAPI operation");
        }
    } else {
        tracing::info!(method = %operation.method, %url, "Executing OpenAPI operation");
    }

    // Execute request
    let start = std::time::Instant::now();
    let resp = req
        .send()
        .await
        .with_context(|| format!("Failed to call {url}"))?;
    let latency_ms = start.elapsed().as_millis();

    let status = resp.status();
    let status_code = status.as_u16();

    // Collect useful response headers
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body = resp.text().await.unwrap_or_default();

    tracing::info!(status = %status, latency_ms, "OpenAPI operation completed");

    // Parse body as JSON if possible
    let body_value = serde_json::from_str::<Value>(&body)
        .unwrap_or_else(|_| Value::String(body));

    // Return structured result so the LLM gets full context
    let result = serde_json::json!({
        "status": status_code,
        "latency_ms": latency_ms,
        "content_type": content_type,
        "body": body_value,
    });

    Ok(result.to_string())
}
