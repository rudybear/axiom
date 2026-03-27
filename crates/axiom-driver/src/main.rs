//! AXIOM compiler CLI driver.

mod compile;
mod lsp;
mod mcp;

use clap::{Parser, Subcommand};
use std::collections::HashSet;
use std::path::Path;

#[derive(Parser)]
#[command(name = "axiom", about = "AXIOM — AI eXchange Intermediate Optimization Medium")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile an AXIOM source file
    Compile {
        /// Input .axm file
        input: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Emit intermediate representation instead of binary
        #[arg(long, value_parser = ["tokens", "ast", "hir", "mir", "mlir", "llvm-ir"])]
        emit: Option<String>,

        /// Target CPU architecture (e.g., "native", "x86-64-v4", "znver3").
        /// Overrides @target annotation. Defaults to native.
        #[arg(long)]
        target: Option<String>,

        /// Error output format: "text" (default) or "json" for machine-readable diagnostics
        #[arg(long, value_parser = ["text", "json"])]
        error_format: Option<String>,

        /// Enable debug mode: runtime bounds checks, assert messages
        #[arg(long)]
        debug: bool,

        /// Additional library search directories (passed as -L to linker)
        #[arg(long = "link-dir", short = 'L')]
        link_dirs: Vec<String>,

        /// Produce an LLVM optimization report (.opt.yaml) showing which
        /// optimizations were applied or missed
        #[arg(long)]
        opt_report: bool,

        /// Enable a sanitizer: address, thread, undefined, or memory
        #[arg(long, value_parser = ["address", "thread", "undefined", "memory"])]
        sanitize: Option<String>,
    },

    /// Tokenize an AXIOM source file (debug tool)
    Lex {
        /// Input .axm file
        input: String,
    },

    /// Benchmark an AXIOM source file
    Bench {
        /// Input .axm file
        input: String,

        /// Number of warmup runs
        #[arg(long, default_value = "3")]
        warmup: usize,

        /// Number of measurement runs
        #[arg(long, default_value = "5")]
        runs: usize,
    },

    /// Start MCP server for AI agent integration
    Mcp {},

    /// AI-driven optimization of an AXIOM program
    Optimize {
        /// Input .axm file
        input: String,

        /// Number of LLM optimization iterations
        #[arg(long, default_value = "5")]
        iterations: usize,

        /// Target architecture
        #[arg(long, default_value = "native")]
        target: String,

        /// Anthropic API key (or set ANTHROPIC_API_KEY env var)
        #[arg(long)]
        api_key: Option<String>,

        /// Just print the LLM prompt without calling the API
        #[arg(long)]
        dry_run: bool,

        /// Agent identifier for history records
        #[arg(long, default_value = "axiom-llm-optimizer")]
        agent: String,

        /// Track each optimization step with git commits (or .axiom-history/ if not in git)
        #[arg(long)]
        track: bool,
    },

    /// Profile an AXIOM program and suggest optimizations
    Profile {
        /// Input .axm file
        input: String,

        /// Number of profiling iterations
        #[arg(long, default_value = "10")]
        iterations: usize,
    },

    /// Format an AXIOM source file (parse → HIR → pretty-print)
    Fmt {
        /// Input .axm file
        input: String,
    },

    /// Generate documentation from AXIOM source
    Doc {
        /// Input .axm file
        input: String,
    },

    /// Start a minimal LSP server over stdio (E4)
    Lsp {},

    /// Profile-Guided Optimization: instrument, train, recompile (L4)
    Pgo {
        /// Input .axm file
        input: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Arguments to pass to the training run
        #[arg(long, num_args = 0..)]
        training_args: Vec<String>,
    },

    /// Watch a file for changes and recompile on save (G4)
    Watch {
        /// Input .axm file to watch
        input: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Build project from axiom.toml manifest or single .axm file with verified pipeline (E5)
    Build {
        /// Path to axiom.toml (defaults to ./axiom.toml)
        #[arg(long)]
        manifest: Option<String>,

        /// Optional .axm file for verified pipeline (verify + test + compile)
        #[arg(long)]
        input: Option<String>,
    },

    /// AI-driven source-to-source rewrite (S3)
    Rewrite {
        /// Input .axm file
        input: String,

        /// Just print the LLM prompt without calling the API
        #[arg(long)]
        dry_run: bool,

        /// Anthropic API key (or set ANTHROPIC_API_KEY env var)
        #[arg(long)]
        api_key: Option<String>,

        /// Write rewritten source to this output file
        #[arg(short, long)]
        output: Option<String>,

        /// Analyze LLVM optimization remarks and print suggestions (no LLM needed)
        #[arg(long)]
        analyze: bool,
    },

    /// Verify annotations in an AXIOM source file
    Verify {
        /// Input .axm file
        input: String,
    },

    /// Run tests for an AXIOM source file
    Test {
        /// Input .axm file
        input: String,

        /// Enable fuzz testing from @precondition constraints
        #[arg(long)]
        fuzz: bool,
    },
}

/// Process `@include "path.axm"` directives in source code.
///
/// Scans the source for lines matching `@include "path"` and replaces them
/// with the contents of the referenced file. Supports recursive includes up
/// to a depth of 10. Paths are resolved relative to `base_dir`.
fn process_includes(source: &str, base_dir: &Path, depth: usize, visited: &mut HashSet<String>) -> miette::Result<String> {
    if depth > 10 {
        return Err(miette::miette!("@include depth exceeds 10 — possible circular include"));
    }

    let mut output = String::with_capacity(source.len());
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@include") {
            let rest = rest.trim();
            // Extract the path from quotes
            if let Some(path_str) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                let include_path = base_dir.join(path_str);
                let canonical = include_path.display().to_string();
                if visited.contains(&canonical) {
                    // Already included — skip to avoid infinite recursion
                    output.push_str(&format!("// [already included: {path_str}]\n"));
                    continue;
                }
                visited.insert(canonical.clone());
                let included_source = std::fs::read_to_string(&include_path)
                    .map_err(|e| miette::miette!("@include failed for \"{}\": {}", include_path.display(), e))?;
                let included_dir = include_path.parent().unwrap_or(base_dir);
                let processed = process_includes(&included_source, included_dir, depth + 1, visited)?;
                output.push_str(&processed);
                output.push('\n');
            } else {
                // Not a valid include — pass through as-is
                output.push_str(line);
                output.push('\n');
            }
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    Ok(output)
}

/// Read a source file and process `@include` directives.
fn read_source_with_includes(input: &str) -> miette::Result<String> {
    let source = std::fs::read_to_string(input)
        .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;
    let base_dir = Path::new(input).parent().unwrap_or(Path::new("."));
    let mut visited = HashSet::new();
    visited.insert(
        std::fs::canonicalize(input)
            .unwrap_or_else(|_| Path::new(input).to_path_buf())
            .display()
            .to_string(),
    );
    process_includes(&source, base_dir, 0, &mut visited)
}

/// Resolve `import` declarations in a parsed module.
///
/// For each `Import` item in the module, look for a corresponding `.axm` file:
///   1. `lib/<name>.axm` relative to the compiler binary
///   2. `lib/<name>.axm` relative to the source file
///   3. `<name>.axm` relative to the source file
///
/// Parse the imported file and merge its extern function declarations and struct
/// definitions into the main module's AST (in-place). This allows user code to
/// `import renderer;` and then call `axiom_renderer_create(...)` directly.
///
/// When a module uses `pub` on any function, only `pub` items are visible to
/// importers. If no items are `pub`, all items are visible (legacy behavior).
///
/// `pub import renderer;` re-exports the imported module's declarations so that
/// transitive importers also see them.
///
/// Circular imports are detected via the `resolving` set and produce a warning.
fn resolve_imports(module: &mut axiom_parser::ast::Module, source_path: &str) {
    let mut resolving = HashSet::new();
    if let Ok(canonical) = std::fs::canonicalize(source_path) {
        resolving.insert(canonical);
    } else {
        resolving.insert(std::path::PathBuf::from(source_path));
    }
    resolve_imports_inner(module, source_path, &mut resolving);
}

/// Inner recursive import resolver with circular import detection.
fn resolve_imports_inner(
    module: &mut axiom_parser::ast::Module,
    source_path: &str,
    resolving: &mut HashSet<std::path::PathBuf>,
) {
    use axiom_parser::ast::Item;
    let source_dir = Path::new(source_path).parent().unwrap_or(Path::new("."));

    // Collect the import paths and their `pub` status first to avoid borrow issues.
    let import_info: Vec<(Vec<String>, bool)> = module
        .items
        .iter()
        .filter_map(|item| {
            if let Item::Import(ref decl) = item.node {
                Some((decl.path.clone(), decl.is_public))
            } else {
                None
            }
        })
        .collect();

    for (path_segments, is_pub_import) in import_info {
        let name = path_segments.join("/");
        let file_name = format!("{name}.axm");

        // Try multiple resolution paths.
        let candidates: Vec<std::path::PathBuf> = {
            let mut c = Vec::new();
            // 1. lib/<name>.axm relative to compiler binary
            if let Ok(exe) = std::env::current_exe() {
                if let Some(exe_dir) = exe.parent() {
                    c.push(exe_dir.join("lib").join(&file_name));
                }
            }
            // 2. lib/<name>.axm relative to workspace root (cwd)
            c.push(std::path::PathBuf::from("lib").join(&file_name));
            // 3. lib/<name>.axm relative to the source file
            c.push(source_dir.join("lib").join(&file_name));
            // 4. <name>.axm relative to the source file
            c.push(source_dir.join(&file_name));
            c
        };

        let resolved = candidates.iter().find(|p| p.exists());
        let resolved_path = match resolved {
            Some(p) => p.clone(),
            None => {
                eprintln!(
                    "warning: could not resolve import '{}' (tried: {})",
                    name,
                    candidates
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                continue;
            }
        };

        // Circular import detection: if this file is already being resolved
        // in the current chain, emit a warning and skip.
        let canonical = std::fs::canonicalize(&resolved_path)
            .unwrap_or_else(|_| resolved_path.clone());
        if resolving.contains(&canonical) {
            eprintln!(
                "warning: circular import detected: '{}' is already being resolved",
                name
            );
            continue;
        }

        let import_source = match std::fs::read_to_string(&resolved_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "warning: failed to read import '{}' from {}: {}",
                    name,
                    resolved_path.display(),
                    e
                );
                continue;
            }
        };

        let import_result = axiom_parser::parse(&import_source);
        if import_result.has_errors() {
            eprintln!(
                "warning: parse errors in import '{}' ({})",
                name,
                resolved_path.display()
            );
            for err in &import_result.errors {
                eprintln!("  {err}");
            }
            continue;
        }

        // Recursively resolve imports in the imported module (for re-exports
        // and transitive dependencies).
        let mut imported_module = import_result.module;
        resolving.insert(canonical.clone());
        resolve_imports_inner(
            &mut imported_module,
            &resolved_path.display().to_string(),
            resolving,
        );
        resolving.remove(&canonical);

        // Check if the imported module uses `pub` visibility at all.
        // If any item is marked `pub`, only `pub` items are visible.
        // If no items are marked `pub`, all items are visible (legacy behavior).
        let has_any_pub = imported_module.items.iter().any(|item| {
            match &item.node {
                Item::Function(f) => f.is_public,
                Item::ExternFunction(ef) => ef.is_public,
                _ => false,
            }
        });

        // Merge imported items into the main module.
        for item in imported_module.items {
            match &item.node {
                Item::ExternFunction(ef) => {
                    if !has_any_pub || ef.is_public {
                        module.items.push(item);
                    }
                }
                Item::Struct(_) => {
                    // Structs are always visible when imported.
                    module.items.push(item);
                }
                Item::Function(f) => {
                    if !has_any_pub || f.is_public {
                        module.items.push(item);
                    }
                }
                Item::Import(decl) if decl.is_public && is_pub_import => {
                    // Re-exported imports: if this import was `pub import X;`
                    // and X itself has `pub import Y;`, propagate Y.
                    module.items.push(item);
                }
                _ => {
                    // Skip non-pub imports and type aliases.
                }
            }
        }
    }
}

