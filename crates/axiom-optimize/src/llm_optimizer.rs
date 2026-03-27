//! LLM-driven self-optimization pipeline for AXIOM programs.
//!
//! This module is the core differentiator of the AXIOM language: an AI feedback
//! loop that analyzes generated LLVM IR, assembly, and benchmark data to suggest
//! optimal `?param` values and code changes.
//!
//! # Architecture
//!
//! The optimization loop proceeds as follows:
//!
//! 1. Compile the AXIOM source to LLVM IR.
//! 2. Optionally compile to assembly via `clang -S`.
//! 3. Benchmark the compiled binary.
//! 4. Extract optimization surfaces (`?params`).
//! 5. Build a structured prompt with all of the above.
//! 6. Call the Claude API (via `curl`) to get optimization suggestions.
//! 7. Parse the response into structured [`LlmSuggestion`]s.
//! 8. Record the result in [`OptHistory`].
//!
//! If no `ANTHROPIC_API_KEY` is set, the pipeline falls back to **dry-run**
//! mode: it writes the prompt to a file and prints it, so the user can pipe
//! it to any LLM of their choice.
//!
//! # Example
//!
//! ```no_run
//! use axiom_optimize::llm_optimizer::{OptimizationContext, LlmSuggestion};
//! use axiom_optimize::llm_optimizer::{build_optimization_prompt, parse_llm_response};
//!
//! let ctx = OptimizationContext {
//!     source: "fn main() -> i32 { return 0; }".to_string(),
//!     llvm_ir: "; ModuleID = 'axiom'\n...".to_string(),
//!     assembly: None,
//!     benchmark_ms: Some(42.0),
//!     surfaces: vec![],
//!     history: vec![],
//!     iteration: 1,
//!     max_iterations: 5,
//!     target: "native".to_string(),
//!     constraints: vec![],
//! };
//!
//! let prompt = build_optimization_prompt(&ctx);
//! assert!(prompt.contains("AXIOM Optimization Request"));
//! ```

use std::collections::HashMap;
use std::process::Command;

use crate::history::OptRecord;
use crate::surface::OptSurface;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Full context passed to the LLM for analysis.
///
/// This struct captures everything the LLM needs to make an informed
/// optimization suggestion: source code, generated IR, assembly, timing
/// data, available tunable parameters, and previous optimization history.
#[derive(Debug, Clone)]
pub struct OptimizationContext {
    /// The AXIOM source code.
    pub source: String,
    /// Generated LLVM IR text.
    pub llvm_ir: String,
    /// Generated x86/ARM assembly, if available.
    pub assembly: Option<String>,
    /// Current median runtime in milliseconds.
    pub benchmark_ms: Option<f64>,
    /// Available optimization surfaces (functions with `?params`).
    pub surfaces: Vec<SurfaceInfo>,
    /// Previous optimization attempts and their results.
    pub history: Vec<HistoryEntry>,
    /// Current iteration number (1-based).
    pub iteration: usize,
    /// Total number of planned iterations.
    pub max_iterations: usize,
    /// Target architecture (e.g., "native", "x86_64-avx2").
    pub target: String,
    /// Extracted `@constraint` annotations from the source.
    pub constraints: Vec<ConstraintInfo>,
}

/// A parsed `@constraint` annotation for prompt display.
#[derive(Debug, Clone)]
pub struct ConstraintInfo {
    /// Constraint key (e.g., "optimize_for", "budget").
    pub key: String,
    /// Constraint value as a string (e.g., "performance", "frame_time < 16.6ms").
    pub value: String,
}

/// Simplified surface info for prompt building.
#[derive(Debug, Clone)]
pub struct SurfaceInfo {
    /// Function name.
    pub function_name: String,
    /// Tunable parameters in this function.
    pub params: Vec<ParamInfo>,
}

/// A single tunable parameter for prompt display.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// Parameter name (without leading `?`).
    pub name: String,
    /// Type description (e.g., "u32", "array[ident]").
    pub type_name: String,
    /// Range as a string (e.g., "1-512"), or "any" if unbounded.
    pub range: String,
    /// Current value as a string, or "unset".
    pub current_value: String,
}

/// A simplified history entry for prompt display.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// Iteration version label.
    pub version: String,
    /// Parameter values used.
    pub params: HashMap<String, String>,
    /// Runtime in milliseconds (if measured).
    pub time_ms: Option<f64>,
    /// Percentage change from previous best.
    pub change_pct: Option<f64>,
}

/// A suggestion from the LLM.
#[derive(Debug, Clone)]
pub struct LlmSuggestion {
    /// Suggested parameter values: `?param_name` -> value string.
    pub param_values: HashMap<String, serde_json::Value>,
    /// The LLM's reasoning for the suggestion.
    pub reasoning: String,
    /// Optional structural code change suggestions.
    pub code_changes: Vec<CodeChange>,
    /// Confidence score in the range `[0.0, 1.0]`.
    pub confidence: f64,
}

/// A structural code change suggestion from the LLM.
#[derive(Debug, Clone)]
pub struct CodeChange {
    /// Human-readable description of the change.
    pub description: String,
    /// The source line number this applies to, if known.
    pub line: Option<usize>,
}

/// Result of the LLM optimization step.
#[derive(Debug)]
pub enum LlmResult {
    /// The LLM returned a valid suggestion.
    Suggestion(LlmSuggestion),
    /// Dry-run mode: the prompt was generated but no LLM was called.
    DryRun {
        /// Path to the prompt file that was written.
        prompt_path: String,
        /// The prompt text itself.
        prompt: String,
    },
    /// The LLM call failed with an error.
    Error(String),
}

// ---------------------------------------------------------------------------
// Prompt building
// ---------------------------------------------------------------------------

