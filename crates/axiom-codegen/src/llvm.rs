//! LLVM IR text generator.
//!
//! Generates valid LLVM IR text from an [`HirModule`]. Uses an alloca-based
//! strategy for local variables (not SSA phi nodes). LLVM's `mem2reg` pass
//! promotes allocas to SSA registers when optimizing.
//!
//! # Example
//!
//! ```ignore
//! let hir_module = axiom_hir::lower(&ast_module)?;
//! let llvm_ir = axiom_codegen::codegen(&hir_module)?;
//! println!("{llvm_ir}");
//! ```

use std::collections::HashMap;
use std::fmt::Write as _;

use axiom_hir::{
    BinOp, HirAnnotationKind, HirBlock, HirExpr, HirExprKind, HirExternFunction, HirFunction,
    HirModule, HirParam, HirStmt, HirStmtKind, HirType, PrimitiveType, UnaryOp,
};

use crate::error::CodegenError;

/// Information about an alloca'd variable.
#[derive(Debug, Clone)]
struct VarInfo {
    /// The LLVM name of the alloca (e.g., `%a`).
    alloca_name: String,
    /// The LLVM type (e.g., `i64`).
    llvm_type: String,
}

/// Information about a function's signature.
#[derive(Debug, Clone)]
struct FuncInfo {
    /// LLVM return type.
    return_type: String,
    /// LLVM parameter types.
    param_types: Vec<String>,
    /// Whether this function uses fastcc (internal, non-main, non-export).
    uses_fastcc: bool,
}

/// Result of emitting an expression -- an SSA register name or immediate.
#[derive(Debug, Clone)]
struct LlvmValue {
    /// The register name (e.g., `%3`) or literal (e.g., `42`).
    reg: String,
    /// The LLVM type (e.g., `i32`, `i64`, `float`, `double`).
    ty: String,
}

/// Mutable state for IR generation.
struct CodegenContext {
    /// Accumulated LLVM IR text.
    output: String,
    /// Next SSA register number for unnamed temporaries.
    next_reg: u32,
    /// Next label number for basic blocks.
    next_label: u32,
    /// Maps variable names to their alloca info.
    variables: HashMap<String, VarInfo>,
    /// Maps function names to their signatures.
    functions: HashMap<String, FuncInfo>,
    /// Collected string constants for global declarations.
    string_literals: Vec<String>,
    /// Whether an i64 format string is needed.
    needs_printf_i64: bool,
    /// Whether an i32 format string is needed.
    needs_printf_i32: bool,
    /// Whether an f64 format string is needed.
    needs_printf_f64: bool,
    /// Whether puts is needed.
    needs_puts: bool,
    /// Whether printf is needed.
    needs_printf: bool,
    /// Whether the `@llvm.sqrt.f64` intrinsic is needed.
    needs_sqrt_f64: bool,
    /// Whether the `@llvm.pow.f64` intrinsic is needed.
    needs_pow_f64: bool,
    /// Whether the `@llvm.abs.i32` intrinsic is needed.
    needs_abs_i32: bool,
    /// Whether the `@llvm.fabs.f64` intrinsic is needed.
    needs_fabs_f64: bool,
    /// Collected errors.
    errors: Vec<CodegenError>,
    /// Whether the current basic block has been terminated (ret or br).
    block_terminated: bool,
    /// The LLVM return type of the current function being emitted.
    current_return_type: String,
}

impl CodegenContext {
    /// Create a new codegen context.
    fn new() -> Self {
        Self {
            output: String::with_capacity(4096),
            next_reg: 0,
            next_label: 0,
            variables: HashMap::new(),
            functions: HashMap::new(),
            string_literals: Vec::new(),
            needs_printf_i64: false,
            needs_printf_i32: false,
            needs_printf_f64: false,
            needs_puts: false,
            needs_printf: false,
            needs_sqrt_f64: false,
            needs_pow_f64: false,
            needs_abs_i32: false,
            needs_fabs_f64: false,
            errors: Vec::new(),
            block_terminated: false,
            current_return_type: String::new(),
        }
    }

    /// Return the next numbered register like `%0`, `%1`, `%2`.
    fn fresh_reg(&mut self) -> String {
        let reg = format!("%{}", self.next_reg);
        self.next_reg += 1;
        reg
    }

    /// Return the next numbered label like `then.0`, `else.0`, `merge.0`.
    fn fresh_label(&mut self, prefix: &str) -> String {
        let label = format!("{prefix}.{}", self.next_label);
        self.next_label += 1;
        label
    }

    /// Emit a line of LLVM IR with indentation.
    fn emit(&mut self, line: &str) {
        let _ = writeln!(self.output, "  {line}");
    }

    /// Emit a line of LLVM IR without indentation (for labels, define, etc.).
    fn emit_raw(&mut self, line: &str) {
        let _ = writeln!(self.output, "{line}");
    }

    /// Emit a blank line.
    fn emit_blank(&mut self) {
        let _ = writeln!(self.output);
    }
}

/// Generate LLVM IR text from an HIR module.
///
/// Returns the complete `.ll` file content on success, or a list of errors.
pub fn codegen(module: &HirModule) -> Result<String, Vec<CodegenError>> {
    let mut ctx = CodegenContext::new();

    // Register all function signatures.
    for func in &module.functions {
        let ret_type = match hir_type_to_llvm(&func.return_type) {
            Ok(t) => t,
            Err(e) => {
                ctx.errors.push(e);
                continue;
            }
        };
        let mut param_types = Vec::new();
        for param in &func.params {
            match hir_type_to_llvm(&param.ty) {
                Ok(t) => param_types.push(t),
                Err(e) => ctx.errors.push(e),
            }
        }
        let is_export = func
            .annotations
            .iter()
            .any(|a| matches!(a.kind, HirAnnotationKind::Export));
        let uses_fastcc = func.name != "main" && !is_export;
        ctx.functions.insert(
            func.name.clone(),
            FuncInfo {
                return_type: ret_type,
                param_types,
                uses_fastcc,
            },
        );
    }

    // Register extern function signatures.
    for ef in &module.extern_functions {
        let ret_type = match hir_type_to_llvm(&ef.return_type) {
            Ok(t) => t,
            Err(e) => {
                ctx.errors.push(e);
                continue;
            }
        };
        let mut param_types = Vec::new();
        for param in &ef.params {
            match hir_type_to_llvm(&param.ty) {
                Ok(t) => param_types.push(t),
                Err(e) => ctx.errors.push(e),
            }
        }
        ctx.functions.insert(
            ef.name.clone(),
            FuncInfo {
                return_type: ret_type,
                param_types,
                uses_fastcc: false,
            },
        );
    }

    // First pass: emit all functions to a buffer (so we know what globals are needed).
    let mut func_output = String::with_capacity(4096);
    for func in &module.functions {
        let saved_output = std::mem::take(&mut ctx.output);
        emit_function(&mut ctx, func);
        let _ = writeln!(func_output, "{}", ctx.output);
        ctx.output = saved_output;
    }

    // Emit module header.
    let module_name = module.name.as_deref().unwrap_or("axiom_module");
    let _ = writeln!(ctx.output, "; ModuleID = '{module_name}'");
    let _ = writeln!(ctx.output, "source_filename = \"{module_name}\"");

    let target_triple = get_target_triple();
    let _ = writeln!(ctx.output, "target triple = \"{target_triple}\"");
    ctx.emit_blank();

    // Emit string literal globals.
    for (i, s) in ctx.string_literals.iter().enumerate() {
        let escaped = escape_llvm_string(s);
        let len = s.len() + 1; // +1 for null terminator
        let _ = writeln!(
            ctx.output,
            "@.str.{i} = private unnamed_addr constant [{len} x i8] c\"{escaped}\\00\""
        );
    }

    // Emit format string globals.
    if ctx.needs_printf_i64 {
        let _ = writeln!(
            ctx.output,
            "@.fmt.i64 = private unnamed_addr constant [6 x i8] c\"%lld\\0A\\00\""
        );
    }

    if ctx.needs_printf_i32 {
        let _ = writeln!(
            ctx.output,
            "@.fmt.i32 = private unnamed_addr constant [4 x i8] c\"%d\\0A\\00\""
        );
    }

    if ctx.needs_printf_f64 {
        let _ = writeln!(
            ctx.output,
            "@.fmt.f64 = private unnamed_addr constant [4 x i8] c\"%f\\0A\\00\""
        );
    }

    let has_globals = !ctx.string_literals.is_empty()
        || ctx.needs_printf_i64
        || ctx.needs_printf_i32
        || ctx.needs_printf_f64;
    if has_globals {
        ctx.emit_blank();
    }

    // Emit function definitions.
    ctx.output.push_str(&func_output);

    // Emit user-declared extern function declarations.
    for ef in &module.extern_functions {
        emit_extern_function_decl(&mut ctx, ef);
    }

    // Emit external declarations for built-in C functions.
    if ctx.needs_puts {
        let _ = writeln!(ctx.output, "declare i32 @puts(ptr)");
    }
    if ctx.needs_printf {
        let _ = writeln!(ctx.output, "declare i32 @printf(ptr, ...)");
    }
    if ctx.needs_sqrt_f64 {
        let _ = writeln!(ctx.output, "declare double @llvm.sqrt.f64(double)");
    }
    if ctx.needs_pow_f64 {
        let _ = writeln!(ctx.output, "declare double @llvm.pow.f64(double, double)");
    }
    if ctx.needs_abs_i32 {
        let _ = writeln!(ctx.output, "declare i32 @llvm.abs.i32(i32, i1)");
    }
    if ctx.needs_fabs_f64 {
        let _ = writeln!(ctx.output, "declare double @llvm.fabs.f64(double)");
    }

    if !ctx.errors.is_empty() {
        return Err(ctx.errors);
    }

    Ok(ctx.output)
}

