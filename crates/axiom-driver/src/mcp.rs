//! MCP (Model Context Protocol) server for AXIOM.
//!
//! Exposes AXIOM optimization as tools that AI agents can call via
//! JSON-RPC 2.0 over stdin/stdout.
//!
//! Supported MCP methods:
//! - `initialize` — handshake
//! - `notifications/initialized` — client acknowledgement (no response)
//! - `tools/list` — enumerate available tools
//! - `tools/call` — invoke a tool

use std::io::{self, BufRead, Write as _};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<JsonValue>,
    method: String,
    #[serde(default)]
    params: Option<JsonValue>,
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<JsonValue>,
}

// ---------------------------------------------------------------------------
// MCP protocol types
// ---------------------------------------------------------------------------

/// MCP tool description returned by `tools/list`.
#[derive(Debug, Serialize)]
struct McpTool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: JsonValue,
}

/// Result of a `tools/call` invocation.
#[derive(Debug, Serialize)]
struct ToolCallResult {
    content: Vec<ToolContent>,
    #[serde(rename = "isError", skip_serializing_if = "std::ops::Not::not")]
    is_error: bool,
}

/// A single content block in a tool call result.
#[derive(Debug, Serialize)]
struct ToolContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the MCP stdio server.
///
/// Reads newline-delimited JSON-RPC messages from stdin and writes
/// responses to stdout. Returns when stdin is closed.
pub fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line_result in stdin.lock().lines() {
        let line = line_result?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let resp = make_error_response(None, -32700, &format!("Parse error: {e}"));
                write_response(&stdout, &resp)?;
                continue;
            }
        };

        // Notifications (id == None) do not get a response.
        if request.id.is_none() {
            // Silently consume notifications like `notifications/initialized`.
            continue;
        }

        if request.jsonrpc != "2.0" {
            let resp =
                make_error_response(request.id, -32600, "Invalid JSON-RPC version (expected 2.0)");
            write_response(&stdout, &resp)?;
            continue;
        }

        let response = handle_request(&request);
        write_response(&stdout, &response)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Request dispatch
// ---------------------------------------------------------------------------

fn handle_request(req: &JsonRpcRequest) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => handle_initialize(req),
        "tools/list" => handle_tools_list(req),
        "tools/call" => handle_tools_call(req),
        _ => make_error_response(
            req.id.clone(),
            -32601,
            &format!("Method not found: {}", req.method),
        ),
    }
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

fn handle_initialize(req: &JsonRpcRequest) -> JsonRpcResponse {
    let result = serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "axiom-mcp",
            "version": env!("CARGO_PKG_VERSION")
        }
    });
    make_success_response(req.id.clone(), result)
}

// ---------------------------------------------------------------------------
// tools/list
// ---------------------------------------------------------------------------

fn handle_tools_list(req: &JsonRpcRequest) -> JsonRpcResponse {
    let tools = vec![
        McpTool {
            name: "axiom_load".to_string(),
            description: "Load an AXIOM source file or string and return surfaces, history, and transfer info.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to an .axm source file"
                    },
                    "source": {
                        "type": "string",
                        "description": "Inline AXIOM source code"
                    }
                },
                "oneOf": [
                    { "required": ["path"] },
                    { "required": ["source"] }
                ]
            }),
        },
        McpTool {
            name: "axiom_surfaces".to_string(),
            description: "Extract optimization surfaces (tunable ?holes) from AXIOM source.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "AXIOM source code"
                    }
                },
                "required": ["source"]
            }),
        },
        McpTool {
            name: "axiom_propose".to_string(),
            description: "Validate a proposal of concrete values for optimization holes.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "AXIOM source code"
                    },
                    "values": {
                        "type": "object",
                        "description": "Map of hole_name -> value"
                    }
                },
                "required": ["source", "values"]
            }),
        },
        McpTool {
            name: "axiom_compile".to_string(),
            description: "Compile AXIOM source to LLVM IR or a native binary.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "AXIOM source code"
                    },
                    "emit": {
                        "type": "string",
                        "enum": ["llvm-ir", "binary"],
                        "description": "Output format: llvm-ir or binary"
                    },
                    "output": {
                        "type": "string",
                        "description": "Output file path (required for binary)"
                    }
                },
                "required": ["source", "emit"]
            }),
        },
        McpTool {
            name: "axiom_history".to_string(),
            description: "Get optimization history for an AXIOM source file.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "AXIOM source code"
                    }
                },
                "required": ["source"]
            }),
        },
    ];

    let result = serde_json::json!({ "tools": tools });
    make_success_response(req.id.clone(), result)
}