/// Build the optimization prompt from the full context.
///
/// The prompt is structured as Markdown so the LLM can reason about it
/// naturally, with explicit instructions to return a JSON block.
pub fn build_optimization_prompt(ctx: &OptimizationContext) -> String {
    let mut p = String::with_capacity(8192);

    // Header
    p.push_str(&format!(
        "# AXIOM Optimization Request -- Iteration {}/{}\n\n",
        ctx.iteration, ctx.max_iterations
    ));

    p.push_str("You are an expert performance engineer analyzing an AXIOM program. ");
    p.push_str("AXIOM is a systems programming language with tunable `?param` holes ");
    p.push_str("inside `@strategy` blocks. Your job is to suggest concrete values for ");
    p.push_str("these parameters AND structural code changes that will maximize performance.\n\n");

    // Optimization Knowledge Base — accumulated wisdom from past sessions
    if let Ok(knowledge) = std::fs::read_to_string("docs/OPTIMIZATION_KNOWLEDGE.md") {
        p.push_str("## Optimization Knowledge Base (from past sessions)\n\n");
        // Include the rules section (skip the header and metadata)
        for line in knowledge.lines() {
            if line.starts_with("## Rule") || line.starts_with("## Anti-Pattern") {
                p.push_str(&format!("{line}\n"));
            } else if line.starts_with("**Pattern:**")
                || line.starts_with("**When to apply:")
                || line.starts_with("**Why:**")
                || line.starts_with("**Impact:**")
            {
                p.push_str(&format!("{line}\n"));
            }
        }
        p.push_str("\nApply any relevant rules from the knowledge base above. ");
        p.push_str("If you discover a NEW optimization pattern, note it in your reasoning ");
        p.push_str("so it can be added to the knowledge base.\n\n");
    }

    // Source code
    p.push_str("## Source Code\n\n```axiom\n");
    p.push_str(&ctx.source);
    if !ctx.source.ends_with('\n') {
        p.push('\n');
    }
    p.push_str("```\n\n");

    // LLVM IR (truncated to key functions)
    p.push_str("## Generated LLVM IR\n\n");
    let ir_snippet = truncate_ir(&ctx.llvm_ir, 200);
    p.push_str("```llvm\n");
    p.push_str(&ir_snippet);
    p.push_str("\n```\n\n");

    // Assembly (if available)
    if let Some(ref asm) = ctx.assembly {
        p.push_str("## Generated Assembly\n\n");
        let asm_snippet = truncate_ir(asm, 150);
        p.push_str("```asm\n");
        p.push_str(&asm_snippet);
        p.push_str("\n```\n\n");
    }

    // Benchmark results
    p.push_str("## Current Performance\n\n");
    if let Some(ms) = ctx.benchmark_ms {
        p.push_str(&format!("- **Runtime**: {:.3} ms (median of measurement runs)\n", ms));
    } else {
        p.push_str("- **Runtime**: not yet measured (first iteration)\n");
    }

    // Best previous result
    if let Some(best) = ctx.history.iter().filter_map(|h| h.time_ms).reduce(f64::min) {
        let best_entry = ctx.history.iter().find(|h| h.time_ms == Some(best));
        if let Some(entry) = best_entry {
            p.push_str(&format!(
                "- **Previous best**: {:.3} ms ({})\n",
                best, entry.version
            ));
        }
    }
    p.push('\n');

    // Available optimization parameters
    if !ctx.surfaces.is_empty() {
        p.push_str("## Available Optimization Parameters\n\n");
        p.push_str("| Parameter | Type | Range | Current Value |\n");
        p.push_str("|-----------|------|-------|---------------|\n");
        for surface in &ctx.surfaces {
            for param in &surface.params {
                p.push_str(&format!(
                    "| ?{} | {} | {} | {} |\n",
                    param.name, param.type_name, param.range, param.current_value
                ));
            }
        }
        p.push('\n');

        // Which function each surface belongs to
        p.push_str("### Functions with tunable parameters\n\n");
        for surface in &ctx.surfaces {
            let param_names: Vec<&str> = surface.params.iter().map(|p| p.name.as_str()).collect();
            p.push_str(&format!(
                "- `{}`: {}\n",
                surface.function_name,
                param_names.join(", ")
            ));
        }
        p.push('\n');
    }

    // Optimization history
    if !ctx.history.is_empty() {
        p.push_str("## Optimization History\n\n");
        p.push_str("| Iteration | Parameters | Runtime (ms) | Change |\n");
        p.push_str("|-----------|------------|-------------|--------|\n");
        for entry in &ctx.history {
            let params_str: Vec<String> = entry
                .params
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            let params_cell = params_str.join(", ");
            let time_cell = entry
                .time_ms
                .map(|t| format!("{:.3}", t))
                .unwrap_or_else(|| "N/A".to_string());
            let change_cell = entry
                .change_pct
                .map(|c| {
                    if c < 0.0 {
                        format!("{:.1}%", c)
                    } else if c > 0.0 {
                        format!("+{:.1}%", c)
                    } else {
                        "baseline".to_string()
                    }
                })
                .unwrap_or_else(|| "baseline".to_string());
            p.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                entry.version, params_cell, time_cell, change_cell
            ));
        }
        p.push('\n');
    }

    // Optimization constraints
    if !ctx.constraints.is_empty() {
        p.push_str("## Optimization Constraints\n\n");
        for c in &ctx.constraints {
            p.push_str(&format!("- {}: \"{}\"", c.key, c.value));
            // Add human-readable hint for well-known constraint keys
            match c.key.as_str() {
                "optimize_for" => match c.value.as_str() {
                    "performance" => p.push_str(" (prefer -O3, aggressive inlining, large tiles)"),
                    "memory" => p.push_str(" (prefer smaller working sets, streaming algorithms)"),
                    "size" => p.push_str(" (minimize binary and data footprint)"),
                    "latency" => p.push_str(" (minimize worst-case paths, avoid allocations)"),
                    _ => {}
                },
                "budget" => p.push_str(" (hard deadline constraint)"),
                _ => {}
            }
            p.push('\n');
        }
        p.push('\n');

        // Constraint-aware instructions for the LLM
        p.push_str("### Constraint-Aware Optimization Guidance\n\n");
        for c in &ctx.constraints {
            if c.key == "optimize_for" {
                match c.value.as_str() {
                    "memory" => {
                        p.push_str("When constraints specify \"memory\", prefer smaller working sets and streaming algorithms.\n");
                        p.push_str("Favor tile sizes that fit in L1 cache. Avoid large temporaries.\n\n");
                    }
                    "latency" => {
                        p.push_str("When constraints specify \"latency\", minimize worst-case paths and avoid allocations.\n");
                        p.push_str("Prefer branch-free code, avoid dynamic dispatch, and minimize pipeline stalls.\n\n");
                    }
                    "performance" => {
                        p.push_str("When constraints specify \"performance\", maximize throughput at all costs.\n");
                        p.push_str("Prefer large tiles, aggressive unrolling, and SIMD vectorization.\n\n");
                    }
                    "size" => {
                        p.push_str("When constraints specify \"size\", minimize code and data footprint.\n");
                        p.push_str("Prefer smaller unroll factors and avoid code duplication.\n\n");
                    }
                    _ => {}
                }
            }
            if c.key == "budget" {
                p.push_str(&format!(
                    "Hard budget constraint: {}. Ensure optimizations meet this deadline.\n\n",
                    c.value
                ));
            }
        }
    }

    // Target info
    p.push_str(&format!("## Target Architecture\n\n`{}`\n\n", ctx.target));

    // Task instructions
    p.push_str("## Task\n\n");
    p.push_str("Analyze the LLVM IR and assembly above. Consider:\n\n");
    p.push_str("1. **Cache behavior**: Are tile sizes aligned with L1/L2 cache line sizes (64 bytes)?\n");
    p.push_str("2. **Vectorization**: Can the inner loop be vectorized? What unroll factor enables SIMD?\n");
    p.push_str("3. **Loop ordering**: Would a different loop order improve spatial locality?\n");
    p.push_str("4. **Prefetching**: Would software prefetch hints help for streaming access patterns?\n");
    p.push_str("5. **Register pressure**: Is the unroll factor causing register spills?\n\n");

    if !ctx.history.is_empty() {
        p.push_str("Look at the history of previous attempts. ");
        p.push_str("Identify which direction of parameter changes improved performance ");
        p.push_str("and continue in that direction. Avoid repeating parameter combinations ");
        p.push_str("that have already been tried.\n\n");
    }

    // Response format
    p.push_str("## Required Response Format\n\n");
    p.push_str("Respond with EXACTLY ONE JSON block (no other text outside it) ");
    p.push_str("in this format:\n\n");
    p.push_str("```json\n");
    p.push_str("{\n");
    p.push_str("  \"params\": {\n");

    // List the actual params with example values
    for surface in &ctx.surfaces {
        for param in &surface.params {
            let example = match param.type_name.as_str() {
                "u32" | "i32" => "64".to_string(),
                "f64" => "1.0".to_string(),
                "bool" => "true".to_string(),
                _ if param.type_name.starts_with("array") => "[\"i\", \"j\", \"k\"]".to_string(),
                _ => "\"value\"".to_string(),
            };
            p.push_str(&format!("    \"{}\": {},\n", param.name, example));
        }
    }

    p.push_str("  },\n");
    p.push_str("  \"reasoning\": \"Explain WHY these values will improve performance based on the IR/assembly analysis...\",\n");
    p.push_str("  \"code_changes\": [\n");
    p.push_str("    { \"description\": \"Optional: suggest adding @vectorizable or other annotations\", \"line\": 0 }\n");
    p.push_str("  ],\n");
    p.push_str("  \"confidence\": 0.8\n");
    p.push_str("}\n");
    p.push_str("```\n");

    p
}