/// Emit a function definition.
fn emit_function(ctx: &mut CodegenContext, func: &HirFunction) {
    // Reset per-function state.
    ctx.next_reg = 0;
    ctx.next_label = 0;
    ctx.variables.clear();
    ctx.block_terminated = false;
    ctx.current_return_type = String::new();

    let ret_type = match hir_type_to_llvm(&func.return_type) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    ctx.current_return_type = ret_type.clone();

    // Build parameter list string.
    let params_str = build_params_str(ctx, &func.params);

    // Check if function has @export annotation.
    let is_export = func
        .annotations
        .iter()
        .any(|a| matches!(a.kind, HirAnnotationKind::Export));

    let is_main = func.name == "main";
    if is_export {
        ctx.emit_raw(&format!(
            "define dso_local {ret_type} @{}({params_str}) {{",
            func.name
        ));
    } else if is_main {
        ctx.emit_raw(&format!("define {ret_type} @{}({params_str}) {{", func.name));
    } else {
        // Internal functions use fastcc for better performance on recursive calls
        ctx.emit_raw(&format!(
            "define internal fastcc {ret_type} @{}({params_str}) {{",
            func.name
        ));
    }
    ctx.emit_raw("entry:");

    // Alloca + store for each parameter.
    emit_param_allocas(ctx, &func.params);

    // Emit function body.
    emit_block(ctx, &func.body);

    ctx.emit_raw("}");
}

/// Emit an extern function declaration (`declare`).
fn emit_extern_function_decl(ctx: &mut CodegenContext, ef: &HirExternFunction) {
    let ret_type = match hir_type_to_llvm(&ef.return_type) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    let mut param_types_str = Vec::new();
    for param in &ef.params {
        match hir_type_to_llvm(&param.ty) {
            Ok(t) => param_types_str.push(t),
            Err(e) => ctx.errors.push(e),
        }
    }

    let params_str = param_types_str.join(", ");
    let _ = writeln!(
        ctx.output,
        "declare {ret_type} @{}({params_str})",
        ef.name
    );
}

/// Build the parameter list string for a function definition.
fn build_params_str(ctx: &mut CodegenContext, params: &[HirParam]) -> String {
    let mut parts = Vec::new();
    for param in params {
        match hir_type_to_llvm(&param.ty) {
            Ok(llvm_type) => {
                parts.push(format!("{llvm_type} %{}", param.name));
            }
            Err(e) => ctx.errors.push(e),
        }
    }
    parts.join(", ")
}

/// Emit alloca + store for function parameters.
fn emit_param_allocas(ctx: &mut CodegenContext, params: &[HirParam]) {
    for param in params {
        let llvm_type = match hir_type_to_llvm(&param.ty) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let alloca_name = format!("%{}.addr", param.name);
        ctx.emit(&format!("{alloca_name} = alloca {llvm_type}"));
        ctx.emit(&format!(
            "store {llvm_type} %{}, ptr {alloca_name}",
            param.name
        ));
        ctx.variables.insert(
            param.name.clone(),
            VarInfo {
                alloca_name,
                llvm_type,
            },
        );
    }
}

/// Emit statements in a block.
fn emit_block(ctx: &mut CodegenContext, block: &HirBlock) {
    for stmt in &block.stmts {
        if ctx.block_terminated {
            // Don't emit code after a terminator in the same basic block.
            break;
        }
        emit_stmt(ctx, stmt);
    }
}

/// Emit a single statement.
fn emit_stmt(ctx: &mut CodegenContext, stmt: &HirStmt) {
    match &stmt.kind {
        HirStmtKind::Let {
            name,
            ty,
            value,
            ..
        } => emit_let(ctx, name, ty, value),
        HirStmtKind::Assign { target, value } => emit_assign(ctx, target, value),
        HirStmtKind::Return { value } => emit_return(ctx, value),
        HirStmtKind::If {
            condition,
            then_block,
            else_block,
        } => emit_if(ctx, condition, then_block, else_block.as_ref()),
        HirStmtKind::For {
            var,
            var_type,
            iterable,
            body,
            ..
        } => emit_for(ctx, var, var_type, iterable, body),
        HirStmtKind::While { condition, body } => emit_while(ctx, condition, body),
        HirStmtKind::Expr { expr } => {
            emit_expr(ctx, expr, None);
        }
    }
}

/// Emit a let binding: alloca + optional store.
fn emit_let(ctx: &mut CodegenContext, name: &str, ty: &HirType, value: &HirExpr) {
    let llvm_type = match hir_type_to_llvm(ty) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    let alloca_name = format!("%{name}");
    ctx.emit(&format!("{alloca_name} = alloca {llvm_type}"));

    let val = emit_expr(ctx, value, Some(&llvm_type));
    ctx.emit(&format!(
        "store {llvm_type} {}, ptr {alloca_name}",
        val.reg
    ));

    ctx.variables.insert(
        name.to_string(),
        VarInfo {
            alloca_name,
            llvm_type,
        },
    );
}

/// Emit an assignment: store to existing alloca.
fn emit_assign(ctx: &mut CodegenContext, target: &HirExpr, value: &HirExpr) {
    if let HirExprKind::Ident { name } = &target.kind {
        let var_info = match ctx.variables.get(name) {
            Some(v) => v.clone(),
            None => {
                ctx.errors.push(CodegenError::UndefinedVariable {
                    name: name.clone(),
                });
                return;
            }
        };
        let val = emit_expr(ctx, value, Some(&var_info.llvm_type));
        ctx.emit(&format!(
            "store {} {}, ptr {}",
            var_info.llvm_type, val.reg, var_info.alloca_name
        ));
    } else {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "non-ident assignment target".to_string(),
            context: "assignment".to_string(),
        });
    }
}

/// Emit a return statement.
fn emit_return(ctx: &mut CodegenContext, value: &HirExpr) {
    let ret_type = ctx.current_return_type.clone();
    let expected = if ret_type.is_empty() {
        None
    } else {
        Some(ret_type.as_str())
    };
    let val = emit_expr(ctx, value, expected);
    let ty = if !ret_type.is_empty() {
        &ret_type
    } else {
        &val.ty
    };
    ctx.emit(&format!("ret {ty} {}", val.reg));
    ctx.block_terminated = true;
}

/// Emit if/else control flow.
fn emit_if(
    ctx: &mut CodegenContext,
    condition: &HirExpr,
    then_block: &HirBlock,
    else_block: Option<&HirBlock>,
) {
    let cond = emit_expr(ctx, condition, Some("i1"));

    let then_label = ctx.fresh_label("then");
    let merge_label = ctx.fresh_label("merge");

    if let Some(else_blk) = else_block {
        let else_label = ctx.fresh_label("else");
        ctx.emit(&format!(
            "br i1 {}, label %{then_label}, label %{else_label}",
            cond.reg
        ));

        // Then block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{then_label}:"));
        ctx.block_terminated = false;
        emit_block(ctx, then_block);
        let then_terminated = ctx.block_terminated;
        if !then_terminated {
            ctx.emit(&format!("br label %{merge_label}"));
        }

        // Else block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{else_label}:"));
        ctx.block_terminated = false;
        emit_block(ctx, else_blk);
        let else_terminated = ctx.block_terminated;
        if !else_terminated {
            ctx.emit(&format!("br label %{merge_label}"));
        }

        // Merge block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{merge_label}:"));
        // If both branches terminated, the merge block is unreachable.
        ctx.block_terminated = then_terminated && else_terminated;
        if ctx.block_terminated {
            // Emit unreachable so that the block is still valid LLVM IR.
            // Actually, if both returned, the merge block will just be unused
            // but we should emit it anyway for valid IR. An unreachable helps.
            // But if further code after the if will be skipped by block_terminated,
            // we don't need unreachable -- the label alone is enough for any
            // branches pointing to it.
        }
    } else {
        ctx.emit(&format!(
            "br i1 {}, label %{then_label}, label %{merge_label}",
            cond.reg
        ));

        // Then block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{then_label}:"));
        ctx.block_terminated = false;
        emit_block(ctx, then_block);
        if !ctx.block_terminated {
            ctx.emit(&format!("br label %{merge_label}"));
        }

        // Merge block.
        ctx.emit_blank();
        ctx.emit_raw(&format!("{merge_label}:"));
        ctx.block_terminated = false;
    }
}