/// Generate markdown documentation from an AXIOM source file.
///
/// Parses the source, extracts `@intent` and `@module` annotations, lists all
/// function signatures, and prints a markdown document to stdout.
fn run_doc(input: &str) -> miette::Result<()> {
    let source = read_source_with_includes(input)?;

    let result = axiom_parser::parse(&source);
    if result.has_errors() {
        eprintln!("--- Parse Errors ---");
        for err in &result.errors {
            eprintln!("  {err}");
        }
        return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
    }

    let module = &result.module;

    // Header
    let module_name = module.name.as_ref().map(|s| s.node.as_str()).unwrap_or("(unnamed)");
    println!("# Module: {module_name}");
    println!();

    // Module-level annotations
    let mut has_module_annotations = false;
    for ann in &module.annotations {
        match &ann.node {
            axiom_parser::ast::Annotation::Intent(text) => {
                if !has_module_annotations {
                    println!("## Module Annotations");
                    println!();
                    has_module_annotations = true;
                }
                println!("- **Intent**: {text}");
            }
            axiom_parser::ast::Annotation::Module(name) => {
                if !has_module_annotations {
                    println!("## Module Annotations");
                    println!();
                    has_module_annotations = true;
                }
                println!("- **Module**: {name}");
            }
            _ => {}
        }
    }
    if has_module_annotations {
        println!();
    }

    // Functions
    println!("## Functions");
    println!();

    for item in &module.items {
        match &item.node {
            axiom_parser::ast::Item::Function(f) => {
                let name = &f.name.node;
                let params: Vec<String> = f.params.iter().map(|p| {
                    format!("{}: {:?}", p.name.node, p.ty)
                }).collect();
                let ret = format!("{:?}", f.return_type);
                println!("### `fn {name}({})`", params.join(", "));
                println!();
                println!("- **Returns**: `{ret}`");

                // Extract annotations
                for ann in &f.annotations {
                    match &ann.node {
                        axiom_parser::ast::Annotation::Intent(text) => {
                            println!("- **Intent**: {text}");
                        }
                        axiom_parser::ast::Annotation::Pure => {
                            println!("- **Pure**: yes");
                        }
                        axiom_parser::ast::Annotation::Const => {
                            println!("- **Const**: yes (compile-time evaluable)");
                        }
                        axiom_parser::ast::Annotation::Complexity(c) => {
                            println!("- **Complexity**: {c}");
                        }
                        axiom_parser::ast::Annotation::Export => {
                            println!("- **Exported**: yes");
                        }
                        axiom_parser::ast::Annotation::Inline(hint) => {
                            println!("- **Inline**: {hint:?}");
                        }
                        _ => {}
                    }
                }
                println!();
            }
            axiom_parser::ast::Item::ExternFunction(f) => {
                let name = &f.name.node;
                let params: Vec<String> = f.params.iter().map(|p| {
                    format!("{}: {:?}", p.name.node, p.ty)
                }).collect();
                let ret = format!("{:?}", f.return_type);
                println!("### `extern fn {name}({})`", params.join(", "));
                println!();
                println!("- **Returns**: `{ret}`");
                println!("- **External**: yes");
                println!();
            }
            axiom_parser::ast::Item::Struct(s) => {
                let name = &s.name.node;
                println!("### `struct {name}`");
                println!();
                for field in &s.fields {
                    println!("- `{}`: `{:?}`", field.name.node, field.ty);
                }
                println!();
            }
            _ => {}
        }
    }

    Ok(())
}

fn main() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Lex { input } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;
            let (tokens, errors) = axiom_lexer::Lexer::new(&source).tokenize();

            for tok in &tokens {
                let text = &source[tok.span.start as usize..tok.span.end as usize];
                println!("{:4}..{:<4} {:?}  {:?}", tok.span.start, tok.span.end, tok.kind, text);
            }

            if !errors.is_empty() {
                eprintln!("\n--- Errors ---");
                for err in &errors {
                    eprintln!("  {:?}", err);
                }
            }

            Ok(())
        }

        Commands::Bench {
            input,
            warmup,
            runs,
        } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

            let config = axiom_optimize::benchmark::BenchmarkConfig {
                warmup_runs: warmup,
                measurement_runs: runs,
                ..Default::default()
            };

            eprintln!(
                "Benchmarking {} ({} warmup, {} measurement runs)...",
                input, config.warmup_runs, config.measurement_runs
            );

            match axiom_optimize::benchmark::benchmark_source(&source, &config) {
                Ok(result) => {
                    println!("{result}");
                    Ok(())
                }
                Err(e) => Err(miette::miette!("benchmark failed: {}", e)),
            }
        }

        Commands::Mcp {} => {
            mcp::run_mcp_server()
                .map_err(|e| miette::miette!("MCP server error: {e}"))?;
            Ok(())
        }

        Commands::Optimize {
            input,
            iterations,
            target,
            api_key,
            dry_run,
            agent,
            track,
        } => run_optimize(&input, iterations, &target, api_key.as_deref(), dry_run, &agent, track),

        Commands::Profile { input, iterations } => run_profile(&input, iterations),

        Commands::Fmt { input } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;
            let result = axiom_parser::parse(&source);
            if result.has_errors() {
                eprintln!("--- Parse Errors ---");
                for err in &result.errors {
                    eprintln!("  {err}");
                }
                return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
            }
            match axiom_hir::lower(&result.module) {
                Ok(hir_module) => {
                    println!("{hir_module}");
                    Ok(())
                }
                Err(errors) => {
                    eprintln!("--- HIR Lowering Errors ---");
                    for err in &errors {
                        eprintln!("  {err}");
                    }
                    Err(miette::miette!("HIR lowering failed with {} error(s)", errors.len()))
                }
            }
        }

        Commands::Compile { input, output, emit, target, error_format, debug, link_dirs, opt_report, sanitize } => {
            let source = read_source_with_includes(&input)?;
            let use_json = error_format.as_deref() == Some("json");

            match emit.as_deref() {
                Some("tokens") => {
                    let (tokens, _) = axiom_lexer::Lexer::new(&source).tokenize();
                    for tok in &tokens {
                        println!("{:?}", tok);
                    }
                }
                Some("ast") => {
                    let result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                    }
                    println!("{:#?}", result.module);
                }
                Some("hir") => {
                    let mut result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                        return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
                    }
                    resolve_imports(&mut result.module, &input);
                    match axiom_hir::lower(&result.module) {
                        Ok(hir_module) => {
                            println!("{hir_module}");
                        }
                        Err(errors) => {
                            eprintln!("--- HIR Lowering Errors ---");
                            for err in &errors {
                                eprintln!("  {err}");
                            }
                            return Err(miette::miette!("HIR lowering failed with {} error(s)", errors.len()));
                        }
                    }
                }
                Some("llvm-ir") => {
                    let mut result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                        return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
                    }
                    resolve_imports(&mut result.module, &input);
                    let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
                        for err in &errors {
                            eprintln!("  {err}");
                        }
                        miette::miette!("HIR lowering failed with {} error(s)", errors.len())
                    })?;
                    let codegen_opts = axiom_codegen::CodegenOptions {
                        debug_mode: debug,
                        ..Default::default()
                    };
                    let llvm_ir = axiom_codegen::codegen_with_options(&hir_module, &codegen_opts).map_err(|errors| {
                        for err in &errors {
                            eprintln!("  {err}");
                        }
                        miette::miette!("codegen failed with {} error(s)", errors.len())
                    })?;
                    println!("{llvm_ir}");
                }
                Some(level) => {
                    return Err(miette::miette!("unknown --emit level '{}'. Valid: tokens, ast, hir, llvm-ir", level));
                }
                None => {
                    // Full compilation: .axm -> native binary
                    let mut result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        if use_json {
                            for err in &result.errors {
                                let msg = err.to_string().replace('\\', "\\\\").replace('"', "\\\"");
                                let file = input.replace('\\', "\\\\").replace('"', "\\\"");
                                eprintln!("{{\"severity\":\"error\",\"message\":\"{msg}\",\"file\":\"{file}\"}}");
                            }
                        } else {
                            eprintln!("--- Parse Errors ---");
                            for err in &result.errors {
                                eprintln!("  {err}");
                            }
                        }
                        return Err(miette::miette!(
                            "parsing failed with {} error(s)",
                            result.errors.len()
                        ));
                    }
                    resolve_imports(&mut result.module, &input);
                    let hir_module =
                        axiom_hir::lower(&result.module).map_err(|errors| {
                            if use_json {
                                for err in &errors {
                                    let msg = err.to_string().replace('\\', "\\\\").replace('"', "\\\"");
                                    let file = input.replace('\\', "\\\\").replace('"', "\\\"");
                                    eprintln!("{{\"severity\":\"error\",\"message\":\"{msg}\",\"file\":\"{file}\"}}");
                                }
                            } else {
                                for err in &errors {
                                    eprintln!("  {err}");
                                }
                            }
                            miette::miette!(
                                "HIR lowering failed with {} error(s)",
                                errors.len()
                            )
                        })?;
                    let codegen_opts = axiom_codegen::CodegenOptions {
                        debug_mode: debug,
                        ..Default::default()
                    };
                    let llvm_ir =
                        axiom_codegen::codegen_with_options(&hir_module, &codegen_opts).map_err(|errors| {
                            if use_json {
                                for err in &errors {
                                    let msg = err.to_string().replace('\\', "\\\\").replace('"', "\\\"");
                                    let file = input.replace('\\', "\\\\").replace('"', "\\\"");
                                    eprintln!("{{\"severity\":\"error\",\"message\":\"{msg}\",\"file\":\"{file}\"}}");
                                }
                            } else {
                                for err in &errors {
                                    eprintln!("  {err}");
                                }
                            }
                            miette::miette!(
                                "codegen failed with {} error(s)",
                                errors.len()
                            )
                        })?;

                    let output_path = output.unwrap_or_else(|| {
                        if cfg!(windows) {
                            "a.exe".into()
                        } else {
                            "a.out".into()
                        }
                    });

                    // Build compile options from CLI flags and source annotations.
                    let constraints = axiom_optimize::llm_optimizer::extract_constraints_from_source(&source);
                    let optimize_for = constraints.iter()
                        .find(|c| c.key == "optimize_for")
                        .map(|c| c.value.clone());
                    let compile_opts = compile::CompileOptions {
                        target_arch: target.clone(),
                        optimize_for,
                        ir_text: Some(llvm_ir.clone()),
                        link_dirs: link_dirs.clone(),
                        opt_report,
                        sanitize: sanitize.clone(),
                        debug_mode: debug,
                        record_mode: false,
                    };

                    compile::compile_to_binary_with_options(&llvm_ir, &output_path, &compile_opts)?;
                    eprintln!("compiled {} -> {}", input, output_path);

                    // If --opt-report was requested, print the path to the .opt.yaml
                    // file produced by -fsave-optimization-record.
                    if opt_report {
                        let opt_yaml_path = format!("{}.opt.yaml", output_path.trim_end_matches(".exe"));
                        let opt_yaml = std::path::Path::new(&opt_yaml_path);
                        if opt_yaml.exists() {
                            eprintln!("[OPT-REPORT] Optimization remarks written to: {}", opt_yaml.display());
                            // Print a summary of the optimization remarks.
                            if let Ok(contents) = std::fs::read_to_string(opt_yaml) {
                                let mut applied = 0u32;
                                let mut missed = 0u32;
                                for line in contents.lines() {
                                    if line.starts_with("--- !Passed") {
                                        applied += 1;
                                    } else if line.starts_with("--- !Missed") {
                                        missed += 1;
                                    }
                                }
                                eprintln!("[OPT-REPORT] Summary: {} optimizations applied, {} missed", applied, missed);
                            }
                        } else {
                            eprintln!("[OPT-REPORT] No .opt.yaml file produced (clang may not support -fsave-optimization-record)");
                        }
                    }
                }
            }

            Ok(())
        }

        Commands::Doc { input } => run_doc(&input),

        // E4: LSP Server
        Commands::Lsp {} => {
            lsp::run_lsp_server()
                .map_err(|e| miette::miette!("LSP server error: {e}"))?;
            Ok(())
        }

        // L4: PGO Bootstrap
        Commands::Pgo { input, output, training_args } => {
            let source = read_source_with_includes(&input)?;
            let result = axiom_parser::parse(&source);
            if result.has_errors() {
                for err in &result.errors {
                    eprintln!("  {err}");
                }
                return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
            }
            let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
                for err in &errors { eprintln!("  {err}"); }
                miette::miette!("HIR lowering failed with {} error(s)", errors.len())
            })?;
            let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
                for err in &errors { eprintln!("  {err}"); }
                miette::miette!("codegen failed with {} error(s)", errors.len())
            })?;

            let output_path = output.unwrap_or_else(|| {
                if cfg!(windows) { "a.exe".into() } else { "a.out".into() }
            });

            let message = compile::compile_with_pgo(&llvm_ir, &output_path, &training_args)?;
            eprintln!("{message}");
            Ok(())
        }

        // G4: Watch (Hot Reload concept)
        Commands::Watch { input, output } => {
            run_watch(&input, output.as_deref())
        }

        // E5: Build from axiom.toml or verified pipeline for single .axm file
        Commands::Build { manifest, input } => {
            if let Some(ref axm_file) = input {
                if axm_file.ends_with(".axm") {
                    return run_verified_build(axm_file);
                }
            }
            let manifest_path = manifest.unwrap_or_else(|| "axiom.toml".to_string());
            run_build(&manifest_path)
        }

        // S3: Source-to-Source AI Rewrite
        Commands::Rewrite { input, dry_run, api_key, output, analyze } => {
            run_rewrite(&input, dry_run, api_key.as_deref(), output.as_deref(), analyze)
        }

        // Verified Development Pipeline: verify annotations
        Commands::Verify { input } => {
            run_verify(&input)
        }

        // Verified Development Pipeline: run @test annotations
        Commands::Test { input, fuzz } => {
            run_test_with_fuzz(&input, fuzz)
        }
    }
}

