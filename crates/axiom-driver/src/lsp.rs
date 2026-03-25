//! Minimal LSP (Language Server Protocol) server for AXIOM.
//!
//! Implements a subset of LSP over stdio (JSON-RPC 2.0):
//! - `initialize` / `initialized` — handshake
//! - `textDocument/didOpen` — parse the file, report diagnostics
//! - `textDocument/didChange` — re-parse, re-send diagnostics
//! - `textDocument/publishDiagnostics` — sent as notification to the client
//!
//! Reuses the JSON-RPC infrastructure pattern from the MCP server.

use std::io::{self, BufRead, Read as _, Write as _};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// JSON-RPC types (shared with MCP)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct JsonRpcMessage {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<JsonValue>,
    method: Option<String>,
    #[serde(default)]
    params: Option<JsonValue>,
}

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

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Serialize)]
struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    params: JsonValue,
}

// ---------------------------------------------------------------------------
// LSP types (minimal subset)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DidOpenParams {
    text_document: TextDocumentItem,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextDocumentItem {
    uri: String,
    #[allow(dead_code)]
    language_id: String,
    #[allow(dead_code)]
    version: i64,
    text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DidChangeParams {
    text_document: VersionedTextDocumentIdentifier,
    content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionedTextDocumentIdentifier {
    uri: String,
    #[allow(dead_code)]
    version: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct TextDocumentContentChangeEvent {
    text: String,
}

#[derive(Debug, Serialize)]
struct Diagnostic {
    range: Range,
    severity: u32, // 1=Error, 2=Warning, 3=Info, 4=Hint
    message: String,
    source: String,
}

#[derive(Debug, Serialize)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Debug, Serialize)]
struct Position {
    line: u32,
    character: u32,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_response(id: Option<JsonValue>, result: JsonValue) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

fn make_error(id: Option<JsonValue>, code: i64, message: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
        }),
    }
}

/// Write a JSON-RPC response using LSP Content-Length framing.
fn write_lsp_message(stdout: &io::Stdout, msg: &[u8]) -> io::Result<()> {
    let mut out = stdout.lock();
    write!(out, "Content-Length: {}\r\n\r\n", msg.len())?;
    out.write_all(msg)?;
    out.flush()
}

fn send_response(stdout: &io::Stdout, resp: &JsonRpcResponse) -> io::Result<()> {
    let json = serde_json::to_string(resp).unwrap();
    write_lsp_message(stdout, json.as_bytes())
}

fn send_notification(stdout: &io::Stdout, notif: &JsonRpcNotification) -> io::Result<()> {
    let json = serde_json::to_string(notif).unwrap();
    write_lsp_message(stdout, json.as_bytes())
}

/// Convert a byte offset in source text to a (line, character) position.
fn offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position { line, character: col }
}

/// Parse AXIOM source and produce LSP diagnostics.
fn parse_and_diagnose(source: &str) -> Vec<Diagnostic> {
    let result = axiom_parser::parse(source);
    let mut diagnostics = Vec::new();

    for err in &result.errors {
        // Extract the span from the error using Display
        let message = format!("{err}");

        // Extract byte offset from the ParseError span
        let (start_offset, end_offset) = extract_span(err);
        let start = offset_to_position(source, start_offset);
        let end = offset_to_position(source, end_offset);

        diagnostics.push(Diagnostic {
            range: Range { start, end },
            severity: 1, // Error
            message,
            source: "axiom".to_string(),
        });
    }

    diagnostics
}

/// Extract (start, end) byte offsets from a ParseError.
fn extract_span(err: &axiom_parser::ParseError) -> (usize, usize) {
    use axiom_parser::ParseError;
    match err {
        ParseError::UnexpectedToken { span, .. } => (span.offset(), span.offset() + span.len()),
        ParseError::UnexpectedEof { span, .. } => (span.offset(), span.offset() + span.len()),
        ParseError::InvalidAnnotation { span, .. } => (span.offset(), span.offset() + span.len()),
        ParseError::InvalidTypeExpression { span, .. } => (span.offset(), span.offset() + span.len()),
        ParseError::MissingSemicolon { span, .. } => (span.offset(), span.offset() + span.len()),
        ParseError::MissingClosingDelimiter { span, .. } => (span.offset(), span.offset() + span.len()),
        ParseError::InvalidExpression { span, .. } => (span.offset(), span.offset() + span.len()),
        ParseError::LexerError { span, .. } => (span.offset(), span.offset() + span.len()),
    }
}

