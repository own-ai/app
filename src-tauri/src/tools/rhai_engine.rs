//! Sandboxed Rhai scripting engine with safe function wrappers.
//!
//! Provides a configured Rhai engine with security limits and a curated set
//! of Rust functions that scripts may call. All file operations are scoped to
//! the instance workspace, and all HTTP requests enforce HTTPS with timeouts.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use regex::Regex;
use rhai::{Dynamic, Engine, Map};
use std::path::{Component, Path, PathBuf};

// ---------------------------------------------------------------------------
// Engine creation
// ---------------------------------------------------------------------------

/// Maximum number of Rhai operations before the script is terminated.
const MAX_OPERATIONS: u64 = 100_000;
/// Maximum string length in bytes (1 MB).
const MAX_STRING_SIZE: usize = 1_048_576;
/// Maximum array length.
const MAX_ARRAY_SIZE: usize = 10_000;
/// Maximum map size.
const MAX_MAP_SIZE: usize = 5_000;
/// HTTP request timeout in seconds.
const HTTP_TIMEOUT_SECS: u64 = 30;

/// Create a sandboxed Rhai engine with security limits and safe built-in functions.
///
/// The `workspace` path defines the root directory for all file operations.
/// Scripts cannot access anything outside this directory.
pub fn create_sandboxed_engine(workspace: PathBuf) -> Engine {
    let mut engine = Engine::new();

    // -- Security limits --
    engine.set_max_operations(MAX_OPERATIONS);
    engine.set_max_string_size(MAX_STRING_SIZE);
    engine.set_max_array_size(MAX_ARRAY_SIZE);
    engine.set_max_map_size(MAX_MAP_SIZE);

    // -- HTTP functions --
    engine.register_fn("http_get", safe_http_get);
    engine.register_fn("http_post", safe_http_post);
    engine.register_fn("http_request", safe_http_request);

    // -- Filesystem functions (workspace-scoped) --
    let ws_read = workspace.clone();
    engine.register_fn(
        "read_file",
        move |path: String| -> Result<String, Box<rhai::EvalAltResult>> {
            safe_read_file(&ws_read, &path)
        },
    );

    let ws_write = workspace;
    engine.register_fn(
        "write_file",
        move |path: String, content: String| -> Result<(), Box<rhai::EvalAltResult>> {
            safe_write_file(&ws_write, &path, &content)
        },
    );

    // -- JSON functions --
    engine.register_fn("json_parse", safe_json_parse);
    engine.register_fn("json_stringify", safe_json_stringify);

    // -- Regex functions --
    engine.register_fn("regex_match", safe_regex_match);
    engine.register_fn("regex_replace", safe_regex_replace);

    // -- Encoding functions --
    engine.register_fn("base64_encode", safe_base64_encode);
    engine.register_fn("base64_decode", safe_base64_decode);
    engine.register_fn("url_encode", safe_url_encode);

    // -- System functions --
    engine.register_fn("get_current_datetime", safe_get_current_datetime);
    engine.register_fn("send_notification", safe_send_notification);

    engine
}

// ---------------------------------------------------------------------------
// Path safety
// ---------------------------------------------------------------------------

/// Resolve a user-provided path within the workspace root.
/// Blocks absolute paths and parent directory traversal.
fn resolve_workspace_path(root: &Path, user_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(user_path);

    if path.is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Parent directory traversal (..) is not allowed".to_string());
    }

    Ok(root.join(path))
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

/// Validate that a URL uses HTTPS.
fn require_https(url: &str) -> Result<(), Box<rhai::EvalAltResult>> {
    if !url.starts_with("https://") {
        return Err(format!("Only HTTPS URLs are allowed, got: {}", url).into());
    }
    Ok(())
}

/// Build a blocking reqwest client with timeout.
fn blocking_client() -> Result<reqwest::blocking::Client, Box<rhai::EvalAltResult>> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e).into())
}