// ---------------------------------------------------------------------------
// G4: Watch — poll file mtime and recompile on change
// ---------------------------------------------------------------------------

fn run_watch(input: &str, output: Option<&str>) -> miette::Result<()> {
    use std::time::{Duration, SystemTime};

    let output_path = output.map(|s| s.to_string()).unwrap_or_else(|| {
        if cfg!(windows) { "a.exe".into() } else { "a.out".into() }
    });

    eprintln!("[AXIOM Watch] Watching {} -> {}", input, output_path);
    eprintln!("[AXIOM Watch] Press Ctrl+C to stop\n");

    let mut last_mtime: Option<SystemTime> = None;

    loop {
        // Check file modification time
        let metadata = match std::fs::metadata(input) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[AXIOM Watch] Error reading {}: {}", input, e);
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };

        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        let changed = match last_mtime {
            Some(prev) => mtime != prev,
            None => true, // First iteration — always compile
        };

        if changed {
            last_mtime = Some(mtime);
            let start = std::time::Instant::now();

            // Attempt compilation
            match try_compile(input, &output_path) {
                Ok(()) => {
                    let elapsed = start.elapsed();
                    eprintln!(
                        "[AXIOM Watch] Compiled OK in {:.3}ms -> {}",
                        elapsed.as_secs_f64() * 1000.0,
                        output_path
                    );
                }
                Err(e) => {
                    let elapsed = start.elapsed();
                    eprintln!(
                        "[AXIOM Watch] Error after {:.3}ms: {}",
                        elapsed.as_secs_f64() * 1000.0,
                        e
                    );
                }
            }
        }

        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Try to compile a single .axm file to a binary. Returns Ok on success.
fn try_compile(input: &str, output_path: &str) -> miette::Result<()> {
    let source = read_source_with_includes(input)?;
    let mut result = axiom_parser::parse(&source);
    if result.has_errors() {
        let msgs: Vec<String> = result.errors.iter().map(|e| format!("{e}")).collect();
        return Err(miette::miette!("parse errors:\n  {}", msgs.join("\n  ")));
    }
    resolve_imports(&mut result.module, input);
    let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        miette::miette!("HIR errors:\n  {}", msgs.join("\n  "))
    })?;
    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        miette::miette!("codegen errors:\n  {}", msgs.join("\n  "))
    })?;

    let constraints = axiom_optimize::llm_optimizer::extract_constraints_from_source(&source);
    let optimize_for = constraints.iter()
        .find(|c| c.key == "optimize_for")
        .map(|c| c.value.clone());
    let compile_opts = compile::CompileOptions {
        target_arch: None,
        optimize_for,
        ir_text: Some(llvm_ir.clone()),
        link_dirs: Vec::new(),
        opt_report: false,
        sanitize: None,
        debug_mode: false,
        record_mode: false,
    };

    compile::compile_to_binary_with_options(&llvm_ir, output_path, &compile_opts)
}

// ---------------------------------------------------------------------------
// E5: Package Manager — build from axiom.toml manifest
// ---------------------------------------------------------------------------

/// Minimal axiom.toml manifest format.
#[derive(Debug)]
struct AxiomManifest {
    package: PackageInfo,
    dependencies: std::collections::HashMap<String, DependencySpec>,
}

#[derive(Debug)]
struct PackageInfo {
    name: String,
    version: String,
}

#[derive(Debug)]
struct DependencySpec {
    path: String,
}

fn run_build(manifest_path: &str) -> miette::Result<()> {
    let manifest_str = std::fs::read_to_string(manifest_path)
        .map_err(|e| miette::miette!("Failed to read {}: {}", manifest_path, e))?;

    // Parse as TOML manually (minimal — we don't want to add a toml crate dep)
    let manifest = parse_axiom_toml(&manifest_str)?;

    eprintln!("[AXIOM Build] Building package: {} v{}", manifest.package.name, manifest.package.version);

    let manifest_dir = Path::new(manifest_path).parent().unwrap_or(Path::new("."));

    // Collect dependency sources
    let mut combined_source = String::new();

    for (dep_name, dep_spec) in &manifest.dependencies {
        let dep_dir = manifest_dir.join(&dep_spec.path);
        eprintln!("[AXIOM Build] Including dependency: {dep_name} ({})", dep_dir.display());

        // Find all .axm files in the dependency directory
        let axm_files = find_axm_files(&dep_dir);
        if axm_files.is_empty() {
            eprintln!("[AXIOM Build] Warning: no .axm files found in {}", dep_dir.display());
        }
        for file in &axm_files {
            let source = std::fs::read_to_string(file)
                .map_err(|e| miette::miette!("Failed to read {}: {}", file.display(), e))?;
            combined_source.push_str(&format!("// --- dependency: {} ({}) ---\n", dep_name, file.display()));
            combined_source.push_str(&source);
            combined_source.push('\n');
        }
    }

    // Find the main source file: look for main.axm or src/main.axm
    let main_candidates = [
        manifest_dir.join("main.axm"),
        manifest_dir.join("src").join("main.axm"),
    ];
    let main_file = main_candidates.iter().find(|p| p.exists());

    match main_file {
        Some(main_path) => {
            let main_source = read_source_with_includes(&main_path.display().to_string())?;
            combined_source.push_str(&format!("// --- main: {} ---\n", main_path.display()));
            combined_source.push_str(&main_source);

            // Compile
            let output_path = if cfg!(windows) {
                format!("{}.exe", manifest.package.name)
            } else {
                manifest.package.name.clone()
            };

            let result = axiom_parser::parse(&combined_source);
            if result.has_errors() {
                for err in &result.errors {
                    eprintln!("  {err}");
                }
                return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
            }
            let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
                for err in &errors { eprintln!("  {err}"); }
                miette::miette!("HIR lowering failed with {} error(s)", errors.len())
            })?;
            let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
                for err in &errors { eprintln!("  {err}"); }
                miette::miette!("codegen failed with {} error(s)", errors.len())
            })?;

            compile::compile_to_binary(&llvm_ir, &output_path)?;
            eprintln!("[AXIOM Build] Built {} -> {}", manifest.package.name, output_path);
            Ok(())
        }
        None => {
            eprintln!("[AXIOM Build] No main.axm found (checked main.axm and src/main.axm)");
            if combined_source.is_empty() {
                return Err(miette::miette!("No source files to build"));
            }
            eprintln!("[AXIOM Build] Dependencies collected ({} bytes of source)", combined_source.len());
            Ok(())
        }
    }
}