/// Build an [`OptimizationContext`] from the raw optimization state.
///
/// This is a convenience function that converts the internal types
/// ([`OptSurface`], [`OptRecord`]) into the prompt-friendly types
/// ([`SurfaceInfo`], [`HistoryEntry`]).
pub fn build_context(
    source: &str,
    llvm_ir: &str,
    assembly: Option<&str>,
    benchmark_ms: Option<f64>,
    surfaces: &[OptSurface],
    history: &[OptRecord],
    iteration: usize,
    max_iterations: usize,
    target: &str,
) -> OptimizationContext {
    let surface_infos: Vec<SurfaceInfo> = surfaces
        .iter()
        .map(|s| SurfaceInfo {
            function_name: s.function_name.clone(),
            params: s
                .holes
                .iter()
                .map(|h| ParamInfo {
                    name: h.name.clone(),
                    type_name: format!("{}", h.hole_type),
                    range: h
                        .range
                        .map(|(lo, hi)| format!("{lo}-{hi}"))
                        .unwrap_or_else(|| "any".to_string()),
                    current_value: h
                        .current_value
                        .as_ref()
                        .map(|v| format!("{v}"))
                        .unwrap_or_else(|| "unset".to_string()),
                })
                .collect(),
        })
        .collect();

    let mut history_entries: Vec<HistoryEntry> = Vec::new();
    let mut prev_best: Option<f64> = None;

    for record in history {
        let time_ms = record.metrics.get("time_ms").copied();

        let change_pct = match (time_ms, prev_best) {
            (Some(t), Some(best)) if best > 0.0 => Some(((t - best) / best) * 100.0),
            _ => None,
        };

        // Update best
        if let Some(t) = time_ms {
            prev_best = Some(match prev_best {
                Some(best) => best.min(t),
                None => t,
            });
        }

        let params: HashMap<String, String> = record
            .params
            .iter()
            .map(|(k, v)| (k.clone(), format!("{v}")))
            .collect();

        history_entries.push(HistoryEntry {
            version: record.version.clone(),
            params,
            time_ms,
            change_pct,
        });
    }

    // Extract @constraint annotations from source text.
    let constraints = extract_constraints_from_source(source);

    OptimizationContext {
        source: source.to_string(),
        llvm_ir: llvm_ir.to_string(),
        assembly: assembly.map(|s| s.to_string()),
        benchmark_ms,
        surfaces: surface_infos,
        history: history_entries,
        iteration,
        max_iterations,
        target: target.to_string(),
        constraints,
    }
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

/// Parse the LLM's response text into a structured [`LlmSuggestion`].
///
/// Extracts the JSON block from the response (handling markdown code fences),
/// then deserializes the fields. Returns an error if no valid JSON block is
/// found or if the required fields are missing.
pub fn parse_llm_response(response: &str) -> Result<LlmSuggestion, String> {
    // Extract JSON block from response
    let json_str = extract_json_block(response)?;

    // Parse as serde_json::Value
    let value: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("invalid JSON: {e}"))?;

    // Extract params
    let param_values: HashMap<String, serde_json::Value> = match value.get("params") {
        Some(serde_json::Value::Object(map)) => map
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        Some(_) => return Err("\"params\" must be an object".to_string()),
        None => return Err("missing \"params\" field".to_string()),
    };

    // Extract reasoning
    let reasoning = value
        .get("reasoning")
        .and_then(|v| v.as_str())
        .unwrap_or("(no reasoning provided)")
        .to_string();

    // Extract code changes (optional)
    let code_changes = match value.get("code_changes") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| {
                let desc = item.get("description")?.as_str()?.to_string();
                let line = item.get("line").and_then(|v| v.as_u64()).map(|v| v as usize);
                Some(CodeChange {
                    description: desc,
                    line,
                })
            })
            .collect(),
        _ => vec![],
    };

    // Extract confidence (optional, default 0.5)
    let confidence = value
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);

    Ok(LlmSuggestion {
        param_values,
        reasoning,
        code_changes,
        confidence,
    })
}

/// Extract a JSON block from text that may contain markdown code fences.
///
/// Handles:
/// - ```json ... ```
/// - ``` ... ```
/// - Bare JSON object `{ ... }`
fn extract_json_block(text: &str) -> Result<String, String> {
    // Try to find ```json ... ```
    if let Some(start) = text.find("```json") {
        let after_fence = start + "```json".len();
        if let Some(end) = text[after_fence..].find("```") {
            let json = text[after_fence..after_fence + end].trim();
            return Ok(json.to_string());
        }
    }

    // Try to find ``` ... ```
    if let Some(start) = text.find("```") {
        let after_fence = start + "```".len();
        // Skip to end of line (the language tag, if any)
        let content_start = text[after_fence..]
            .find('\n')
            .map(|pos| after_fence + pos + 1)
            .unwrap_or(after_fence);
        if let Some(end) = text[content_start..].find("```") {
            let json = text[content_start..content_start + end].trim();
            return Ok(json.to_string());
        }
    }

    // Try to find bare JSON object
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if end > start {
                let json = &text[start..=end];
                // Quick validation: try to parse it
                if serde_json::from_str::<serde_json::Value>(json).is_ok() {
                    return Ok(json.to_string());
                }
            }
        }
    }

    Err("no JSON block found in LLM response".to_string())
}

// ---------------------------------------------------------------------------
// LLM API calling
// ---------------------------------------------------------------------------

/// Call the Claude API via `curl`.
///
/// Uses the `ANTHROPIC_API_KEY` environment variable. If the `claude` CLI is
/// available and preferred, use [`call_claude_cli`] instead.
///
/// # Errors
///
/// Returns an error string if `curl` is not found, the API returns an error,
/// or the response cannot be parsed.
pub fn call_claude_api(prompt: &str, api_key: &str) -> Result<String, String> {
    // Build the request body as JSON
    let body = serde_json::json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 4096,
        "messages": [{
            "role": "user",
            "content": prompt
        }]
    });

    let body_str = body.to_string();

    let output = Command::new("curl")
        .arg("-s")
        .arg("-X")
        .arg("POST")
        .arg("https://api.anthropic.com/v1/messages")
        .arg("-H")
        .arg(format!("x-api-key: {}", api_key))
        .arg("-H")
        .arg("anthropic-version: 2023-06-01")
        .arg("-H")
        .arg("content-type: application/json")
        .arg("-d")
        .arg(&body_str)
        .output()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl failed ({}): {}", output.status, stderr));
    }

    let response_text = String::from_utf8_lossy(&output.stdout);

    // Parse the API response JSON
    let resp: serde_json::Value = serde_json::from_str(&response_text)
        .map_err(|e| format!("invalid API response JSON: {e}"))?;

    // Check for API errors
    if let Some(error) = resp.get("error") {
        let msg = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(format!("Claude API error: {msg}"));
    }

    // Extract text content from the response
    resp.get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|block| block.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no text content in API response: {response_text}"))
}