// ---------------------------------------------------------------------------
// Safe functions: HTTP
// ---------------------------------------------------------------------------

/// Simple HTTPS GET request. Returns response body as string.
fn safe_http_get(url: String) -> Result<String, Box<rhai::EvalAltResult>> {
    require_https(&url)?;
    let client = blocking_client()?;
    let response = client
        .get(&url)
        .send()
        .map_err(|e| -> Box<rhai::EvalAltResult> { format!("HTTP GET failed: {}", e).into() })?;

    response.text().map_err(|e| -> Box<rhai::EvalAltResult> {
        format!("Failed to read response: {}", e).into()
    })
}

/// Simple HTTPS POST request with a string body. Returns response body as string.
fn safe_http_post(url: String, body: String) -> Result<String, Box<rhai::EvalAltResult>> {
    require_https(&url)?;
    let client = blocking_client()?;
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .map_err(|e| -> Box<rhai::EvalAltResult> { format!("HTTP POST failed: {}", e).into() })?;

    response.text().map_err(|e| -> Box<rhai::EvalAltResult> {
        format!("Failed to read response: {}", e).into()
    })
}

/// Flexible HTTPS request with custom method, headers, and body.
///
/// `headers` is a Rhai Map of `String -> String` key-value pairs.
fn safe_http_request(
    method: String,
    url: String,
    headers: Map,
    body: String,
) -> Result<String, Box<rhai::EvalAltResult>> {
    require_https(&url)?;
    let client = blocking_client()?;

    let method_parsed = method.to_uppercase();
    let mut request = match method_parsed.as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        "HEAD" => client.head(&url),
        _ => {
            return Err(format!("Unsupported HTTP method: {}", method).into());
        }
    };

    // Add custom headers
    for (key, value) in &headers {
        let header_value =
            value
                .clone()
                .into_string()
                .map_err(|e| -> Box<rhai::EvalAltResult> {
                    format!("Header value must be a string: {}", e).into()
                })?;
        request = request.header(key.as_str(), header_value);
    }

    // Add body for methods that support it
    if !body.is_empty() && matches!(method_parsed.as_str(), "POST" | "PUT" | "PATCH") {
        request = request.body(body);
    }

    let response = request.send().map_err(|e| -> Box<rhai::EvalAltResult> {
        format!("HTTP {} failed: {}", method_parsed, e).into()
    })?;

    response.text().map_err(|e| -> Box<rhai::EvalAltResult> {
        format!("Failed to read response: {}", e).into()
    })
}

// ---------------------------------------------------------------------------
// Safe functions: Filesystem
// ---------------------------------------------------------------------------

/// Read a file within the workspace directory.
fn safe_read_file(workspace: &Path, path: &str) -> Result<String, Box<rhai::EvalAltResult>> {
    let resolved = resolve_workspace_path(workspace, path)
        .map_err(|e| -> Box<rhai::EvalAltResult> { e.into() })?;

    std::fs::read_to_string(&resolved).map_err(|e| -> Box<rhai::EvalAltResult> {
        format!("Failed to read file '{}': {}", path, e).into()
    })
}

/// Write content to a file within the workspace directory.
/// Creates parent directories if they do not exist.
fn safe_write_file(
    workspace: &Path,
    path: &str,
    content: &str,
) -> Result<(), Box<rhai::EvalAltResult>> {
    let resolved = resolve_workspace_path(workspace, path)
        .map_err(|e| -> Box<rhai::EvalAltResult> { e.into() })?;

    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent).map_err(|e| -> Box<rhai::EvalAltResult> {
            format!("Failed to create directories: {}", e).into()
        })?;
    }

    std::fs::write(&resolved, content).map_err(|e| -> Box<rhai::EvalAltResult> {
        format!("Failed to write file '{}': {}", path, e).into()
    })
}

// ---------------------------------------------------------------------------
// Safe functions: JSON
// ---------------------------------------------------------------------------