/// Minimal TOML parser for axiom.toml. Uses serde_json as intermediate.
/// This avoids adding a toml dependency.
fn parse_axiom_toml(input: &str) -> miette::Result<AxiomManifest> {
    let mut package_name = String::new();
    let mut package_version = String::new();
    let mut deps: std::collections::HashMap<String, DependencySpec> = std::collections::HashMap::new();

    let mut current_section = String::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Section header
        if trimmed.starts_with('[') {
            current_section = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .to_string();
            continue;
        }

        // Key = value
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            let unquoted = value.trim_matches('"');

            match current_section.as_str() {
                "package" => {
                    match key {
                        "name" => package_name = unquoted.to_string(),
                        "version" => package_version = unquoted.to_string(),
                        _ => {}
                    }
                }
                "dependencies" => {
                    // Parse: dep_name = { path = "..." }
                    if let Some(path_start) = value.find("path") {
                        let rest = &value[path_start..];
                        if let Some(eq_pos) = rest.find('=') {
                            let path_val = rest[eq_pos + 1..]
                                .trim()
                                .trim_matches(|c: char| c == '"' || c == '\'' || c == '{' || c == '}' || c == ' ');
                            deps.insert(key.to_string(), DependencySpec {
                                path: path_val.to_string(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if package_name.is_empty() {
        return Err(miette::miette!("axiom.toml missing [package] name"));
    }

    Ok(AxiomManifest {
        package: PackageInfo {
            name: package_name,
            version: if package_version.is_empty() { "0.0.0".to_string() } else { package_version },
        },
        dependencies: deps,
    })
}

/// Find all .axm files in a directory (non-recursive).
fn find_axm_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "axm") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// Profile an AXIOM program: compile, run multiple times, collect timing data,
/// extract optimization surfaces, and suggest tuning parameters.
fn run_profile(input: &str, iterations: usize) -> miette::Result<()> {
    // 1. Read and compile source
    let source = std::fs::read_to_string(input)
        .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

    eprintln!("Profiling {input} ({iterations} iteration(s))...\n");

    // 2. Verify compilation succeeds
    let result = axiom_parser::parse(&source);
    if result.has_errors() {
        eprintln!("--- Parse Errors ---");
        for err in &result.errors {
            eprintln!("  {err}");
        }
        return Err(miette::miette!(
            "parsing failed with {} error(s)",
            result.errors.len()
        ));
    }
    let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("HIR lowering failed with {} error(s)", errors.len())
    })?;
    let _llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("codegen failed with {} error(s)", errors.len())
    })?;

    eprintln!("  Compilation: OK\n");

    // 3. Benchmark: run the program multiple times
    let bench_config = axiom_optimize::benchmark::BenchmarkConfig {
        warmup_runs: 2,
        measurement_runs: iterations,
        ..Default::default()
    };

    eprintln!("  Timing ({iterations} runs):");
    match axiom_optimize::benchmark::benchmark_source(&source, &bench_config) {
        Ok(bench_result) => {
            eprintln!("    min:    {:.3} ms", bench_result.min_ms);
            eprintln!("    max:    {:.3} ms", bench_result.max_ms);
            eprintln!("    mean:   {:.3} ms", bench_result.mean_ms);
            eprintln!("    median: {:.3} ms", bench_result.median_ms);
            eprintln!("    stddev: {:.3} ms", bench_result.stddev_ms);
            println!("{bench_result}");
        }
        Err(e) => {
            eprintln!("    Benchmark failed: {e}");
            eprintln!("    (Timing data unavailable -- compilation-only profile)");
        }
    }

    // 4. Extract optimization surfaces
    eprintln!("\n  Optimization surfaces:");
    match axiom_optimize::extract_surfaces(&source) {
        Ok(surfaces) if surfaces.is_empty() => {
            eprintln!("    (none found -- add @strategy blocks to enable tuning)");
        }
        Ok(surfaces) => {
            for s in &surfaces {
                eprintln!(
                    "    function `{}`: {} hole(s)",
                    s.function_name,
                    s.holes.len()
                );
                for h in &s.holes {
                    let range_str = match h.range {
                        Some((lo, hi)) => format!(" [{lo}..{hi}]"),
                        None => String::new(),
                    };
                    eprintln!("      ?{}: {}{}", h.name, h.hole_type, range_str);
                }
            }
        }
        Err(errs) => {
            eprintln!(
                "    (extraction failed: {})",
                errs.join("; ")
            );
        }
    }

    // 5. Suggestions
    eprintln!("\n  Suggestions:");
    eprintln!("    - Run `axiom optimize {input}` to auto-tune ?params");
    eprintln!("    - Add @strategy blocks to expose tunable parameters");
    eprintln!("    - Use @pure on hot functions to enable LLVM optimizations");

    Ok(())
}

/// AI-driven optimization: compile, benchmark, ask LLM for suggestions,
/// apply, and record results over multiple iterations.
fn run_optimize(
    input: &str,
    iterations: usize,
    target: &str,
    api_key: Option<&str>,
    dry_run: bool,
    agent: &str,
    track: bool,
) -> miette::Result<()> {
    use axiom_optimize::history::{OptHistory, OptRecord};
    use axiom_optimize::llm_optimizer::{self, LlmResult};
    use std::collections::HashMap;

    // Resolve API key: --api-key flag > ANTHROPIC_API_KEY env var > None
    let resolved_api_key: Option<String> = api_key
        .map(|k| k.to_string())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

    // 1. Read source
    let source = std::fs::read_to_string(input)
        .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

    // 2. Extract optimization surfaces
    let surfaces = axiom_optimize::extract_surfaces(&source).map_err(|errs| {
        miette::miette!("Failed to extract surfaces: {}", errs.join("; "))
    })?;

    if surfaces.is_empty() {
        eprintln!("No optimization surfaces found in {input}.");
        eprintln!("Add @strategy blocks with ?param holes to enable LLM-driven tuning.");
        return Ok(());
    }

    eprintln!("=== AXIOM LLM-Driven Self-Optimization ===\n");
    eprintln!("Source: {input}");
    eprintln!("Target: {target}");
    eprintln!("Iterations: {iterations}");
    eprintln!(
        "LLM: {}",
        if dry_run {
            "dry-run (prompt only)"
        } else if resolved_api_key.is_some() {
            "Claude API (via curl)"
        } else {
            "auto-detect (claude CLI or dry-run)"
        }
    );
    eprintln!();

    eprintln!("Found {} optimization surface(s):", surfaces.len());
    for s in &surfaces {
        eprintln!(
            "  function `{}`: {} hole(s)",
            s.function_name,
            s.holes.len()
        );
        for h in &s.holes {
            let range_str = match h.range {
                Some((lo, hi)) => format!(" [{lo}..{hi}]"),
                None => String::new(),
            };
            eprintln!("    ?{}: {}{}", h.name, h.hole_type, range_str);
        }
    }

    // 3. Compile to LLVM IR
    eprintln!("\nCompiling to LLVM IR...");
    let parse_result = axiom_parser::parse(&source);
    if parse_result.has_errors() {
        return Err(miette::miette!(
            "parse errors: {}",
            parse_result
                .errors
                .iter()
                .map(|e| format!("{e}"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let hir_module = axiom_hir::lower(&parse_result.module).map_err(|errors| {
        miette::miette!(
            "HIR lowering errors: {}",
            errors
                .iter()
                .map(|e| format!("{e}"))
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        miette::miette!(
            "codegen errors: {}",
            errors
                .iter()
                .map(|e| format!("{e}"))
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    eprintln!("  LLVM IR: {} bytes", llvm_ir.len());

    // 4. Generate assembly (optional, best-effort)
    eprintln!("Generating assembly...");
    let assembly = llm_optimizer::generate_assembly(&llvm_ir, target);
    match &assembly {
        Some(asm) => eprintln!("  Assembly: {} bytes", asm.len()),
        None => eprintln!("  Assembly: not available (clang not found)"),
    }

    // 5. Benchmark baseline
    eprintln!("\nBenchmarking baseline...");
    let bench_config = axiom_optimize::benchmark::BenchmarkConfig {
        warmup_runs: 2,
        measurement_runs: 5,
        timeout_ms: 30_000,
    };

    let baseline_ms = match axiom_optimize::benchmark::benchmark_source(&source, &bench_config) {
        Ok(result) => {
            eprintln!(
                "  Baseline: median={:.3}ms, mean={:.3}ms, stddev={:.3}ms",
                result.median_ms, result.mean_ms, result.stddev_ms
            );
            Some(result.median_ms)
        }
        Err(e) => {
            eprintln!("  Benchmark not available: {e}");
            None
        }
    };

    // 6. Load existing history (if any)
    let history_path = format!("{input}.opt_history.json");
    let mut history = match std::fs::read_to_string(&history_path) {
        Ok(json) => OptHistory::from_json(&json)
            .map_err(|e| miette::miette!("Failed to parse history {}: {}", history_path, e))?,
        Err(_) => OptHistory::new(),
    };

    // Record baseline if this is a fresh history
    if history.records.is_empty() {
        if let Some(ms) = baseline_ms {
            let mut metrics = HashMap::new();
            metrics.insert("time_ms".to_string(), ms);
            history.add_record(OptRecord {
                version: history.next_version(),
                params: HashMap::new(),
                metrics,
                agent: Some(agent.to_string()),
                target: Some(target.to_string()),
                timestamp: get_timestamp(),
            });
            eprintln!("  Recorded baseline as {}", history.records.last().unwrap().version);
        }
    }

    // 7. Run LLM optimization iterations
    eprintln!(
        "\n=== Starting LLM optimization ({iterations} iteration(s)) ===\n"
    );

    let current_benchmark = baseline_ms;

    for i in 0..iterations {
        let iter_num = i + 1;
        eprintln!("--- Iteration {iter_num}/{iterations} ---\n");

        // Call LLM
        let result = llm_optimizer::run_llm_optimization(
            &source,
            &llvm_ir,
            assembly.as_deref(),
            current_benchmark,
            &surfaces,
            &history.records,
            iter_num,
            iterations,
            target,
            resolved_api_key.as_deref(),
            dry_run,
        );

        match result {
            LlmResult::Suggestion(suggestion) => {
                eprintln!("  LLM Suggestion (confidence: {:.0}%):", suggestion.confidence * 100.0);
                eprintln!("  Reasoning: {}", suggestion.reasoning);
                eprintln!("  Proposed parameters:");
                for (name, value) in &suggestion.param_values {
                    eprintln!("    ?{name} = {value}");
                }

                if !suggestion.code_changes.is_empty() {
                    eprintln!("  Code change suggestions:");
                    for change in &suggestion.code_changes {
                        let line_str = change
                            .line
                            .map(|l| format!(" (line {l})"))
                            .unwrap_or_default();
                        eprintln!("    - {}{}", change.description, line_str);
                    }
                }

                // Record the LLM suggestion in history
                let version = history.next_version();
                let params: HashMap<String, serde_json::Value> = suggestion
                    .param_values
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                let mut metrics: HashMap<String, f64> = HashMap::new();
                metrics.insert("llm_confidence".to_string(), suggestion.confidence);

                // Benchmark with the suggestion applied
                // (In a full implementation, we would apply the params to the source
                //  and recompile. For now, we record the benchmark of the current source.)
                if let Some(ms) = current_benchmark {
                    metrics.insert("time_ms".to_string(), ms);
                }

                history.add_record(OptRecord {
                    version: version.clone(),
                    params,
                    metrics,
                    agent: Some(agent.to_string()),
                    target: Some(target.to_string()),
                    timestamp: get_timestamp(),
                });

                eprintln!("  Recorded as {version}\n");
                if track {
                    let _ = git_track_optimization(input, &format!("axiom optimize: iteration {iter_num} — {version}"));
                }
            }

            LlmResult::DryRun { prompt_path, prompt } => {
                eprintln!("  [DRY RUN] Prompt written to: {prompt_path}");
                eprintln!("  Prompt length: {} chars\n", prompt.len());

                // In dry-run mode on the first iteration, print the full prompt
                if i == 0 {
                    println!("{prompt}");
                }

                // Still record an iteration so history advances
                let version = history.next_version();
                let mut metrics = HashMap::new();
                if let Some(ms) = current_benchmark {
                    metrics.insert("time_ms".to_string(), ms);
                }

                // Use grid-search fallback for params
                let mut params: HashMap<String, serde_json::Value> = HashMap::new();
                for surface in &surfaces {
                    for hole in &surface.holes {
                        let value = generate_hole_value(hole, i);
                        let json_value = value_to_json(&value);
                        params.insert(hole.name.clone(), json_value);
                    }
                }

                history.add_record(OptRecord {
                    version: version.clone(),
                    params,
                    metrics,
                    agent: Some(format!("{agent}-grid-search")),
                    target: Some(target.to_string()),
                    timestamp: get_timestamp(),
                });

                eprintln!("  Recorded grid-search fallback as {version}\n");
                if track {
                    let _ = git_track_optimization(input, &format!("axiom optimize: dry-run iteration {iter_num} — {version}"));
                }
            }

            LlmResult::Error(err) => {
                eprintln!("  LLM error: {err}");
                eprintln!("  Falling back to grid-search for this iteration.\n");

                // Grid-search fallback
                let version = history.next_version();
                let mut params: HashMap<String, serde_json::Value> = HashMap::new();
                let mut proposal = axiom_optimize::Proposal::new();

                for surface in &surfaces {
                    for hole in &surface.holes {
                        let value = generate_hole_value(hole, i);
                        let json_value = value_to_json(&value);
                        params.insert(hole.name.clone(), json_value);
                        proposal.set(hole.name.clone(), value);
                    }
                }

                let mut metrics: HashMap<String, f64> = HashMap::new();
                if let Some(ms) = current_benchmark {
                    metrics.insert("time_ms".to_string(), ms);
                }

                history.add_record(OptRecord {
                    version: version.clone(),
                    params,
                    metrics,
                    agent: Some(format!("{agent}-grid-search")),
                    target: Some(target.to_string()),
                    timestamp: get_timestamp(),
                });

                eprintln!("  Recorded grid-search fallback as {version}\n");
                if track {
                    let _ = git_track_optimization(input, &format!("axiom optimize: error fallback iteration {iter_num} — {version}"));
                }
            }
        }
    }

    // 8. Print summary
    eprintln!("=== Optimization Summary ===");
    eprintln!("Total records: {}", history.records.len());

    if let Some(best) = history.best_by_metric("time_ms") {
        eprintln!(
            "Best result: {} (time_ms={:.3}ms)",
            best.version,
            best.metrics.get("time_ms").copied().unwrap_or(f64::NAN)
        );
        if !best.params.is_empty() {
            eprintln!("  Parameters:");
            for (name, value) in &best.params {
                eprintln!("    ?{name} = {value}");
            }
        }
    }

    if let Some(baseline) = baseline_ms {
        if let Some(best) = history.best_by_metric("time_ms") {
            let best_ms = best.metrics.get("time_ms").copied().unwrap_or(baseline);
            let improvement = ((baseline - best_ms) / baseline) * 100.0;
            if improvement > 0.0 {
                eprintln!("  Improvement: {:.1}% faster than baseline", improvement);
            }
        }
    }

    // 9. Save history
    let json = history
        .to_json()
        .map_err(|e| miette::miette!("Failed to serialize history: {e}"))?;
    std::fs::write(&history_path, &json)
        .map_err(|e| miette::miette!("Failed to write history to {}: {}", history_path, e))?;

    eprintln!("\nHistory saved to {history_path}");
    // Print JSON to stdout for piping
    println!("{json}");

    Ok(())
}

/// Generate a concrete value for a hole based on its type, range, and iteration.
fn generate_hole_value(
    hole: &axiom_optimize::OptHole,
    iteration: usize,
) -> axiom_optimize::Value {
    use axiom_optimize::{HoleType, Value};

    match &hole.hole_type {
        HoleType::U32 | HoleType::I32 => {
            if let Some((lo, hi)) = hole.range {
                // Grid-search: spread across the range
                let range_size = (hi - lo + 1) as usize;
                let offset = if range_size > 0 {
                    (iteration % range_size) as i64
                } else {
                    0
                };
                // Start from midpoint and move outward
                let mid = (lo + hi) / 2;
                let sign = if iteration.is_multiple_of(2) { 1_i64 } else { -1_i64 };
                let step = iteration.div_ceil(2) as i64;
                let candidate = mid + sign * step;
                // Clamp to range
                let value = candidate.clamp(lo, hi);
                // If we've wrapped around, use offset from lo
                if iteration > (range_size / 2) {
                    Value::Int(lo + offset)
                } else {
                    Value::Int(value)
                }
            } else {
                // No range — use powers of 2 starting from 1
                let exp = iteration.min(10);
                Value::Int(1 << exp)
            }
        }
        HoleType::F64 => {
            Value::Float(1.0 + iteration as f64 * 0.5)
        }
        HoleType::Bool => {
            Value::Bool(iteration.is_multiple_of(2))
        }
        HoleType::Ident => {
            Value::Ident("i".to_string())
        }
        HoleType::Array(inner) => match inner.as_ref() {
            HoleType::Ident => {
                // Common case: loop order — rotate a standard set
                let dims = ["i", "j", "k"];
                let rotated: Vec<Value> = (0..dims.len())
                    .map(|idx| {
                        let shifted = (idx + iteration) % dims.len();
                        Value::Ident(dims[shifted].to_string())
                    })
                    .collect();
                Value::Array(rotated)
            }
            _ => Value::Array(vec![]),
        },
    }
}

/// Convert a surface::Value to a serde_json::Value for storage.
fn value_to_json(value: &axiom_optimize::Value) -> serde_json::Value {
    use axiom_optimize::Value;

    match value {
        Value::Int(v) => serde_json::Value::Number((*v).into()),
        Value::Float(v) => serde_json::json!(*v),
        Value::Bool(v) => serde_json::Value::Bool(*v),
        Value::Ident(s) => serde_json::Value::String(s.clone()),
        Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
    }
}

/// Get a simple ISO-8601-like timestamp without pulling in `chrono`.
fn get_timestamp() -> String {
    // Use system time to produce a Unix epoch seconds timestamp.
    // For a simple CLI tool this is sufficient.
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("{}s", d.as_secs()),
        Err(_) => "unknown".to_string(),
    }
}

// ---------------------------------------------------------------------------
// S3: Source-to-Source AI Rewrite
// ---------------------------------------------------------------------------

fn run_rewrite(input: &str, dry_run: bool, api_key: Option<&str>, output: Option<&str>, analyze: bool) -> miette::Result<()> {
    use axiom_optimize::llm_optimizer::{self, LlmResult};

    // Resolve API key
    let resolved_api_key: Option<String> = api_key
        .map(|k| k.to_string())
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok());

    // Read source
    let source = read_source_with_includes(input)?;

    eprintln!("=== AXIOM Source-to-Source Rewrite ===\n");
    eprintln!("Source: {input} ({} bytes)", source.len());
    eprintln!(
        "Mode: {}",
        if analyze {
            "analyze (LLVM optimization remarks)"
        } else if dry_run {
            "dry-run (prompt only)"
        } else if resolved_api_key.is_some() {
            "Claude API"
        } else {
            "auto-detect (claude CLI or dry-run)"
        }
    );
    eprintln!();

    // Verify source parses correctly
    let mut result = axiom_parser::parse(&source);
    if result.has_errors() {
        eprintln!("Warning: source has parse errors:");
        for err in &result.errors {
            eprintln!("  {err}");
        }
        eprintln!();
    }

    // Extract constraints for reporting
    let constraints = llm_optimizer::extract_constraints_from_source(&source);
    if !constraints.is_empty() {
        eprintln!("Detected constraints:");
        for c in &constraints {
            eprintln!("  {}: {}", c.key, c.value);
        }
        eprintln!();
    }

    // -----------------------------------------------------------------------
    // CompilerGPT: extract LLVM optimization remarks
    // -----------------------------------------------------------------------
    let missed_opts = if analyze || !dry_run {
        // Compile the source to a temp binary with --opt-report to collect
        // LLVM optimization remarks.
        match compile_for_opt_remarks(input, &mut result, &constraints) {
            Ok(missed) => missed,
            Err(e) => {
                eprintln!("[REWRITE] Could not collect optimization remarks: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // --analyze mode: just print remarks and suggestions, no LLM needed
    if analyze {
        if missed_opts.is_empty() {
            eprintln!("[REWRITE] No missed optimizations found -- LLVM applied all optimizations.");
        } else {
            eprintln!("[REWRITE] {} missed optimizations found:", missed_opts.len());
            for m in &missed_opts {
                eprintln!("  {m}");
            }

            let suggestions = llm_optimizer::suggest_actions_for_missed(&missed_opts);
            if !suggestions.is_empty() {
                eprintln!("\nSuggested actions:");
                for s in &suggestions {
                    eprintln!("  -> {s}");
                }
            }
        }
        return Ok(());
    }

    // Run rewrite (with remarks if we collected any)
    let rewrite_result = llm_optimizer::run_rewrite_with_remarks(
        &source,
        resolved_api_key.as_deref(),
        dry_run,
        &missed_opts,
    );

    match rewrite_result {
        LlmResult::Suggestion(suggestion) => {
            eprintln!("Rewrite suggestion (confidence: {:.0}%):\n", suggestion.confidence * 100.0);

            if !suggestion.code_changes.is_empty() {
                eprintln!("Changes applied:");
                for change in &suggestion.code_changes {
                    let line_str = change
                        .line
                        .map(|l| format!(" (line {l})"))
                        .unwrap_or_default();
                    eprintln!("  - {}{}", change.description, line_str);
                }
                eprintln!();
            }

            // The rewritten source is stored in the reasoning field
            let rewritten = &suggestion.reasoning;

            match output {
                Some(out_path) => {
                    std::fs::write(out_path, rewritten)
                        .map_err(|e| miette::miette!("Failed to write {}: {}", out_path, e))?;
                    eprintln!("Rewritten source written to: {out_path}");
                }
                None => {
                    println!("{rewritten}");
                }
            }
        }

        LlmResult::DryRun { prompt_path, prompt } => {
            eprintln!("[DRY RUN] Prompt written to: {prompt_path}");
            eprintln!("Prompt length: {} chars\n", prompt.len());
            println!("{prompt}");
        }

        LlmResult::Error(err) => {
            return Err(miette::miette!("Rewrite failed: {err}"));
        }
    }

    Ok(())
}

/// Compile an AXIOM source file with `-fsave-optimization-record` and extract
/// missed optimization remarks from the resulting `.opt.yaml` file.
fn compile_for_opt_remarks(
    input: &str,
    parse_result: &mut axiom_parser::ParseResult,
    constraints: &[axiom_optimize::llm_optimizer::ConstraintInfo],
) -> miette::Result<Vec<String>> {
    use axiom_optimize::llm_optimizer;

    // Need a successful parse to generate IR
    if parse_result.has_errors() {
        return Err(miette::miette!("Cannot compile for remarks: source has parse errors"));
    }

    resolve_imports(&mut parse_result.module, input);
    let hir_module = axiom_hir::lower(&parse_result.module).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        miette::miette!("HIR errors:\n  {}", msgs.join("\n  "))
    })?;
    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        let msgs: Vec<String> = errors.iter().map(|e| format!("{e}")).collect();
        miette::miette!("codegen errors:\n  {}", msgs.join("\n  "))
    })?;

    let optimize_for = constraints.iter()
        .find(|c| c.key == "optimize_for")
        .map(|c| c.value.clone());

    let tmp_dir = std::env::temp_dir();
    let tmp_bin = if cfg!(windows) {
        tmp_dir.join("axiom_analyze.exe")
    } else {
        tmp_dir.join("axiom_analyze")
    };

    let compile_opts = compile::CompileOptions {
        target_arch: None,
        optimize_for,
        ir_text: Some(llvm_ir.clone()),
        link_dirs: Vec::new(),
        opt_report: true,
        sanitize: None,
        debug_mode: false,
        record_mode: false,
    };

    eprintln!("[REWRITE] Compiling with optimization remarks...");
    compile::compile_to_binary_with_options(&llvm_ir, tmp_bin.to_str().unwrap(), &compile_opts)?;

    // The .opt.yaml file is placed next to the output binary.
    // clang names it based on the input .ll file, so we need to search for it.
    let yaml_path = format!("{}.opt.yaml", tmp_bin.display());

    // Also check for the yaml file based on the .ll temp file name
    // (clang sometimes names it after the input file, not the output).
    let missed = if std::path::Path::new(&yaml_path).exists() {
        llm_optimizer::extract_missed_optimizations(&yaml_path)
    } else {
        // Try to find .opt.yaml files in the temp directory
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".opt.yaml") && name.contains("axiom") {
                        found = llm_optimizer::extract_missed_optimizations(
                            path.to_str().unwrap_or_default(),
                        );
                        // Clean up the found yaml file
                        std::fs::remove_file(&path).ok();
                        break;
                    }
                }
            }
        }
        found
    };

    // Cleanup
    std::fs::remove_file(&tmp_bin).ok();
    std::fs::remove_file(&yaml_path).ok();

    Ok(missed)
}

// ---------------------------------------------------------------------------
// Verified Development Pipeline: axiom verify
// ---------------------------------------------------------------------------

fn run_verify(input: &str) -> miette::Result<()> {
    let source = read_source_with_includes(input)?;

    let parse_result = axiom_parser::parse(&source);
    if parse_result.has_errors() {
        eprintln!("--- Parse Errors ---");
        for err in &parse_result.errors {
            eprintln!("  {err}");
        }
        return Err(miette::miette!(
            "parsing failed with {} error(s)",
            parse_result.errors.len()
        ));
    }

    let hir = axiom_hir::lower(&parse_result.module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("HIR lowering failed with {} error(s)", errors.len())
    })?;

    // Count functions and check annotations
    let mut total = 0;
    let mut with_intent = 0;
    let mut with_contract = 0;
    for func in &hir.functions {
        if func.name == "main" {
            continue;
        }
        total += 1;
        if func.annotations.iter().any(|a| {
            matches!(a.kind, axiom_hir::HirAnnotationKind::Intent(_))
        }) {
            with_intent += 1;
        }
        if func.annotations.iter().any(|a| {
            matches!(
                a.kind,
                axiom_hir::HirAnnotationKind::Precondition(_)
                    | axiom_hir::HirAnnotationKind::Postcondition(_)
                    | axiom_hir::HirAnnotationKind::Requires(_)
                    | axiom_hir::HirAnnotationKind::Ensures(_)
            )
        }) {
            with_contract += 1;
        }
    }

    eprintln!("[VERIFY] {input}: {total} functions checked");
    eprintln!("[VERIFY]   {with_intent}/{total} have @intent");
    eprintln!("[VERIFY]   {with_contract}/{total} have contracts");
    if total == 0 {
        eprintln!("[VERIFY] PASS — no non-main functions to check");
    } else if with_intent == total && with_contract == total {
        eprintln!("[VERIFY] PASS — all annotations present");
    } else {
        eprintln!(
            "[VERIFY] WARN — {}/{} functions fully annotated",
            std::cmp::min(with_intent, with_contract),
            total
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Verified Development Pipeline: axiom build --input (verified pipeline)
// ---------------------------------------------------------------------------

/// Run the verified build pipeline: verify -> test -> compile.
fn run_verified_build(input: &str) -> miette::Result<()> {
    eprintln!("[BUILD] Phase 1: Verifying annotations...");
    run_verify(input)?;

    eprintln!();
    eprintln!("[BUILD] Phase 2: Running tests...");
    // Test phase is best-effort: if no @test annotations, we skip gracefully.
    // run_test returns Ok(()) when there are no tests.
    run_test(input)?;

    eprintln!();
    eprintln!("[BUILD] Phase 3: Compiling (release)...");
    let source = read_source_with_includes(input)?;
    let parse_result = axiom_parser::parse(&source);
    if parse_result.has_errors() {
        for err in &parse_result.errors {
            eprintln!("  {err}");
        }
        return Err(miette::miette!("parsing failed"));
    }
    let hir_module = axiom_hir::lower(&parse_result.module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("HIR lowering failed")
    })?;
    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("codegen failed")
    })?;

    let output_path = if cfg!(windows) {
        input.replace(".axm", ".exe")
    } else {
        input.replace(".axm", "")
    };

    let constraints = axiom_optimize::llm_optimizer::extract_constraints_from_source(&source);
    let optimize_for = constraints
        .iter()
        .find(|c| c.key == "optimize_for")
        .map(|c| c.value.clone());
    let compile_opts = compile::CompileOptions {
        target_arch: None,
        optimize_for,
        ir_text: Some(llvm_ir.clone()),
        link_dirs: Vec::new(),
        opt_report: false,
        sanitize: None,
        debug_mode: false,
        record_mode: false,
    };

    compile::compile_to_binary_with_options(&llvm_ir, &output_path, &compile_opts)?;
    eprintln!("[BUILD] Compiled {} -> {}", input, output_path);
    eprintln!("[BUILD] DONE");

    Ok(())
}

// ---------------------------------------------------------------------------
// Verified Development Pipeline: axiom test
// ---------------------------------------------------------------------------

/// Convert a HIR expression back to AXIOM source text for the test harness.
fn format_hir_expr_as_axiom(expr: &axiom_hir::HirExpr) -> String {
    // HirExpr implements Display, which produces valid AXIOM source text.
    format!("{expr}")
}

/// Strip the `fn main() { ... }` function from source, counting brace nesting.
///
/// Returns the source with the main function removed. If no main function is
/// found, returns the source unchanged.
fn strip_main_function(source: &str) -> String {
    // Find the start of "fn main()" — skip any leading whitespace on the line
    let mut result = String::with_capacity(source.len());
    let chars = source.char_indices().peekable();
    let mut in_main = false;
    let mut brace_depth: i32 = 0;
    let mut main_start: Option<usize> = None;
    let mut main_end: Option<usize> = None;

    // Simple state machine: find "fn main()" then count braces to find the end.
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if !in_main {
            // Look for "fn main("
            if i + 8 <= len && &source[i..i + 8] == "fn main(" {
                // Check that this is preceded by start-of-line or whitespace
                let preceded_ok = i == 0 || bytes[i - 1] == b'\n' || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t' || bytes[i - 1] == b'\r';
                if preceded_ok {
                    // Find the line start (for removing any annotations on the same line)
                    let mut line_start = i;
                    while line_start > 0 && bytes[line_start - 1] != b'\n' {
                        line_start -= 1;
                    }
                    main_start = Some(line_start);
                    // Skip ahead to the opening brace
                    while i < len && bytes[i] != b'{' {
                        i += 1;
                    }
                    if i < len {
                        in_main = true;
                        brace_depth = 1;
                        i += 1;
                        continue;
                    }
                }
            }
            i += 1;
        } else {
            // Inside main — count braces
            if bytes[i] == b'{' {
                brace_depth += 1;
            } else if bytes[i] == b'}' {
                brace_depth -= 1;
                if brace_depth == 0 {
                    // Skip past trailing newline if present
                    let mut end = i + 1;
                    if end < len && bytes[end] == b'\n' {
                        end += 1;
                    } else if end + 1 < len && bytes[end] == b'\r' && bytes[end + 1] == b'\n' {
                        end += 2;
                    }
                    main_end = Some(end);
                    in_main = false;
                    i = end;
                    continue;
                }
            }
            i += 1;
        }
    }

    // Rebuild source without the main function region
    match (main_start, main_end) {
        (Some(start), Some(end)) => {
            result.push_str(&source[..start]);
            result.push_str(&source[end..]);
        }
        _ => {
            // No main found — return source unchanged
            result.push_str(source);
        }
    }

    // Drop the unused peekable iterator
    drop(chars);

    result
}

/// Run `axiom test` with optional `--fuzz` flag.
///
/// When `--fuzz` is enabled, functions with `@precondition` annotations get
/// auto-generated test inputs derived from the constraint ranges.
fn run_test_with_fuzz(input: &str, fuzz: bool) -> miette::Result<()> {
    if !fuzz {
        return run_test(input);
    }

    let source = read_source_with_includes(input)?;

    let parse_result = axiom_parser::parse(&source);
    if parse_result.has_errors() {
        eprintln!("--- Parse Errors ---");
        for err in &parse_result.errors {
            eprintln!("  {err}");
        }
        return Err(miette::miette!(
            "parsing failed with {} error(s)",
            parse_result.errors.len()
        ));
    }

    let hir = axiom_hir::lower(&parse_result.module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("HIR lowering failed with {} error(s)", errors.len())
    })?;

    // Collect existing @test cases plus fuzz-generated inputs from @precondition.
    let mut test_cases: Vec<(String, Vec<axiom_hir::HirTestCase>)> = Vec::new();
    let mut fuzz_test_count = 0usize;

    for func in &hir.functions {
        // Collect existing @test annotations.
        let mut tests: Vec<axiom_hir::HirTestCase> = func
            .annotations
            .iter()
            .filter_map(|a| {
                if let axiom_hir::HirAnnotationKind::Test(tc) = &a.kind {
                    Some(tc.clone())
                } else {
                    None
                }
            })
            .collect();

        // Extract @precondition annotations for fuzz input generation.
        let preconditions: Vec<axiom_hir::HirExpr> = func
            .annotations
            .iter()
            .filter_map(|a| match &a.kind {
                axiom_hir::HirAnnotationKind::Precondition(expr) => Some((**expr).clone()),
                axiom_hir::HirAnnotationKind::Requires(expr) => Some((**expr).clone()),
                _ => None,
            })
            .collect();

        if !preconditions.is_empty() {
            let ranges = axiom_optimize::fuzz::extract_fuzz_ranges(&preconditions, &func.params);
            let fuzz_inputs = axiom_optimize::fuzz::generate_fuzz_inputs(&ranges, 20);

            eprintln!(
                "[FUZZ] {} @precondition(s) on '{}' -> {} ranges -> {} generated inputs",
                preconditions.len(),
                func.name,
                ranges.len(),
                fuzz_inputs.len()
            );

            for fuzz_input in &fuzz_inputs {
                fuzz_test_count += 1;
                let inputs: Vec<axiom_hir::HirExpr> = fuzz_input
                    .iter()
                    .map(|&v| axiom_hir::HirExpr {
                        id: axiom_hir::NodeId(0),
                        kind: axiom_hir::HirExprKind::IntLiteral { value: v as i128 },
                        span: axiom_hir::SPAN_DUMMY,
                    })
                    .collect();

                tests.push(axiom_hir::HirTestCase {
                    inputs,
                    // Sentinel: fuzz tests use __fuzz_any__ to indicate
                    // "just check it doesn't crash".
                    expected: axiom_hir::HirExpr {
                        id: axiom_hir::NodeId(0),
                        kind: axiom_hir::HirExprKind::Ident {
                            name: "__fuzz_any__".to_string(),
                        },
                        span: axiom_hir::SPAN_DUMMY,
                    },
                });
            }
        }

        if !tests.is_empty() {
            test_cases.push((func.name.clone(), tests));
        }
    }

    if test_cases.is_empty() {
        eprintln!("[TEST] No @test or @precondition annotations found in {input}");
        return Ok(());
    }

    let total_tests: usize = test_cases.iter().map(|(_, ts)| ts.len()).sum();
    eprintln!(
        "[TEST] Found {total_tests} test(s) across {} function(s) ({fuzz_test_count} fuzz-generated)",
        test_cases.len()
    );

    // Build the test harness main function.
    let mut test_main = String::new();
    test_main.push_str("fn main() -> i32 {\n");
    test_main.push_str("    let passed: i32 = 0;\n");
    test_main.push_str("    let failed: i32 = 0;\n");

    for (func_name, tests) in &test_cases {
        for (i, tc) in tests.iter().enumerate() {
            let is_fuzz = matches!(
                &tc.expected.kind,
                axiom_hir::HirExprKind::Ident { name } if name == "__fuzz_any__"
            );

            let args: Vec<String> = tc
                .inputs
                .iter()
                .map(|e| format_hir_expr_as_axiom(e))
                .collect();
            let args_str = args.join(", ");

            let var = format!("t{}_{}", i, func_name);

            if is_fuzz {
                // Fuzz test: just call the function to check it doesn't crash.
                test_main.push_str(&format!(
                    "    let {var}: i32 = {func_name}({args_str});\n"
                ));
                test_main.push_str(&format!(
                    "    print(\"[FUZZ] {func_name}({args_str}): OK\\n\");\n"
                ));
                test_main.push_str("    passed = passed + 1;\n");
            } else {
                let expected_str = format_hir_expr_as_axiom(&tc.expected);
                test_main.push_str(&format!(
                    "    let {var}: i32 = {func_name}({args_str});\n"
                ));
                test_main.push_str(&format!(
                    "    if {var} == {expected_str} {{\n"
                ));
                test_main.push_str(&format!(
                    "        print(\"[TEST] {func_name}({args_str}) == {expected_str}: PASS\\n\");\n"
                ));
                test_main.push_str("        passed = passed + 1;\n");
                test_main.push_str("    } else {\n");
                test_main.push_str(&format!(
                    "        print(\"[TEST] {func_name}({args_str}) == {expected_str}: FAIL\\n\");\n"
                ));
                test_main.push_str("        failed = failed + 1;\n");
                test_main.push_str("    }\n");
            }
        }
    }

    test_main.push_str("    print_i32(passed);\n");
    test_main.push_str("    print(\" passed, \");\n");
    test_main.push_str("    print_i32(failed);\n");
    test_main.push_str("    print(\" failed\\n\");\n");
    test_main.push_str("    return failed;\n");
    test_main.push_str("}\n");

    let stripped = strip_main_function(&source);
    let test_source = format!("{stripped}\n{test_main}");

    let test_path = std::env::temp_dir().join("axiom_fuzz_test_harness.axm");
    std::fs::write(&test_path, &test_source)
        .map_err(|e| miette::miette!("Failed to write fuzz test harness: {e}"))?;

    let test_exe = std::env::temp_dir().join(if cfg!(windows) {
        "axiom_fuzz_test_harness.exe"
    } else {
        "axiom_fuzz_test_harness"
    });

    let test_parse = axiom_parser::parse(&test_source);
    if test_parse.has_errors() {
        eprintln!("--- Fuzz Test Harness Parse Errors ---");
        for err in &test_parse.errors {
            eprintln!("  {err}");
        }
        eprintln!("\n--- Generated source ---");
        for (i, line) in test_source.lines().enumerate() {
            eprintln!("{:4} | {}", i + 1, line);
        }
        return Err(miette::miette!("fuzz test harness parsing failed"));
    }

    let test_hir = axiom_hir::lower(&test_parse.module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("fuzz test harness HIR lowering failed")
    })?;

    let codegen_opts = axiom_codegen::CodegenOptions {
        debug_mode: true,
        ..Default::default()
    };
    let test_ir = axiom_codegen::codegen_with_options(&test_hir, &codegen_opts).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("fuzz test harness codegen failed")
    })?;

    compile::compile_to_binary(&test_ir, &test_exe.display().to_string())?;

    let output = std::process::Command::new(&test_exe)
        .output()
        .map_err(|e| miette::miette!("Failed to run fuzz test binary: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.is_empty() {
        print!("{stdout}");
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }

    let _ = std::fs::remove_file(&test_path);
    let _ = std::fs::remove_file(&test_exe);

    if output.status.success() {
        Ok(())
    } else {
        let code = output.status.code().unwrap_or(1);
        Err(miette::miette!("fuzz tests failed (exit code {})", code))
    }
}