/// Call the `claude` CLI tool (Claude Code) with the prompt.
///
/// Falls back to [`call_claude_api`] if the `claude` CLI is not installed.
pub fn call_claude_cli(prompt: &str) -> Result<String, String> {
    let output = Command::new("claude")
        .arg("--print")
        .arg("--model")
        .arg("sonnet")
        .arg(prompt)
        .output()
        .map_err(|e| format!("failed to run claude CLI: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("claude CLI failed ({}): {}", output.status, stderr));
    }

    let response = String::from_utf8_lossy(&output.stdout).to_string();
    if response.trim().is_empty() {
        return Err("claude CLI returned empty response".to_string());
    }

    Ok(response)
}

// ---------------------------------------------------------------------------
// Top-level optimization runner
// ---------------------------------------------------------------------------

/// Run one iteration of LLM-driven optimization.
///
/// Depending on the environment, this will:
///
/// 1. If `api_key` is `Some`, call the Claude API via `curl`.
/// 2. If `api_key` is `None` but the `claude` CLI is on PATH, use that.
/// 3. Otherwise, fall back to dry-run mode: write the prompt to a temp file.
///
/// Returns an [`LlmResult`] describing what happened.
pub fn run_llm_optimization(
    source: &str,
    llvm_ir: &str,
    assembly: Option<&str>,
    benchmark_ms: Option<f64>,
    surfaces: &[OptSurface],
    history: &[OptRecord],
    iteration: usize,
    max_iterations: usize,
    target: &str,
    api_key: Option<&str>,
    dry_run: bool,
) -> LlmResult {
    // Build context and prompt
    let ctx = build_context(
        source,
        llvm_ir,
        assembly,
        benchmark_ms,
        surfaces,
        history,
        iteration,
        max_iterations,
        target,
    );
    let prompt = build_optimization_prompt(&ctx);

    // Dry-run mode: just write the prompt
    if dry_run {
        let prompt_path = write_prompt_to_temp(&prompt, iteration);
        return LlmResult::DryRun {
            prompt_path,
            prompt,
        };
    }

    // Try API key first
    if let Some(key) = api_key {
        match call_claude_api(&prompt, key) {
            Ok(response) => match parse_llm_response(&response) {
                Ok(suggestion) => return LlmResult::Suggestion(suggestion),
                Err(e) => {
                    return LlmResult::Error(format!(
                        "failed to parse LLM response: {e}\n\nRaw response:\n{response}"
                    ));
                }
            },
            Err(e) => return LlmResult::Error(format!("API call failed: {e}")),
        }
    }

    // Try claude CLI
    if is_claude_cli_available() {
        match call_claude_cli(&prompt) {
            Ok(response) => match parse_llm_response(&response) {
                Ok(suggestion) => return LlmResult::Suggestion(suggestion),
                Err(e) => {
                    return LlmResult::Error(format!(
                        "failed to parse claude CLI response: {e}\n\nRaw response:\n{response}"
                    ));
                }
            },
            Err(e) => {
                return LlmResult::Error(format!("claude CLI failed: {e}"));
            }
        }
    }

    // Fallback: dry-run
    let prompt_path = write_prompt_to_temp(&prompt, iteration);
    LlmResult::DryRun {
        prompt_path,
        prompt,
    }
}

// ---------------------------------------------------------------------------
// Assembly generation
// ---------------------------------------------------------------------------

/// Generate assembly from LLVM IR text using `clang -S`.
///
/// Returns `None` if clang is not available or compilation fails.
pub fn generate_assembly(llvm_ir: &str, target: &str) -> Option<String> {
    let temp_dir = std::env::temp_dir();
    let pid = std::process::id();
    let ll_path = temp_dir.join(format!("axiom_opt_{pid}.ll"));
    let asm_path = temp_dir.join(format!("axiom_opt_{pid}.s"));

    // Write IR
    std::fs::write(&ll_path, llvm_ir).ok()?;

    // Find clang
    let clang = find_clang()?;

    // Build args
    let mut cmd = Command::new(&clang);
    cmd.arg("-S")
        .arg("-O2")
        .arg("-Wno-override-module")
        .arg(&ll_path)
        .arg("-o")
        .arg(&asm_path);

    // Add target flags if not "native"
    if target != "native" {
        cmd.arg("-target").arg(target);
    }

    let output = cmd.output().ok()?;

    // Clean up .ll
    let _ = std::fs::remove_file(&ll_path);

    if !output.status.success() {
        let _ = std::fs::remove_file(&asm_path);
        return None;
    }

    let asm = std::fs::read_to_string(&asm_path).ok()?;
    let _ = std::fs::remove_file(&asm_path);

    Some(asm)
}

// ---------------------------------------------------------------------------
// Constraint extraction
// ---------------------------------------------------------------------------

/// Extract `@constraint { key: value, ... }` annotations from AXIOM source text.
///
/// This performs a lightweight textual scan so the LLM optimizer does not need to
/// depend on a full parse/HIR lower pass just to read constraints. The function
/// recognises the pattern `@constraint { key: "value", ... }` and returns each
/// key/value pair as a [`ConstraintInfo`].
pub fn extract_constraints_from_source(source: &str) -> Vec<ConstraintInfo> {
    let mut constraints = Vec::new();

    // Find all @constraint { ... } blocks in the source.
    let mut search_from = 0;
    while let Some(start) = source[search_from..].find("@constraint") {
        let abs_start = search_from + start;
        search_from = abs_start + "@constraint".len();

        // Find the opening brace
        let after_kw = &source[search_from..];
        let brace_start = match after_kw.find('{') {
            Some(pos) => search_from + pos,
            None => continue,
        };

        // Find matching closing brace (simple: first '}' after '{')
        let brace_end = match source[brace_start..].find('}') {
            Some(pos) => brace_start + pos,
            None => continue,
        };

        let inner = &source[brace_start + 1..brace_end];
        search_from = brace_end + 1;

        // Parse key: value pairs. Handles both `key: "string"` and `key: ident`.
        for pair in inner.split(',') {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some(colon_pos) = pair.find(':') {
                let key = pair[..colon_pos].trim().to_string();
                let raw_value = pair[colon_pos + 1..].trim();
                // Strip surrounding quotes if present
                let value = if (raw_value.starts_with('"') && raw_value.ends_with('"'))
                    || (raw_value.starts_with('\'') && raw_value.ends_with('\''))
                {
                    raw_value[1..raw_value.len() - 1].to_string()
                } else {
                    raw_value.to_string()
                };
                if !key.is_empty() {
                    constraints.push(ConstraintInfo { key, value });
                }
            }
        }
    }

    constraints
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Check if the `claude` CLI is available on PATH.
fn is_claude_cli_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Find a clang binary on PATH.
fn find_clang() -> Option<String> {
    let candidates = [
        "clang",
        "clang-19",
        "clang-18",
        "clang-17",
        "clang-16",
        "clang-15",
    ];
    for name in &candidates {
        let ok = Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some((*name).to_string());
        }
    }
    None
}

/// Write the optimization prompt to a temporary file and return its path.
fn write_prompt_to_temp(prompt: &str, iteration: usize) -> String {
    let temp_dir = std::env::temp_dir();
    let pid = std::process::id();
    let path = temp_dir.join(format!("axiom_opt_prompt_iter{iteration}_{pid}.md"));
    let path_str = path.to_string_lossy().to_string();

    if let Err(e) = std::fs::write(&path, prompt) {
        eprintln!("warning: could not write prompt to {}: {e}", path_str);
    }

    path_str
}

/// Truncate IR/assembly to at most `max_lines` lines, keeping the most
/// relevant parts (function definitions).
fn truncate_ir(ir: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = ir.lines().collect();
    if lines.len() <= max_lines {
        return ir.to_string();
    }

    // Try to keep function definitions -- lines starting with "define"
    let mut kept = Vec::new();
    let mut in_function = false;
    let mut function_lines = 0;
    let max_per_function = max_lines / 2;

    for line in &lines {
        if line.starts_with("define ") || line.starts_with("define internal ") {
            in_function = true;
            function_lines = 0;
            kept.push(*line);
        } else if in_function {
            function_lines += 1;
            if function_lines <= max_per_function {
                kept.push(*line);
            } else if function_lines == max_per_function + 1 {
                kept.push("  ; ... (truncated)");
            }
            if line.trim() == "}" {
                in_function = false;
            }
        } else if line.starts_with("@")
            || line.starts_with("target ")
            || line.starts_with("; ModuleID")
            || line.starts_with("source_filename")
            || line.starts_with("declare ")
        {
            kept.push(*line);
        }

        if kept.len() >= max_lines {
            kept.push("; ... (truncated, full IR available on request)");
            break;
        }
    }

    kept.join("\n")
}

// ---------------------------------------------------------------------------
// Source-to-source rewrite prompt (S3)
// ---------------------------------------------------------------------------

/// Build a source-to-source rewrite prompt for the `axiom rewrite` command.
///
/// Unlike the optimization prompt (which focuses on `?param` tuning), the
/// rewrite prompt asks the LLM for **code-level** improvements: extracting
/// `@pure` functions, using `heap_alloc_zeroed`, adding `@inline(always)` to
/// hot helpers, restructuring loops for vectorization, etc.
pub fn build_rewrite_prompt(source: &str) -> String {
    build_rewrite_prompt_with_remarks(source, &[])
}

/// Build a rewrite prompt with optional LLVM optimization remarks.
///
/// When `missed_opts` is non-empty, the prompt includes a section listing the
/// specific optimizations that LLVM attempted but could not apply, so the LLM
/// can address them directly in its rewrite.
pub fn build_rewrite_prompt_with_remarks(source: &str, missed_opts: &[String]) -> String {
    let mut p = String::with_capacity(4096);

    p.push_str("# AXIOM Source-to-Source Rewrite Request\n\n");

    p.push_str("You are an expert AXIOM performance engineer. Analyze the following AXIOM ");
    p.push_str("source code and suggest **code-level improvements** (not parameter tuning). ");
    p.push_str("Your goal is to produce a rewritten, optimized version of the source.\n\n");

    // Optimization Knowledge Base
    p.push_str("## Optimization Knowledge Base\n\n");
    p.push_str("Apply these rules when rewriting:\n\n");
    p.push_str("1. **Extract `@pure` functions**: Any function that only reads/writes through its parameters\n");
    p.push_str("   and has no side effects should be marked `@pure`. This enables LLVM `readnone`/`readonly`.\n\n");
    p.push_str("2. **Use `heap_alloc_zeroed` for large arrays**: When allocating arrays that need zero-\n");
    p.push_str("   initialization, prefer `heap_alloc_zeroed` over `heap_alloc` + manual zeroing loop.\n\n");
    p.push_str("3. **`@inline(always)` on hot helpers**: Small functions called in tight loops should be\n");
    p.push_str("   marked `@inline(always)` to avoid call overhead.\n\n");
    p.push_str("4. **Data layout by access pattern**: Choose data layout based on access pattern: SOA (flat arrays)\n");
    p.push_str("   is better for sweeping one property across all entities. AOS (structs with vec3 fields) is better\n");
    p.push_str("   when accessing all fields of one entity together (e.g., sphere center + radius + color in a\n");
    p.push_str("   raytracer). When you see 3 consecutive ptr_read_f64 calls packed into vec3(), the data should\n");
    p.push_str("   be stored as a struct with vec3 fields instead.\n\n");
    p.push_str("5. **Arena allocation**: Use `arena_create`/`arena_alloc` for groups of related allocations\n");
    p.push_str("   to reduce per-allocation overhead and improve locality.\n\n");
    p.push_str("6. **Loop strength reduction**: Replace `x * constant` with accumulating additions\n");
    p.push_str("   when the constant is a power of 2 or simple.\n\n");
    p.push_str("7. **Minimize pointer reads in loops**: Hoist `ptr_read_*` calls out of inner loops\n");
    p.push_str("   when the value doesn't change between iterations.\n\n");

    // LLVM optimization remarks (CompilerGPT feedback loop)
    if !missed_opts.is_empty() {
        p.push_str("## LLVM Optimization Remarks (missed)\n\n");
        p.push_str("The following optimizations were attempted by LLVM but failed. ");
        p.push_str("Your rewrite should address these specific issues:\n\n");
        for opt in missed_opts {
            p.push_str(&format!("- {opt}\n"));
        }
        p.push('\n');
    }

    // Include constraints if present
    let constraints = extract_constraints_from_source(source);
    if !constraints.is_empty() {
        p.push_str("## Source Constraints\n\n");
        for c in &constraints {
            p.push_str(&format!("- {}: \"{}\"\n", c.key, c.value));
        }
        p.push('\n');
    }

    // Source code
    p.push_str("## Source Code\n\n```axiom\n");
    p.push_str(source);
    if !source.ends_with('\n') {
        p.push('\n');
    }
    p.push_str("```\n\n");

    // Task instructions
    p.push_str("## Task\n\n");
    p.push_str("Rewrite the source code above applying the optimization rules. Focus on:\n\n");
    p.push_str("1. Adding `@pure` annotations to all qualifying functions\n");
    p.push_str("2. Adding `@inline(always)` to small hot helpers\n");
    p.push_str("3. Replacing `heap_alloc` + zero loops with `heap_alloc_zeroed`\n");
    p.push_str("4. Hoisting invariant `ptr_read_*` calls out of inner loops\n");
    p.push_str("5. Any other code-level improvements from the knowledge base\n");
    if !missed_opts.is_empty() {
        p.push_str("6. Addressing the LLVM missed optimization remarks listed above\n");
    }
    p.push('\n');

    // Response format
    p.push_str("## Required Response Format\n\n");
    p.push_str("Respond with EXACTLY ONE JSON block:\n\n");
    p.push_str("```json\n");
    p.push_str("{\n");
    p.push_str("  \"rewritten_source\": \"... the full rewritten AXIOM source ...\",\n");
    p.push_str("  \"changes\": [\n");
    p.push_str("    { \"description\": \"Added @pure to function X\", \"line\": 10 },\n");
    p.push_str("    { \"description\": \"Replaced heap_alloc + loop with heap_alloc_zeroed\", \"line\": 25 }\n");
    p.push_str("  ],\n");
    p.push_str("  \"reasoning\": \"Explain the performance impact of each change.\",\n");
    p.push_str("  \"confidence\": 0.8\n");
    p.push_str("}\n");
    p.push_str("```\n");

    p
}

/// A rewrite suggestion from the LLM.
#[derive(Debug, Clone)]
pub struct RewriteSuggestion {
    /// The rewritten AXIOM source code.
    pub rewritten_source: String,
    /// List of changes made.
    pub changes: Vec<CodeChange>,
    /// Reasoning for the changes.
    pub reasoning: String,
    /// Confidence score.
    pub confidence: f64,
}

/// Parse the LLM's response for a rewrite request.
pub fn parse_rewrite_response(response: &str) -> Result<RewriteSuggestion, String> {
    let json_str = extract_json_block(response)?;
    let value: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("invalid JSON: {e}"))?;

    let rewritten_source = value
        .get("rewritten_source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing \"rewritten_source\" field".to_string())?
        .to_string();

    let reasoning = value
        .get("reasoning")
        .and_then(|v| v.as_str())
        .unwrap_or("(no reasoning provided)")
        .to_string();

    let changes = match value.get("changes") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|item| {
                let desc = item.get("description")?.as_str()?.to_string();
                let line = item.get("line").and_then(|v| v.as_u64()).map(|v| v as usize);
                Some(CodeChange {
                    description: desc,
                    line,
                })
            })
            .collect(),
        _ => vec![],
    };

    let confidence = value
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);

    Ok(RewriteSuggestion {
        rewritten_source,
        changes,
        reasoning,
        confidence,
    })
}