/// Parse a JSON string into a Rhai Dynamic value (Map or Array).
fn safe_json_parse(text: String) -> Result<Dynamic, Box<rhai::EvalAltResult>> {
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| -> Box<rhai::EvalAltResult> { format!("JSON parse error: {}", e).into() })?;

    json_value_to_dynamic(value)
}

/// Convert a Rhai Dynamic value to a JSON string.
fn safe_json_stringify(value: Dynamic) -> Result<String, Box<rhai::EvalAltResult>> {
    let json_value = dynamic_to_json_value(value)?;
    serde_json::to_string_pretty(&json_value)
        .map_err(|e| -> Box<rhai::EvalAltResult> { format!("JSON stringify error: {}", e).into() })
}

/// Convert serde_json::Value to Rhai Dynamic
fn json_value_to_dynamic(value: serde_json::Value) -> Result<Dynamic, Box<rhai::EvalAltResult>> {
    match value {
        serde_json::Value::Null => Ok(Dynamic::UNIT),
        serde_json::Value::Bool(b) => Ok(Dynamic::from(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Dynamic::from(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Dynamic::from(f))
            } else {
                Ok(Dynamic::from(n.to_string()))
            }
        }
        serde_json::Value::String(s) => Ok(Dynamic::from(s)),
        serde_json::Value::Array(arr) => {
            let mut rhai_arr = rhai::Array::new();
            for item in arr {
                rhai_arr.push(json_value_to_dynamic(item)?);
            }
            Ok(Dynamic::from(rhai_arr))
        }
        serde_json::Value::Object(obj) => {
            let mut map = Map::new();
            for (key, val) in obj {
                map.insert(key.into(), json_value_to_dynamic(val)?);
            }
            Ok(Dynamic::from(map))
        }
    }
}

/// Convert Rhai Dynamic to serde_json::Value
fn dynamic_to_json_value(value: Dynamic) -> Result<serde_json::Value, Box<rhai::EvalAltResult>> {
    if value.is_unit() {
        Ok(serde_json::Value::Null)
    } else if value.is_bool() {
        Ok(serde_json::Value::Bool(value.as_bool().unwrap()))
    } else if value.is_int() {
        Ok(serde_json::Value::Number(serde_json::Number::from(
            value.as_int().unwrap(),
        )))
    } else if value.is_float() {
        let f = value.as_float().unwrap();
        match serde_json::Number::from_f64(f) {
            Some(n) => Ok(serde_json::Value::Number(n)),
            None => Ok(serde_json::Value::Null),
        }
    } else if value.is_string() {
        Ok(serde_json::Value::String(value.into_string().unwrap()))
    } else if value.is_array() {
        let arr = value.into_array().unwrap();
        let mut json_arr = Vec::new();
        for item in arr {
            json_arr.push(dynamic_to_json_value(item)?);
        }
        Ok(serde_json::Value::Array(json_arr))
    } else if value.is_map() {
        let map: Map = value
            .try_cast()
            .ok_or_else(|| -> Box<rhai::EvalAltResult> {
                "Failed to cast Dynamic to Map".into()
            })?;
        let mut json_obj = serde_json::Map::new();
        for (key, val) in map {
            json_obj.insert(key.to_string(), dynamic_to_json_value(val)?);
        }
        Ok(serde_json::Value::Object(json_obj))
    } else {
        // Fallback: convert to string representation
        Ok(serde_json::Value::String(format!("{:?}", value)))
    }
}

// ---------------------------------------------------------------------------
// Safe functions: Regex
// ---------------------------------------------------------------------------

/// Find all matches of a regex pattern in the given text.
/// Returns an array of matched strings.
fn safe_regex_match(
    text: String,
    pattern: String,
) -> Result<rhai::Array, Box<rhai::EvalAltResult>> {
    let re = Regex::new(&pattern)
        .map_err(|e| -> Box<rhai::EvalAltResult> { format!("Invalid regex: {}", e).into() })?;

    let matches: rhai::Array = re
        .find_iter(&text)
        .map(|m| Dynamic::from(m.as_str().to_string()))
        .collect();

    Ok(matches)
}

