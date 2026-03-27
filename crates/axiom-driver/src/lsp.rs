//! LSP (Language Server Protocol) server for AXIOM.
//!
//! Implements a subset of LSP over stdio (JSON-RPC 2.0):
//! - `initialize` / `initialized` — handshake
//! - `textDocument/didOpen` — parse the file, report diagnostics (parse + HIR)
//! - `textDocument/didChange` — re-parse, re-send diagnostics
//! - `textDocument/publishDiagnostics` — sent as notification to the client
//! - `textDocument/hover` — show function signatures and annotation info
//! - `textDocument/definition` — jump to function definition
//!
//! Reuses the JSON-RPC infrastructure pattern from the MCP server.

use std::collections::HashMap;
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

/// Parameters for `textDocument/hover` requests.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HoverParams {
    text_document: TextDocumentPositionUri,
    position: LspPosition,
}

/// Parameters for `textDocument/definition` requests.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DefinitionParams {
    text_document: TextDocumentPositionUri,
    position: LspPosition,
}

/// Document identifier with just a URI (used in hover/definition params).
#[derive(Debug, Deserialize)]
struct TextDocumentPositionUri {
    uri: String,
}

/// Position in a text document (0-based line and character).
#[derive(Debug, Deserialize)]
struct LspPosition {
    line: u32,
    character: u32,
}

#[derive(Debug, Serialize)]
struct Diagnostic {
    range: Range,
    severity: u32, // 1=Error, 2=Warning, 3=Info, 4=Hint
    message: String,
    source: String,
}

#[derive(Debug, Serialize, Clone)]
struct Range {
    start: Position,
    end: Position,
}

#[derive(Debug, Serialize, Clone)]
struct Position {
    line: u32,
    character: u32,
}

// ---------------------------------------------------------------------------
// Symbol table for hover and go-to-definition
// ---------------------------------------------------------------------------

/// Information about a function extracted from the parse/HIR for IDE features.
#[derive(Debug, Clone)]
struct FunctionSymbol {
    /// Function name.
    name: String,
    /// Parameters as `(name, type_string)` pairs.
    params: Vec<(String, String)>,
    /// Return type as a string.
    return_type: String,
    /// Annotations on this function (for hover display).
    annotations: Vec<String>,
    /// Byte offset of the function name in source.
    name_offset: usize,
    /// Length of the function name in bytes.
    name_len: usize,
}