/// Run `axiom test` — collect @test annotations, generate a test harness,
/// compile and execute it.
fn run_test(input: &str) -> miette::Result<()> {
    let source = read_source_with_includes(input)?;

    let parse_result = axiom_parser::parse(&source);
    if parse_result.has_errors() {
        eprintln!("--- Parse Errors ---");
        for err in &parse_result.errors {
            eprintln!("  {err}");
        }
        return Err(miette::miette!(
            "parsing failed with {} error(s)",
            parse_result.errors.len()
        ));
    }

    let hir = axiom_hir::lower(&parse_result.module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("HIR lowering failed with {} error(s)", errors.len())
    })?;

    // Collect test cases: (function_name, Vec<HirTestCase>)
    let mut test_cases: Vec<(String, Vec<axiom_hir::HirTestCase>)> = Vec::new();
    for func in &hir.functions {
        let tests: Vec<axiom_hir::HirTestCase> = func
            .annotations
            .iter()
            .filter_map(|a| {
                if let axiom_hir::HirAnnotationKind::Test(tc) = &a.kind {
                    Some(tc.clone())
                } else {
                    None
                }
            })
            .collect();
        if !tests.is_empty() {
            test_cases.push((func.name.clone(), tests));
        }
    }

    if test_cases.is_empty() {
        eprintln!("[TEST] No @test annotations found in {input}");
        return Ok(());
    }

    let total_tests: usize = test_cases.iter().map(|(_, ts)| ts.len()).sum();
    eprintln!("[TEST] Found {total_tests} test(s) across {} function(s)", test_cases.len());

    // Build the test harness main function
    let mut test_main = String::new();
    test_main.push_str("fn main() -> i32 {\n");
    test_main.push_str("    let passed: i32 = 0;\n");
    test_main.push_str("    let failed: i32 = 0;\n");

    for (func_name, tests) in &test_cases {
        for (i, tc) in tests.iter().enumerate() {
            let args: Vec<String> = tc
                .inputs
                .iter()
                .map(|e| format_hir_expr_as_axiom(e))
                .collect();
            let args_str = args.join(", ");
            let expected_str = format_hir_expr_as_axiom(&tc.expected);

            // Use a unique variable name for each test result
            let var = format!("t{}_{}", i, func_name);
            test_main.push_str(&format!(
                "    let {var}: i32 = {func_name}({args_str});\n"
            ));
            test_main.push_str(&format!(
                "    if {var} == {expected_str} {{\n"
            ));
            test_main.push_str(&format!(
                "        print(\"[TEST] {func_name}({args_str}) == {expected_str}: PASS\\n\");\n"
            ));
            test_main.push_str("        passed = passed + 1;\n");
            test_main.push_str("    } else {\n");
            test_main.push_str(&format!(
                "        print(\"[TEST] {func_name}({args_str}) == {expected_str}: FAIL\\n\");\n"
            ));
            test_main.push_str("        failed = failed + 1;\n");
            test_main.push_str("    }\n");
        }
    }

    test_main.push_str("    print_i32(passed);\n");
    test_main.push_str("    print(\" passed, \");\n");
    test_main.push_str("    print_i32(failed);\n");
    test_main.push_str("    print(\" failed\\n\");\n");
    test_main.push_str("    return failed;\n");
    test_main.push_str("}\n");

    // Strip the original main function and append the test harness
    let stripped = strip_main_function(&source);
    let test_source = format!("{stripped}\n{test_main}");

    // Write to temp file
    let test_path = std::env::temp_dir().join("axiom_test_harness.axm");
    std::fs::write(&test_path, &test_source)
        .map_err(|e| miette::miette!("Failed to write test harness: {e}"))?;

    // Compile the test harness
    let test_exe = std::env::temp_dir().join(if cfg!(windows) {
        "axiom_test_harness.exe"
    } else {
        "axiom_test_harness"
    });

    // Parse and compile the generated test source
    let test_parse = axiom_parser::parse(&test_source);
    if test_parse.has_errors() {
        eprintln!("--- Test Harness Parse Errors ---");
        for err in &test_parse.errors {
            eprintln!("  {err}");
        }
        // Print the generated source for debugging
        eprintln!("\n--- Generated test source ---");
        for (i, line) in test_source.lines().enumerate() {
            eprintln!("{:4} | {}", i + 1, line);
        }
        return Err(miette::miette!("test harness parsing failed"));
    }

    let test_hir = axiom_hir::lower(&test_parse.module).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("test harness HIR lowering failed")
    })?;

    let codegen_opts = axiom_codegen::CodegenOptions {
        debug_mode: true,
        ..Default::default()
    };
    let test_ir = axiom_codegen::codegen_with_options(&test_hir, &codegen_opts).map_err(|errors| {
        for err in &errors {
            eprintln!("  {err}");
        }
        miette::miette!("test harness codegen failed")
    })?;

    compile::compile_to_binary(&test_ir, &test_exe.display().to_string())?;

    // Run the test binary
    let output = std::process::Command::new(&test_exe)
        .output()
        .map_err(|e| miette::miette!("Failed to run test binary: {e}"))?;

    // Print stdout and stderr
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.is_empty() {
        print!("{stdout}");
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }

    // Clean up temp files
    let _ = std::fs::remove_file(&test_path);
    let _ = std::fs::remove_file(&test_exe);

    if output.status.success() {
        Ok(())
    } else {
        let code = output.status.code().unwrap_or(1);
        Err(miette::miette!("tests failed (exit code {})", code))
    }
}