// ---------------------------------------------------------------------------
// tools/call
// ---------------------------------------------------------------------------

fn handle_tools_call(req: &JsonRpcRequest) -> JsonRpcResponse {
    let params = match req.params.as_ref() {
        Some(p) => p,
        None => {
            return make_error_response(req.id.clone(), -32602, "Missing params for tools/call");
        }
    };

    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(name) => name,
        None => {
            return make_error_response(
                req.id.clone(),
                -32602,
                "Missing 'name' in tools/call params",
            );
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(JsonValue::Object(serde_json::Map::new()));

    let tool_result = match tool_name {
        "axiom_load" => tool_axiom_load(&arguments),
        "axiom_surfaces" => tool_axiom_surfaces(&arguments),
        "axiom_propose" => tool_axiom_propose(&arguments),
        "axiom_compile" => tool_axiom_compile(&arguments),
        "axiom_history" => tool_axiom_history(&arguments),
        _ => Err(format!("Unknown tool: {tool_name}")),
    };

    let call_result = match tool_result {
        Ok(json_text) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: json_text,
            }],
            is_error: false,
        },
        Err(err_msg) => ToolCallResult {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: err_msg,
            }],
            is_error: true,
        },
    };

    let result =
        serde_json::to_value(&call_result).unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
    make_success_response(req.id.clone(), result)
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