/// Run the source-to-source rewrite pipeline.
///
/// In dry-run mode, prints the prompt. Otherwise, calls the LLM and returns
/// the rewrite suggestion.
pub fn run_rewrite(
    source: &str,
    api_key: Option<&str>,
    dry_run: bool,
) -> LlmResult {
    run_rewrite_with_remarks(source, api_key, dry_run, &[])
}

/// Run the source-to-source rewrite pipeline with LLVM optimization remarks.
///
/// When `missed_opts` is non-empty, the remarks are included in the prompt
/// so the LLM knows exactly which optimizations LLVM could not apply and can
/// address them in the rewrite (CompilerGPT feedback loop).
pub fn run_rewrite_with_remarks(
    source: &str,
    api_key: Option<&str>,
    dry_run: bool,
    missed_opts: &[String],
) -> LlmResult {
    let prompt = build_rewrite_prompt_with_remarks(source, missed_opts);

    if dry_run {
        let prompt_path = write_prompt_to_temp(&prompt, 0);
        return LlmResult::DryRun {
            prompt_path,
            prompt,
        };
    }

    // Try API key
    if let Some(key) = api_key {
        match call_claude_api(&prompt, key) {
            Ok(response) => {
                // For rewrite, we still return the raw suggestion as an LlmSuggestion
                // with the rewritten source in the reasoning field for simplicity.
                match parse_rewrite_response(&response) {
                    Ok(rewrite) => {
                        let suggestion = LlmSuggestion {
                            param_values: std::collections::HashMap::new(),
                            reasoning: rewrite.rewritten_source,
                            code_changes: rewrite.changes,
                            confidence: rewrite.confidence,
                        };
                        return LlmResult::Suggestion(suggestion);
                    }
                    Err(e) => {
                        return LlmResult::Error(format!(
                            "failed to parse rewrite response: {e}\n\nRaw response:\n{response}"
                        ));
                    }
                }
            }
            Err(e) => return LlmResult::Error(format!("API call failed: {e}")),
        }
    }

    // Try claude CLI
    if is_claude_cli_available() {
        match call_claude_cli(&prompt) {
            Ok(response) => {
                match parse_rewrite_response(&response) {
                    Ok(rewrite) => {
                        let suggestion = LlmSuggestion {
                            param_values: std::collections::HashMap::new(),
                            reasoning: rewrite.rewritten_source,
                            code_changes: rewrite.changes,
                            confidence: rewrite.confidence,
                        };
                        return LlmResult::Suggestion(suggestion);
                    }
                    Err(e) => {
                        return LlmResult::Error(format!(
                            "failed to parse claude CLI rewrite response: {e}\n\nRaw response:\n{response}"
                        ));
                    }
                }
            }
            Err(e) => return LlmResult::Error(format!("claude CLI failed: {e}")),
        }
    }

    // Fallback: dry-run
    let prompt_path = write_prompt_to_temp(&prompt, 0);
    LlmResult::DryRun {
        prompt_path,
        prompt,
    }
}

// ---------------------------------------------------------------------------
// CompilerGPT: LLVM optimization remarks extraction
// ---------------------------------------------------------------------------