/// Replace all occurrences of a regex pattern in the text with a replacement string.
fn safe_regex_replace(
    text: String,
    pattern: String,
    replacement: String,
) -> Result<String, Box<rhai::EvalAltResult>> {
    let re = Regex::new(&pattern)
        .map_err(|e| -> Box<rhai::EvalAltResult> { format!("Invalid regex: {}", e).into() })?;

    Ok(re.replace_all(&text, replacement.as_str()).to_string())
}

// ---------------------------------------------------------------------------
// Safe functions: Encoding
// ---------------------------------------------------------------------------

/// Encode a string to Base64.
fn safe_base64_encode(text: String) -> String {
    BASE64.encode(text.as_bytes())
}

/// Decode a Base64 string. Returns an error if the input is not valid Base64.
fn safe_base64_decode(encoded: String) -> Result<String, Box<rhai::EvalAltResult>> {
    let bytes = BASE64
        .decode(encoded.as_bytes())
        .map_err(|e| -> Box<rhai::EvalAltResult> {
            format!("Base64 decode error: {}", e).into()
        })?;

    String::from_utf8(bytes)
        .map_err(|e| -> Box<rhai::EvalAltResult> { format!("UTF-8 decode error: {}", e).into() })
}

/// URL-encode a string (percent-encoding).
fn safe_url_encode(text: String) -> String {
    // Minimal percent-encoding for common special characters
    let mut encoded = String::with_capacity(text.len() * 2);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

// ---------------------------------------------------------------------------
// Safe functions: System
// ---------------------------------------------------------------------------

/// Get the current date and time as an ISO 8601 string (UTC).
fn safe_get_current_datetime() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Send a system notification.
///
/// Note: This is a placeholder that logs the notification. Full system
/// notification support requires the Tauri notification plugin which will
/// be integrated in a later phase.
fn safe_send_notification(title: String, body: String) -> String {
    tracing::info!(
        "Notification requested - title: '{}', body: '{}'",
        title,
        body
    );
    format!(
        "Notification queued: '{}' - '{}' (system notifications will be available in a future update)",
        title, body
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_engine_does_not_panic() {
        let workspace = PathBuf::from("/tmp/test_workspace");
        let _engine = create_sandboxed_engine(workspace);
    }

    #[test]
    fn test_max_operations_limit() {
        let workspace = PathBuf::from("/tmp/test_workspace");
        let engine = create_sandboxed_engine(workspace);

        // An infinite loop should be stopped by max_operations
        let result = engine.eval::<()>("loop { }");
        assert!(result.is_err(), "Infinite loop should be terminated");
    }

    #[test]
    fn test_resolve_workspace_path_normal() {
        let root = PathBuf::from("/workspace");
        assert_eq!(
            resolve_workspace_path(&root, "test.txt").unwrap(),
            PathBuf::from("/workspace/test.txt")
        );
    }

    #[test]
    fn test_resolve_workspace_path_subdirectory() {
        let root = PathBuf::from("/workspace");
        assert_eq!(
            resolve_workspace_path(&root, "sub/test.txt").unwrap(),
            PathBuf::from("/workspace/sub/test.txt")
        );
    }

    #[test]
    fn test_resolve_workspace_path_blocks_traversal() {
        let root = PathBuf::from("/workspace");
        assert!(resolve_workspace_path(&root, "../etc/passwd").is_err());
    }

    #[test]
    fn test_resolve_workspace_path_blocks_absolute() {
        let root = PathBuf::from("/workspace");
        assert!(resolve_workspace_path(&root, "/etc/passwd").is_err());
    }

    #[test]
    fn test_require_https() {
        assert!(require_https("https://example.com").is_ok());
        assert!(require_https("http://example.com").is_err());
        assert!(require_https("ftp://example.com").is_err());
    }

    #[test]
    fn test_json_roundtrip() {
        let json_str = r#"{"name": "test", "value": 42, "active": true}"#.to_string();
        let parsed = safe_json_parse(json_str.clone()).unwrap();
        let stringified = safe_json_stringify(parsed).unwrap();
        // Re-parse to compare structurally
        let v1: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&stringified).unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_json_parse_array() {
        let json_str = r#"[1, "two", true, null]"#.to_string();
        let parsed = safe_json_parse(json_str).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn test_json_parse_invalid() {
        let result = safe_json_parse("not valid json".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_regex_match_finds_matches() {
        let text = "The price is $42.50 and $100.00".to_string();
        let pattern = r"\$\d+\.\d{2}".to_string();
        let matches = safe_regex_match(text, pattern).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].clone().into_string().unwrap(), "$42.50");
        assert_eq!(matches[1].clone().into_string().unwrap(), "$100.00");
    }

    #[test]
    fn test_regex_match_no_matches() {
        let text = "Hello, world!".to_string();
        let pattern = r"\d+".to_string();
        let matches = safe_regex_match(text, pattern).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_regex_match_invalid_pattern() {
        let result = safe_regex_match("test".to_string(), "[invalid".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_regex_replace() {
        let text = "Hello 123 World 456".to_string();
        let result = safe_regex_replace(text, r"\d+".to_string(), "NUM".to_string()).unwrap();
        assert_eq!(result, "Hello NUM World NUM");
    }

    #[test]
    fn test_base64_roundtrip() {
        let original = "Hello, World! Special chars: ä ö ü".to_string();
        let encoded = safe_base64_encode(original.clone());
        let decoded = safe_base64_decode(encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_base64_decode_invalid() {
        let result = safe_base64_decode("!!!not-base64!!!".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(safe_url_encode("hello world".to_string()), "hello%20world");
        assert_eq!(
            safe_url_encode("foo=bar&baz".to_string()),
            "foo%3Dbar%26baz"
        );
        assert_eq!(
            safe_url_encode("safe-text_123.test~ok".to_string()),
            "safe-text_123.test~ok"
        );
    }

    #[test]
    fn test_get_current_datetime() {
        let dt = safe_get_current_datetime();
        // Should be a valid RFC 3339 timestamp
        assert!(dt.contains('T'));
        assert!(dt.len() > 20);
    }

    #[test]
    fn test_send_notification() {
        let result = safe_send_notification("Test".to_string(), "Body".to_string());
        assert!(result.contains("Notification queued"));
    }

    #[test]
    fn test_engine_basic_script() {
        let workspace = PathBuf::from("/tmp/test_workspace");
        let engine = create_sandboxed_engine(workspace);

        let result: i64 = engine.eval("let x = 40; x + 2").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_engine_json_in_script() {
        let workspace = PathBuf::from("/tmp/test_workspace");
        let engine = create_sandboxed_engine(workspace);

        let result: String = engine
            .eval(r#"let data = json_parse("{\"key\": \"value\"}"); json_stringify(data)"#)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["key"], "value");
    }

    #[test]
    fn test_engine_regex_in_script() {
        let workspace = PathBuf::from("/tmp/test_workspace");
        let engine = create_sandboxed_engine(workspace);

        let result: rhai::Array = engine
            .eval(r#"regex_match("Price: $42", "\\$\\d+")"#)
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_engine_base64_in_script() {
        let workspace = PathBuf::from("/tmp/test_workspace");
        let engine = create_sandboxed_engine(workspace);

        let result: String = engine
            .eval(r#"let enc = base64_encode("hello"); base64_decode(enc)"#)
            .unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_engine_datetime_in_script() {
        let workspace = PathBuf::from("/tmp/test_workspace");
        let engine = create_sandboxed_engine(workspace);

        let result: String = engine.eval("get_current_datetime()").unwrap();
        assert!(result.contains('T'));
    }
}