/// Emit a for loop with range() recognition.
fn emit_for(
    ctx: &mut CodegenContext,
    var: &str,
    var_type: &HirType,
    iterable: &HirExpr,
    body: &HirBlock,
) {
    let loop_type = match hir_type_to_llvm(var_type) {
        Ok(t) => t,
        Err(e) => {
            ctx.errors.push(e);
            return;
        }
    };

    // Recognize range(start, end) or range(end) pattern.
    let (start_expr, end_expr) = match &iterable.kind {
        HirExprKind::Call { func, args } => {
            if let HirExprKind::Ident { name } = &func.kind {
                if name == "range" {
                    match args.len() {
                        1 => (None, Some(&args[0])),
                        2 => (Some(&args[0]), Some(&args[1])),
                        _ => {
                            ctx.errors.push(CodegenError::UnsupportedExpression {
                                expr: "range() with wrong number of arguments".to_string(),
                                context: "for loop".to_string(),
                            });
                            return;
                        }
                    }
                } else {
                    ctx.errors.push(CodegenError::UnsupportedExpression {
                        expr: format!("for-in with non-range iterable `{name}`"),
                        context: "for loop".to_string(),
                    });
                    return;
                }
            } else {
                ctx.errors.push(CodegenError::UnsupportedExpression {
                    expr: "for-in with non-ident callee".to_string(),
                    context: "for loop".to_string(),
                });
                return;
            }
        }
        _ => {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: "for-in with non-range iterable".to_string(),
                context: "for loop".to_string(),
            });
            return;
        }
    };

    // Emit start value.
    let start_val = match start_expr {
        Some(expr) => emit_expr(ctx, expr, Some(&loop_type)),
        None => LlvmValue {
            reg: "0".to_string(),
            ty: loop_type.clone(),
        },
    };

    // Emit end value (once, before the loop).
    let end_val = emit_expr(ctx, end_expr.expect("end_expr should be Some"), Some(&loop_type));

    // Alloca for loop variable.
    let alloca_name = format!("%{var}");
    ctx.emit(&format!("{alloca_name} = alloca {loop_type}"));
    ctx.emit(&format!(
        "store {loop_type} {}, ptr {alloca_name}",
        start_val.reg
    ));

    // Save old variable info if shadowed.
    let old_var = ctx.variables.remove(var);

    ctx.variables.insert(
        var.to_string(),
        VarInfo {
            alloca_name: alloca_name.clone(),
            llvm_type: loop_type.clone(),
        },
    );

    let cond_label = ctx.fresh_label("for.cond");
    let body_label = ctx.fresh_label("for.body");
    let end_label = ctx.fresh_label("for.end");

    ctx.emit(&format!("br label %{cond_label}"));

    // Condition block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{cond_label}:"));
    ctx.block_terminated = false;
    let load_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{load_reg} = load {loop_type}, ptr {alloca_name}"
    ));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = icmp slt {loop_type} {load_reg}, {}",
        end_val.reg
    ));
    ctx.emit(&format!(
        "br i1 {cmp_reg}, label %{body_label}, label %{end_label}"
    ));

    // Body block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{body_label}:"));
    ctx.block_terminated = false;
    emit_block(ctx, body);

    // Increment loop variable.
    if !ctx.block_terminated {
        let inc_load = ctx.fresh_reg();
        ctx.emit(&format!(
            "{inc_load} = load {loop_type}, ptr {alloca_name}"
        ));
        let inc_add = ctx.fresh_reg();
        ctx.emit(&format!("{inc_add} = add {loop_type} {inc_load}, 1"));
        ctx.emit(&format!(
            "store {loop_type} {inc_add}, ptr {alloca_name}"
        ));
        ctx.emit(&format!("br label %{cond_label}"));
    }

    // End block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{end_label}:"));
    ctx.block_terminated = false;

    // Restore old variable if shadowed.
    if let Some(old) = old_var {
        ctx.variables.insert(var.to_string(), old);
    }
}

/// Emit a while loop.
fn emit_while(ctx: &mut CodegenContext, condition: &HirExpr, body: &HirBlock) {
    let cond_label = ctx.fresh_label("while.cond");
    let body_label = ctx.fresh_label("while.body");
    let end_label = ctx.fresh_label("while.end");

    ctx.emit(&format!("br label %{cond_label}"));

    // Condition block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{cond_label}:"));
    ctx.block_terminated = false;
    let cond = emit_expr(ctx, condition, Some("i1"));
    ctx.emit(&format!(
        "br i1 {}, label %{body_label}, label %{end_label}",
        cond.reg
    ));

    // Body block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{body_label}:"));
    ctx.block_terminated = false;
    emit_block(ctx, body);
    if !ctx.block_terminated {
        ctx.emit(&format!("br label %{cond_label}"));
    }

    // End block.
    ctx.emit_blank();
    ctx.emit_raw(&format!("{end_label}:"));
    ctx.block_terminated = false;
}

/// Emit an expression. Returns the LLVM value (register or literal) and type.
///
/// `expected_type` is a hint for literals that don't carry their own type.
fn emit_expr(ctx: &mut CodegenContext, expr: &HirExpr, expected_type: Option<&str>) -> LlvmValue {
    match &expr.kind {
        HirExprKind::IntLiteral { value } => {
            let ty = expected_type.unwrap_or("i64").to_string();
            LlvmValue {
                reg: format!("{value}"),
                ty,
            }
        }
        HirExprKind::FloatLiteral { value } => {
            let ty = expected_type.unwrap_or("double").to_string();
            // LLVM requires decimal format with digit on each side of the dot.
            let formatted = format_float(*value);
            LlvmValue {
                reg: formatted,
                ty,
            }
        }
        HirExprKind::BoolLiteral { value } => LlvmValue {
            reg: if *value {
                "1".to_string()
            } else {
                "0".to_string()
            },
            ty: "i1".to_string(),
        },
        HirExprKind::StringLiteral { value } => {
            let idx = ctx.string_literals.len();
            ctx.string_literals.push(value.clone());
            LlvmValue {
                reg: format!("@.str.{idx}"),
                ty: "ptr".to_string(),
            }
        }
        HirExprKind::Ident { name } => emit_ident(ctx, name),
        HirExprKind::BinaryOp { op, lhs, rhs } => emit_binary_op(ctx, *op, lhs, rhs),
        HirExprKind::UnaryOp { op, operand } => emit_unary_op(ctx, *op, operand),
        HirExprKind::Call { func, args } => emit_call(ctx, func, args),
        _ => {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: format!("{:?}", expr.kind),
                context: "expression".to_string(),
            });
            LlvmValue {
                reg: "0".to_string(),
                ty: "i32".to_string(),
            }
        }
    }
}

/// Emit a variable reference (load from alloca).
fn emit_ident(ctx: &mut CodegenContext, name: &str) -> LlvmValue {
    let var_info = match ctx.variables.get(name) {
        Some(v) => v.clone(),
        None => {
            ctx.errors.push(CodegenError::UndefinedVariable {
                name: name.to_string(),
            });
            return LlvmValue {
                reg: "0".to_string(),
                ty: "i32".to_string(),
            };
        }
    };

    let reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{reg} = load {}, ptr {}",
        var_info.llvm_type, var_info.alloca_name
    ));
    LlvmValue {
        reg,
        ty: var_info.llvm_type,
    }
}