/// Extract missed optimization remarks from a `.opt.yaml` file produced by
/// clang's `-fsave-optimization-record` flag.
///
/// Each returned string has the form:
/// `"MISSED: <name> in <function> (pass: <pass>)"`
pub fn extract_missed_optimizations(yaml_path: &str) -> Vec<String> {
    let content = match std::fs::read_to_string(yaml_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut missed = Vec::new();
    let mut in_missed = false;
    let mut current_pass = String::new();
    let mut current_name = String::new();
    let mut current_function = String::new();

    for line in content.lines() {
        if line.starts_with("--- !Missed") {
            in_missed = true;
            current_pass.clear();
            current_name.clear();
            current_function.clear();
        } else if line.starts_with("---") {
            if in_missed && !current_name.is_empty() {
                missed.push(format!(
                    "MISSED: {} in {} (pass: {})",
                    current_name, current_function, current_pass
                ));
            }
            in_missed = false;
        } else if in_missed {
            if let Some(val) = line.strip_prefix("Pass:") {
                current_pass = val.trim().trim_matches('\'').to_string();
            } else if let Some(val) = line.strip_prefix("Name:") {
                current_name = val.trim().trim_matches('\'').to_string();
            } else if let Some(val) = line.strip_prefix("Function:") {
                current_function = val.trim().trim_matches('\'').to_string();
            }
        }
    }
    // Catch last entry if file doesn't end with `---`
    if in_missed && !current_name.is_empty() {
        missed.push(format!(
            "MISSED: {} in {} (pass: {})",
            current_name, current_function, current_pass
        ));
    }

    missed
}

/// Generate actionable suggestions from missed optimization remarks.
///
/// Maps common LLVM pass names to concrete AXIOM-level actions.
pub fn suggest_actions_for_missed(missed: &[String]) -> Vec<String> {
    let mut suggestions = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for m in missed {
        let lower = m.to_lowercase();
        if lower.contains("loop-vectorize") || lower.contains("vectoriz") {
            let s = "Add @vectorizable to the function containing the loop".to_string();
            if seen.insert(s.clone()) {
                suggestions.push(s);
            }
        }
        if lower.contains("inline") {
            let s = "Consider @inline(always) for small hot functions".to_string();
            if seen.insert(s.clone()) {
                suggestions.push(s);
            }
        }
        if lower.contains("licm") || lower.contains("loop-invariant") {
            let s = "Move loop-invariant computations outside the loop".to_string();
            if seen.insert(s.clone()) {
                suggestions.push(s);
            }
        }
        if lower.contains("unroll") {
            let s = "Consider @strategy { unroll: N } to help the loop unroller".to_string();
            if seen.insert(s.clone()) {
                suggestions.push(s);
            }
        }
        if lower.contains("slp") || lower.contains("superword") {
            let s = "Restructure adjacent scalar operations to enable SLP vectorization".to_string();
            if seen.insert(s.clone()) {
                suggestions.push(s);
            }
        }
        if lower.contains("alias") || lower.contains("noalias") {
            let s = "Mark @pure on functions that don't alias memory, or use readonly_ptr/writeonly_ptr".to_string();
            if seen.insert(s.clone()) {
                suggestions.push(s);
            }
        }
    }

    suggestions
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> OptimizationContext {
        OptimizationContext {
            source: r#"fn matmul(a: i32, b: i32) -> i32 {
    @strategy {
        tiling: { M: ?tile_m, N: ?tile_n }
        unroll: ?unroll_factor
    }
    return a + b;
}"#
            .to_string(),
            llvm_ir: "; ModuleID = 'axiom'\ndefine i32 @matmul(i32 %a, i32 %b) {\n  %1 = add i32 %a, %b\n  ret i32 %1\n}\n".to_string(),
            assembly: Some(".text\n.globl matmul\nmatmul:\n  addl %esi, %edi\n  movl %edi, %eax\n  retq\n".to_string()),
            benchmark_ms: Some(42.5),
            surfaces: vec![SurfaceInfo {
                function_name: "matmul".to_string(),
                params: vec![
                    ParamInfo {
                        name: "tile_m".to_string(),
                        type_name: "u32".to_string(),
                        range: "1-512".to_string(),
                        current_value: "32".to_string(),
                    },
                    ParamInfo {
                        name: "tile_n".to_string(),
                        type_name: "u32".to_string(),
                        range: "1-512".to_string(),
                        current_value: "32".to_string(),
                    },
                    ParamInfo {
                        name: "unroll_factor".to_string(),
                        type_name: "u32".to_string(),
                        range: "1-32".to_string(),
                        current_value: "4".to_string(),
                    },
                ],
            }],
            history: vec![
                HistoryEntry {
                    version: "v1".to_string(),
                    params: [
                        ("tile_m".to_string(), "32".to_string()),
                        ("tile_n".to_string(), "32".to_string()),
                        ("unroll_factor".to_string(), "4".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                    time_ms: Some(45.2),
                    change_pct: None,
                },
                HistoryEntry {
                    version: "v2".to_string(),
                    params: [
                        ("tile_m".to_string(), "64".to_string()),
                        ("tile_n".to_string(), "64".to_string()),
                        ("unroll_factor".to_string(), "8".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                    time_ms: Some(28.1),
                    change_pct: Some(-37.8),
                },
            ],
            iteration: 3,
            max_iterations: 5,
            target: "native".to_string(),
            constraints: vec![],
        }
    }

    #[test]
    fn test_build_prompt_contains_key_sections() {
        let ctx = sample_context();
        let prompt = build_optimization_prompt(&ctx);

        // Header
        assert!(prompt.contains("AXIOM Optimization Request -- Iteration 3/5"));

        // Source code section
        assert!(prompt.contains("## Source Code"));
        assert!(prompt.contains("fn matmul"));
        assert!(prompt.contains("@strategy"));

        // LLVM IR section
        assert!(prompt.contains("## Generated LLVM IR"));
        assert!(prompt.contains("define i32 @matmul"));

        // Assembly section
        assert!(prompt.contains("## Generated Assembly"));
        assert!(prompt.contains("matmul:"));

        // Performance section
        assert!(prompt.contains("## Current Performance"));
        assert!(prompt.contains("42.500 ms"));

        // Parameters table
        assert!(prompt.contains("## Available Optimization Parameters"));
        assert!(prompt.contains("?tile_m"));
        assert!(prompt.contains("?tile_n"));
        assert!(prompt.contains("?unroll_factor"));
        assert!(prompt.contains("1-512"));
        assert!(prompt.contains("1-32"));

        // History table
        assert!(prompt.contains("## Optimization History"));
        assert!(prompt.contains("v1"));
        assert!(prompt.contains("v2"));
        assert!(prompt.contains("45.2"));
        assert!(prompt.contains("28.1"));

        // Task section
        assert!(prompt.contains("## Task"));
        assert!(prompt.contains("Cache behavior"));
        assert!(prompt.contains("Vectorization"));

        // Response format
        assert!(prompt.contains("## Required Response Format"));
        assert!(prompt.contains("\"params\""));
        assert!(prompt.contains("\"reasoning\""));
        assert!(prompt.contains("\"confidence\""));
    }

    #[test]
    fn test_build_prompt_no_assembly() {
        let mut ctx = sample_context();
        ctx.assembly = None;
        let prompt = build_optimization_prompt(&ctx);
        assert!(!prompt.contains("## Generated Assembly"));
    }

    #[test]
    fn test_build_prompt_no_benchmark() {
        let mut ctx = sample_context();
        ctx.benchmark_ms = None;
        let prompt = build_optimization_prompt(&ctx);
        assert!(prompt.contains("not yet measured"));
    }

    #[test]
    fn test_build_prompt_no_history() {
        let mut ctx = sample_context();
        ctx.history.clear();
        let prompt = build_optimization_prompt(&ctx);
        assert!(!prompt.contains("## Optimization History"));
    }

    #[test]
    fn test_build_prompt_no_surfaces() {
        let mut ctx = sample_context();
        ctx.surfaces.clear();
        let prompt = build_optimization_prompt(&ctx);
        assert!(!prompt.contains("## Available Optimization Parameters"));
    }

    #[test]
    fn test_parse_llm_response_valid_json_block() {
        let response = r#"Here is my analysis:

```json
{
  "params": {
    "tile_m": 128,
    "tile_n": 64,
    "unroll_factor": 8
  },
  "reasoning": "Doubling tile_m improves L1 cache utilization because each cache line is 64 bytes.",
  "code_changes": [
    { "description": "Add @vectorizable to inner loop", "line": 45 }
  ],
  "confidence": 0.85
}
```

This should improve performance significantly."#;

        let suggestion = parse_llm_response(response).expect("should parse");
        assert_eq!(suggestion.param_values.len(), 3);
        assert_eq!(suggestion.param_values["tile_m"], 128);
        assert_eq!(suggestion.param_values["tile_n"], 64);
        assert_eq!(suggestion.param_values["unroll_factor"], 8);
        assert!(suggestion.reasoning.contains("cache line"));
        assert_eq!(suggestion.code_changes.len(), 1);
        assert_eq!(suggestion.code_changes[0].line, Some(45));
        assert!((suggestion.confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_llm_response_bare_json() {
        let response = r#"{
  "params": { "x": 42 },
  "reasoning": "test",
  "confidence": 0.5
}"#;

        let suggestion = parse_llm_response(response).expect("should parse");
        assert_eq!(suggestion.param_values["x"], 42);
        assert_eq!(suggestion.reasoning, "test");
    }

    #[test]
    fn test_parse_llm_response_missing_params() {
        let response = r#"{ "reasoning": "no params" }"#;
        let result = parse_llm_response(response);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("missing \"params\""));
    }

    #[test]
    fn test_parse_llm_response_no_json() {
        let response = "This is just plain text with no JSON at all.";
        let result = parse_llm_response(response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_llm_response_defaults() {
        let response = r#"```json
{
  "params": { "tile": 64 }
}
```"#;
        let suggestion = parse_llm_response(response).expect("should parse");
        // reasoning defaults to placeholder
        assert_eq!(suggestion.reasoning, "(no reasoning provided)");
        // confidence defaults to 0.5
        assert!((suggestion.confidence - 0.5).abs() < f64::EPSILON);
        // code_changes defaults to empty
        assert!(suggestion.code_changes.is_empty());
    }

    #[test]
    fn test_parse_llm_response_confidence_clamped() {
        let response = r#"{ "params": { "x": 1 }, "confidence": 99.9 }"#;
        let suggestion = parse_llm_response(response).expect("should parse");
        assert!((suggestion.confidence - 1.0).abs() < f64::EPSILON);

        let response = r#"{ "params": { "x": 1 }, "confidence": -5.0 }"#;
        let suggestion = parse_llm_response(response).expect("should parse");
        assert!((suggestion.confidence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_llm_response_string_params() {
        let response = r#"{
  "params": {
    "loop_order": ["k", "j", "i"],
    "tile_m": 128
  },
  "reasoning": "reorder for better locality",
  "confidence": 0.7
}"#;
        let suggestion = parse_llm_response(response).expect("should parse");
        assert_eq!(suggestion.param_values.len(), 2);
        assert!(suggestion.param_values["loop_order"].is_array());
        assert_eq!(suggestion.param_values["tile_m"], 128);
    }

    #[test]
    fn test_extract_json_block_fenced() {
        let text = "Some text\n```json\n{\"a\": 1}\n```\nMore text";
        let json = extract_json_block(text).expect("should extract");
        assert_eq!(json, "{\"a\": 1}");
    }

    #[test]
    fn test_extract_json_block_plain_fence() {
        let text = "```\n{\"b\": 2}\n```";
        let json = extract_json_block(text).expect("should extract");
        assert_eq!(json, "{\"b\": 2}");
    }

    #[test]
    fn test_extract_json_block_bare() {
        let text = "Here: {\"c\": 3} done";
        let json = extract_json_block(text).expect("should extract");
        assert_eq!(json, "{\"c\": 3}");
    }

    #[test]
    fn test_extract_json_block_none() {
        let text = "no json here";
        assert!(extract_json_block(text).is_err());
    }

    #[test]
    fn test_truncate_ir_short() {
        let ir = "define i32 @main() {\n  ret i32 0\n}\n";
        let result = truncate_ir(ir, 100);
        assert_eq!(result, ir);
    }

    #[test]
    fn test_truncate_ir_long() {
        let mut ir = String::new();
        ir.push_str("; ModuleID = 'test'\n");
        ir.push_str("define i32 @func() {\n");
        for i in 0..500 {
            ir.push_str(&format!("  %v{i} = add i32 0, {i}\n"));
        }
        ir.push_str("  ret i32 %v0\n}\n");

        let result = truncate_ir(&ir, 50);
        let line_count = result.lines().count();
        assert!(
            line_count <= 55,
            "truncated IR should be around 50 lines, got {line_count}"
        );
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_build_context_converts_surfaces() {
        use crate::surface::{HoleType, OptHole, OptSurface};

        let surfaces = vec![OptSurface {
            function_name: "test_fn".to_string(),
            holes: vec![OptHole {
                name: "tile_m".to_string(),
                hole_type: HoleType::U32,
                range: Some((1, 512)),
                current_value: None,
            }],
            strategy: None,
        }];

        let ctx = build_context(
            "fn test_fn() -> i32 { return 0; }",
            "; IR",
            None,
            Some(10.0),
            &surfaces,
            &[],
            1,
            5,
            "native",
        );

        assert_eq!(ctx.surfaces.len(), 1);
        assert_eq!(ctx.surfaces[0].function_name, "test_fn");
        assert_eq!(ctx.surfaces[0].params.len(), 1);
        assert_eq!(ctx.surfaces[0].params[0].name, "tile_m");
        assert_eq!(ctx.surfaces[0].params[0].range, "1-512");
        assert_eq!(ctx.surfaces[0].params[0].current_value, "unset");
    }

    #[test]
    fn test_build_context_converts_history() {
        use crate::history::OptRecord;

        let history = vec![
            OptRecord::new("v1")
                .with_metric("time_ms", 50.0)
                .with_param("tile_m", serde_json::json!(32)),
            OptRecord::new("v2")
                .with_metric("time_ms", 30.0)
                .with_param("tile_m", serde_json::json!(64)),
        ];

        let ctx = build_context("source", "ir", None, None, &[], &history, 3, 5, "native");

        assert_eq!(ctx.history.len(), 2);
        assert_eq!(ctx.history[0].version, "v1");
        assert_eq!(ctx.history[0].time_ms, Some(50.0));
        assert!(ctx.history[0].change_pct.is_none()); // first entry is baseline
        assert_eq!(ctx.history[1].version, "v2");
        assert_eq!(ctx.history[1].time_ms, Some(30.0));
        // v2 is 30ms vs baseline 50ms => -40%
        let change = ctx.history[1].change_pct.expect("should have change");
        assert!((change - (-40.0)).abs() < 0.1);
    }

    #[test]
    fn test_write_prompt_to_temp() {
        let path = write_prompt_to_temp("test prompt content", 1);
        assert!(std::path::Path::new(&path).exists());
        let content = std::fs::read_to_string(&path).expect("should read");
        assert_eq!(content, "test prompt content");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_llm_suggestion_with_code_changes() {
        let response = r#"```json
{
  "params": { "tile_m": 256 },
  "reasoning": "Larger tiles improve cache reuse.",
  "code_changes": [
    { "description": "Add @vectorizable to inner loop", "line": 10 },
    { "description": "Consider @prefetch on array access" }
  ],
  "confidence": 0.9
}
```"#;
        let suggestion = parse_llm_response(response).expect("should parse");
        assert_eq!(suggestion.code_changes.len(), 2);
        assert_eq!(suggestion.code_changes[0].line, Some(10));
        assert!(suggestion.code_changes[1].line.is_none());
    }

    #[test]
    fn test_extract_constraints_from_source_basic() {
        let source = r#"
@constraint { optimize_for: "performance", budget: "frame_time < 16.6ms" }
fn render() -> i32 { return 0; }
"#;
        let constraints = extract_constraints_from_source(source);
        assert_eq!(constraints.len(), 2);
        assert_eq!(constraints[0].key, "optimize_for");
        assert_eq!(constraints[0].value, "performance");
        assert_eq!(constraints[1].key, "budget");
        assert_eq!(constraints[1].value, "frame_time < 16.6ms");
    }

    #[test]
    fn test_extract_constraints_from_source_ident_value() {
        let source = r#"@constraint { optimize_for: memory }"#;
        let constraints = extract_constraints_from_source(source);
        assert_eq!(constraints.len(), 1);
        assert_eq!(constraints[0].key, "optimize_for");
        assert_eq!(constraints[0].value, "memory");
    }

    #[test]
    fn test_extract_constraints_from_source_multiple_blocks() {
        let source = r#"
@constraint { optimize_for: "performance" }
fn fast_func() -> i32 { return 0; }

@constraint { optimize_for: "size" }
fn small_func() -> i32 { return 0; }
"#;
        let constraints = extract_constraints_from_source(source);
        assert_eq!(constraints.len(), 2);
        assert_eq!(constraints[0].value, "performance");
        assert_eq!(constraints[1].value, "size");
    }

    #[test]
    fn test_extract_constraints_from_source_no_constraints() {
        let source = "fn main() -> i32 { return 0; }";
        let constraints = extract_constraints_from_source(source);
        assert!(constraints.is_empty());
    }

    #[test]
    fn test_build_prompt_includes_constraints() {
        let mut ctx = sample_context();
        ctx.constraints = vec![
            ConstraintInfo {
                key: "optimize_for".to_string(),
                value: "performance".to_string(),
            },
            ConstraintInfo {
                key: "budget".to_string(),
                value: "frame_time < 16.6ms".to_string(),
            },
        ];
        let prompt = build_optimization_prompt(&ctx);
        assert!(prompt.contains("## Optimization Constraints"));
        assert!(prompt.contains("optimize_for"));
        assert!(prompt.contains("performance"));
        assert!(prompt.contains("prefer -O3, aggressive inlining, large tiles"));
        assert!(prompt.contains("budget"));
        assert!(prompt.contains("frame_time < 16.6ms"));
        assert!(prompt.contains("Constraint-Aware Optimization Guidance"));
        assert!(prompt.contains("maximize throughput"));
    }

    #[test]
    fn test_build_prompt_constraint_memory_guidance() {
        let mut ctx = sample_context();
        ctx.constraints = vec![ConstraintInfo {
            key: "optimize_for".to_string(),
            value: "memory".to_string(),
        }];
        let prompt = build_optimization_prompt(&ctx);
        assert!(prompt.contains("smaller working sets and streaming algorithms"));
    }

    #[test]
    fn test_build_prompt_constraint_latency_guidance() {
        let mut ctx = sample_context();
        ctx.constraints = vec![ConstraintInfo {
            key: "optimize_for".to_string(),
            value: "latency".to_string(),
        }];
        let prompt = build_optimization_prompt(&ctx);
        assert!(prompt.contains("minimize worst-case paths and avoid allocations"));
    }

    // -----------------------------------------------------------------------
    // S3: Rewrite prompt tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_rewrite_prompt_contains_key_sections() {
        let source = r#"fn add(a: i32, b: i32) -> i32 { return a + b; }"#;
        let prompt = build_rewrite_prompt(source);

        assert!(prompt.contains("AXIOM Source-to-Source Rewrite Request"));
        assert!(prompt.contains("## Optimization Knowledge Base"));
        assert!(prompt.contains("@pure"));
        assert!(prompt.contains("heap_alloc_zeroed"));
        assert!(prompt.contains("@inline(always)"));
        assert!(prompt.contains("## Source Code"));
        assert!(prompt.contains("fn add"));
        assert!(prompt.contains("## Task"));
        assert!(prompt.contains("## Required Response Format"));
        assert!(prompt.contains("rewritten_source"));
    }

    #[test]
    fn test_build_rewrite_prompt_includes_constraints() {
        let source = r#"@constraint { optimize_for: "performance" }
fn fast() -> i32 { return 0; }"#;
        let prompt = build_rewrite_prompt(source);
        assert!(prompt.contains("## Source Constraints"));
        assert!(prompt.contains("optimize_for"));
        assert!(prompt.contains("performance"));
    }

    #[test]
    fn test_parse_rewrite_response_valid() {
        let response = r#"```json
{
  "rewritten_source": "@pure\nfn add(a: i32, b: i32) -> i32 { return a + b; }",
  "changes": [
    { "description": "Added @pure annotation", "line": 1 }
  ],
  "reasoning": "The function has no side effects.",
  "confidence": 0.95
}
```"#;
        let result = parse_rewrite_response(response).expect("should parse");
        assert!(result.rewritten_source.contains("@pure"));
        assert_eq!(result.changes.len(), 1);
        assert!(result.reasoning.contains("no side effects"));
        assert!((result.confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_rewrite_response_missing_source() {
        let response = r#"{ "reasoning": "no source" }"#;
        let result = parse_rewrite_response(response);
        assert!(result.is_err());
        assert!(result.err().unwrap().contains("rewritten_source"));
    }

    // -----------------------------------------------------------------------
    // CompilerGPT: optimization remarks extraction tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_missed_optimizations_empty() {
        let missed = extract_missed_optimizations("/nonexistent/path.opt.yaml");
        assert!(missed.is_empty());
    }

    #[test]
    fn test_extract_missed_optimizations_parses_yaml() {
        let yaml = r#"--- !Missed
Pass:            loop-vectorize
Name:            CantVectorizeMemory
Function:        compute
--- !Passed
Pass:            inline
Name:            Inlined
Function:        helper
--- !Missed
Pass:            licm
Name:            LoopInvariantCondition
Function:        render
---
"#;
        let tmp = std::env::temp_dir().join("axiom_test_opt_remarks.opt.yaml");
        std::fs::write(&tmp, yaml).unwrap();
        let missed = extract_missed_optimizations(tmp.to_str().unwrap());
        std::fs::remove_file(&tmp).ok();

        assert_eq!(missed.len(), 2);
        assert!(missed[0].contains("CantVectorizeMemory"));
        assert!(missed[0].contains("compute"));
        assert!(missed[0].contains("loop-vectorize"));
        assert!(missed[1].contains("LoopInvariantCondition"));
        assert!(missed[1].contains("render"));
        assert!(missed[1].contains("licm"));
    }

    #[test]
    fn test_extract_missed_optimizations_last_entry_no_trailing_separator() {
        let yaml = r#"--- !Missed
Pass:            inline
Name:            TooCostly
Function:        big_func
"#;
        let tmp = std::env::temp_dir().join("axiom_test_opt_remarks_last.opt.yaml");
        std::fs::write(&tmp, yaml).unwrap();
        let missed = extract_missed_optimizations(tmp.to_str().unwrap());
        std::fs::remove_file(&tmp).ok();

        assert_eq!(missed.len(), 1);
        assert!(missed[0].contains("TooCostly"));
        assert!(missed[0].contains("big_func"));
        assert!(missed[0].contains("inline"));
    }

    #[test]
    fn test_build_rewrite_prompt_with_remarks_includes_section() {
        let source = "fn add(a: i32, b: i32) -> i32 { return a + b; }";
        let missed = vec![
            "MISSED: CantVectorizeMemory in compute (pass: loop-vectorize)".to_string(),
            "MISSED: TooCostly in render (pass: inline)".to_string(),
        ];
        let prompt = build_rewrite_prompt_with_remarks(source, &missed);

        assert!(prompt.contains("## LLVM Optimization Remarks (missed)"));
        assert!(prompt.contains("CantVectorizeMemory"));
        assert!(prompt.contains("TooCostly"));
        assert!(prompt.contains("Addressing the LLVM missed optimization remarks"));
    }

    #[test]
    fn test_build_rewrite_prompt_with_remarks_empty_no_section() {
        let source = "fn add(a: i32, b: i32) -> i32 { return a + b; }";
        let prompt = build_rewrite_prompt_with_remarks(source, &[]);

        assert!(!prompt.contains("## LLVM Optimization Remarks"));
        assert!(!prompt.contains("Addressing the LLVM missed"));
    }

    #[test]
    fn test_suggest_actions_for_missed() {
        let missed = vec![
            "MISSED: CantVectorizeMemory in compute (pass: loop-vectorize)".to_string(),
            "MISSED: TooCostly in helper (pass: inline)".to_string(),
            "MISSED: LoopInvariantCondition in render (pass: licm)".to_string(),
            "MISSED: UnrollFailed in tight_loop (pass: loop-unroll)".to_string(),
        ];
        let suggestions = suggest_actions_for_missed(&missed);

        assert!(suggestions.iter().any(|s| s.contains("@vectorizable")));
        assert!(suggestions.iter().any(|s| s.contains("@inline(always)")));
        assert!(suggestions.iter().any(|s| s.contains("loop-invariant")));
        assert!(suggestions.iter().any(|s| s.contains("@strategy")));
    }

    #[test]
    fn test_suggest_actions_deduplicates() {
        let missed = vec![
            "MISSED: CantVectorize1 in f1 (pass: loop-vectorize)".to_string(),
            "MISSED: CantVectorize2 in f2 (pass: loop-vectorize)".to_string(),
        ];
        let suggestions = suggest_actions_for_missed(&missed);

        // Should only have one vectorization suggestion, not two
        let vectorize_count = suggestions.iter()
            .filter(|s| s.contains("@vectorizable"))
            .count();
        assert_eq!(vectorize_count, 1);
    }
}