/// `axiom_load` — Load an AXIOM source file or string.
fn tool_axiom_load(args: &JsonValue) -> Result<String, String> {
    let source = get_source(args)?;

    let session = axiom_optimize::AgentSession::from_source(&source)
        .map_err(|e| format!("Failed to load source: {e}"))?;

    let surfaces_json = surfaces_to_json(session.surfaces());
    let history_json =
        serde_json::to_value(session.history()).map_err(|e| format!("History error: {e}"))?;
    let transfer_json = session
        .transfer()
        .and_then(|t| serde_json::to_value(t).ok())
        .unwrap_or(JsonValue::Null);

    let result = serde_json::json!({
        "surfaces": surfaces_json,
        "history": history_json,
        "transfer": transfer_json,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {e}"))
}

/// `axiom_surfaces` — Get optimization surfaces.
fn tool_axiom_surfaces(args: &JsonValue) -> Result<String, String> {
    let source = get_source_required(args)?;

    let surfaces = axiom_optimize::extract_surfaces(&source)
        .map_err(|errs| format!("Surface extraction failed: {}", errs.join("; ")))?;

    let surfaces_json = surfaces_to_json(&surfaces);
    let result = serde_json::json!({ "surfaces": surfaces_json });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {e}"))
}

/// `axiom_propose` — Validate a proposal.
fn tool_axiom_propose(args: &JsonValue) -> Result<String, String> {
    let source = get_source_required(args)?;

    let values_obj = args
        .get("values")
        .ok_or_else(|| "Missing 'values' argument".to_string())?;
    let values_map = values_obj
        .as_object()
        .ok_or_else(|| "'values' must be a JSON object".to_string())?;

    let surfaces = axiom_optimize::extract_surfaces(&source)
        .map_err(|errs| format!("Surface extraction failed: {}", errs.join("; ")))?;

    let mut proposal = axiom_optimize::Proposal::new();
    for (name, json_val) in values_map {
        let value = json_to_value(json_val)
            .ok_or_else(|| format!("Cannot convert value for hole '{name}' to AXIOM Value"))?;
        proposal.set(name.clone(), value);
    }

    match axiom_optimize::validate_proposal(&proposal, &surfaces) {
        Ok(()) => {
            let result = serde_json::json!({ "valid": true, "errors": [] });
            serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {e}"))
        }
        Err(errors) => {
            let error_strs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
            let result = serde_json::json!({ "valid": false, "errors": error_strs });
            serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {e}"))
        }
    }
}

/// `axiom_compile` — Compile AXIOM source to LLVM IR or binary.
fn tool_axiom_compile(args: &JsonValue) -> Result<String, String> {
    let source = get_source_required(args)?;
    let emit = args
        .get("emit")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'emit' argument".to_string())?;

    // Parse -> HIR
    let parse_result = axiom_parser::parse(&source);
    if parse_result.has_errors() {
        let errs: Vec<String> = parse_result.errors.iter().map(|e| format!("{e}")).collect();
        return Err(format!("Parse errors: {}", errs.join("; ")));
    }

    let hir_module = axiom_hir::lower(&parse_result.module).map_err(|errs| {
        let msgs: Vec<String> = errs.iter().map(|e| format!("{e}")).collect();
        format!("HIR lowering errors: {}", msgs.join("; "))
    })?;

    match emit {
        "llvm-ir" => {
            let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errs| {
                let msgs: Vec<String> = errs.iter().map(|e| format!("{e}")).collect();
                format!("Codegen errors: {}", msgs.join("; "))
            })?;
            let result = serde_json::json!({ "success": true, "output": llvm_ir });
            serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {e}"))
        }
        "binary" => {
            let output_path = args
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or(if cfg!(windows) { "a.exe" } else { "a.out" });

            let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errs| {
                let msgs: Vec<String> = errs.iter().map(|e| format!("{e}")).collect();
                format!("Codegen errors: {}", msgs.join("; "))
            })?;

            crate::compile::compile_to_binary(&llvm_ir, output_path)
                .map_err(|e| format!("Compilation error: {e}"))?;

            let result = serde_json::json!({ "success": true, "output": output_path });
            serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {e}"))
        }
        other => Err(format!(
            "Invalid emit value '{other}': expected 'llvm-ir' or 'binary'"
        )),
    }
}

/// `axiom_history` — Get optimization history.
fn tool_axiom_history(args: &JsonValue) -> Result<String, String> {
    let source = get_source_required(args)?;

    // Validate that the source parses.
    let _session = axiom_optimize::AgentSession::from_source(&source)
        .map_err(|e| format!("Failed to load source: {e}"))?;

    // A fresh session always has empty history. In a real scenario the
    // caller would pass a path and we'd load persisted history.
    // For now, return the empty history and inform the caller.
    let session = axiom_optimize::AgentSession::from_source(&source)
        .map_err(|e| format!("Failed to load source: {e}"))?;

    let history_json =
        serde_json::to_value(session.history()).map_err(|e| format!("History error: {e}"))?;

    let result = serde_json::json!({ "records": history_json.get("records").cloned().unwrap_or(JsonValue::Array(vec![])) });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {e}"))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get source from either `path` or `source` argument.
fn get_source(args: &JsonValue) -> Result<String, String> {
    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read file '{path}': {e}"))
    } else if let Some(source) = args.get("source").and_then(|v| v.as_str()) {
        Ok(source.to_string())
    } else {
        Err("Must provide either 'path' or 'source' argument".to_string())
    }
}

/// Get source from the `source` argument (required).
fn get_source_required(args: &JsonValue) -> Result<String, String> {
    args.get("source")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Missing required 'source' argument".to_string())
}

/// Convert a JSON value to an AXIOM `Value`.
fn json_to_value(json: &JsonValue) -> Option<axiom_optimize::Value> {
    match json {
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(axiom_optimize::Value::Int(i))
            } else {
                n.as_f64().map(axiom_optimize::Value::Float)
            }
        }
        JsonValue::Bool(b) => Some(axiom_optimize::Value::Bool(*b)),
        JsonValue::String(s) => Some(axiom_optimize::Value::Ident(s.clone())),
        JsonValue::Array(items) => {
            let converted: Option<Vec<axiom_optimize::Value>> =
                items.iter().map(json_to_value).collect();
            converted.map(axiom_optimize::Value::Array)
        }
        _ => None,
    }
}