/// Emit a binary operation.
fn emit_binary_op(
    ctx: &mut CodegenContext,
    op: BinOp,
    lhs: &HirExpr,
    rhs: &HirExpr,
) -> LlvmValue {
    let lhs_val = emit_expr(ctx, lhs, None);
    let rhs_val = emit_expr(ctx, rhs, Some(&lhs_val.ty));

    let is_float = is_float_type(&lhs_val.ty);
    let result_reg = ctx.fresh_reg();

    let instruction = match (op, is_float) {
        // Arithmetic.
        (BinOp::Add, false) | (BinOp::AddWrap, false) => "add",
        (BinOp::Sub, false) | (BinOp::SubWrap, false) => "sub",
        (BinOp::Mul, false) | (BinOp::MulWrap, false) => "mul",
        (BinOp::Div, false) => "sdiv",
        (BinOp::Mod, false) => "srem",
        (BinOp::Add, true) => "fadd",
        (BinOp::Sub, true) => "fsub",
        (BinOp::Mul, true) => "fmul",
        (BinOp::Div, true) => "fdiv",
        (BinOp::Mod, true) => "frem",
        // Logical.
        (BinOp::And, _) => "and",
        (BinOp::Or, _) => "or",
        // Comparisons (integer).
        (BinOp::Eq, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp eq {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::NotEq, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp ne {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::Lt, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp slt {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::Gt, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp sgt {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::LtEq, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp sle {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::GtEq, false) => {
            ctx.emit(&format!(
                "{result_reg} = icmp sge {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        // Comparisons (float).
        (BinOp::Eq, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp oeq {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::NotEq, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp one {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::Lt, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp olt {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::Gt, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp ogt {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::LtEq, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp ole {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        (BinOp::GtEq, true) => {
            ctx.emit(&format!(
                "{result_reg} = fcmp oge {} {}, {}",
                lhs_val.ty, lhs_val.reg, rhs_val.reg
            ));
            return LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            };
        }
        // Unsupported ops.
        (op, _) => {
            ctx.errors.push(CodegenError::UnsupportedExpression {
                expr: format!("{op:?}"),
                context: "binary operation".to_string(),
            });
            return LlvmValue {
                reg: "0".to_string(),
                ty: lhs_val.ty,
            };
        }
    };

    ctx.emit(&format!(
        "{result_reg} = {instruction} {} {}, {}",
        lhs_val.ty, lhs_val.reg, rhs_val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: lhs_val.ty,
    }
}

/// Emit a unary operation.
fn emit_unary_op(ctx: &mut CodegenContext, op: UnaryOp, operand: &HirExpr) -> LlvmValue {
    let val = emit_expr(ctx, operand, None);
    let result_reg = ctx.fresh_reg();

    match op {
        UnaryOp::Neg => {
            if is_float_type(&val.ty) {
                ctx.emit(&format!(
                    "{result_reg} = fneg {} {}",
                    val.ty, val.reg
                ));
            } else {
                ctx.emit(&format!(
                    "{result_reg} = sub {} 0, {}",
                    val.ty, val.reg
                ));
            }
            LlvmValue {
                reg: result_reg,
                ty: val.ty,
            }
        }
        UnaryOp::Not => {
            ctx.emit(&format!("{result_reg} = xor i1 {}, 1", val.reg));
            LlvmValue {
                reg: result_reg,
                ty: "i1".to_string(),
            }
        }
    }
}

/// Emit a function call expression.
fn emit_call(ctx: &mut CodegenContext, func: &HirExpr, args: &[HirExpr]) -> LlvmValue {
    if let HirExprKind::Ident { name } = &func.kind {
        // Check for built-in functions first.
        match name.as_str() {
            "print" => return emit_builtin_print(ctx, args),
            "print_i64" => return emit_builtin_print_i64(ctx, args),
            "print_i32" => return emit_builtin_print_i32(ctx, args),
            "print_f64" => return emit_builtin_print_f64(ctx, args),
            "widen" => return emit_builtin_widen(ctx, args),
            "narrow" => return emit_builtin_narrow(ctx, args),
            "truncate" => return emit_builtin_truncate(ctx, args),
            "abs" => return emit_builtin_abs(ctx, args),
            "abs_f64" => return emit_builtin_abs_f64(ctx, args),
            "min" => return emit_builtin_min(ctx, args),
            "min_f64" => return emit_builtin_min_f64(ctx, args),
            "max" => return emit_builtin_max(ctx, args),
            "max_f64" => return emit_builtin_max_f64(ctx, args),
            "sqrt" => return emit_builtin_sqrt(ctx, args),
            "pow" => return emit_builtin_pow(ctx, args),
            "to_f64" => return emit_builtin_to_f64(ctx, args),
            "to_f64_i64" => return emit_builtin_to_f64_i64(ctx, args),
            _ => {}
        }

        // Regular function call.
        let func_info = match ctx.functions.get(name.as_str()) {
            Some(f) => f.clone(),
            None => {
                ctx.errors.push(CodegenError::UndefinedFunction {
                    name: name.clone(),
                });
                return LlvmValue {
                    reg: "0".to_string(),
                    ty: "i32".to_string(),
                };
            }
        };

        // Emit arguments with type hints from the function signature.
        let mut arg_strs = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let expected_ty = func_info.param_types.get(i).map(|s| s.as_str());
            let val = emit_expr(ctx, arg, expected_ty);
            let arg_ty = if let Some(pt) = func_info.param_types.get(i) {
                pt.clone()
            } else {
                val.ty.clone()
            };
            arg_strs.push(format!("{arg_ty} {}", val.reg));
        }

        let result_reg = ctx.fresh_reg();
        let args_str = arg_strs.join(", ");

        let cc = if func_info.uses_fastcc { "fastcc " } else { "" };
        if func_info.return_type == "void" {
            ctx.emit(&format!("{cc}call void @{name}({args_str})"));
            LlvmValue {
                reg: "0".to_string(),
                ty: "void".to_string(),
            }
        } else {
            ctx.emit(&format!(
                "{result_reg} = call {cc}{} @{name}({args_str})",
                func_info.return_type
            ));
            LlvmValue {
                reg: result_reg,
                ty: func_info.return_type,
            }
        }
    } else {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "indirect function call".to_string(),
            context: "call expression".to_string(),
        });
        LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        }
    }
}

/// Emit built-in `print(msg)` -- calls C `puts()`.
fn emit_builtin_print(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_puts = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "print() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("ptr"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!("{result_reg} = call i32 @puts(ptr {})", val.reg));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `print_i64(n)` -- calls C `printf("%lld\n", n)`.
fn emit_builtin_print_i64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_printf = true;
    ctx.needs_printf_i64 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "print_i64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i64"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 (ptr, ...) @printf(ptr @.fmt.i64, i64 {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `widen(n)` -- sign-extends i32 to i64.
fn emit_builtin_widen(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "widen() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i64".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = sext i32 {} to i64",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i64".to_string(),
    }
}

/// Emit built-in `print_i32(n)` -- calls C `printf("%d\n", n)`.
fn emit_builtin_print_i32(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_printf = true;
    ctx.needs_printf_i32 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "print_i32() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 (ptr, ...) @printf(ptr @.fmt.i32, i32 {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `print_f64(x)` -- calls C `printf("%f\n", x)`.
fn emit_builtin_print_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_printf = true;
    ctx.needs_printf_f64 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "print_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call i32 (ptr, ...) @printf(ptr @.fmt.f64, double {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `narrow(n)` -- truncates i64 to i32.
fn emit_builtin_narrow(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "narrow() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i64"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = trunc i64 {} to i32",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `truncate(x)` -- converts f64 to i32 via fptosi.
fn emit_builtin_truncate(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "truncate() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = fptosi double {} to i32",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `abs(x: i32) -> i32` -- uses `@llvm.abs.i32`.
fn emit_builtin_abs(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_abs_i32 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "abs() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    // The second argument (i1 false) means INT_MIN is not poison.
    ctx.emit(&format!(
        "{result_reg} = call i32 @llvm.abs.i32(i32 {}, i1 false)",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `abs_f64(x: f64) -> f64` -- uses `@llvm.fabs.f64`.
fn emit_builtin_abs_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_fabs_f64 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "abs_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call double @llvm.fabs.f64(double {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `min(a: i32, b: i32) -> i32` -- uses icmp + select.
fn emit_builtin_min(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "min() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let b = emit_expr(ctx, &args[1], Some("i32"));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = icmp slt i32 {}, {}",
        a.reg, b.reg
    ));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = select i1 {cmp_reg}, i32 {}, i32 {}",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `min_f64(a: f64, b: f64) -> f64` -- uses fcmp + select.
fn emit_builtin_min_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "min_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("double"));
    let b = emit_expr(ctx, &args[1], Some("double"));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = fcmp olt double {}, {}",
        a.reg, b.reg
    ));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = select i1 {cmp_reg}, double {}, double {}",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `max(a: i32, b: i32) -> i32` -- uses icmp + select.
fn emit_builtin_max(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "max() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "i32".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("i32"));
    let b = emit_expr(ctx, &args[1], Some("i32"));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = icmp sgt i32 {}, {}",
        a.reg, b.reg
    ));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = select i1 {cmp_reg}, i32 {}, i32 {}",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "i32".to_string(),
    }
}

/// Emit built-in `max_f64(a: f64, b: f64) -> f64` -- uses fcmp + select.
fn emit_builtin_max_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "max_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let a = emit_expr(ctx, &args[0], Some("double"));
    let b = emit_expr(ctx, &args[1], Some("double"));
    let cmp_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{cmp_reg} = fcmp ogt double {}, {}",
        a.reg, b.reg
    ));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = select i1 {cmp_reg}, double {}, double {}",
        a.reg, b.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `sqrt(x: f64) -> f64` -- uses `@llvm.sqrt.f64`.
fn emit_builtin_sqrt(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_sqrt_f64 = true;

    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "sqrt() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call double @llvm.sqrt.f64(double {})",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `pow(base: f64, exp: f64) -> f64` -- uses `@llvm.pow.f64`.
fn emit_builtin_pow(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    ctx.needs_pow_f64 = true;

    if args.len() != 2 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "pow() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let base = emit_expr(ctx, &args[0], Some("double"));
    let exp = emit_expr(ctx, &args[1], Some("double"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = call double @llvm.pow.f64(double {}, double {})",
        base.reg, exp.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `to_f64(x: i32) -> f64` -- converts i32 to f64 via sitofp.
fn emit_builtin_to_f64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "to_f64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i32"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = sitofp i32 {} to double",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Emit built-in `to_f64_i64(x: i64) -> f64` -- converts i64 to f64 via sitofp.
fn emit_builtin_to_f64_i64(ctx: &mut CodegenContext, args: &[HirExpr]) -> LlvmValue {
    if args.len() != 1 {
        ctx.errors.push(CodegenError::UnsupportedExpression {
            expr: "to_f64_i64() with wrong number of arguments".to_string(),
            context: "built-in call".to_string(),
        });
        return LlvmValue {
            reg: "0".to_string(),
            ty: "double".to_string(),
        };
    }

    let val = emit_expr(ctx, &args[0], Some("i64"));
    let result_reg = ctx.fresh_reg();
    ctx.emit(&format!(
        "{result_reg} = sitofp i64 {} to double",
        val.reg
    ));
    LlvmValue {
        reg: result_reg,
        ty: "double".to_string(),
    }
}

/// Convert an HIR type to its LLVM IR type string.
fn hir_type_to_llvm(ty: &HirType) -> Result<String, CodegenError> {
    match ty {
        HirType::Primitive(p) => Ok(primitive_to_llvm(*p)),
        HirType::UserDefined(name) => Err(CodegenError::UnsupportedType {
            ty: name.clone(),
            context: "user-defined types not yet supported".to_string(),
        }),
        HirType::Tensor { .. } => Err(CodegenError::UnsupportedType {
            ty: "tensor".to_string(),
            context: "tensor types not yet supported".to_string(),
        }),
        HirType::Array { .. } => Err(CodegenError::UnsupportedType {
            ty: "array".to_string(),
            context: "array types not yet supported".to_string(),
        }),
        HirType::Slice { .. } => Err(CodegenError::UnsupportedType {
            ty: "slice".to_string(),
            context: "slice types not yet supported".to_string(),
        }),
        HirType::Ptr { .. } => Ok("ptr".to_string()),
        HirType::Tuple { .. } => Err(CodegenError::UnsupportedType {
            ty: "tuple".to_string(),
            context: "tuple types not yet supported".to_string(),
        }),
        HirType::Fn { .. } => Err(CodegenError::UnsupportedType {
            ty: "fn".to_string(),
            context: "function types not yet supported".to_string(),
        }),
        HirType::Unknown(name) => Err(CodegenError::UnsupportedType {
            ty: name.clone(),
            context: "unknown type".to_string(),
        }),
    }
}

/// Convert a primitive type to its LLVM IR type string.
fn primitive_to_llvm(p: PrimitiveType) -> String {
    match p {
        PrimitiveType::I8 | PrimitiveType::U8 => "i8".to_string(),
        PrimitiveType::I16 | PrimitiveType::U16 => "i16".to_string(),
        PrimitiveType::I32 | PrimitiveType::U32 => "i32".to_string(),
        PrimitiveType::I64 | PrimitiveType::U64 => "i64".to_string(),
        PrimitiveType::I128 | PrimitiveType::U128 => "i128".to_string(),
        PrimitiveType::F16 => "half".to_string(),
        PrimitiveType::Bf16 => "bfloat".to_string(),
        PrimitiveType::F32 => "float".to_string(),
        PrimitiveType::F64 => "double".to_string(),
        PrimitiveType::Bool => "i1".to_string(),
    }
}

/// Check whether an LLVM type string represents a floating-point type.
fn is_float_type(ty: &str) -> bool {
    matches!(ty, "float" | "double" | "half" | "bfloat")
}

/// Get the target triple for the current host platform.
fn get_target_triple() -> &'static str {
    if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-macosx"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

/// Escape a string for LLVM IR constant syntax.
///
/// LLVM uses `\xx` hex escapes for non-printable characters.
fn escape_llvm_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'\n' => result.push_str("\\0A"),
            b'\r' => result.push_str("\\0D"),
            b'\t' => result.push_str("\\09"),
            b'\0' => result.push_str("\\00"),
            b'\\' => result.push_str("\\5C"),
            b'"' => result.push_str("\\22"),
            0x20..=0x7E => result.push(byte as char),
            _ => {
                let _ = write!(result, "\\{byte:02X}");
            }
        }
    }
    result
}

/// Format a float value for LLVM IR.
///
/// LLVM requires at least one digit on each side of the decimal point.
fn format_float(value: f64) -> String {
    if value == 0.0 {
        return "0.0".to_string();
    }
    let s = format!("{value}");
    if s.contains('.') || s.contains('e') || s.contains('E') {
        // Ensure there's a digit after the dot.
        if s.ends_with('.') {
            format!("{s}0")
        } else {
            s
        }
    } else {
        format!("{s}.0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_hir::{
        HirAnnotation, HirAnnotationKind, HirBlock, HirExpr, HirExprKind,
        HirExternFunction, HirFunction, HirModule, HirParam, HirStmt, HirStmtKind,
        HirType, NodeId, PrimitiveType, SPAN_DUMMY,
    };

    /// Helper: create a dummy span.
    fn span() -> axiom_lexer::Span {
        SPAN_DUMMY
    }

    /// Helper: create a dummy node ID.
    fn nid(n: u32) -> NodeId {
        NodeId(n)
    }

    /// Helper: create an integer literal expression.
    fn int_lit(value: i128) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::IntLiteral { value },
            span: span(),
        }
    }

    /// Helper: create a float literal expression.
    fn float_lit(value: f64) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::FloatLiteral { value },
            span: span(),
        }
    }

    /// Helper: create a bool literal expression.
    fn bool_lit(value: bool) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::BoolLiteral { value },
            span: span(),
        }
    }

    /// Helper: create a string literal expression.
    fn str_lit(value: &str) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::StringLiteral {
                value: value.to_string(),
            },
            span: span(),
        }
    }

    /// Helper: create an identifier expression.
    fn ident(name: &str) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::Ident {
                name: name.to_string(),
            },
            span: span(),
        }
    }

    /// Helper: create a binary op expression.
    fn binop(op: BinOp, lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::BinaryOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span: span(),
        }
    }

    /// Helper: create a function call expression.
    fn call(func_name: &str, args: Vec<HirExpr>) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::Call {
                func: Box::new(ident(func_name)),
                args,
            },
            span: span(),
        }
    }

    /// Helper: create a unary op expression.
    fn unaryop(op: UnaryOp, operand: HirExpr) -> HirExpr {
        HirExpr {
            id: nid(0),
            kind: HirExprKind::UnaryOp {
                op,
                operand: Box::new(operand),
            },
            span: span(),
        }
    }

    /// Helper: create a block with statements.
    fn block(stmts: Vec<HirStmt>) -> HirBlock {
        HirBlock {
            id: nid(0),
            annotations: vec![],
            stmts,
            span: span(),
        }
    }

    /// Helper: create a statement.
    fn stmt(kind: HirStmtKind) -> HirStmt {
        HirStmt {
            id: nid(0),
            kind,
            span: span(),
        }
    }

    /// Helper: create a function.
    fn func(
        name: &str,
        params: Vec<HirParam>,
        return_type: HirType,
        body: HirBlock,
    ) -> HirFunction {
        HirFunction {
            id: nid(0),
            name: name.to_string(),
            name_span: span(),
            annotations: vec![],
            params,
            return_type,
            body,
            span: span(),
        }
    }

    /// Helper: create a param.
    fn param(name: &str, ty: HirType) -> HirParam {
        HirParam {
            id: nid(0),
            name: name.to_string(),
            name_span: span(),
            ty,
            annotations: vec![],
        }
    }

    /// Helper: create a module with functions.
    fn module(name: Option<&str>, functions: Vec<HirFunction>) -> HirModule {
        HirModule {
            name: name.map(|s| s.to_string()),
            annotations: vec![],
            functions,
            extern_functions: vec![],
            structs: vec![],
            type_aliases: vec![],
            imports: vec![],
        }
    }

    /// Helper: create a module with functions and extern functions.
    fn module_with_externs(
        name: Option<&str>,
        functions: Vec<HirFunction>,
        extern_functions: Vec<HirExternFunction>,
    ) -> HirModule {
        HirModule {
            name: name.map(|s| s.to_string()),
            annotations: vec![],
            functions,
            extern_functions,
            structs: vec![],
            type_aliases: vec![],
            imports: vec![],
        }
    }

    // -----------------------------------------------------------------------
    // Type mapping tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_numeric_types() {
        assert_eq!(primitive_to_llvm(PrimitiveType::I8), "i8");
        assert_eq!(primitive_to_llvm(PrimitiveType::I16), "i16");
        assert_eq!(primitive_to_llvm(PrimitiveType::I32), "i32");
        assert_eq!(primitive_to_llvm(PrimitiveType::I64), "i64");
        assert_eq!(primitive_to_llvm(PrimitiveType::I128), "i128");
        assert_eq!(primitive_to_llvm(PrimitiveType::U8), "i8");
        assert_eq!(primitive_to_llvm(PrimitiveType::U16), "i16");
        assert_eq!(primitive_to_llvm(PrimitiveType::U32), "i32");
        assert_eq!(primitive_to_llvm(PrimitiveType::U64), "i64");
        assert_eq!(primitive_to_llvm(PrimitiveType::U128), "i128");
        assert_eq!(primitive_to_llvm(PrimitiveType::F16), "half");
        assert_eq!(primitive_to_llvm(PrimitiveType::Bf16), "bfloat");
        assert_eq!(primitive_to_llvm(PrimitiveType::F32), "float");
        assert_eq!(primitive_to_llvm(PrimitiveType::F64), "double");
        assert_eq!(primitive_to_llvm(PrimitiveType::Bool), "i1");
    }

    // -----------------------------------------------------------------------
    // Basic function tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_main_return_zero() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::Return {
                    value: int_lit(0),
                })]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("define i32 @main()"), "should define main");
        assert!(ir.contains("ret i32 0"), "should return 0");
    }

    #[test]
    fn test_function_params() {
        let m = module(
            Some("test"),
            vec![func(
                "add",
                vec![
                    param("a", HirType::Primitive(PrimitiveType::I32)),
                    param("b", HirType::Primitive(PrimitiveType::I32)),
                ],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::Return {
                    value: binop(BinOp::Add, ident("a"), ident("b")),
                })]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("@add(i32 %a, i32 %b)"),
            "should define add with params"
        );
        assert!(ir.contains("%a.addr = alloca i32"), "should alloca param a");
        assert!(
            ir.contains("store i32 %a, ptr %a.addr"),
            "should store param a"
        );
        assert!(ir.contains("%b.addr = alloca i32"), "should alloca param b");
        assert!(
            ir.contains("store i32 %b, ptr %b.addr"),
            "should store param b"
        );
        assert!(ir.contains("add i32"), "should add");
    }

    // -----------------------------------------------------------------------
    // Let binding and assignment tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_let_binding() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(42),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("%x = alloca i32"), "should alloca x");
        assert!(ir.contains("store i32 42, ptr %x"), "should store 42");
        assert!(ir.contains("load i32, ptr %x"), "should load x");
    }

    #[test]
    fn test_assignment() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(0),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::Assign {
                        target: ident("x"),
                        value: int_lit(42),
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("store i32 42, ptr %x"), "should store 42 to x");
    }

    // -----------------------------------------------------------------------
    // If/else tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_if_no_else() {
        let m = module(
            Some("test"),
            vec![func(
                "test_fn",
                vec![param("x", HirType::Primitive(PrimitiveType::I32))],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::If {
                        condition: binop(BinOp::Gt, ident("x"), int_lit(0)),
                        then_block: block(vec![stmt(HirStmtKind::Return {
                            value: int_lit(1),
                        })]),
                        else_block: None,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("icmp sgt"), "should have comparison");
        assert!(ir.contains("br i1"), "should have conditional branch");
        assert!(ir.contains("then."), "should have then label");
        assert!(ir.contains("merge."), "should have merge label");
    }

    #[test]
    fn test_if_else() {
        let m = module(
            Some("test"),
            vec![func(
                "test_fn",
                vec![param("x", HirType::Primitive(PrimitiveType::I32))],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::If {
                    condition: binop(BinOp::Gt, ident("x"), int_lit(0)),
                    then_block: block(vec![stmt(HirStmtKind::Return {
                        value: int_lit(1),
                    })]),
                    else_block: Some(block(vec![stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    })])),
                })]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("icmp sgt"), "should have comparison");
        assert!(ir.contains("br i1"), "should have conditional branch");
        assert!(ir.contains("then."), "should have then label");
        assert!(ir.contains("else."), "should have else label");
        assert!(ir.contains("merge."), "should have merge label");
    }

    // -----------------------------------------------------------------------
    // For loop tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_for_loop() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "sum".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(0),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::For {
                        var: "i".to_string(),
                        var_span: span(),
                        var_type: HirType::Primitive(PrimitiveType::I32),
                        iterable: call("range", vec![int_lit(0), int_lit(10)]),
                        body: block(vec![stmt(HirStmtKind::Assign {
                            target: ident("sum"),
                            value: binop(BinOp::Add, ident("sum"), ident("i")),
                        })]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("sum"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("%i = alloca i32"), "should alloca loop var");
        assert!(ir.contains("icmp slt"), "should have loop comparison");
        assert!(ir.contains("for.cond."), "should have for.cond label");
        assert!(ir.contains("for.body."), "should have for.body label");
        assert!(ir.contains("for.end."), "should have for.end label");
        assert!(ir.contains("add i32"), "should have increment");
        assert!(
            ir.contains("br label %for.cond."),
            "should branch back to cond"
        );
    }

    // -----------------------------------------------------------------------
    // Function call tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_function_call() {
        let m = module(
            Some("test"),
            vec![
                func(
                    "fib",
                    vec![param("n", HirType::Primitive(PrimitiveType::I32))],
                    HirType::Primitive(PrimitiveType::I64),
                    block(vec![stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    })]),
                ),
                func(
                    "main",
                    vec![],
                    HirType::Primitive(PrimitiveType::I32),
                    block(vec![
                        stmt(HirStmtKind::Let {
                            name: "result".to_string(),
                            name_span: span(),
                            ty: HirType::Primitive(PrimitiveType::I64),
                            value: call("fib", vec![int_lit(40)]),
                            mutable: false,
                        }),
                        stmt(HirStmtKind::Return {
                            value: int_lit(0),
                        }),
                    ]),
                ),
            ],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("@fib(i32 40)"), "should call fib");
    }

    // -----------------------------------------------------------------------
    // Built-in function tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_print_string() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Expr {
                        expr: call("print", vec![str_lit("hello")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("c\"hello\\00\""), "should have string constant");
        assert!(ir.contains("call i32 @puts"), "should call puts");
        assert!(ir.contains("declare i32 @puts(ptr)"), "should declare puts");
    }

    #[test]
    fn test_print_i64() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: int_lit(42),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("print_i64", vec![ident("x")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("@.fmt.i64"), "should have format string");
        assert!(
            ir.contains("call i32 (ptr, ...) @printf"),
            "should call printf"
        );
        assert!(
            ir.contains("declare i32 @printf(ptr, ...)"),
            "should declare printf"
        );
    }

    #[test]
    fn test_widen() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(5),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: call("widen", vec![ident("x")]),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("sext i32"), "should have sext");
        assert!(ir.contains("to i64"), "should extend to i64");
    }

    // -----------------------------------------------------------------------
    // Boolean tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bool_literal() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::Bool),
                        value: bool_lit(true),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::Bool),
                        value: bool_lit(false),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("store i1 1, ptr %x"), "true should be i1 1");
        assert!(ir.contains("store i1 0, ptr %y"), "false should be i1 0");
    }

    // -----------------------------------------------------------------------
    // Unary op tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_unary_neg() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(5),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: unaryop(UnaryOp::Neg, ident("x")),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("sub i32 0,"), "neg should be sub 0, x");
    }

    #[test]
    fn test_unary_not() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::Bool),
                        value: bool_lit(true),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::Bool),
                        value: unaryop(UnaryOp::Not, ident("x")),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("xor i1"), "not should be xor i1");
    }

    // -----------------------------------------------------------------------
    // Float tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_float_operations() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "a".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: float_lit(1.5),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "b".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: float_lit(2.5),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "c".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: binop(BinOp::Add, ident("a"), ident("b")),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("fadd double"), "should use fadd for f64 add");
    }

    // -----------------------------------------------------------------------
    // Error tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_unsupported_type_error() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Tensor {
                            element: Box::new(HirType::Primitive(PrimitiveType::F32)),
                            dims: vec![],
                        },
                        value: int_lit(0),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let result = codegen(&m);
        assert!(result.is_err(), "should error on unsupported type");
    }

    // -----------------------------------------------------------------------
    // Integration tests: full programs
    // -----------------------------------------------------------------------

    #[test]
    fn test_hello() {
        let source = std::fs::read_to_string("../../tests/samples/hello.axm")
            .expect("should read hello.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "hello.axm should parse without errors"
        );
        let hir_module =
            axiom_hir::lower(&parse_result.module).expect("hello.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("hello.axm should codegen");

        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        assert!(
            ir.contains("Hello from AXIOM!"),
            "should contain string: {ir}"
        );
        assert!(ir.contains("call i32 @puts"), "should call puts: {ir}");
        assert!(
            ir.contains("declare i32 @puts(ptr)"),
            "should declare puts: {ir}"
        );
        assert!(ir.contains("ret i32 0"), "should return 0: {ir}");
    }

    #[test]
    fn test_fibonacci() {
        let source = std::fs::read_to_string("../../tests/samples/fibonacci.axm")
            .expect("should read fibonacci.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "fibonacci.axm should parse without errors"
        );
        let hir_module =
            axiom_hir::lower(&parse_result.module).expect("fibonacci.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("fibonacci.axm should codegen");

        assert!(
            ir.contains("@fib(i32"),
            "should define fib: {ir}"
        );
        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        assert!(ir.contains("sext i32"), "should have sext (widen): {ir}");
        assert!(
            ir.contains("icmp slt i32"),
            "should have icmp slt (range loop): {ir}"
        );
        assert!(ir.contains("@fib("), "should call fib: {ir}");
        assert!(
            ir.contains("call i32 (ptr, ...) @printf"),
            "should call printf: {ir}"
        );
        assert!(
            ir.contains("declare i32 @printf(ptr, ...)"),
            "should declare printf: {ir}"
        );
        assert!(ir.contains("ret i32 0"), "should return 0 from main: {ir}");
        assert!(ir.contains("ret i64"), "should return i64 from fib: {ir}");
    }

    #[test]
    fn test_empty_module() {
        let m = module(Some("empty"), vec![]);
        let ir = codegen(&m).expect("empty module should codegen");
        assert!(ir.contains("; ModuleID = 'empty'"), "should have module ID");
        assert!(
            ir.contains("source_filename = \"empty\""),
            "should have source_filename"
        );
        assert!(!ir.contains("define "), "should have no function defs");
    }

    #[test]
    fn test_multiple_functions() {
        let m = module(
            Some("test"),
            vec![
                func(
                    "helper",
                    vec![param("x", HirType::Primitive(PrimitiveType::I32))],
                    HirType::Primitive(PrimitiveType::I32),
                    block(vec![stmt(HirStmtKind::Return {
                        value: ident("x"),
                    })]),
                ),
                func(
                    "main",
                    vec![],
                    HirType::Primitive(PrimitiveType::I32),
                    block(vec![stmt(HirStmtKind::Return {
                        value: call("helper", vec![int_lit(42)]),
                    })]),
                ),
            ],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("@helper("), "should define helper");
        assert!(ir.contains("define i32 @main()"), "should define main");
        assert!(
            ir.contains("@helper(i32 42)"),
            "should call helper"
        );
    }

    #[test]
    fn test_while_loop() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(0),
                        mutable: true,
                    }),
                    stmt(HirStmtKind::While {
                        condition: binop(BinOp::Lt, ident("x"), int_lit(10)),
                        body: block(vec![stmt(HirStmtKind::Assign {
                            target: ident("x"),
                            value: binop(BinOp::Add, ident("x"), int_lit(1)),
                        })]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: ident("x"),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(ir.contains("while.cond."), "should have while.cond label");
        assert!(ir.contains("while.body."), "should have while.body label");
        assert!(ir.contains("while.end."), "should have while.end label");
    }

    #[test]
    fn test_nested_expressions() {
        // a + b * c should emit mul first, then add.
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "a".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(1),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "b".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(2),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "c".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(3),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "result".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        // a + (b * c)  -- parser already handles precedence in AST
                        value: binop(
                            BinOp::Add,
                            ident("a"),
                            binop(BinOp::Mul, ident("b"), ident("c")),
                        ),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        // Mul should appear before add in the IR.
        let mul_pos = ir.find("mul i32").expect("should have mul");
        let add_pos = ir
            .rfind("add i32")
            .expect("should have add");
        assert!(
            mul_pos < add_pos,
            "mul should come before add in the IR"
        );
    }

    #[test]
    fn test_string_escaping() {
        assert_eq!(escape_llvm_string("hello"), "hello");
        assert_eq!(escape_llvm_string("hello\nworld"), "hello\\0Aworld");
        assert_eq!(escape_llvm_string("tab\there"), "tab\\09here");
        assert_eq!(escape_llvm_string("quote\"here"), "quote\\22here");
        assert_eq!(escape_llvm_string("back\\slash"), "back\\5Cslash");
    }

    #[test]
    fn test_float_formatting() {
        assert_eq!(format_float(0.0), "0.0");
        assert_eq!(format_float(1.5), "1.5");
        assert_eq!(format_float(42.0), "42.0");
        assert_eq!(format_float(-3.14), "-3.14");
    }

    // -----------------------------------------------------------------------
    // Standard library built-in tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_math_builtins() {
        // Test abs, min, max, sqrt, pow
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // abs(x: i32) -> i32
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(-5),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "a".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: call("abs", vec![ident("x")]),
                        mutable: false,
                    }),
                    // abs_f64(x: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "fx".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: float_lit(-3.14),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "fa".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: call("abs_f64", vec![ident("fx")]),
                        mutable: false,
                    }),
                    // min(a: i32, b: i32) -> i32
                    stmt(HirStmtKind::Let {
                        name: "mn".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: call("min", vec![int_lit(3), int_lit(7)]),
                        mutable: false,
                    }),
                    // max(a: i32, b: i32) -> i32
                    stmt(HirStmtKind::Let {
                        name: "mx".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: call("max", vec![int_lit(3), int_lit(7)]),
                        mutable: false,
                    }),
                    // min_f64(a: f64, b: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "fmn".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: call("min_f64", vec![float_lit(1.5), float_lit(2.5)]),
                        mutable: false,
                    }),
                    // max_f64(a: f64, b: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "fmx".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: call("max_f64", vec![float_lit(1.5), float_lit(2.5)]),
                        mutable: false,
                    }),
                    // sqrt(x: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "sq".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: call("sqrt", vec![float_lit(4.0)]),
                        mutable: false,
                    }),
                    // pow(base: f64, exp: f64) -> f64
                    stmt(HirStmtKind::Let {
                        name: "pw".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: call("pow", vec![float_lit(2.0), float_lit(3.0)]),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // abs: @llvm.abs.i32
        assert!(
            ir.contains("call i32 @llvm.abs.i32(i32"),
            "should call llvm.abs.i32: {ir}"
        );
        assert!(
            ir.contains("declare i32 @llvm.abs.i32(i32, i1)"),
            "should declare llvm.abs.i32: {ir}"
        );

        // abs_f64: @llvm.fabs.f64
        assert!(
            ir.contains("call double @llvm.fabs.f64(double"),
            "should call llvm.fabs.f64: {ir}"
        );
        assert!(
            ir.contains("declare double @llvm.fabs.f64(double)"),
            "should declare llvm.fabs.f64: {ir}"
        );

        // min: icmp slt + select
        assert!(
            ir.contains("icmp slt i32"),
            "min should use icmp slt: {ir}"
        );
        assert!(
            ir.contains("select i1"),
            "min/max should use select: {ir}"
        );

        // max: icmp sgt + select
        assert!(
            ir.contains("icmp sgt i32"),
            "max should use icmp sgt: {ir}"
        );

        // min_f64: fcmp olt + select
        assert!(
            ir.contains("fcmp olt double"),
            "min_f64 should use fcmp olt: {ir}"
        );

        // max_f64: fcmp ogt + select
        assert!(
            ir.contains("fcmp ogt double"),
            "max_f64 should use fcmp ogt: {ir}"
        );

        // sqrt: @llvm.sqrt.f64
        assert!(
            ir.contains("call double @llvm.sqrt.f64(double"),
            "should call llvm.sqrt.f64: {ir}"
        );
        assert!(
            ir.contains("declare double @llvm.sqrt.f64(double)"),
            "should declare llvm.sqrt.f64: {ir}"
        );

        // pow: @llvm.pow.f64
        assert!(
            ir.contains("call double @llvm.pow.f64(double"),
            "should call llvm.pow.f64: {ir}"
        );
        assert!(
            ir.contains("declare double @llvm.pow.f64(double, double)"),
            "should declare llvm.pow.f64: {ir}"
        );
    }

    #[test]
    fn test_conversion_builtins() {
        // Test narrow, truncate
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // narrow(x: i64) -> i32
                    stmt(HirStmtKind::Let {
                        name: "wide".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: int_lit(42),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "narrow_val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: call("narrow", vec![ident("wide")]),
                        mutable: false,
                    }),
                    // truncate(x: f64) -> i32
                    stmt(HirStmtKind::Let {
                        name: "fval".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: float_lit(3.14),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "trunc_val".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: call("truncate", vec![ident("fval")]),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // narrow: trunc i64 to i32
        assert!(
            ir.contains("trunc i64"),
            "narrow should use trunc: {ir}"
        );
        assert!(
            ir.contains("to i32"),
            "narrow should truncate to i32: {ir}"
        );

        // truncate: fptosi double to i32
        assert!(
            ir.contains("fptosi double"),
            "truncate should use fptosi: {ir}"
        );
        assert!(
            ir.contains("to i32"),
            "truncate should convert to i32: {ir}"
        );
    }

    #[test]
    fn test_io_builtins() {
        // Test print_f64, print_i32
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    // print_i32
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(42),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("print_i32", vec![ident("x")]),
                    }),
                    // print_f64
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: float_lit(3.14),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Expr {
                        expr: call("print_f64", vec![ident("y")]),
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");

        // print_i32: format string + printf call
        assert!(
            ir.contains("@.fmt.i32"),
            "should have i32 format string: {ir}"
        );
        assert!(
            ir.contains("call i32 (ptr, ...) @printf(ptr @.fmt.i32, i32"),
            "should call printf with i32 format: {ir}"
        );

        // print_f64: format string + printf call
        assert!(
            ir.contains("@.fmt.f64"),
            "should have f64 format string: {ir}"
        );
        assert!(
            ir.contains("call i32 (ptr, ...) @printf(ptr @.fmt.f64, double"),
            "should call printf with f64 format: {ir}"
        );

        // Should declare printf
        assert!(
            ir.contains("declare i32 @printf(ptr, ...)"),
            "should declare printf: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // FFI / Extern function tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_extern_decl() {
        let ef = HirExternFunction {
            id: nid(0),
            name: "sin".to_string(),
            name_span: span(),
            annotations: vec![],
            params: vec![param("x", HirType::Primitive(PrimitiveType::F64))],
            return_type: HirType::Primitive(PrimitiveType::F64),
            span: span(),
        };

        let m = module_with_externs(Some("test"), vec![], vec![ef]);
        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("declare double @sin(double)"),
            "should declare extern sin: {ir}"
        );
    }

    #[test]
    fn test_extern_call() {
        let ef = HirExternFunction {
            id: nid(0),
            name: "clock".to_string(),
            name_span: span(),
            annotations: vec![],
            params: vec![],
            return_type: HirType::Primitive(PrimitiveType::I64),
            span: span(),
        };

        let main_func = func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "t".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: call("clock", vec![]),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: int_lit(0),
                }),
            ]),
        );

        let m = module_with_externs(Some("test"), vec![main_func], vec![ef]);
        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("declare i64 @clock()"),
            "should declare extern clock: {ir}"
        );
        assert!(
            ir.contains("call i64 @clock()"),
            "should call clock: {ir}"
        );
    }

    #[test]
    fn test_export_function() {
        let export_ann = HirAnnotation {
            kind: HirAnnotationKind::Export,
            span: SPAN_DUMMY,
        };

        let add_func = HirFunction {
            id: nid(0),
            name: "add".to_string(),
            name_span: span(),
            annotations: vec![export_ann],
            params: vec![
                param("a", HirType::Primitive(PrimitiveType::I32)),
                param("b", HirType::Primitive(PrimitiveType::I32)),
            ],
            return_type: HirType::Primitive(PrimitiveType::I32),
            body: block(vec![stmt(HirStmtKind::Return {
                value: binop(BinOp::Add, ident("a"), ident("b")),
            })]),
            span: span(),
        };

        let m = module(Some("test"), vec![add_func]);
        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("define dso_local i32 @add(i32 %a, i32 %b)"),
            "should define exported function with dso_local: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Self-hosting bootstrap tests (M5.1)
    // -----------------------------------------------------------------------

    /// Integration test: self-hosting lexer example compiles through the full
    /// pipeline (parse -> HIR -> LLVM IR).
    #[test]
    fn test_self_host_lexer() {
        let source = std::fs::read_to_string("../../examples/self_host/lexer.axm")
            .expect("should read lexer.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "lexer.axm should parse without errors: {:?}",
            parse_result.errors
        );
        let hir_module =
            axiom_hir::lower(&parse_result.module).expect("lexer.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("lexer.axm should codegen");

        // Verify the classify_char function is emitted.
        assert!(
            ir.contains("@classify_char(i32"),
            "should define classify_char: {ir}"
        );
        // Verify main is emitted.
        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        // Verify classify_char is called with ASCII character codes.
        assert!(
            ir.contains("@classify_char(i32 49)"),
            "should call classify_char with '1' (49): {ir}"
        );
        assert!(
            ir.contains("@classify_char(i32 43)"),
            "should call classify_char with '+' (43): {ir}"
        );
        // Verify printf is used for output.
        assert!(
            ir.contains("call i32 (ptr, ...) @printf"),
            "should call printf for output: {ir}"
        );
        // Verify the `and` logic is compiled (digit range check: c >= 48 and c <= 57).
        assert!(
            ir.contains("and i1"),
            "should have logical AND for range check: {ir}"
        );
        assert!(ir.contains("ret i32 0"), "main should return 0: {ir}");
    }

    /// Integration test: self-hosting token counter compiles through the full
    /// pipeline (parse -> HIR -> LLVM IR).
    #[test]
    fn test_self_host_token_counter() {
        let source = std::fs::read_to_string("../../examples/self_host/token_counter.axm")
            .expect("should read token_counter.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "token_counter.axm should parse without errors: {:?}",
            parse_result.errors
        );
        let hir_module = axiom_hir::lower(&parse_result.module)
            .expect("token_counter.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("token_counter.axm should codegen");

        // Verify classify_char function.
        assert!(
            ir.contains("@classify_char(i32"),
            "should define classify_char: {ir}"
        );
        // Verify main with mutable counters.
        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        // Verify alloca for mutable counter variables.
        assert!(
            ir.contains("%numbers = alloca i32"),
            "should have numbers counter: {ir}"
        );
        assert!(
            ir.contains("%operators = alloca i32"),
            "should have operators counter: {ir}"
        );
        // Verify if/else branches for counting logic.
        assert!(ir.contains("then."), "should have then branches: {ir}");
        assert!(ir.contains("else."), "should have else branches: {ir}");
        // Verify printf calls for output.
        assert!(
            ir.contains("call i32 (ptr, ...) @printf"),
            "should call printf: {ir}"
        );
        assert!(ir.contains("ret i32 0"), "main should return 0: {ir}");
    }

    // -----------------------------------------------------------------------
    // to_f64 / to_f64_i64 conversion builtin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_to_f64() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I32),
                        value: int_lit(42),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: call("to_f64", vec![ident("x")]),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("sitofp i32"),
            "should have sitofp i32: {ir}"
        );
        assert!(
            ir.contains("to double"),
            "should convert to double: {ir}"
        );
    }

    #[test]
    fn test_to_f64_i64() {
        let m = module(
            Some("test"),
            vec![func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![
                    stmt(HirStmtKind::Let {
                        name: "x".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::I64),
                        value: int_lit(100),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Let {
                        name: "y".to_string(),
                        name_span: span(),
                        ty: HirType::Primitive(PrimitiveType::F64),
                        value: call("to_f64_i64", vec![ident("x")]),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: int_lit(0),
                    }),
                ]),
            )],
        );

        let ir = codegen(&m).expect("codegen should succeed");
        assert!(
            ir.contains("sitofp i64"),
            "should have sitofp i64: {ir}"
        );
        assert!(
            ir.contains("to double"),
            "should convert to double: {ir}"
        );
    }

    // -----------------------------------------------------------------------
    // Benchmark program integration tests
    // -----------------------------------------------------------------------

    /// Integration test: recursive fibonacci benchmark compiles through the
    /// full pipeline (parse -> HIR -> LLVM IR).
    #[test]
    fn test_benchmark_fib() {
        let source = std::fs::read_to_string("../../benchmarks/fib/fib.axm")
            .expect("should read fib.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "fib.axm should parse without errors: {:?}",
            parse_result.errors
        );
        let hir_module = axiom_hir::lower(&parse_result.module)
            .expect("fib.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("fib.axm should codegen");

        // Verify recursive fib function with i64 params.
        assert!(
            ir.contains("@fib(i64 %n)"),
            "should define fib with i64 param: {ir}"
        );
        // Verify recursive calls.
        assert!(
            ir.contains("@fib(i64"),
            "should have recursive call: {ir}"
        );
        // Verify i64 comparison.
        assert!(
            ir.contains("icmp sle i64"),
            "should have i64 comparison: {ir}"
        );
        // Verify i64 subtraction.
        assert!(
            ir.contains("sub i64"),
            "should have i64 subtraction: {ir}"
        );
        // Verify i64 addition.
        assert!(
            ir.contains("add i64"),
            "should have i64 addition: {ir}"
        );
        // Verify main calls fib(47).
        assert!(
            ir.contains("@fib(i64 47)"),
            "should call fib(47): {ir}"
        );
    }

    /// Integration test: Leibniz Pi benchmark compiles through the full
    /// pipeline (parse -> HIR -> LLVM IR).
    #[test]
    fn test_benchmark_leibniz() {
        let source = std::fs::read_to_string("../../benchmarks/leibniz/leibniz.axm")
            .expect("should read leibniz.axm");
        let parse_result = axiom_parser::parse(&source);
        assert!(
            !parse_result.has_errors(),
            "leibniz.axm should parse without errors: {:?}",
            parse_result.errors
        );
        let hir_module = axiom_hir::lower(&parse_result.module)
            .expect("leibniz.axm should lower to HIR");
        let ir = codegen(&hir_module).expect("leibniz.axm should codegen");

        // Verify main function.
        assert!(
            ir.contains("define i32 @main()"),
            "should define main: {ir}"
        );
        // Verify sitofp for to_f64 builtin.
        assert!(
            ir.contains("sitofp i32"),
            "should have sitofp for to_f64: {ir}"
        );
        // Verify float division for 1.0/d.
        assert!(
            ir.contains("fdiv double"),
            "should have float division: {ir}"
        );
        // Verify float subtraction and addition.
        assert!(
            ir.contains("fsub double"),
            "should have float subtraction: {ir}"
        );
        assert!(
            ir.contains("fadd double"),
            "should have float addition: {ir}"
        );
        // Verify for loop structure.
        assert!(
            ir.contains("for.cond."),
            "should have for loop condition: {ir}"
        );
        // Verify printf for f64 output.
        assert!(
            ir.contains("@.fmt.f64"),
            "should have f64 format string: {ir}"
        );
    }
}
