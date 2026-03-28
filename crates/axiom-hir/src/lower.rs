//! AST-to-HIR lowering with annotation validation and type validation.
//!
//! The lowering pass walks the AST, produces HIR nodes, validates annotation
//! placement, and checks that type references refer to known primitives or
//! user-defined structs. It collects all errors rather than stopping at the first.
//!
//! # Usage
//!
//! ```ignore
//! let parse_result = axiom_parser::parse(source);
//! let hir_module = axiom_hir::lower(&parse_result.module)?;
//! ```

use std::collections::{HashMap, HashSet};

use axiom_lexer::Span;
use axiom_parser::ast;

use crate::error::{span_to_source_span, AnnotationTarget, LowerError};
use crate::hir::*;

/// Lower an AST module to HIR.
///
/// Validates annotation targets and type references. Returns all collected
/// errors on failure. The lowering uses a two-pass approach:
/// 1. Collect all user-defined type names (structs and type aliases)
/// 2. Lower all items with complete type knowledge
///
/// # Errors
///
/// Returns `Err(Vec<LowerError>)` if any annotation placement is invalid,
/// any type reference is unresolved, or any duplicate definitions are found.
pub fn lower(module: &ast::Module) -> Result<HirModule, Vec<LowerError>> {
    let mut ctx = LoweringContext::new();
    ctx.collect_user_defined_types(module);
    let hir = ctx.lower_module(module);

    // Check @strict enforcement: every non-main function must have @intent + contract.
    let is_strict = hir
        .annotations
        .iter()
        .any(|a| matches!(a.kind, HirAnnotationKind::Strict));
    if is_strict {
        for func in &hir.functions {
            if func.name == "main" {
                continue;
            }
            let has_intent = func
                .annotations
                .iter()
                .any(|a| matches!(a.kind, HirAnnotationKind::Intent(_)));
            let has_contract = func.annotations.iter().any(|a| {
                matches!(
                    a.kind,
                    HirAnnotationKind::Precondition(_)
                        | HirAnnotationKind::Postcondition(_)
                        | HirAnnotationKind::Requires(_)
                        | HirAnnotationKind::Ensures(_)
                )
            });
            if !has_intent || !has_contract {
                let missing = match (!has_intent, !has_contract) {
                    (true, true) => "@intent and @precondition/@postcondition (or @requires/@ensures)",
                    (true, false) => "@intent",
                    (false, true) => "@precondition/@postcondition (or @requires/@ensures)",
                    _ => unreachable!(),
                };
                ctx.errors.push(LowerError::StrictMissingAnnotations {
                    name: func.name.clone(),
                    missing: missing.to_string(),
                    span: span_to_source_span(func.name_span),
                });
            }

            // Check for vacuous contracts: if ALL contracts are trivially `true`,
            // warn the developer that the contracts are meaningless.
            let contract_annotations: Vec<&HirAnnotation> = func
                .annotations
                .iter()
                .filter(|a| {
                    matches!(
                        a.kind,
                        HirAnnotationKind::Precondition(_) | HirAnnotationKind::Postcondition(_)
                    )
                })
                .collect();
            if !contract_annotations.is_empty() {
                let all_trivial = contract_annotations.iter().all(|a| match &a.kind {
                    HirAnnotationKind::Precondition(expr)
                    | HirAnnotationKind::Postcondition(expr) => {
                        matches!(expr.kind, HirExprKind::BoolLiteral { value: true })
                    }
                    _ => false,
                });
                if all_trivial {
                    eprintln!(
                        "warning: function `{}` has only trivial contracts (@precondition(true)) \
                         -- consider adding meaningful constraints",
                        func.name
                    );
                }
            }
        }
    }

    // Even without @strict, emit warnings for functions missing @intent.
    // This nudges AI agents toward annotating without breaking existing code.
    if !is_strict {
        for func in &hir.functions {
            if func.name == "main" {
                continue;
            }
            let has_intent = func
                .annotations
                .iter()
                .any(|a| matches!(a.kind, HirAnnotationKind::Intent(_)));
            if !has_intent {
                eprintln!(
                    "warning: function `{}` has no @intent annotation (add @strict to make this an error)",
                    func.name
                );
            }
        }
    }

    if ctx.errors.is_empty() {
        Ok(hir)
    } else {
        Err(ctx.errors)
    }
}

/// Internal context for the lowering pass.
struct LoweringContext {
    /// Generates unique IDs for HIR nodes.
    id_gen: NodeIdGen,
    /// Collected errors.
    errors: Vec<LowerError>,
    /// Set of known type names (primitives + user-defined structs + type aliases).
    known_types: HashSet<String>,
    /// Map from function name to the span of the first definition (for duplicate detection).
    seen_functions: HashMap<String, Span>,
    /// Map from struct name to the span of the first definition (for duplicate detection).
    seen_structs: HashMap<String, Span>,
    /// Map from type alias name to the span of the first definition (for duplicate detection).
    seen_type_aliases: HashMap<String, Span>,
}

impl LoweringContext {
    /// Create a new lowering context with all primitive type names pre-registered.
    fn new() -> Self {
        let mut known_types = HashSet::new();
        for name in PRIMITIVE_NAMES {
            known_types.insert((*name).to_string());
        }
        Self {
            id_gen: NodeIdGen::new(),
            errors: Vec::new(),
            known_types,
            seen_functions: HashMap::new(),
            seen_structs: HashMap::new(),
            seen_type_aliases: HashMap::new(),
        }
    }

    /// First pass: scan all items to collect struct names and type alias names.
    fn collect_user_defined_types(&mut self, module: &ast::Module) {
        for item in &module.items {
            match &item.node {
                ast::Item::Struct(s) => {
                    self.known_types.insert(s.name.node.clone());
                }
                ast::Item::TypeAlias(ta) => {
                    self.known_types.insert(ta.name.node.clone());
                }
                _ => {}
            }
        }
    }

    /// Lower the entire module (second pass).
    fn lower_module(&mut self, module: &ast::Module) -> HirModule {
        // Lower module-level annotations
        let annotations: Vec<HirAnnotation> = module
            .annotations
            .iter()
            .map(|a| self.lower_annotation(a))
            .collect();

        // Validate module-level annotations
        self.validate_annotations(&annotations, AnnotationTarget::Module);

        // Extract module name from @module annotation
        let mut name = None;
        let mut seen_module_annotation = false;
        for ann in &annotations {
            if let HirAnnotationKind::Module(ref n) = ann.kind {
                if seen_module_annotation {
                    self.errors.push(LowerError::DuplicateModuleAnnotation {
                        span: span_to_source_span(ann.span),
                    });
                } else {
                    name = Some(n.clone());
                    seen_module_annotation = true;
                }
            }
        }

        // Also check if module name comes from Module.name field
        if let Some(ref spanned_name) = module.name {
            if name.is_none() {
                name = Some(spanned_name.node.clone());
            }
        }

        let mut functions = Vec::new();
        let mut extern_functions = Vec::new();
        let mut structs = Vec::new();
        let mut type_aliases = Vec::new();
        let mut imports = Vec::new();

        // Determine the current platform for @cfg filtering.
        let current_os = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "unknown"
        };

        for item in &module.items {
            match &item.node {
                ast::Item::Function(f) => {
                    // Skip functions with @cfg that doesn't match the current platform.
                    if !cfg_matches(&f.annotations, current_os) {
                        continue;
                    }
                    let hir_func = self.lower_function(f, item.span);
                    functions.push(hir_func);
                }
                ast::Item::ExternFunction(ef) => {
                    // Skip extern functions with @cfg that doesn't match the current platform.
                    if !cfg_matches(&ef.annotations, current_os) {
                        continue;
                    }
                    let hir_extern = self.lower_extern_function(ef, item.span);
                    extern_functions.push(hir_extern);
                }
                ast::Item::Struct(s) => {
                    let hir_struct = self.lower_struct(s, item.span);
                    structs.push(hir_struct);
                }
                ast::Item::TypeAlias(ta) => {
                    let hir_ta = self.lower_type_alias(ta, item.span);
                    type_aliases.push(hir_ta);
                }
                ast::Item::Import(imp) => {
                    let hir_import = self.lower_import(imp, item.span);
                    imports.push(hir_import);
                }
            }
        }