// ---------------------------------------------------------------------------
// Git versioning for axiom optimize --track
// ---------------------------------------------------------------------------

/// Track an optimization step with git commit or .axiom-history/ fallback.
fn git_track_optimization(file_path: &str, message: &str) -> Result<(), String> {
    // Check if we're in a git repo
    let in_git = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if in_git {
        // git add <file>
        let _ = std::process::Command::new("git")
            .args(["add", file_path])
            .output();
        // Also add the history file
        let history_file = format!("{file_path}.opt_history.json");
        let _ = std::process::Command::new("git")
            .args(["add", &history_file])
            .output();
        // git commit
        let result = std::process::Command::new("git")
            .args(["commit", "-m", message])
            .output();
        match result {
            Ok(out) if out.status.success() => {
                eprintln!("[TRACK] git commit: {message}");
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!("[TRACK] git commit skipped: {}", stderr.trim());
            }
            Err(e) => {
                eprintln!("[TRACK] git error: {e}");
            }
        }
    } else {
        // Fallback: copy to .axiom-history/
        let history_dir = std::path::Path::new(".axiom-history");
        std::fs::create_dir_all(history_dir).ok();
        let version = std::fs::read_dir(history_dir)
            .map(|d| d.count())
            .unwrap_or(0);
        let dest = history_dir.join(format!("v{}.axm", version + 1));
        std::fs::copy(file_path, &dest).ok();
        eprintln!("[TRACK] saved to {}", dest.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests for new features (E5: manifest parsing, G4/L4 helpers)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // E5: axiom.toml manifest parsing

    #[test]
    fn test_parse_axiom_toml_basic() {
        let input = r#"
[package]
name = "my-game"
version = "0.1.0"

[dependencies]
math = { path = "lib/math" }
"#;
        let manifest = parse_axiom_toml(input).unwrap();
        assert_eq!(manifest.package.name, "my-game");
        assert_eq!(manifest.package.version, "0.1.0");
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dependencies["math"].path, "lib/math");
    }

    #[test]
    fn test_parse_axiom_toml_no_deps() {
        let input = r#"
[package]
name = "simple"
version = "1.0.0"
"#;
        let manifest = parse_axiom_toml(input).unwrap();
        assert_eq!(manifest.package.name, "simple");
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn test_parse_axiom_toml_missing_name() {
        let input = r#"
[package]
version = "1.0.0"
"#;
        assert!(parse_axiom_toml(input).is_err());
    }

    #[test]
    fn test_parse_axiom_toml_multiple_deps() {
        let input = r#"
[package]
name = "game"
version = "0.2.0"

[dependencies]
math = { path = "lib/math" }
physics = { path = "lib/physics" }
"#;
        let manifest = parse_axiom_toml(input).unwrap();
        assert_eq!(manifest.dependencies.len(), 2);
        assert_eq!(manifest.dependencies["math"].path, "lib/math");
        assert_eq!(manifest.dependencies["physics"].path, "lib/physics");
    }

    #[test]
    fn test_parse_axiom_toml_comments() {
        let input = r#"
# This is a comment
[package]
name = "test"
version = "0.1.0"

# Another comment
[dependencies]
# dep comment
"#;
        let manifest = parse_axiom_toml(input).unwrap();
        assert_eq!(manifest.package.name, "test");
    }

    // G4: Watch helper (try_compile)

    #[test]
    fn test_try_compile_nonexistent_file() {
        let result = try_compile("nonexistent_file_12345.axm", "out.exe");
        assert!(result.is_err());
    }

    // S3: Rewrite command integration

    #[test]
    fn test_process_includes_basic() {
        let source = "let x: i32 = 1;\n";
        let base_dir = Path::new(".");
        let mut visited = HashSet::new();
        let result = process_includes(source, base_dir, 0, &mut visited).unwrap();
        assert_eq!(result, "let x: i32 = 1;\n");
    }

    #[test]
    fn test_process_includes_depth_limit() {
        // Simulate deep include chain — should error at depth > 10
        let source = "@include \"nonexistent.axm\"\n";
        let base_dir = Path::new(".");
        let mut visited = HashSet::new();
        let result = process_includes(source, base_dir, 11, &mut visited);
        assert!(result.is_err());
    }

    // Verified Development Pipeline: strip_main_function

    #[test]
    fn test_strip_main_function_simple() {
        let source = "fn double(x: i32) -> i32 {\n    return x * 2;\n}\n\nfn main() -> i32 {\n    return 0;\n}\n";
        let stripped = strip_main_function(source);
        assert!(stripped.contains("fn double"));
        assert!(!stripped.contains("fn main"));
    }

    #[test]
    fn test_strip_main_function_no_main() {
        let source = "fn double(x: i32) -> i32 {\n    return x * 2;\n}\n";
        let stripped = strip_main_function(source);
        assert_eq!(stripped, source);
    }

    #[test]
    fn test_strip_main_function_nested_braces() {
        let source = "fn helper() -> i32 { return 1; }\nfn main() -> i32 {\n    if true {\n        return 0;\n    }\n    return 1;\n}\n";
        let stripped = strip_main_function(source);
        assert!(stripped.contains("fn helper"));
        assert!(!stripped.contains("fn main"));
    }

    #[test]
    fn test_strip_main_function_preserves_trailing() {
        let source = "fn a() -> i32 { return 1; }\nfn main() -> i32 { return 0; }\nfn b() -> i32 { return 2; }\n";
        let stripped = strip_main_function(source);
        assert!(stripped.contains("fn a"));
        assert!(stripped.contains("fn b"));
        assert!(!stripped.contains("fn main"));
    }

    // Verified Development Pipeline: format_hir_expr_as_axiom

    #[test]
    fn test_format_hir_expr_int() {
        let expr = axiom_hir::HirExpr {
            id: axiom_hir::NodeId(0),
            kind: axiom_hir::HirExprKind::IntLiteral { value: 42 },
            span: axiom_hir::SPAN_DUMMY,
        };
        assert_eq!(format_hir_expr_as_axiom(&expr), "42");
    }

    #[test]
    fn test_format_hir_expr_bool() {
        let expr = axiom_hir::HirExpr {
            id: axiom_hir::NodeId(0),
            kind: axiom_hir::HirExprKind::BoolLiteral { value: true },
            span: axiom_hir::SPAN_DUMMY,
        };
        assert_eq!(format_hir_expr_as_axiom(&expr), "true");
    }

    #[test]
    fn test_format_hir_expr_neg() {
        let inner = axiom_hir::HirExpr {
            id: axiom_hir::NodeId(1),
            kind: axiom_hir::HirExprKind::IntLiteral { value: 5 },
            span: axiom_hir::SPAN_DUMMY,
        };
        let expr = axiom_hir::HirExpr {
            id: axiom_hir::NodeId(0),
            kind: axiom_hir::HirExprKind::UnaryOp {
                op: axiom_hir::UnaryOp::Neg,
                operand: Box::new(inner),
            },
            span: axiom_hir::SPAN_DUMMY,
        };
        assert_eq!(format_hir_expr_as_axiom(&expr), "-5");
    }

    // Verified Development Pipeline: run_verify

    #[test]
    fn test_run_verify_fully_annotated() {
        let source = r#"
@intent("double a number")
@precondition(x >= 0)
fn double(x: i32) -> i32 {
    return x * 2;
}

fn main() -> i32 {
    return 0;
}
"#;
        // Write to temp file
        let path = std::env::temp_dir().join("axiom_test_verify_full.axm");
        std::fs::write(&path, source).unwrap();
        let result = run_verify(&path.display().to_string());
        assert!(result.is_ok());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_run_verify_missing_annotations() {
        let source = r#"
fn double(x: i32) -> i32 {
    return x * 2;
}

fn main() -> i32 {
    return 0;
}
"#;
        let path = std::env::temp_dir().join("axiom_test_verify_missing.axm");
        std::fs::write(&path, source).unwrap();
        // Should still succeed (it warns, does not error)
        let result = run_verify(&path.display().to_string());
        assert!(result.is_ok());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_run_verify_parse_error() {
        let source = "fn broken( { }";
        let path = std::env::temp_dir().join("axiom_test_verify_broken.axm");
        std::fs::write(&path, source).unwrap();
        let result = run_verify(&path.display().to_string());
        // Parse errors should cause an error return
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    // Verified Development Pipeline: git_track_optimization

    #[test]
    fn test_git_track_optimization_returns_ok() {
        // Should not panic regardless of git state
        let result = git_track_optimization("nonexistent.axm", "test message");
        assert!(result.is_ok());
    }
}