/// Convert AXIOM optimization surfaces to JSON.
fn surfaces_to_json(surfaces: &[axiom_optimize::OptSurface]) -> JsonValue {
    let items: Vec<JsonValue> = surfaces
        .iter()
        .map(|s| {
            let holes: Vec<JsonValue> = s
                .holes
                .iter()
                .map(|h| {
                    let mut hole_json = serde_json::json!({
                        "name": h.name,
                        "type": format!("{}", h.hole_type),
                    });
                    if let Some((lo, hi)) = h.range {
                        hole_json["range"] = serde_json::json!({ "lo": lo, "hi": hi });
                    }
                    if let Some(ref cv) = h.current_value {
                        hole_json["current_value"] = serde_json::json!(format!("{cv}"));
                    }
                    hole_json
                })
                .collect();
            serde_json::json!({
                "function": s.function_name,
                "holes": holes,
            })
        })
        .collect();
    JsonValue::Array(items)
}

/// Construct a success response.
fn make_success_response(id: Option<JsonValue>, result: JsonValue) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

/// Construct an error response.
fn make_error_response(id: Option<JsonValue>, code: i64, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
            data: None,
        }),
    }
}

/// Write a JSON-RPC response to stdout as a single line.
fn write_response(
    stdout: &io::Stdout,
    response: &JsonRpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(response)?;
    let mut handle = stdout.lock();
    writeln!(handle, "{json}")?;
    handle.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SOURCE: &str = r#"
fn add(a: i32, b: i32) -> i32 {
    @strategy {
        unroll: ?unroll_factor
    }
    return a + b;
}
"#;

    const MULTI_HOLE_SOURCE: &str = r#"
fn matmul(a: i32, b: i32) -> i32 {
    @strategy {
        tiling: { M: ?tile_m, N: ?tile_n }
        unroll: ?unroll_factor
    }
    return a + b;
}
"#;

    // --- Tool handler tests ---

    #[test]
    fn test_tool_axiom_load_with_source() {
        let args = serde_json::json!({ "source": TEST_SOURCE });
        let result = tool_axiom_load(&args);
        assert!(result.is_ok(), "tool_axiom_load failed: {:?}", result.err());
        let text = result.expect("already checked");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert!(parsed.get("surfaces").is_some());
        assert!(parsed.get("history").is_some());
        assert!(parsed.get("transfer").is_some());

        let surfaces = parsed["surfaces"].as_array().expect("surfaces array");
        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0]["function"], "add");
    }

    #[test]
    fn test_tool_axiom_load_invalid_source() {
        let args = serde_json::json!({ "source": "@@@ not valid" });
        let result = tool_axiom_load(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_axiom_load_missing_args() {
        let args = serde_json::json!({});
        let result = tool_axiom_load(&args);
        assert!(result.is_err());
        assert!(result.err().expect("error").contains("Must provide"));
    }

    #[test]
    fn test_tool_axiom_surfaces() {
        let args = serde_json::json!({ "source": TEST_SOURCE });
        let result = tool_axiom_surfaces(&args);
        assert!(result.is_ok(), "tool_axiom_surfaces failed: {:?}", result.err());
        let text = result.expect("already checked");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        let surfaces = parsed["surfaces"].as_array().expect("surfaces array");
        assert_eq!(surfaces.len(), 1);
        assert_eq!(surfaces[0]["function"], "add");

        let holes = surfaces[0]["holes"].as_array().expect("holes array");
        assert_eq!(holes.len(), 1);
        assert_eq!(holes[0]["name"], "unroll_factor");
        assert_eq!(holes[0]["type"], "u32");
    }

    #[test]
    fn test_tool_axiom_surfaces_multi_hole() {
        let args = serde_json::json!({ "source": MULTI_HOLE_SOURCE });
        let result = tool_axiom_surfaces(&args);
        assert!(result.is_ok());
        let text = result.expect("ok");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        let holes = parsed["surfaces"][0]["holes"]
            .as_array()
            .expect("holes array");
        assert_eq!(holes.len(), 3);
    }

    #[test]
    fn test_tool_axiom_surfaces_no_strategy() {
        let source = r#"
fn main() -> i32 {
    return 0;
}
"#;
        let args = serde_json::json!({ "source": source });
        let result = tool_axiom_surfaces(&args);
        assert!(result.is_ok());
        let text = result.expect("ok");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        let surfaces = parsed["surfaces"].as_array().expect("surfaces array");
        assert!(surfaces.is_empty());
    }

    #[test]
    fn test_tool_axiom_propose_valid() {
        let args = serde_json::json!({
            "source": TEST_SOURCE,
            "values": { "unroll_factor": 4 }
        });
        let result = tool_axiom_propose(&args);
        assert!(result.is_ok(), "tool_axiom_propose failed: {:?}", result.err());
        let text = result.expect("already checked");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["valid"], true);
        let errors = parsed["errors"].as_array().expect("errors array");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_tool_axiom_propose_invalid_type() {
        let args = serde_json::json!({
            "source": TEST_SOURCE,
            "values": { "unroll_factor": true }
        });
        let result = tool_axiom_propose(&args);
        assert!(result.is_ok());
        let text = result.expect("ok");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["valid"], false);
        let errors = parsed["errors"].as_array().expect("errors array");
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_tool_axiom_propose_out_of_range() {
        let args = serde_json::json!({
            "source": TEST_SOURCE,
            "values": { "unroll_factor": 100 }
        });
        let result = tool_axiom_propose(&args);
        assert!(result.is_ok());
        let text = result.expect("ok");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["valid"], false);
        let errors = parsed["errors"].as_array().expect("errors array");
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_tool_axiom_propose_missing_hole() {
        let args = serde_json::json!({
            "source": TEST_SOURCE,
            "values": {}
        });
        let result = tool_axiom_propose(&args);
        assert!(result.is_ok());
        let text = result.expect("ok");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["valid"], false);
    }

    #[test]
    fn test_tool_axiom_propose_unknown_hole() {
        let args = serde_json::json!({
            "source": TEST_SOURCE,
            "values": { "unroll_factor": 4, "nonexistent": 1 }
        });
        let result = tool_axiom_propose(&args);
        assert!(result.is_ok());
        let text = result.expect("ok");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["valid"], false);
    }

    #[test]
    fn test_tool_axiom_compile_llvm_ir() {
        let args = serde_json::json!({
            "source": TEST_SOURCE,
            "emit": "llvm-ir"
        });
        let result = tool_axiom_compile(&args);
        assert!(result.is_ok(), "tool_axiom_compile failed: {:?}", result.err());
        let text = result.expect("already checked");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["success"], true);
        let output = parsed["output"].as_str().expect("output string");
        // LLVM IR should contain standard markers
        assert!(output.contains("define"), "Expected LLVM IR define, got: {output}");
    }

    #[test]
    fn test_tool_axiom_compile_invalid_emit() {
        let args = serde_json::json!({
            "source": TEST_SOURCE,
            "emit": "wasm"
        });
        let result = tool_axiom_compile(&args);
        assert!(result.is_err());
        assert!(result.err().expect("error").contains("Invalid emit value"));
    }

    #[test]
    fn test_tool_axiom_compile_invalid_source() {
        let args = serde_json::json!({
            "source": "@@@ broken",
            "emit": "llvm-ir"
        });
        let result = tool_axiom_compile(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_axiom_history() {
        let args = serde_json::json!({ "source": TEST_SOURCE });
        let result = tool_axiom_history(&args);
        assert!(result.is_ok(), "tool_axiom_history failed: {:?}", result.err());
        let text = result.expect("already checked");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert!(parsed.get("records").is_some());
        let records = parsed["records"].as_array().expect("records array");
        assert!(records.is_empty()); // fresh session has no history
    }

    #[test]
    fn test_tool_axiom_history_invalid_source() {
        let args = serde_json::json!({ "source": "@@@ broken" });
        let result = tool_axiom_history(&args);
        assert!(result.is_err());
    }

    // --- JSON-RPC protocol tests ---

    #[test]
    fn test_handle_initialize() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(1.into())),
            method: "initialize".to_string(),
            params: None,
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.expect("should have result");
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["capabilities"]["tools"].is_object());
        assert_eq!(result["serverInfo"]["name"], "axiom-mcp");
    }

    #[test]
    fn test_handle_tools_list() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(2.into())),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.expect("should have result");
        let tools = result["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 5);

        let tool_names: Vec<&str> = tools
            .iter()
            .map(|t| t["name"].as_str().expect("name"))
            .collect();
        assert!(tool_names.contains(&"axiom_load"));
        assert!(tool_names.contains(&"axiom_surfaces"));
        assert!(tool_names.contains(&"axiom_propose"));
        assert!(tool_names.contains(&"axiom_compile"));
        assert!(tool_names.contains(&"axiom_history"));
    }

    #[test]
    fn test_handle_tools_call_surfaces() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(3.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "axiom_surfaces",
                "arguments": { "source": TEST_SOURCE }
            })),
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.expect("should have result");
        // isError is omitted when false (skip_serializing_if)
        assert!(result.get("isError").is_none() || result["isError"] == false);
        let content = result["content"].as_array().expect("content array");
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");

        // Parse the inner text as JSON
        let inner_text = content[0]["text"].as_str().expect("text string");
        let inner: JsonValue = serde_json::from_str(inner_text).expect("valid inner JSON");
        assert!(inner["surfaces"].is_array());
    }

    #[test]
    fn test_handle_unknown_method() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(99.into())),
            method: "nonexistent/method".to_string(),
            params: None,
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_some());
        let err = resp.error.expect("should have error");
        assert_eq!(err.code, -32601);
    }

    #[test]
    fn test_handle_tools_call_unknown_tool() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(4.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "nonexistent_tool",
                "arguments": {}
            })),
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_none()); // Tool errors are returned in the result
        let result = resp.result.expect("should have result");
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn test_handle_tools_call_missing_name() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(5.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({ "arguments": {} })),
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_some());
        let err = resp.error.expect("error");
        assert_eq!(err.code, -32602);
    }

    #[test]
    fn test_handle_tools_call_missing_params() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(6.into())),
            method: "tools/call".to_string(),
            params: None,
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_some());
    }

    // --- Helper tests ---

    #[test]
    fn test_json_to_value_int() {
        let v = json_to_value(&serde_json::json!(42));
        assert_eq!(v, Some(axiom_optimize::Value::Int(42)));
    }

    #[test]
    fn test_json_to_value_float() {
        let v = json_to_value(&serde_json::json!(3.14));
        assert!(matches!(v, Some(axiom_optimize::Value::Float(f)) if (f - 3.14).abs() < f64::EPSILON));
    }

    #[test]
    fn test_json_to_value_bool() {
        let v = json_to_value(&serde_json::json!(true));
        assert_eq!(v, Some(axiom_optimize::Value::Bool(true)));
    }

    #[test]
    fn test_json_to_value_string() {
        let v = json_to_value(&serde_json::json!("i"));
        assert_eq!(v, Some(axiom_optimize::Value::Ident("i".to_string())));
    }

    #[test]
    fn test_json_to_value_array() {
        let v = json_to_value(&serde_json::json!(["i", "j", "k"]));
        assert_eq!(
            v,
            Some(axiom_optimize::Value::Array(vec![
                axiom_optimize::Value::Ident("i".to_string()),
                axiom_optimize::Value::Ident("j".to_string()),
                axiom_optimize::Value::Ident("k".to_string()),
            ]))
        );
    }

    #[test]
    fn test_json_to_value_null() {
        let v = json_to_value(&serde_json::json!(null));
        assert_eq!(v, None);
    }

    #[test]
    fn test_surfaces_to_json_empty() {
        let surfaces: Vec<axiom_optimize::OptSurface> = vec![];
        let json = surfaces_to_json(&surfaces);
        assert!(json.is_array());
        assert!(json.as_array().expect("array").is_empty());
    }

    #[test]
    fn test_surfaces_to_json_with_range() {
        let surfaces = axiom_optimize::extract_surfaces(TEST_SOURCE).expect("should parse");
        let json = surfaces_to_json(&surfaces);
        let arr = json.as_array().expect("array");
        assert_eq!(arr.len(), 1);
        let hole = &arr[0]["holes"].as_array().expect("holes")[0];
        assert!(hole.get("range").is_some());
        assert_eq!(hole["range"]["lo"], 1);
        assert_eq!(hole["range"]["hi"], 32);
    }

    #[test]
    fn test_make_success_response_format() {
        let resp = make_success_response(
            Some(JsonValue::Number(1.into())),
            serde_json::json!({"test": true}),
        );
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
        let json_str = serde_json::to_string(&resp).expect("serialize");
        assert!(json_str.contains("\"jsonrpc\":\"2.0\""));
        assert!(!json_str.contains("\"error\""));
    }

    #[test]
    fn test_make_error_response_format() {
        let resp = make_error_response(Some(JsonValue::Number(1.into())), -32600, "test error");
        assert_eq!(resp.jsonrpc, "2.0");
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        let err = resp.error.as_ref().expect("error");
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "test error");
    }

    #[test]
    fn test_tool_axiom_load_with_transfer() {
        let source = r#"
fn main() -> i32 {
    @transfer {
        source_agent: "test-agent"
        context: "testing"
    }
    return 0;
}
"#;
        let args = serde_json::json!({ "source": source });
        let result = tool_axiom_load(&args);
        assert!(result.is_ok());
        let text = result.expect("ok");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert!(parsed["transfer"].is_object());
        assert_eq!(parsed["transfer"]["source_agent"], "test-agent");
    }

    #[test]
    fn test_tool_axiom_propose_multi_hole_valid() {
        let args = serde_json::json!({
            "source": MULTI_HOLE_SOURCE,
            "values": {
                "tile_m": 64,
                "tile_n": 64,
                "unroll_factor": 4
            }
        });
        let result = tool_axiom_propose(&args);
        assert!(result.is_ok());
        let text = result.expect("ok");
        let parsed: JsonValue = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["valid"], true);
    }

    #[test]
    fn test_full_protocol_flow() {
        // Simulate a full MCP session: initialize -> tools/list -> tools/call

        // 1. initialize
        let req1 = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(1.into())),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test-client", "version": "1.0" }
            })),
        };
        let resp1 = handle_request(&req1);
        assert!(resp1.error.is_none());

        // 2. tools/list
        let req2 = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(2.into())),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp2 = handle_request(&req2);
        assert!(resp2.error.is_none());
        let result2 = resp2.result.expect("result");
        let tools = result2["tools"].as_array().expect("tools");
        assert!(!tools.is_empty());

        // 3. tools/call — axiom_surfaces
        let req3 = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(3.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "axiom_surfaces",
                "arguments": { "source": TEST_SOURCE }
            })),
        };
        let resp3 = handle_request(&req3);
        assert!(resp3.error.is_none());
        let result3 = resp3.result.expect("result");
        assert!(result3.get("isError").is_none() || result3["isError"] == false);

        // 4. tools/call — axiom_propose
        let req4 = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(JsonValue::Number(4.into())),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "axiom_propose",
                "arguments": {
                    "source": TEST_SOURCE,
                    "values": { "unroll_factor": 4 }
                }
            })),
        };
        let resp4 = handle_request(&req4);
        assert!(resp4.error.is_none());
        let result4 = resp4.result.expect("result");
        let content4 = result4["content"][0]["text"].as_str().expect("text");
        let inner4: JsonValue = serde_json::from_str(content4).expect("JSON");
        assert_eq!(inner4["valid"], true);
    }
}