        HirModule {
            name,
            annotations,
            functions,
            extern_functions,
            structs,
            type_aliases,
            imports,
        }
    }

    /// Lower a function definition.
    fn lower_function(&mut self, func: &ast::Function, span: Span) -> HirFunction {
        let id = self.id_gen.next_id();

        // Check for duplicate function names
        if let Some(&first_span) = self.seen_functions.get(&func.name.node) {
            self.errors.push(LowerError::DuplicateDefinition {
                name: func.name.node.clone(),
                kind: "function".to_string(),
                first_span: span_to_source_span(first_span),
                second_span: span_to_source_span(func.name.span),
            });
        } else {
            self.seen_functions
                .insert(func.name.node.clone(), func.name.span);
        }

        let annotations: Vec<HirAnnotation> = func
            .annotations
            .iter()
            .map(|a| self.lower_annotation(a))
            .collect();
        self.validate_annotations(&annotations, AnnotationTarget::Function);

        let params: Vec<HirParam> = func.params.iter().map(|p| self.lower_param(p)).collect();

        let return_type = self.lower_type(&func.return_type, func.name.span);

        let body = self.lower_block(&func.body, span);

        HirFunction {
            id,
            name: func.name.node.clone(),
            name_span: func.name.span,
            annotations,
            params,
            return_type,
            body,
            span,
        }
    }

    /// Lower an extern function declaration.
    fn lower_extern_function(
        &mut self,
        ef: &ast::ExternFunction,
        span: Span,
    ) -> HirExternFunction {
        let id = self.id_gen.next_id();

        // Check for duplicate function names (extern functions share namespace with regular fns)
        if let Some(&first_span) = self.seen_functions.get(&ef.name.node) {
            self.errors.push(LowerError::DuplicateDefinition {
                name: ef.name.node.clone(),
                kind: "function".to_string(),
                first_span: span_to_source_span(first_span),
                second_span: span_to_source_span(ef.name.span),
            });
        } else {
            self.seen_functions
                .insert(ef.name.node.clone(), ef.name.span);
        }

        let annotations: Vec<HirAnnotation> = ef
            .annotations
            .iter()
            .map(|a| self.lower_annotation(a))
            .collect();
        self.validate_annotations(&annotations, AnnotationTarget::Function);

        let params: Vec<HirParam> = ef.params.iter().map(|p| self.lower_param(p)).collect();

        let return_type = self.lower_type(&ef.return_type, ef.name.span);

        // Map calling convention: default to "C" if not specified.
        let convention = ef.convention.clone().unwrap_or_else(|| "C".to_string());

        HirExternFunction {
            id,
            name: ef.name.node.clone(),
            name_span: ef.name.span,
            annotations,
            params,
            return_type,
            span,
            convention,
            is_variadic: ef.is_variadic,
        }
    }

    /// Lower a function parameter.
    fn lower_param(&mut self, param: &ast::Param) -> HirParam {
        let id = self.id_gen.next_id();

        let annotations: Vec<HirAnnotation> = param
            .annotations
            .iter()
            .map(|a| self.lower_annotation(a))
            .collect();
        self.validate_annotations(&annotations, AnnotationTarget::Param);

        let ty = self.lower_type(&param.ty, param.name.span);

        HirParam {
            id,
            name: param.name.node.clone(),
            name_span: param.name.span,
            ty,
            annotations,
        }
    }

    /// Lower a struct definition.
    fn lower_struct(&mut self, s: &ast::StructDef, span: Span) -> HirStruct {
        let id = self.id_gen.next_id();

        // Check for duplicate struct names
        if let Some(&first_span) = self.seen_structs.get(&s.name.node) {
            self.errors.push(LowerError::DuplicateDefinition {
                name: s.name.node.clone(),
                kind: "struct".to_string(),
                first_span: span_to_source_span(first_span),
                second_span: span_to_source_span(s.name.span),
            });
        } else {
            self.seen_structs.insert(s.name.node.clone(), s.name.span);
        }

        let annotations: Vec<HirAnnotation> = s
            .annotations
            .iter()
            .map(|a| self.lower_annotation(a))
            .collect();
        self.validate_annotations(&annotations, AnnotationTarget::StructDef);

        let fields: Vec<HirStructField> =
            s.fields.iter().map(|f| self.lower_struct_field(f)).collect();

        HirStruct {
            id,
            name: s.name.node.clone(),
            name_span: s.name.span,
            annotations,
            fields,
            span,
        }
    }

    /// Lower a struct field.
    fn lower_struct_field(&mut self, field: &ast::StructField) -> HirStructField {
        let id = self.id_gen.next_id();

        let annotations: Vec<HirAnnotation> = field
            .annotations
            .iter()
            .map(|a| self.lower_annotation(a))
            .collect();
        self.validate_annotations(&annotations, AnnotationTarget::StructField);

        let ty = self.lower_type(&field.ty, field.name.span);

        HirStructField {
            id,
            name: field.name.node.clone(),
            name_span: field.name.span,
            ty,
            annotations,
        }
    }

    /// Lower a type alias.
    fn lower_type_alias(&mut self, ta: &ast::TypeAlias, span: Span) -> HirTypeAlias {
        let id = self.id_gen.next_id();

        // Check for duplicate type alias names
        if let Some(&first_span) = self.seen_type_aliases.get(&ta.name.node) {
            self.errors.push(LowerError::DuplicateDefinition {
                name: ta.name.node.clone(),
                kind: "type alias".to_string(),
                first_span: span_to_source_span(first_span),
                second_span: span_to_source_span(ta.name.span),
            });
        } else {
            self.seen_type_aliases
                .insert(ta.name.node.clone(), ta.name.span);
        }

        let ty = self.lower_type(&ta.ty, ta.name.span);

        HirTypeAlias {
            id,
            name: ta.name.node.clone(),
            name_span: ta.name.span,
            ty,
            span,
        }
    }

    /// Lower an import declaration.
    fn lower_import(&mut self, imp: &ast::ImportDecl, span: Span) -> HirImport {
        let id = self.id_gen.next_id();
        HirImport {
            id,
            path: imp.path.clone(),
            alias: imp.alias.clone(),
            span,
        }
    }

    /// Lower a block of statements.
    ///
    /// `@parallel_for` annotations that were placed before a for-loop in the source
    /// are injected into the for-loop's body block by the parser. The `lower_stmt`
    /// method extracts them from the body block and attaches them to the `HirStmt`.
    fn lower_block(&mut self, block: &ast::Block, outer_span: Span) -> HirBlock {
        let id = self.id_gen.next_id();

        // Filter out @parallel_for from block annotations: they belong on For statements,
        // not on blocks. The parser may inject them here as a transport mechanism.
        let annotations: Vec<HirAnnotation> = block
            .annotations
            .iter()
            .filter(|a| !matches!(&a.node, ast::Annotation::ParallelFor(_)))
            .map(|a| self.lower_annotation(a))
            .collect();
        self.validate_annotations(&annotations, AnnotationTarget::Block);

        let stmts: Vec<HirStmt> = block.stmts.iter().map(|s| self.lower_stmt(s)).collect();

        HirBlock {
            id,
            annotations,
            stmts,
            span: outer_span,
        }
    }

    /// Lower a statement.
    fn lower_stmt(&mut self, stmt: &ast::Spanned<ast::Stmt>) -> HirStmt {
        let id = self.id_gen.next_id();
        let span = stmt.span;
        let mut stmt_annotations: Vec<HirAnnotation> = Vec::new();

        let kind = match &stmt.node {
            ast::Stmt::Let {
                name,
                ty,
                value,
                mutable,
                annotations,
            } => {
                // Extract @lifetime annotations from the let statement and
                // attach them to the HirStmt's annotations.
                let let_anns: Vec<HirAnnotation> = annotations
                    .iter()
                    .map(|a| self.lower_annotation(a))
                    .collect();
                stmt_annotations.extend(let_anns);
                HirStmtKind::Let {
                    name: name.node.clone(),
                    name_span: name.span,
                    ty: self.lower_type(ty, name.span),
                    value: value.as_ref().map(|v| self.lower_expr(v, span)),
                    mutable: *mutable,
                }
            }
            ast::Stmt::Assign { target, value } => HirStmtKind::Assign {
                target: self.lower_expr(target, span),
                value: self.lower_expr(value, span),
            },
            ast::Stmt::Return(expr_opt) => HirStmtKind::Return {
                value: expr_opt.as_ref().map(|e| self.lower_expr(e, span)),
            },
            ast::Stmt::If {
                condition,
                then_block,
                else_block,
            } => HirStmtKind::If {
                condition: self.lower_expr(condition, span),
                then_block: self.lower_block(then_block, span),
                else_block: else_block
                    .as_ref()
                    .map(|b| self.lower_block(b, span)),
            },
            ast::Stmt::For {
                var,
                var_type,
                iterable,
                body,
            } => {
                // Extract @parallel_for annotations from the body block annotations
                // and save them to be attached to this For statement.
                let parallel_for_anns: Vec<HirAnnotation> = body
                    .annotations
                    .iter()
                    .filter(|a| matches!(&a.node, ast::Annotation::ParallelFor(_)))
                    .map(|a| self.lower_annotation(a))
                    .collect();
                stmt_annotations = parallel_for_anns;

                HirStmtKind::For {
                    var: var.node.clone(),
                    var_span: var.span,
                    var_type: self.lower_type(var_type, var.span),
                    iterable: self.lower_expr(iterable, span),
                    body: self.lower_block(body, span),
                }
            }
            ast::Stmt::While { condition, body } => HirStmtKind::While {
                condition: self.lower_expr(condition, span),
                body: self.lower_block(body, span),
            },
            ast::Stmt::Break => HirStmtKind::Break,
            ast::Stmt::Continue => HirStmtKind::Continue,
            ast::Stmt::Match { value, arms, default } => HirStmtKind::Match {
                value: self.lower_expr(value, span),
                arms: arms.iter().map(|(pat, block)| {
                    (self.lower_expr(pat, span), self.lower_block(block, span))
                }).collect(),
                default: default.as_ref().map(|b| self.lower_block(b, span)),
            },
            ast::Stmt::Expr(expr) => HirStmtKind::Expr {
                expr: self.lower_expr(expr, span),
            },
        };

        HirStmt {
            id,
            kind,
            span,
            annotations: stmt_annotations,
        }
    }

    /// Lower an expression.
    ///
    /// The `span` parameter is the span of the enclosing statement or a dummy,
    /// since `ast::Expr` does not carry its own span.
    fn lower_expr(&mut self, expr: &ast::Expr, span: Span) -> HirExpr {
        let id = self.id_gen.next_id();

        let kind = match expr {
            ast::Expr::IntLiteral(v) => HirExprKind::IntLiteral { value: *v },
            ast::Expr::FloatLiteral(v) => HirExprKind::FloatLiteral { value: *v },
            ast::Expr::StringLiteral(v) => HirExprKind::StringLiteral {
                value: v.clone(),
            },
            ast::Expr::BoolLiteral(v) => HirExprKind::BoolLiteral { value: *v },
            ast::Expr::Ident(name) => HirExprKind::Ident {
                name: name.clone(),
            },
            ast::Expr::OptHole(name) => HirExprKind::OptHole {
                name: name.clone(),
            },
            ast::Expr::BinaryOp { op, lhs, rhs } => HirExprKind::BinaryOp {
                op: *op,
                lhs: Box::new(self.lower_expr(lhs, SPAN_DUMMY)),
                rhs: Box::new(self.lower_expr(rhs, SPAN_DUMMY)),
            },
            ast::Expr::UnaryOp { op, operand } => HirExprKind::UnaryOp {
                op: *op,
                operand: Box::new(self.lower_expr(operand, SPAN_DUMMY)),
            },
            ast::Expr::Call { func, args } => HirExprKind::Call {
                func: Box::new(self.lower_expr(func, SPAN_DUMMY)),
                args: args
                    .iter()
                    .map(|a| self.lower_expr(a, SPAN_DUMMY))
                    .collect(),
            },
            ast::Expr::Index { expr, indices } => HirExprKind::Index {
                expr: Box::new(self.lower_expr(expr, SPAN_DUMMY)),
                indices: indices
                    .iter()
                    .map(|i| self.lower_expr(i, SPAN_DUMMY))
                    .collect(),
            },
            ast::Expr::FieldAccess { expr, field } => HirExprKind::FieldAccess {
                expr: Box::new(self.lower_expr(expr, SPAN_DUMMY)),
                field: field.clone(),
            },
            ast::Expr::MethodCall { expr, method, args } => HirExprKind::MethodCall {
                expr: Box::new(self.lower_expr(expr, SPAN_DUMMY)),
                method: method.clone(),
                args: args
                    .iter()
                    .map(|a| self.lower_expr(a, SPAN_DUMMY))
                    .collect(),
            },
            ast::Expr::ArrayZeros {
                element_type,
                size,
            } => {
                let elem_hir = self.lower_type(element_type, span);
                let sz = Self::extract_array_size(size);
                if sz.is_none() {
                    self.errors.push(LowerError::InvalidArraySize {
                        span: span_to_source_span(span),
                    });
                }
                HirExprKind::ArrayZeros {
                    element_type: elem_hir,
                    size: sz.unwrap_or(0),
                }
            }
            ast::Expr::StructLiteral { type_name, fields } => HirExprKind::StructLiteral {
                type_name: type_name.clone(),
                fields: fields
                    .iter()
                    .map(|(name, expr)| (name.clone(), self.lower_expr(expr, SPAN_DUMMY)))
                    .collect(),
            },
            ast::Expr::TupleLiteral { elements } => HirExprKind::TupleLiteral {
                elements: elements
                    .iter()
                    .map(|expr| self.lower_expr(expr, SPAN_DUMMY))
                    .collect(),
            },
        };

        HirExpr { id, kind, span }
    }

    /// Extract a compile-time constant array size from an AST expression.
    fn extract_array_size(expr: &ast::Expr) -> Option<usize> {
        match expr {
            ast::Expr::IntLiteral(v) => {
                if *v >= 0 {
                    Some(*v as usize)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Lower a type expression, validating named types against `known_types`.
    fn lower_type(&mut self, ty: &ast::TypeExpr, span: Span) -> HirType {
        match ty {
            ast::TypeExpr::Named(name) => {
                if name == "void" {
                    // The parser emits `Named("void")` for functions with no
                    // return type.  This is not a real user-facing type — just
                    // pass it through so the codegen can emit `ret void`.
                    HirType::Unknown("void".to_string())
                } else if let Some(prim) = resolve_primitive_type(name) {
                    HirType::Primitive(prim)
                } else if self.known_types.contains(name) {
                    HirType::UserDefined(name.clone())
                } else {
                    self.errors.push(LowerError::UnknownType {
                        name: name.clone(),
                        span: span_to_source_span(span),
                    });
                    HirType::Unknown(name.clone())
                }
            }
            ast::TypeExpr::Tensor(element, dims) => HirType::Tensor {
                element: Box::new(self.lower_type(element, span)),
                dims: dims.iter().map(lower_dim_expr).collect(),
            },
            ast::TypeExpr::Array(element, length) => {
                let size = Self::extract_array_size(length);
                if size.is_none() {
                    self.errors.push(LowerError::InvalidArraySize {
                        span: span_to_source_span(span),
                    });
                }
                HirType::Array {
                    element: Box::new(self.lower_type(element, span)),
                    size: size.unwrap_or(0),
                }
            }
            ast::TypeExpr::Slice(element) => HirType::Slice {
                element: Box::new(self.lower_type(element, span)),
            },
            ast::TypeExpr::Ptr(element) => HirType::Ptr {
                element: Box::new(self.lower_type(element, span)),
            },
            ast::TypeExpr::ReadonlyPtr(element) => HirType::ReadonlyPtr {
                element: Box::new(self.lower_type(element, span)),
            },
            ast::TypeExpr::WriteonlyPtr(element) => HirType::WriteonlyPtr {
                element: Box::new(self.lower_type(element, span)),
            },
            ast::TypeExpr::Tuple(elements) => HirType::Tuple {
                elements: elements.iter().map(|e| self.lower_type(e, span)).collect(),
            },
            ast::TypeExpr::Fn(params, ret) => HirType::Fn {
                params: params.iter().map(|p| self.lower_type(p, span)).collect(),
                ret: Box::new(self.lower_type(ret, span)),
            },
        }
    }

    /// Lower an annotation.
    fn lower_annotation(&mut self, ann: &ast::Spanned<ast::Annotation>) -> HirAnnotation {
        let kind = match &ann.node {
            ast::Annotation::Pure => HirAnnotationKind::Pure,
            ast::Annotation::Const => HirAnnotationKind::Const,
            ast::Annotation::Inline(hint) => HirAnnotationKind::Inline(hint.clone()),
            ast::Annotation::Complexity(expr) => HirAnnotationKind::Complexity(expr.clone()),
            ast::Annotation::Intent(desc) => HirAnnotationKind::Intent(desc.clone()),
            ast::Annotation::Module(name) => HirAnnotationKind::Module(name.clone()),
            ast::Annotation::Constraint(entries) => {
                HirAnnotationKind::Constraint(entries.clone())
            }
            ast::Annotation::Target(targets) => HirAnnotationKind::Target(targets.clone()),
            ast::Annotation::Strategy(block) => HirAnnotationKind::Strategy(block.clone()),
            ast::Annotation::Transfer(block) => HirAnnotationKind::Transfer(block.clone()),
            ast::Annotation::Vectorizable(dims) => {
                HirAnnotationKind::Vectorizable(dims.clone())
            }
            ast::Annotation::Parallel(dims) => HirAnnotationKind::Parallel(dims.clone()),
            ast::Annotation::Layout(kind) => HirAnnotationKind::Layout(kind.clone()),
            ast::Annotation::Align(bytes) => HirAnnotationKind::Align(*bytes),
            ast::Annotation::OptimizationLog(entries) => {
                HirAnnotationKind::OptimizationLog(entries.clone())
            }
            ast::Annotation::Export => HirAnnotationKind::Export,
            ast::Annotation::Lifetime(scope) => HirAnnotationKind::Lifetime(scope.clone()),
            ast::Annotation::ParallelFor(config) => {
                HirAnnotationKind::ParallelFor(config.clone())
            }
            ast::Annotation::Strict => HirAnnotationKind::Strict,
            ast::Annotation::Precondition(expr) => {
                HirAnnotationKind::Precondition(Box::new(self.lower_expr(expr, ann.span)))
            }
            ast::Annotation::Postcondition(expr) => {
                HirAnnotationKind::Postcondition(Box::new(self.lower_expr(expr, ann.span)))
            }
            ast::Annotation::Test(tc) => HirAnnotationKind::Test(HirTestCase {
                inputs: tc.inputs.iter().map(|e| self.lower_expr(e, ann.span)).collect(),
                expected: self.lower_expr(&tc.expected, ann.span),
            }),
            ast::Annotation::Link { library, kind } => {
                HirAnnotationKind::Link {
                    library: library.clone(),
                    kind: kind.clone().unwrap_or_else(|| "dylib".to_string()),
                }
            }
            ast::Annotation::Trace => HirAnnotationKind::Trace,
            ast::Annotation::Cfg(target) => HirAnnotationKind::Cfg(target.clone()),
            ast::Annotation::Requires(expr) => {
                HirAnnotationKind::Requires(Box::new(self.lower_expr(expr, ann.span)))
            }
            ast::Annotation::Ensures(expr) => {
                HirAnnotationKind::Ensures(Box::new(self.lower_expr(expr, ann.span)))
            }
            ast::Annotation::Invariant(expr) => {
                HirAnnotationKind::Invariant(Box::new(self.lower_expr(expr, ann.span)))
            }
            ast::Annotation::Custom(name, args) => {
                HirAnnotationKind::Custom(name.clone(), args.clone())
            }
        };

        HirAnnotation {
            kind,
            span: ann.span,
        }
    }

    /// Validate that each annotation is valid for its target.
    fn validate_annotations(&mut self, annotations: &[HirAnnotation], target: AnnotationTarget) {
        for ann in annotations {
            let (ann_name, valid_targets) = annotation_valid_targets(&ann.kind);
            if !valid_targets.contains(&target) {
                let valid_str = valid_targets
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                self.errors.push(LowerError::InvalidAnnotationTarget {
                    annotation: ann_name.to_string(),
                    target: target.to_string(),
                    valid_targets: valid_str,
                    span: span_to_source_span(ann.span),
                });
            }
        }
    }
}

/// Map a dimension expression from AST to HIR.
fn lower_dim_expr(dim: &ast::DimExpr) -> HirDimExpr {
    match dim {
        ast::DimExpr::Const(v) => HirDimExpr::Const(*v),
        ast::DimExpr::Named(name) => HirDimExpr::Named(name.clone()),
        ast::DimExpr::Dynamic => HirDimExpr::Dynamic,
    }
}

/// All known primitive type names.
const PRIMITIVE_NAMES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f16", "bf16", "f32",
    "f64", "bool", "vec2", "vec3", "vec4", "ivec2", "ivec3", "ivec4", "fvec2", "fvec3", "fvec4",
    "mat3", "mat4",
];

/// Map a type name string to a [`PrimitiveType`] enum variant.
fn resolve_primitive_type(name: &str) -> Option<PrimitiveType> {
    match name {
        "i8" => Some(PrimitiveType::I8),
        "i16" => Some(PrimitiveType::I16),
        "i32" => Some(PrimitiveType::I32),
        "i64" => Some(PrimitiveType::I64),
        "i128" => Some(PrimitiveType::I128),
        "u8" => Some(PrimitiveType::U8),
        "u16" => Some(PrimitiveType::U16),
        "u32" => Some(PrimitiveType::U32),
        "u64" => Some(PrimitiveType::U64),
        "u128" => Some(PrimitiveType::U128),
        "f16" => Some(PrimitiveType::F16),
        "bf16" => Some(PrimitiveType::Bf16),
        "f32" => Some(PrimitiveType::F32),
        "f64" => Some(PrimitiveType::F64),
        "bool" => Some(PrimitiveType::Bool),
        "vec2" => Some(PrimitiveType::Vec2),
        "vec3" => Some(PrimitiveType::Vec3),
        "vec4" => Some(PrimitiveType::Vec4),
        "ivec2" => Some(PrimitiveType::IVec2),
        "ivec3" => Some(PrimitiveType::IVec3),
        "ivec4" => Some(PrimitiveType::IVec4),
        "fvec2" => Some(PrimitiveType::FVec2),
        "fvec3" => Some(PrimitiveType::FVec3),
        "fvec4" => Some(PrimitiveType::FVec4),
        "mat3" => Some(PrimitiveType::Mat3),
        "mat4" => Some(PrimitiveType::Mat4),
        _ => None,
    }
}

/// Return the annotation name and the set of valid targets for a given annotation kind.
fn annotation_valid_targets(kind: &HirAnnotationKind) -> (&str, Vec<AnnotationTarget>) {
    use AnnotationTarget::*;
    match kind {
        HirAnnotationKind::Pure => ("pure", vec![Function]),
        HirAnnotationKind::Const => ("const", vec![Function]),
        HirAnnotationKind::Inline(_) => ("inline", vec![Function]),
        HirAnnotationKind::Complexity(_) => ("complexity", vec![Function]),
        HirAnnotationKind::Intent(_) => ("intent", vec![Function, Module]),
        HirAnnotationKind::Module(_) => ("module", vec![Module]),
        HirAnnotationKind::Constraint(_) => ("constraint", vec![Function, Module]),
        HirAnnotationKind::Target(_) => ("target", vec![Function, Module]),
        HirAnnotationKind::Strategy(_) => ("strategy", vec![Function, Block]),
        HirAnnotationKind::Transfer(_) => ("transfer", vec![Function, Module, Block]),
        HirAnnotationKind::Vectorizable(_) => ("vectorizable", vec![Function]),
        HirAnnotationKind::Parallel(_) => ("parallel", vec![Function]),
        HirAnnotationKind::Layout(_) => ("layout", vec![Param, StructField]),
        HirAnnotationKind::Align(_) => ("align", vec![Param, StructField]),
        HirAnnotationKind::OptimizationLog(_) => ("optimization_log", vec![Function]),
        HirAnnotationKind::Export => ("export", vec![Function]),
        HirAnnotationKind::Lifetime(_) => ("lifetime", vec![Function, Block]),
        HirAnnotationKind::ParallelFor(_) => ("parallel_for", vec![Block]),
        HirAnnotationKind::Strict => ("strict", vec![Module]),
        HirAnnotationKind::Precondition(_) => ("precondition", vec![Function]),
        HirAnnotationKind::Postcondition(_) => ("postcondition", vec![Function]),
        HirAnnotationKind::Test(_) => ("test", vec![Function]),
        HirAnnotationKind::Link { .. } => ("link", vec![Function]),
        HirAnnotationKind::Trace => ("trace", vec![Function]),
        HirAnnotationKind::Cfg(_) => ("cfg", vec![Function]),
        HirAnnotationKind::Requires(_) => ("requires", vec![Function]),
        HirAnnotationKind::Ensures(_) => ("ensures", vec![Function]),
        HirAnnotationKind::Invariant(_) => ("invariant", vec![Block]),
        HirAnnotationKind::Custom(_, _) => (
            "custom",
            vec![Function, Module, Param, StructDef, StructField, Block],
        ),
    }
}

/// Check whether an item with the given AST annotations should be included
/// based on `@cfg` annotations and the current platform.
///
/// Returns `true` if the item should be included (i.e., no `@cfg` annotation
/// present, or a `@cfg` annotation matches the current OS). Returns `false`
/// if a `@cfg` annotation is present and does NOT match the current OS.
fn cfg_matches(annotations: &[ast::Spanned<ast::Annotation>], current_os: &str) -> bool {
    for ann in annotations {
        if let ast::Annotation::Cfg(ref target) = ann.node {
            if target != current_os {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse source and lower to HIR, returning the result.
    fn parse_and_lower(source: &str) -> Result<HirModule, Vec<LowerError>> {
        let parse_result = axiom_parser::parse(source);
        assert!(
            !parse_result.has_errors(),
            "Parse errors: {:?}",
            parse_result.errors
        );
        lower(&parse_result.module)
    }

    #[test]
    fn test_lower_hello() {
        let source = r#"
@module hello;
@intent("Print greeting to stdout");

fn main() -> i32 {
    print("Hello from AXIOM!");
    return 0;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        assert_eq!(hir.name.as_deref(), Some("hello"));
        assert_eq!(hir.functions.len(), 1);
        assert_eq!(hir.functions[0].name, "main");
        assert_eq!(
            hir.functions[0].return_type,
            HirType::Primitive(PrimitiveType::I32)
        );
    }

    #[test]
    fn test_lower_fibonacci() {
        let source = r#"
@module fibonacci;
@intent("Compute Nth Fibonacci number iteratively");

@pure
@complexity O(n)
fn fib(n: i32) -> i64 {
    if n <= 1 {
        return widen(n);
    }
    let a: i64 = 0;
    let b: i64 = 1;
    for i: i32 in range(2, n + 1) {
        let temp: i64 = b;
        b = a + b;
        a = temp;
    }
    return b;
}

fn main() -> i32 {
    let result: i64 = fib(40);
    print_i64(result);
    return 0;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        assert_eq!(hir.name.as_deref(), Some("fibonacci"));
        assert_eq!(hir.functions.len(), 2);

        let fib = &hir.functions[0];
        assert_eq!(fib.name, "fib");
        assert!(fib
            .annotations
            .iter()
            .any(|a| matches!(a.kind, HirAnnotationKind::Pure)));
        assert!(fib
            .annotations
            .iter()
            .any(|a| matches!(a.kind, HirAnnotationKind::Complexity(_))));
        assert_eq!(fib.params.len(), 1);
        assert_eq!(
            fib.params[0].ty,
            HirType::Primitive(PrimitiveType::I32)
        );
        assert_eq!(fib.return_type, HirType::Primitive(PrimitiveType::I64));

        let main_fn = &hir.functions[1];
        assert_eq!(main_fn.name, "main");
        assert_eq!(
            main_fn.return_type,
            HirType::Primitive(PrimitiveType::I32)
        );
    }

    #[test]
    fn test_annotation_preservation() {
        let source = r#"
@pure
@intent("Do something")
@complexity O(n)
@inline(always)
fn foo(x: i32) -> i32 {
    return x;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        let func = &hir.functions[0];

        assert!(func
            .annotations
            .iter()
            .any(|a| matches!(a.kind, HirAnnotationKind::Pure)));
        assert!(func.annotations.iter().any(
            |a| matches!(&a.kind, HirAnnotationKind::Intent(s) if s == "Do something")
        ));
        assert!(func
            .annotations
            .iter()
            .any(|a| matches!(&a.kind, HirAnnotationKind::Complexity(s) if s == "O(n)")));
        assert!(func.annotations.iter().any(
            |a| matches!(&a.kind, HirAnnotationKind::Inline(InlineHint::Always))
        ));
    }

    #[test]
    fn test_annotation_validation_pure_on_param() {
        // Build AST with @pure on a parameter — should be rejected
        let module = ast::Module {
            name: None,
            annotations: vec![],
            items: vec![ast::Spanned::new(
                ast::Item::Function(ast::Function {
                    name: ast::Spanned::new("f".to_string(), Span::new(0, 1)),
                    annotations: vec![],
                    params: vec![ast::Param {
                        name: ast::Spanned::new("x".to_string(), Span::new(5, 6)),
                        ty: ast::TypeExpr::Named("i32".to_string()),
                        annotations: vec![ast::Spanned::new(
                            ast::Annotation::Pure,
                            Span::new(2, 7),
                        )],
                    }],
                    return_type: ast::TypeExpr::Named("i32".to_string()),
                    body: ast::Block {
                        annotations: vec![],
                        stmts: vec![ast::Spanned::new(
                            ast::Stmt::Return(Some(ast::Expr::IntLiteral(0))),
                            Span::new(10, 20),
                        )],
                    },
                    is_public: false,
                }),
                Span::new(0, 30),
            )],
        };

        let err = lower(&module).unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            LowerError::InvalidAnnotationTarget {
                annotation,
                target,
                ..
            } if annotation == "pure" && target == "parameter"
        )));
    }

    #[test]
    fn test_annotation_validation_layout_on_function() {
        let module = ast::Module {
            name: None,
            annotations: vec![],
            items: vec![ast::Spanned::new(
                ast::Item::Function(ast::Function {
                    name: ast::Spanned::new("f".to_string(), Span::new(0, 1)),
                    annotations: vec![ast::Spanned::new(
                        ast::Annotation::Layout(ast::LayoutKind::RowMajor),
                        Span::new(0, 10),
                    )],
                    params: vec![],
                    return_type: ast::TypeExpr::Named("i32".to_string()),
                    body: ast::Block {
                        annotations: vec![],
                        stmts: vec![ast::Spanned::new(
                            ast::Stmt::Return(Some(ast::Expr::IntLiteral(0))),
                            Span::new(10, 20),
                        )],
                    },
                    is_public: false,
                }),
                Span::new(0, 30),
            )],
        };

        let err = lower(&module).unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            LowerError::InvalidAnnotationTarget {
                annotation,
                ..
            } if annotation == "layout"
        )));
    }

    #[test]
    fn test_annotation_validation_module_on_function() {
        let module = ast::Module {
            name: None,
            annotations: vec![],
            items: vec![ast::Spanned::new(
                ast::Item::Function(ast::Function {
                    name: ast::Spanned::new("f".to_string(), Span::new(0, 1)),
                    annotations: vec![ast::Spanned::new(
                        ast::Annotation::Module("test".to_string()),
                        Span::new(0, 10),
                    )],
                    params: vec![],
                    return_type: ast::TypeExpr::Named("i32".to_string()),
                    body: ast::Block {
                        annotations: vec![],
                        stmts: vec![ast::Spanned::new(
                            ast::Stmt::Return(Some(ast::Expr::IntLiteral(0))),
                            Span::new(10, 20),
                        )],
                    },
                    is_public: false,
                }),
                Span::new(0, 30),
            )],
        };

        let err = lower(&module).unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            LowerError::InvalidAnnotationTarget {
                annotation,
                ..
            } if annotation == "module"
        )));
    }

    #[test]
    fn test_annotation_validation_pure_on_struct() {
        let module = ast::Module {
            name: None,
            annotations: vec![],
            items: vec![ast::Spanned::new(
                ast::Item::Struct(ast::StructDef {
                    name: ast::Spanned::new("S".to_string(), Span::new(0, 1)),
                    annotations: vec![ast::Spanned::new(
                        ast::Annotation::Pure,
                        Span::new(0, 5),
                    )],
                    fields: vec![],
                }),
                Span::new(0, 20),
            )],
        };

        let err = lower(&module).unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            LowerError::InvalidAnnotationTarget {
                annotation,
                target,
                ..
            } if annotation == "pure" && target == "struct"
        )));
    }

    #[test]
    fn test_annotation_validation_align_on_function() {
        let module = ast::Module {
            name: None,
            annotations: vec![],
            items: vec![ast::Spanned::new(
                ast::Item::Function(ast::Function {
                    name: ast::Spanned::new("f".to_string(), Span::new(0, 1)),
                    annotations: vec![ast::Spanned::new(
                        ast::Annotation::Align(64),
                        Span::new(0, 10),
                    )],
                    params: vec![],
                    return_type: ast::TypeExpr::Named("i32".to_string()),
                    body: ast::Block {
                        annotations: vec![],
                        stmts: vec![ast::Spanned::new(
                            ast::Stmt::Return(Some(ast::Expr::IntLiteral(0))),
                            Span::new(10, 20),
                        )],
                    },
                    is_public: false,
                }),
                Span::new(0, 30),
            )],
        };

        let err = lower(&module).unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            LowerError::InvalidAnnotationTarget {
                annotation,
                ..
            } if annotation == "align"
        )));
    }

    #[test]
    fn test_annotation_validation_valid_targets() {
        // @layout on param should be accepted
        let module = ast::Module {
            name: None,
            annotations: vec![],
            items: vec![ast::Spanned::new(
                ast::Item::Function(ast::Function {
                    name: ast::Spanned::new("f".to_string(), Span::new(0, 1)),
                    annotations: vec![],
                    params: vec![ast::Param {
                        name: ast::Spanned::new("x".to_string(), Span::new(5, 6)),
                        ty: ast::TypeExpr::Named("i32".to_string()),
                        annotations: vec![ast::Spanned::new(
                            ast::Annotation::Layout(ast::LayoutKind::RowMajor),
                            Span::new(2, 7),
                        )],
                    }],
                    return_type: ast::TypeExpr::Named("i32".to_string()),
                    body: ast::Block {
                        annotations: vec![],
                        stmts: vec![ast::Spanned::new(
                            ast::Stmt::Return(Some(ast::Expr::IntLiteral(0))),
                            Span::new(10, 20),
                        )],
                    },
                    is_public: false,
                }),
                Span::new(0, 30),
            )],
        };

        let result = lower(&module);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
    }

    #[test]
    fn test_type_validation() {
        let source = r#"
fn foo(x: i32, y: f64, z: bool) -> i32 {
    return 0;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        let func = &hir.functions[0];

        assert_eq!(func.params[0].ty, HirType::Primitive(PrimitiveType::I32));
        assert_eq!(func.params[1].ty, HirType::Primitive(PrimitiveType::F64));
        assert_eq!(func.params[2].ty, HirType::Primitive(PrimitiveType::Bool));
    }

    #[test]
    fn test_type_validation_unknown() {
        let module = ast::Module {
            name: None,
            annotations: vec![],
            items: vec![ast::Spanned::new(
                ast::Item::Function(ast::Function {
                    name: ast::Spanned::new("f".to_string(), Span::new(0, 1)),
                    annotations: vec![],
                    params: vec![ast::Param {
                        name: ast::Spanned::new("x".to_string(), Span::new(5, 6)),
                        ty: ast::TypeExpr::Named("Nonexistent".to_string()),
                        annotations: vec![],
                    }],
                    return_type: ast::TypeExpr::Named("i32".to_string()),
                    body: ast::Block {
                        annotations: vec![],
                        stmts: vec![ast::Spanned::new(
                            ast::Stmt::Return(Some(ast::Expr::IntLiteral(0))),
                            Span::new(10, 20),
                        )],
                    },
                    is_public: false,
                }),
                Span::new(0, 30),
            )],
        };

        let err = lower(&module).unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            LowerError::UnknownType { name, .. } if name == "Nonexistent"
        )));
    }

    #[test]
    fn test_type_validation_user_defined() {
        let source = r#"
struct Point {
    x: f64,
    y: f64
}

fn origin() -> Point {
    return 0;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        assert_eq!(hir.structs.len(), 1);
        assert_eq!(hir.structs[0].name, "Point");
        let func = &hir.functions[0];
        assert_eq!(
            func.return_type,
            HirType::UserDefined("Point".to_string())
        );
    }

    #[test]
    fn test_type_validation_forward_reference() {
        // Function using struct defined later in the file
        let source = r#"
fn make_point() -> Point {
    return 0;
}

struct Point {
    x: f64,
    y: f64
}
"#;
        let hir = parse_and_lower(source).expect("forward reference should succeed");
        assert_eq!(
            hir.functions[0].return_type,
            HirType::UserDefined("Point".to_string())
        );
    }

    #[test]
    fn test_hir_coverage() {
        // Build AST containing every variant to ensure full coverage
        let module = ast::Module {
            name: Some(ast::Spanned::new("test".to_string(), Span::new(0, 4))),
            annotations: vec![],
            items: vec![
                // Function with all statement types
                ast::Spanned::new(
                    ast::Item::Function(ast::Function {
                        name: ast::Spanned::new("all_stmts".to_string(), Span::new(0, 9)),
                        annotations: vec![],
                        params: vec![ast::Param {
                            name: ast::Spanned::new("x".to_string(), Span::new(10, 11)),
                            ty: ast::TypeExpr::Named("i32".to_string()),
                            annotations: vec![],
                        }],
                        return_type: ast::TypeExpr::Named("i32".to_string()),
                        body: ast::Block {
                            annotations: vec![],
                            stmts: vec![
                                // Let
                                ast::Spanned::new(
                                    ast::Stmt::Let {
                                        name: ast::Spanned::new(
                                            "a".to_string(),
                                            Span::new(20, 21),
                                        ),
                                        ty: ast::TypeExpr::Named("i32".to_string()),
                                        value: Some(ast::Expr::IntLiteral(42)),
                                        mutable: false,
                                        annotations: vec![],
                                    },
                                    Span::new(20, 30),
                                ),
                                // Mutable Let
                                ast::Spanned::new(
                                    ast::Stmt::Let {
                                        name: ast::Spanned::new(
                                            "b".to_string(),
                                            Span::new(30, 31),
                                        ),
                                        ty: ast::TypeExpr::Named("f64".to_string()),
                                        value: Some(ast::Expr::FloatLiteral(3.14)),
                                        mutable: true,
                                        annotations: vec![],
                                    },
                                    Span::new(30, 40),
                                ),
                                // Assign
                                ast::Spanned::new(
                                    ast::Stmt::Assign {
                                        target: ast::Expr::Ident("b".to_string()),
                                        value: ast::Expr::BinaryOp {
                                            op: ast::BinOp::Add,
                                            lhs: Box::new(ast::Expr::Ident("b".to_string())),
                                            rhs: Box::new(ast::Expr::FloatLiteral(1.0)),
                                        },
                                    },
                                    Span::new(40, 50),
                                ),
                                // If
                                ast::Spanned::new(
                                    ast::Stmt::If {
                                        condition: ast::Expr::BoolLiteral(true),
                                        then_block: ast::Block {
                                            annotations: vec![],
                                            stmts: vec![],
                                        },
                                        else_block: Some(ast::Block {
                                            annotations: vec![],
                                            stmts: vec![],
                                        }),
                                    },
                                    Span::new(50, 60),
                                ),
                                // For
                                ast::Spanned::new(
                                    ast::Stmt::For {
                                        var: ast::Spanned::new(
                                            "i".to_string(),
                                            Span::new(60, 61),
                                        ),
                                        var_type: ast::TypeExpr::Named("i32".to_string()),
                                        iterable: ast::Expr::Call {
                                            func: Box::new(ast::Expr::Ident(
                                                "range".to_string(),
                                            )),
                                            args: vec![ast::Expr::IntLiteral(10)],
                                        },
                                        body: ast::Block {
                                            annotations: vec![],
                                            stmts: vec![],
                                        },
                                    },
                                    Span::new(60, 70),
                                ),
                                // While
                                ast::Spanned::new(
                                    ast::Stmt::While {
                                        condition: ast::Expr::BinaryOp {
                                            op: ast::BinOp::Lt,
                                            lhs: Box::new(ast::Expr::Ident("a".to_string())),
                                            rhs: Box::new(ast::Expr::IntLiteral(100)),
                                        },
                                        body: ast::Block {
                                            annotations: vec![],
                                            stmts: vec![],
                                        },
                                    },
                                    Span::new(70, 80),
                                ),
                                // Expr stmt with method call
                                ast::Spanned::new(
                                    ast::Stmt::Expr(ast::Expr::MethodCall {
                                        expr: Box::new(ast::Expr::Ident("x".to_string())),
                                        method: "to_string".to_string(),
                                        args: vec![],
                                    }),
                                    Span::new(80, 90),
                                ),
                                // Expr stmt with unary op
                                ast::Spanned::new(
                                    ast::Stmt::Expr(ast::Expr::UnaryOp {
                                        op: ast::UnaryOp::Neg,
                                        operand: Box::new(ast::Expr::IntLiteral(1)),
                                    }),
                                    Span::new(90, 95),
                                ),
                                // Expr stmt with index
                                ast::Spanned::new(
                                    ast::Stmt::Expr(ast::Expr::Index {
                                        expr: Box::new(ast::Expr::Ident("arr".to_string())),
                                        indices: vec![ast::Expr::IntLiteral(0)],
                                    }),
                                    Span::new(95, 100),
                                ),
                                // Expr stmt with field access
                                ast::Spanned::new(
                                    ast::Stmt::Expr(ast::Expr::FieldAccess {
                                        expr: Box::new(ast::Expr::Ident("pt".to_string())),
                                        field: "x".to_string(),
                                    }),
                                    Span::new(100, 105),
                                ),
                                // Expr stmt with string literal
                                ast::Spanned::new(
                                    ast::Stmt::Expr(ast::Expr::StringLiteral(
                                        "hello".to_string(),
                                    )),
                                    Span::new(105, 110),
                                ),
                                // Expr stmt with opt hole
                                ast::Spanned::new(
                                    ast::Stmt::Expr(ast::Expr::OptHole(
                                        "param".to_string(),
                                    )),
                                    Span::new(110, 115),
                                ),
                                // Return
                                ast::Spanned::new(
                                    ast::Stmt::Return(Some(ast::Expr::Ident("a".to_string()))),
                                    Span::new(115, 120),
                                ),
                            ],
                        },
                        is_public: false,
                    }),
                    Span::new(0, 120),
                ),
                // Struct
                ast::Spanned::new(
                    ast::Item::Struct(ast::StructDef {
                        name: ast::Spanned::new("MyStruct".to_string(), Span::new(120, 128)),
                        annotations: vec![],
                        fields: vec![ast::StructField {
                            name: ast::Spanned::new("field".to_string(), Span::new(130, 135)),
                            ty: ast::TypeExpr::Named("i32".to_string()),
                            annotations: vec![],
                        }],
                    }),
                    Span::new(120, 140),
                ),
                // TypeAlias
                ast::Spanned::new(
                    ast::Item::TypeAlias(ast::TypeAlias {
                        name: ast::Spanned::new("Alias".to_string(), Span::new(140, 145)),
                        ty: ast::TypeExpr::Named("i64".to_string()),
                    }),
                    Span::new(140, 150),
                ),
                // Import
                ast::Spanned::new(
                    ast::Item::Import(ast::ImportDecl {
                        path: vec!["std".to_string(), "io".to_string()],
                        alias: None,
                        is_public: false,
                    }),
                    Span::new(150, 160),
                ),
            ],
        };

        let hir = lower(&module).expect("coverage module should lower");
        assert_eq!(hir.name.as_deref(), Some("test"));
        assert_eq!(hir.functions.len(), 1);
        assert_eq!(hir.structs.len(), 1);
        assert_eq!(hir.type_aliases.len(), 1);
        assert_eq!(hir.imports.len(), 1);

        // Verify all statement types present
        let stmts = &hir.functions[0].body.stmts;
        assert!(stmts.iter().any(|s| matches!(s.kind, HirStmtKind::Let { .. })));
        assert!(stmts.iter().any(|s| matches!(s.kind, HirStmtKind::Assign { .. })));
        assert!(stmts.iter().any(|s| matches!(s.kind, HirStmtKind::Return { .. })));
        assert!(stmts.iter().any(|s| matches!(s.kind, HirStmtKind::If { .. })));
        assert!(stmts.iter().any(|s| matches!(s.kind, HirStmtKind::For { .. })));
        assert!(stmts.iter().any(|s| matches!(s.kind, HirStmtKind::While { .. })));
        assert!(stmts.iter().any(|s| matches!(s.kind, HirStmtKind::Expr { .. })));

        // Verify expression types are produced
        let has_expr_kind = |kind_check: fn(&HirExprKind) -> bool| -> bool {
            fn check_expr(expr: &HirExpr, f: fn(&HirExprKind) -> bool) -> bool {
                if f(&expr.kind) {
                    return true;
                }
                match &expr.kind {
                    HirExprKind::BinaryOp { lhs, rhs, .. } => {
                        check_expr(lhs, f) || check_expr(rhs, f)
                    }
                    HirExprKind::UnaryOp { operand, .. } => check_expr(operand, f),
                    HirExprKind::Call { func, args, .. } => {
                        check_expr(func, f) || args.iter().any(|a| check_expr(a, f))
                    }
                    HirExprKind::Index { expr, indices, .. } => {
                        check_expr(expr, f) || indices.iter().any(|i| check_expr(i, f))
                    }
                    HirExprKind::FieldAccess { expr, .. } => check_expr(expr, f),
                    HirExprKind::MethodCall { expr, args, .. } => {
                        check_expr(expr, f) || args.iter().any(|a| check_expr(a, f))
                    }
                    _ => false,
                }
            }

            for stmt in stmts {
                let found = match &stmt.kind {
                    HirStmtKind::Let { value, .. } => {
                        value.as_ref().is_some_and(|v| check_expr(v, kind_check))
                    }
                    HirStmtKind::Assign { target, value } => {
                        check_expr(target, kind_check) || check_expr(value, kind_check)
                    }
                    HirStmtKind::Return { value } => value.as_ref().is_some_and(|v| check_expr(v, kind_check)),
                    HirStmtKind::If { condition, .. } => check_expr(condition, kind_check),
                    HirStmtKind::For { iterable, .. } => check_expr(iterable, kind_check),
                    HirStmtKind::While { condition, .. } => check_expr(condition, kind_check),
                    HirStmtKind::Break | HirStmtKind::Continue => false,
                    HirStmtKind::Match { value, .. } => check_expr(value, kind_check),
                    HirStmtKind::Expr { expr } => check_expr(expr, kind_check),
                };
                if found {
                    return true;
                }
            }
            false
        };

        assert!(has_expr_kind(|k| matches!(k, HirExprKind::IntLiteral { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::FloatLiteral { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::StringLiteral { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::BoolLiteral { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::Ident { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::OptHole { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::BinaryOp { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::UnaryOp { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::Call { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::Index { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::FieldAccess { .. })));
        assert!(has_expr_kind(|k| matches!(k, HirExprKind::MethodCall { .. })));
    }

    #[test]
    fn test_empty_module() {
        let source = "";
        let hir = parse_and_lower(source).expect("empty module should succeed");
        assert_eq!(hir.name, None);
        assert!(hir.annotations.is_empty());
        assert!(hir.functions.is_empty());
        assert!(hir.structs.is_empty());
        assert!(hir.type_aliases.is_empty());
        assert!(hir.imports.is_empty());
    }

    #[test]
    fn test_mutable_let_binding() {
        let source = r#"
fn f() -> i32 {
    let mut x: i32 = 0;
    return x;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        let stmts = &hir.functions[0].body.stmts;
        assert!(stmts.iter().any(|s| matches!(
            &s.kind,
            HirStmtKind::Let { mutable: true, .. }
        )));
    }

    #[test]
    fn test_all_primitive_types() {
        for prim_name in PRIMITIVE_NAMES {
            let result = resolve_primitive_type(prim_name);
            assert!(
                result.is_some(),
                "Primitive type '{prim_name}' should be resolved"
            );
        }
    }

    #[test]
    fn test_duplicate_function_names() {
        let module = ast::Module {
            name: None,
            annotations: vec![],
            items: vec![
                ast::Spanned::new(
                    ast::Item::Function(ast::Function {
                        name: ast::Spanned::new("foo".to_string(), Span::new(0, 3)),
                        annotations: vec![],
                        params: vec![],
                        return_type: ast::TypeExpr::Named("i32".to_string()),
                        body: ast::Block {
                            annotations: vec![],
                            stmts: vec![ast::Spanned::new(
                                ast::Stmt::Return(Some(ast::Expr::IntLiteral(0))),
                                Span::new(5, 15),
                            )],
                        },
                        is_public: false,
                    }),
                    Span::new(0, 20),
                ),
                ast::Spanned::new(
                    ast::Item::Function(ast::Function {
                        name: ast::Spanned::new("foo".to_string(), Span::new(25, 28)),
                        annotations: vec![],
                        params: vec![],
                        return_type: ast::TypeExpr::Named("i32".to_string()),
                        body: ast::Block {
                            annotations: vec![],
                            stmts: vec![ast::Spanned::new(
                                ast::Stmt::Return(Some(ast::Expr::IntLiteral(1))),
                                Span::new(30, 40),
                            )],
                        },
                        is_public: false,
                    }),
                    Span::new(20, 45),
                ),
            ],
        };

        let err = lower(&module).unwrap_err();
        assert!(err.iter().any(|e| matches!(
            e,
            LowerError::DuplicateDefinition { name, kind, .. }
            if name == "foo" && kind == "function"
        )));
    }

    #[test]
    fn test_duplicate_module_annotation() {
        let module = ast::Module {
            name: None,
            annotations: vec![
                ast::Spanned::new(
                    ast::Annotation::Module("a".to_string()),
                    Span::new(0, 10),
                ),
                ast::Spanned::new(
                    ast::Annotation::Module("b".to_string()),
                    Span::new(10, 20),
                ),
            ],
            items: vec![],
        };

        let err = lower(&module).unwrap_err();
        assert!(err
            .iter()
            .any(|e| matches!(e, LowerError::DuplicateModuleAnnotation { .. })));
    }

    #[test]
    fn test_lower_extern_function() {
        let source = r#"
extern fn sin(x: f64) -> f64;
extern fn clock() -> i64;
fn main() -> i32 {
    return 0;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        assert_eq!(hir.extern_functions.len(), 2);
        assert_eq!(hir.extern_functions[0].name, "sin");
        assert_eq!(
            hir.extern_functions[0].return_type,
            HirType::Primitive(PrimitiveType::F64)
        );
        assert_eq!(hir.extern_functions[0].params.len(), 1);
        assert_eq!(hir.extern_functions[1].name, "clock");
        assert_eq!(
            hir.extern_functions[1].return_type,
            HirType::Primitive(PrimitiveType::I64)
        );
        assert!(hir.extern_functions[1].params.is_empty());
        assert_eq!(hir.functions.len(), 1);
    }

    #[test]
    fn test_lower_export_annotation() {
        let source = r#"
@export
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        assert_eq!(hir.functions.len(), 1);
        let func = &hir.functions[0];
        assert_eq!(func.name, "add");
        assert!(
            func.annotations
                .iter()
                .any(|a| matches!(a.kind, HirAnnotationKind::Export)),
            "should have @export annotation"
        );
    }

    #[test]
    fn test_lower_ffi_test_sample() {
        let source = r#"
@module ffi_test;
extern fn clock() -> i64;

@export
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}

fn main() -> i32 {
    let t: i64 = clock();
    print_i64(t);
    return 0;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        assert_eq!(hir.name.as_deref(), Some("ffi_test"));
        assert_eq!(hir.extern_functions.len(), 1);
        assert_eq!(hir.extern_functions[0].name, "clock");
        assert_eq!(hir.functions.len(), 2);

        // Check add has @export
        let add = &hir.functions[0];
        assert_eq!(add.name, "add");
        assert!(add
            .annotations
            .iter()
            .any(|a| matches!(a.kind, HirAnnotationKind::Export)));
    }

    #[test]
    fn test_parallel_for_annotation_lowered() {
        let source = r#"
@module pf_test;

fn main() -> i32 {
    let n: i32 = 100;
    let data: ptr[f64] = heap_alloc(n, 8);
    let results: ptr[f64] = heap_alloc(n, 8);

    for i: i32 in range(0, n) {
        ptr_write_f64(data, i, to_f64(i));
    }

    @parallel_for(shared_read: [data], shared_write: [results])
    for i: i32 in range(0, n) {
        let val: f64 = ptr_read_f64(data, i);
        ptr_write_f64(results, i, val * val);
    }

    return 0;
}
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        let func = &hir.functions[0];
        assert_eq!(func.name, "main");

        // Find the second for-loop (which should have the @parallel_for annotation).
        let for_stmts: Vec<_> = func
            .body
            .stmts
            .iter()
            .filter(|s| matches!(s.kind, HirStmtKind::For { .. }))
            .collect();
        assert_eq!(for_stmts.len(), 2, "expected two for statements");

        // First for-loop should NOT have @parallel_for.
        assert!(
            !for_stmts[0]
                .annotations
                .iter()
                .any(|a| matches!(a.kind, HirAnnotationKind::ParallelFor(_))),
            "first for-loop should not have @parallel_for"
        );

        // Second for-loop SHOULD have @parallel_for.
        let pf_ann = for_stmts[1]
            .annotations
            .iter()
            .find(|a| matches!(a.kind, HirAnnotationKind::ParallelFor(_)));
        assert!(pf_ann.is_some(), "second for-loop should have @parallel_for");

        if let HirAnnotationKind::ParallelFor(ref config) = pf_ann.expect("pf").kind {
            assert_eq!(config.shared_read, vec!["data".to_string()]);
            assert_eq!(config.shared_write, vec!["results".to_string()]);
        } else {
            panic!("expected ParallelFor");
        }
    }

    #[test]
    fn test_default_strict_warnings_do_not_cause_error() {
        // Without @strict, missing @intent should produce a warning (to stderr)
        // but should NOT cause lowering to fail.
        let source = r#"
fn helper(x: i32) -> i32 {
    return x + 1;
}

fn main() -> i32 {
    return helper(5);
}
"#;
        let hir = parse_and_lower(source);
        assert!(
            hir.is_ok(),
            "lowering should succeed without @strict even when @intent is missing"
        );
        let hir = hir.unwrap();
        assert_eq!(hir.functions.len(), 2);
    }

    #[test]
    fn test_strict_mode_missing_intent_is_error() {
        // With @strict, missing @intent should cause a lowering error.
        let source = r#"
@strict;

fn helper(x: i32) -> i32 {
    return x + 1;
}

fn main() -> i32 {
    return helper(5);
}
"#;
        let result = parse_and_lower(source);
        assert!(
            result.is_err(),
            "lowering should fail in @strict mode when @intent is missing"
        );
    }

    #[test]
    fn test_no_warning_for_main_function() {
        // main() should never trigger the warning even without @intent.
        let source = r#"
fn main() -> i32 {
    return 0;
}
"#;
        let hir = parse_and_lower(source);
        assert!(hir.is_ok(), "main-only module should lower fine without @intent");
    }

    #[test]
    fn test_no_warning_when_intent_present() {
        // Functions with @intent should NOT trigger warnings.
        let source = r#"
@intent("adds one")
fn helper(x: i32) -> i32 {
    return x + 1;
}

fn main() -> i32 {
    return helper(5);
}
"#;
        let hir = parse_and_lower(source);
        assert!(hir.is_ok(), "functions with @intent should lower without issue");
    }

    #[test]
    fn test_strict_mode_missing_contract_only() {
        // Under @strict, a function with @intent but no @precondition/@postcondition
        // should fail.
        let source = r#"
@strict;

@intent("adds one")
fn helper(x: i32) -> i32 {
    return x + 1;
}

fn main() -> i32 {
    return helper(5);
}
"#;
        let result = parse_and_lower(source);
        assert!(
            result.is_err(),
            "should fail: @intent present but @precondition/@postcondition missing under @strict"
        );
    }

    #[test]
    fn test_strict_mode_missing_intent_only() {
        // Under @strict, a function with @precondition but no @intent should fail.
        let source = r#"
@strict;

@precondition(x >= 0)
fn helper(x: i32) -> i32 {
    return x + 1;
}

fn main() -> i32 {
    return helper(5);
}
"#;
        let result = parse_and_lower(source);
        assert!(
            result.is_err(),
            "should fail: @precondition present but @intent missing under @strict"
        );
    }

    #[test]
    fn test_strict_mode_all_annotations_present() {
        // Under @strict, a function with both @intent and @precondition/@postcondition
        // should pass.
        let source = r#"
@strict;

@intent("adds one to a non-negative number")
@precondition(x >= 0)
@postcondition(return > 0)
fn helper(x: i32) -> i32 {
    return x + 1;
}

fn main() -> i32 {
    return helper(5);
}
"#;
        let result = parse_and_lower(source);
        assert!(
            result.is_ok(),
            "should pass: all @strict annotations present: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_cfg_filters_non_matching_functions() {
        let source = r#"
@module cfg_test;

@cfg("windows")
fn win_only() -> i32 { return 1; }

@cfg("linux")
fn linux_only() -> i32 { return 2; }

@cfg("macos")
fn mac_only() -> i32 { return 3; }

fn always_present() -> i32 { return 0; }
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");

        // `always_present` has no @cfg, so it should always be present.
        assert!(
            hir.functions.iter().any(|f| f.name == "always_present"),
            "always_present should be in HIR"
        );

        // Exactly one of the platform-specific functions should be present
        // (the one matching the current build platform).
        let current_os = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "unknown"
        };

        match current_os {
            "windows" => {
                assert!(
                    hir.functions.iter().any(|f| f.name == "win_only"),
                    "win_only should be present on Windows"
                );
                assert!(
                    !hir.functions.iter().any(|f| f.name == "linux_only"),
                    "linux_only should NOT be present on Windows"
                );
                assert!(
                    !hir.functions.iter().any(|f| f.name == "mac_only"),
                    "mac_only should NOT be present on Windows"
                );
            }
            "linux" => {
                assert!(
                    !hir.functions.iter().any(|f| f.name == "win_only"),
                    "win_only should NOT be present on Linux"
                );
                assert!(
                    hir.functions.iter().any(|f| f.name == "linux_only"),
                    "linux_only should be present on Linux"
                );
                assert!(
                    !hir.functions.iter().any(|f| f.name == "mac_only"),
                    "mac_only should NOT be present on Linux"
                );
            }
            "macos" => {
                assert!(
                    !hir.functions.iter().any(|f| f.name == "win_only"),
                    "win_only should NOT be present on macOS"
                );
                assert!(
                    !hir.functions.iter().any(|f| f.name == "linux_only"),
                    "linux_only should NOT be present on macOS"
                );
                assert!(
                    hir.functions.iter().any(|f| f.name == "mac_only"),
                    "mac_only should be present on macOS"
                );
            }
            _ => {
                // Unknown platform: none of the platform-specific functions should be present
                assert!(
                    !hir.functions.iter().any(|f| f.name == "win_only"),
                    "win_only should NOT be present on unknown platform"
                );
            }
        }
    }

    #[test]
    fn test_cfg_filters_extern_functions() {
        let source = r#"
@module cfg_extern_test;

@cfg("windows")
extern "C" fn GetTickCount() -> i64;

@cfg("linux")
extern "C" fn clock_gettime() -> i64;

fn main() -> i32 { return 0; }
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");

        if cfg!(target_os = "windows") {
            assert!(
                hir.extern_functions.iter().any(|f| f.name == "GetTickCount"),
                "GetTickCount should be present on Windows"
            );
            assert!(
                !hir.extern_functions.iter().any(|f| f.name == "clock_gettime"),
                "clock_gettime should NOT be present on Windows"
            );
        } else if cfg!(target_os = "linux") {
            assert!(
                !hir.extern_functions.iter().any(|f| f.name == "GetTickCount"),
                "GetTickCount should NOT be present on Linux"
            );
            assert!(
                hir.extern_functions.iter().any(|f| f.name == "clock_gettime"),
                "clock_gettime should be present on Linux"
            );
        }
    }

    #[test]
    fn test_cfg_no_annotation_always_included() {
        let source = r#"
fn always() -> i32 { return 42; }
"#;
        let hir = parse_and_lower(source).expect("lowering should succeed");
        assert_eq!(hir.functions.len(), 1);
        assert_eq!(hir.functions[0].name, "always");
    }

    #[test]
    fn test_strict_trivial_precondition_warning() {
        // Under @strict, a function with only @precondition(true) should still
        // pass (it has a contract), but should emit a warning about vacuous contracts.
        // We verify the lowering succeeds (doesn't error).
        let source = r#"
@strict;

@intent("does nothing meaningful")
@precondition(true)
fn trivial(x: i32) -> i32 {
    return x;
}

fn main() -> i32 {
    return trivial(5);
}
"#;
        let result = parse_and_lower(source);
        // Should succeed: @precondition(true) IS a contract, so @strict is satisfied.
        assert!(
            result.is_ok(),
            "should pass: trivial @precondition(true) counts as a contract: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_strict_nontrivial_precondition_no_warning() {
        // Under @strict, a function with a meaningful @precondition should NOT
        // trigger the trivial-contract warning.
        let source = r#"
@strict;

@intent("adds one")
@precondition(x > 0)
fn add_one(x: i32) -> i32 {
    return x + 1;
}

fn main() -> i32 {
    return add_one(5);
}
"#;
        let result = parse_and_lower(source);
        assert!(
            result.is_ok(),
            "should pass with meaningful precondition: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_tuple_literal_parsing_and_lowering() {
        let source = r#"
fn make_pair() -> (i32, f64) {
    return (1, 2.0);
}

fn main() -> i32 {
    return 0;
}
"#;
        let result = parse_and_lower(source);
        assert!(
            result.is_ok(),
            "tuple literal should parse and lower: {:?}",
            result.err()
        );
        let hir = result.unwrap();
        let make_pair = hir.functions.iter().find(|f| f.name == "make_pair").unwrap();
        // Return type should be Tuple
        assert!(
            matches!(make_pair.return_type, HirType::Tuple { .. }),
            "make_pair return type should be Tuple, got: {:?}",
            make_pair.return_type
        );
    }
}