/// Send publishDiagnostics notification for a document.
fn publish_diagnostics(stdout: &io::Stdout, uri: &str, source: &str) -> io::Result<()> {
    let diagnostics = parse_and_diagnose(source);
    let notif = JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "textDocument/publishDiagnostics".to_string(),
        params: serde_json::json!({
            "uri": uri,
            "diagnostics": diagnostics,
        }),
    };
    send_notification(stdout, &notif)
}

// ---------------------------------------------------------------------------
// LSP message reading (Content-Length framing)
// ---------------------------------------------------------------------------

/// Read a single LSP message from stdin using Content-Length framing.
/// Returns None on EOF.
fn read_lsp_message(stdin: &io::Stdin) -> io::Result<Option<String>> {
    let mut reader = stdin.lock();

    // Read headers until empty line
    let mut content_length: Option<usize> = None;
    loop {
        let mut header_line = String::new();
        let n = reader.read_line(&mut header_line)?;
        if n == 0 {
            return Ok(None); // EOF
        }
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            if let Ok(len) = rest.trim().parse::<usize>() {
                content_length = Some(len);
            }
        }
    }

    let length = match content_length {
        Some(l) => l,
        None => return Ok(None),
    };

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok(Some(String::from_utf8_lossy(&body).to_string()))
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the LSP stdio server. Returns when stdin is closed.
pub fn run_lsp_server() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    eprintln!("[AXIOM LSP] Server starting...");

    loop {
        let message = match read_lsp_message(&stdin)? {
            Some(m) => m,
            None => break, // EOF
        };

        let msg: JsonRpcMessage = match serde_json::from_str(&message) {
            Ok(m) => m,
            Err(e) => {
                let resp = make_error(None, -32700, &format!("Parse error: {e}"));
                send_response(&stdout, &resp)?;
                continue;
            }
        };

        let method = match &msg.method {
            Some(m) => m.clone(),
            None => continue, // Response to us — ignore
        };

        match method.as_str() {
            "initialize" => {
                let result = serde_json::json!({
                    "capabilities": {
                        "textDocumentSync": 1, // Full sync
                    },
                    "serverInfo": {
                        "name": "axiom-lsp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                });
                let resp = make_response(msg.id, result);
                send_response(&stdout, &resp)?;
            }
            "initialized" => {
                // Client acknowledgement — nothing to do
                eprintln!("[AXIOM LSP] Client initialized");
            }
            "textDocument/didOpen" => {
                if let Some(params) = msg.params {
                    if let Ok(open_params) = serde_json::from_value::<DidOpenParams>(params) {
                        let uri = &open_params.text_document.uri;
                        let text = &open_params.text_document.text;
                        eprintln!("[AXIOM LSP] didOpen: {uri}");
                        publish_diagnostics(&stdout, uri, text)?;
                    }
                }
            }
            "textDocument/didChange" => {
                if let Some(params) = msg.params {
                    if let Ok(change_params) = serde_json::from_value::<DidChangeParams>(params) {
                        let uri = &change_params.text_document.uri;
                        if let Some(change) = change_params.content_changes.last() {
                            eprintln!("[AXIOM LSP] didChange: {uri}");
                            publish_diagnostics(&stdout, uri, &change.text)?;
                        }
                    }
                }
            }
            "shutdown" => {
                eprintln!("[AXIOM LSP] Shutdown requested");
                let resp = make_response(msg.id, serde_json::json!(null));
                send_response(&stdout, &resp)?;
            }
            "exit" => {
                eprintln!("[AXIOM LSP] Exiting");
                break;
            }
            _ => {
                // Unknown method — if it has an id, send method-not-found error
                if msg.id.is_some() {
                    let resp = make_error(msg.id, -32601, &format!("Method not found: {method}"));
                    send_response(&stdout, &resp)?;
                }
            }
        }
    }

    eprintln!("[AXIOM LSP] Server stopped");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_to_position_start() {
        let pos = offset_to_position("hello\nworld", 0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn test_offset_to_position_second_line() {
        let pos = offset_to_position("hello\nworld", 6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn test_offset_to_position_mid_line() {
        let pos = offset_to_position("hello\nworld", 8);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 2);
    }

    #[test]
    fn test_parse_and_diagnose_valid() {
        let source = "fn main() -> i32 { return 0; }";
        let diags = parse_and_diagnose(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_parse_and_diagnose_error() {
        let source = "fn { broken";
        let diags = parse_and_diagnose(source);
        assert!(!diags.is_empty());
        assert_eq!(diags[0].severity, 1);
        assert_eq!(diags[0].source, "axiom");
    }

    #[test]
    fn test_make_response() {
        let resp = make_response(Some(JsonValue::from(1)), serde_json::json!({"ok": true}));
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_make_error() {
        let resp = make_error(Some(JsonValue::from(1)), -32601, "not found");
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
