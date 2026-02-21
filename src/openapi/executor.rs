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

    // Add request body (collect non-parameter fields)
    if operation.request_body_content_type.is_some() {
        let param_names: std::collections::HashSet<&str> =
            operation.parameters.iter().map(|p| p.name.as_str()).collect();
        let body_fields: serde_json::Map<String, Value> = args
            .into_iter()
            .filter(|(k, _)| !param_names.contains(k.as_str()))
            .collect();

        if !body_fields.is_empty() {
            req = req.json(&body_fields);
        }
    }

    // Execute request
    let resp = req
        .send()
        .await
        .with_context(|| format!("Failed to call {url}"))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.is_success() {
        // Try to format as pretty JSON
        if let Ok(json) = serde_json::from_str::<Value>(&body) {
            Ok(serde_json::to_string_pretty(&json).unwrap_or(body))
        } else {
            Ok(body)
        }
    } else {
        Ok(format!("HTTP {status}: {body}"))
    }
}