/// Per-document state: source text and extracted symbols.
#[derive(Debug, Clone)]
struct DocumentState {
    /// The latest source text.
    source: String,
    /// Function symbols extracted from the last successful parse.
    functions: Vec<FunctionSymbol>,
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

/// Convert a (line, character) position to a byte offset in source text.
fn position_to_offset(source: &str, line: u32, character: u32) -> usize {
    let mut current_line = 0u32;
    let mut current_col = 0u32;
    for (i, ch) in source.char_indices() {
        if current_line == line && current_col == character {
            return i;
        }
        if ch == '\n' {
            if current_line == line {
                // Past end of this line, return end of line
                return i;
            }
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }
    source.len()
}

/// Format a type expression as a string for display.
fn type_expr_to_string(ty: &axiom_parser::ast::TypeExpr) -> String {
    use axiom_parser::ast::TypeExpr;
    match ty {
        TypeExpr::Named(name) => name.clone(),
        TypeExpr::Ptr(inner) => format!("ptr[{}]", type_expr_to_string(inner)),
        TypeExpr::ReadonlyPtr(inner) => format!("readonly_ptr[{}]", type_expr_to_string(inner)),
        TypeExpr::WriteonlyPtr(inner) => format!("writeonly_ptr[{}]", type_expr_to_string(inner)),
        TypeExpr::Array(inner, _size) => format!("array[{}]", type_expr_to_string(inner)),
        TypeExpr::Slice(inner) => format!("slice[{}]", type_expr_to_string(inner)),
        TypeExpr::Tensor(inner, _dims) => format!("tensor[{}]", type_expr_to_string(inner)),
        TypeExpr::Tuple(elems) => {
            let parts: Vec<String> = elems.iter().map(type_expr_to_string).collect();
            format!("({})", parts.join(", "))
        }
        TypeExpr::Fn(params, ret) => {
            let parts: Vec<String> = params.iter().map(type_expr_to_string).collect();
            format!("fn({}) -> {}", parts.join(", "), type_expr_to_string(ret))
        }
    }
}

/// Format an annotation as a string for display.
fn annotation_to_string(ann: &axiom_parser::ast::Annotation) -> String {
    use axiom_parser::ast::Annotation;
    match ann {
        Annotation::Pure => "@pure".to_string(),
        Annotation::Const => "@const".to_string(),
        Annotation::Inline(hint) => format!("@inline({hint:?})"),
        Annotation::Export => "@export".to_string(),
        Annotation::Intent(desc) => format!("@intent(\"{desc}\")"),
        Annotation::Complexity(expr) => format!("@complexity({expr})"),
        Annotation::Vectorizable(_) => "@vectorizable".to_string(),
        Annotation::Parallel(_) => "@parallel".to_string(),
        _ => "@...".to_string(),
    }
}

/// Extract function symbols from a parsed AST module.
fn extract_symbols(module: &axiom_parser::ast::Module) -> Vec<FunctionSymbol> {
    let mut symbols = Vec::new();
    for item in &module.items {
        match &item.node {
            axiom_parser::ast::Item::Function(func) => {
                let params: Vec<(String, String)> = func
                    .params
                    .iter()
                    .map(|p| (p.name.node.clone(), type_expr_to_string(&p.ty)))
                    .collect();
                let return_type = type_expr_to_string(&func.return_type);
                let annotations: Vec<String> = func
                    .annotations
                    .iter()
                    .map(|a| annotation_to_string(&a.node))
                    .collect();
                symbols.push(FunctionSymbol {
                    name: func.name.node.clone(),
                    params,
                    return_type,
                    annotations,
                    name_offset: func.name.span.start as usize,
                    name_len: (func.name.span.end - func.name.span.start) as usize,
                });
            }
            axiom_parser::ast::Item::ExternFunction(ef) => {
                let params: Vec<(String, String)> = ef
                    .params
                    .iter()
                    .map(|p| (p.name.node.clone(), type_expr_to_string(&p.ty)))
                    .collect();
                let return_type = type_expr_to_string(&ef.return_type);
                let annotations: Vec<String> = ef
                    .annotations
                    .iter()
                    .map(|a| annotation_to_string(&a.node))
                    .collect();
                symbols.push(FunctionSymbol {
                    name: ef.name.node.clone(),
                    params,
                    return_type,
                    annotations,
                    name_offset: ef.name.span.start as usize,
                    name_len: (ef.name.span.end - ef.name.span.start) as usize,
                });
            }
            _ => {}
        }
    }
    symbols
}

/// Find the word (identifier) at a given byte offset in the source.
/// Returns `(word, word_start_offset)` or None if not on a word.
fn word_at_offset(source: &str, offset: usize) -> Option<(String, usize)> {
    if offset > source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    // Find the start of the word
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    // Find the end of the word
    let mut end = offset;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    let word = source[start..end].to_string();
    Some((word, start))
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Parse AXIOM source and produce LSP diagnostics (parse errors + HIR errors).
fn parse_and_diagnose(source: &str) -> (Vec<Diagnostic>, Vec<FunctionSymbol>) {
    let result = axiom_parser::parse(source);
    let mut diagnostics = Vec::new();

    for err in &result.errors {
        let message = format!("{err}");
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

    // Extract symbols from the AST regardless of parse errors (best-effort).
    let symbols = extract_symbols(&result.module);

    // If parsing succeeded, also run HIR lowering for additional diagnostics.
    if result.errors.is_empty() {
        match axiom_hir::lower(&result.module) {
            Ok(_) => {
                // No HIR errors -- symbols already extracted from AST.
            }
            Err(hir_errors) => {
                for err in &hir_errors {
                    let message = format!("{err}");
                    let (start_offset, end_offset) = extract_hir_span(err);
                    let start = offset_to_position(source, start_offset);
                    let end = offset_to_position(source, end_offset);

                    diagnostics.push(Diagnostic {
                        range: Range { start, end },
                        severity: 1, // Error
                        message,
                        source: "axiom-hir".to_string(),
                    });
                }
            }
        }
    }

    (diagnostics, symbols)
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

/// Extract (start, end) byte offsets from a HIR LowerError.
fn extract_hir_span(err: &axiom_hir::LowerError) -> (usize, usize) {
    use axiom_hir::LowerError;
    match err {
        LowerError::InvalidAnnotationTarget { span, .. } => (span.offset(), span.offset() + span.len()),
        LowerError::UnknownType { span, .. } => (span.offset(), span.offset() + span.len()),
        LowerError::DuplicateDefinition { second_span, .. } => {
            (second_span.offset(), second_span.offset() + second_span.len())
        }
        LowerError::DuplicateModuleAnnotation { span, .. } => (span.offset(), span.offset() + span.len()),
        LowerError::InvalidArraySize { span, .. } => (span.offset(), span.offset() + span.len()),
        LowerError::StrictMissingAnnotations { span, .. } => (span.offset(), span.offset() + span.len()),
    }
}

/// Send publishDiagnostics notification for a document.
/// Returns the extracted function symbols for caching in document state.
fn publish_diagnostics(stdout: &io::Stdout, uri: &str, source: &str) -> io::Result<Vec<FunctionSymbol>> {
    let (diagnostics, symbols) = parse_and_diagnose(source);
    let notif = JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "textDocument/publishDiagnostics".to_string(),
        params: serde_json::json!({
            "uri": uri,
            "diagnostics": diagnostics,
        }),
    };
    send_notification(stdout, &notif)?;
    Ok(symbols)
}

/// Handle a `textDocument/hover` request.
/// Looks up the word under the cursor and returns function signature info.
fn handle_hover(
    documents: &HashMap<String, DocumentState>,
    uri: &str,
    line: u32,
    character: u32,
) -> Option<JsonValue> {
    let doc = documents.get(uri)?;
    let offset = position_to_offset(&doc.source, line, character);
    let (word, _word_start) = word_at_offset(&doc.source, offset)?;

    // Look up the word in function symbols.
    for func in &doc.functions {
        if func.name == word {
            let params_str: Vec<String> = func
                .params
                .iter()
                .map(|(name, ty)| format!("{name}: {ty}"))
                .collect();
            let mut hover_text = format!(
                "fn {}({}) -> {}",
                func.name,
                params_str.join(", "),
                func.return_type
            );
            if !func.annotations.is_empty() {
                hover_text = format!(
                    "{}\n{}",
                    func.annotations.join("\n"),
                    hover_text
                );
            }
            return Some(serde_json::json!({
                "contents": {
                    "kind": "markdown",
                    "value": format!("```axiom\n{hover_text}\n```")
                }
            }));
        }
    }

    // Check if hovering over a known annotation keyword.
    let annotation_docs = get_annotation_docs(&word);
    if let Some(doc_text) = annotation_docs {
        return Some(serde_json::json!({
            "contents": {
                "kind": "markdown",
                "value": doc_text
            }
        }));
    }

    None
}

/// Return documentation for known annotation keywords.
fn get_annotation_docs(word: &str) -> Option<&'static str> {
    match word {
        "pure" => Some("**@pure** -- No side effects. Enables fast-math, `memory(none)`, and `noalias` optimizations in LLVM."),
        "const" => Some("**@const** -- Compile-time evaluable. Enables `speculatable` attribute and constant folding."),
        "inline" => Some("**@inline(always | never | hint)** -- Controls function inlining. `always` maps to LLVM `alwaysinline`."),
        "export" => Some("**@export** -- Exports the function with C ABI (`dso_local`). Used for FFI."),
        "intent" => Some("**@intent(\"description\")** -- Semantic intent annotation for AI agent optimization."),
        "complexity" => Some("**@complexity(expr)** -- Documents the algorithmic complexity class (e.g., O(n log n))."),
        "vectorizable" => Some("**@vectorizable(dims)** -- Hint that a loop can be auto-vectorized. Emits LLVM loop vectorize metadata."),
        "parallel" => Some("**@parallel(dims)** -- Hint that a loop can be parallelized across dimensions."),
        "parallel_for" => Some("**@parallel_for(shared_read: [...], shared_write: [...], reduction(+: var))** -- Data-parallel for loop with OpenMP-style sharing clauses."),
        "constraint" => Some("**@constraint { key: value }** -- Hard performance constraints. `optimize_for: \"performance\"` maps to `-O3`."),
        "strategy" => Some("**@strategy { ... }** -- Optimization surface with `?holes` that AI agents can explore."),
        "target" => Some("**@target(device_class)** -- Target hardware specification for code generation."),
        "layout" => Some("**@layout(row_major | col_major)** -- Memory layout hint for arrays and tensors."),
        "align" => Some("**@align(bytes)** -- Memory alignment hint."),
        "lifetime" => Some("**@lifetime(scope | static | manual)** -- Lifetime control. `scope` enables heap-to-stack promotion."),
        "transfer" => Some("**@transfer { ... }** -- Inter-agent handoff block with confidence scores."),
        "optimization_log" => Some("**@optimization_log { ... }** -- Records optimization history for traceability."),
        _ => None,
    }
}

/// Handle a `textDocument/definition` request.
/// Returns the location of the function definition if the cursor is on a function name.
fn handle_definition(
    documents: &HashMap<String, DocumentState>,
    uri: &str,
    line: u32,
    character: u32,
) -> Option<JsonValue> {
    let doc = documents.get(uri)?;
    let offset = position_to_offset(&doc.source, line, character);
    let (word, _word_start) = word_at_offset(&doc.source, offset)?;

    // Look up the word in function symbols.
    for func in &doc.functions {
        if func.name == word {
            let def_start = offset_to_position(&doc.source, func.name_offset);
            let def_end = offset_to_position(&doc.source, func.name_offset + func.name_len);
            return Some(serde_json::json!({
                "uri": uri,
                "range": {
                    "start": { "line": def_start.line, "character": def_start.character },
                    "end": { "line": def_end.line, "character": def_end.character }
                }
            }));
        }
    }

    None
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

    // Per-document state for hover and go-to-definition.
    let mut documents: HashMap<String, DocumentState> = HashMap::new();

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
                        "hoverProvider": true,
                        "definitionProvider": true,
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
                        let uri = open_params.text_document.uri.clone();
                        let text = open_params.text_document.text.clone();
                        eprintln!("[AXIOM LSP] didOpen: {uri}");
                        let symbols = publish_diagnostics(&stdout, &uri, &text)?;
                        documents.insert(uri, DocumentState {
                            source: text,
                            functions: symbols,
                        });
                    }
                }
            }
            "textDocument/didChange" => {
                if let Some(params) = msg.params {
                    if let Ok(change_params) = serde_json::from_value::<DidChangeParams>(params) {
                        let uri = change_params.text_document.uri.clone();
                        if let Some(change) = change_params.content_changes.last() {
                            let text = change.text.clone();
                            eprintln!("[AXIOM LSP] didChange: {uri}");
                            let symbols = publish_diagnostics(&stdout, &uri, &text)?;
                            documents.insert(uri, DocumentState {
                                source: text,
                                functions: symbols,
                            });
                        }
                    }
                }
            }
            "textDocument/hover" => {
                if let Some(params) = msg.params {
                    if let Ok(hover_params) = serde_json::from_value::<HoverParams>(params) {
                        let uri = &hover_params.text_document.uri;
                        let result = handle_hover(
                            &documents,
                            uri,
                            hover_params.position.line,
                            hover_params.position.character,
                        );
                        let resp = make_response(
                            msg.id,
                            result.unwrap_or(serde_json::json!(null)),
                        );
                        send_response(&stdout, &resp)?;
                    } else if msg.id.is_some() {
                        let resp = make_response(msg.id, serde_json::json!(null));
                        send_response(&stdout, &resp)?;
                    }
                } else if msg.id.is_some() {
                    let resp = make_response(msg.id, serde_json::json!(null));
                    send_response(&stdout, &resp)?;
                }
            }
            "textDocument/definition" => {
                if let Some(params) = msg.params {
                    if let Ok(def_params) = serde_json::from_value::<DefinitionParams>(params) {
                        let uri = &def_params.text_document.uri;
                        let result = handle_definition(
                            &documents,
                            uri,
                            def_params.position.line,
                            def_params.position.character,
                        );
                        let resp = make_response(
                            msg.id,
                            result.unwrap_or(serde_json::json!(null)),
                        );
                        send_response(&stdout, &resp)?;
                    } else if msg.id.is_some() {
                        let resp = make_response(msg.id, serde_json::json!(null));
                        send_response(&stdout, &resp)?;
                    }
                } else if msg.id.is_some() {
                    let resp = make_response(msg.id, serde_json::json!(null));
                    send_response(&stdout, &resp)?;
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
    fn test_position_to_offset_start() {
        let offset = position_to_offset("hello\nworld", 0, 0);
        assert_eq!(offset, 0);
    }

    #[test]
    fn test_position_to_offset_second_line() {
        let offset = position_to_offset("hello\nworld", 1, 0);
        assert_eq!(offset, 6);
    }

    #[test]
    fn test_position_to_offset_mid_line() {
        let offset = position_to_offset("hello\nworld", 1, 2);
        assert_eq!(offset, 8);
    }

    #[test]
    fn test_word_at_offset_ident() {
        let source = "fn main() -> i32 { return 0; }";
        let result = word_at_offset(source, 3);
        assert!(result.is_some());
        let (word, start) = result.unwrap();
        assert_eq!(word, "main");
        assert_eq!(start, 3);
    }

    #[test]
    fn test_word_at_offset_no_word() {
        // Between two non-ident chars: "()" has no ident chars inside
        let source = "() + ()";
        // At offset 3 which is ' ' surrounded by '+' and ' '
        let result = word_at_offset(source, 3);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_and_diagnose_valid() {
        let source = "fn main() -> i32 { return 0; }";
        let (diags, symbols) = parse_and_diagnose(source);
        assert!(diags.is_empty());
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "main");
    }

    #[test]
    fn test_parse_and_diagnose_parse_error() {
        let source = "fn { broken";
        let (diags, _symbols) = parse_and_diagnose(source);
        assert!(!diags.is_empty());
        assert_eq!(diags[0].severity, 1);
        assert_eq!(diags[0].source, "axiom");
    }

    #[test]
    fn test_parse_and_diagnose_hir_error() {
        // A program that parses OK but has a HIR error (unknown type).
        let source = "fn foo(x: UnknownType) -> i32 { return 0; }";
        let (diags, symbols) = parse_and_diagnose(source);
        // Should have at least one HIR error about unknown type.
        assert!(!diags.is_empty());
        let has_hir_diag = diags.iter().any(|d| d.source == "axiom-hir");
        assert!(has_hir_diag, "expected HIR diagnostic for unknown type");
        // Symbols should still be extracted from the AST.
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
    }

    #[test]
    fn test_extract_symbols() {
        let source = "fn add(a: i32, b: i32) -> i32 { return a; }";
        let result = axiom_parser::parse(source);
        let symbols = extract_symbols(&result.module);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "add");
        assert_eq!(symbols[0].params.len(), 2);
        assert_eq!(symbols[0].params[0].0, "a");
        assert_eq!(symbols[0].params[0].1, "i32");
        assert_eq!(symbols[0].return_type, "i32");
    }

    #[test]
    fn test_handle_hover_function() {
        let source = "fn add(a: i32, b: i32) -> i32 { return a; }";
        let result = axiom_parser::parse(source);
        let symbols = extract_symbols(&result.module);
        let mut docs = HashMap::new();
        docs.insert("file:///test.axm".to_string(), DocumentState {
            source: source.to_string(),
            functions: symbols,
        });
        // Hover over "add" at offset 3 (line 0, char 3)
        let hover = handle_hover(&docs, "file:///test.axm", 0, 3);
        assert!(hover.is_some());
        let hover_val = hover.unwrap();
        let contents = hover_val["contents"]["value"].as_str().unwrap();
        assert!(contents.contains("fn add(a: i32, b: i32) -> i32"));
    }

    #[test]
    fn test_handle_hover_annotation() {
        let source = "@pure\nfn add(a: i32) -> i32 { return a; }";
        let result = axiom_parser::parse(source);
        let symbols = extract_symbols(&result.module);
        let mut docs = HashMap::new();
        docs.insert("file:///test.axm".to_string(), DocumentState {
            source: source.to_string(),
            functions: symbols,
        });
        // Hover over "pure" at line 0, char 1
        let hover = handle_hover(&docs, "file:///test.axm", 0, 1);
        assert!(hover.is_some());
        let hover_val = hover.unwrap();
        let contents = hover_val["contents"]["value"].as_str().unwrap();
        assert!(contents.contains("@pure"));
    }

    #[test]
    fn test_handle_definition() {
        let source = "fn add(a: i32, b: i32) -> i32 { return a; }\nfn main() -> i32 { return add(1, 2); }";
        let result = axiom_parser::parse(source);
        let symbols = extract_symbols(&result.module);
        let mut docs = HashMap::new();
        docs.insert("file:///test.axm".to_string(), DocumentState {
            source: source.to_string(),
            functions: symbols,
        });
        // "add" in the call on the second line
        // Find the offset of the second "add" occurrence
        let second_add_pos = source.rfind("add").unwrap();
        let pos = offset_to_position(source, second_add_pos);
        let def = handle_definition(&docs, "file:///test.axm", pos.line, pos.character);
        assert!(def.is_some());
        let def_val = def.unwrap();
        // Should point back to line 0 where the function is defined
        assert_eq!(def_val["range"]["start"]["line"], 0);
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

    #[test]
    fn test_type_expr_to_string_named() {
        let ty = axiom_parser::ast::TypeExpr::Named("i32".to_string());
        assert_eq!(type_expr_to_string(&ty), "i32");
    }

    #[test]
    fn test_type_expr_to_string_ptr() {
        let ty = axiom_parser::ast::TypeExpr::Ptr(
            Box::new(axiom_parser::ast::TypeExpr::Named("f64".to_string()))
        );
        assert_eq!(type_expr_to_string(&ty), "ptr[f64]");
    }

    #[test]
    fn test_get_annotation_docs() {
        assert!(get_annotation_docs("pure").is_some());
        assert!(get_annotation_docs("const").is_some());
        assert!(get_annotation_docs("nonexistent").is_none());
    }
}
