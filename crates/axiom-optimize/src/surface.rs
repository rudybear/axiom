//! Optimization surface extraction from AXIOM source programs.
//!
//! An *optimization surface* is a structured description of all tunable
//! parameters (`?holes`) inside a function's `@strategy` block. This module
//! parses AXIOM source, lowers it to HIR, then walks the HIR looking for
//! `@strategy` annotations and `?hole` expressions to build [`OptSurface`]
//! descriptors.
//!
//! # Usage
//!
//! ```
//! use axiom_optimize::surface::extract_surfaces;
//!
//! let source = r#"
//! fn add(a: i32, b: i32) -> i32 {
//!     @strategy {
//!         unroll: ?unroll_factor
//!     }
//!     return a + b;
//! }
//! "#;
//!
//! let surfaces = extract_surfaces(source).expect("extraction should succeed");
//! assert_eq!(surfaces.len(), 1);
//! assert_eq!(surfaces[0].function_name, "add");
//! assert_eq!(surfaces[0].holes.len(), 1);
//! assert_eq!(surfaces[0].holes[0].name, "unroll_factor");
//! ```

use std::fmt;

use axiom_hir::hir::{
    HirAnnotation, HirAnnotationKind, HirBlock, HirExpr, HirExprKind, HirFunction, HirModule,
    HirStmtKind,
};
use axiom_parser::ast::{AnnotationValue, StrategyBlock, StrategyValue};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// An optimization surface discovered in an AXIOM program.
///
/// Each surface corresponds to a single function that contains at least one
/// `@strategy` block or `?hole` expression.
#[derive(Debug, Clone)]
pub struct OptSurface {
    /// Name of the function this surface belongs to.
    pub function_name: String,
    /// All optimisation holes discovered in this function.
    pub holes: Vec<OptHole>,
    /// Strategy metadata extracted from the `@strategy` annotation, if present.
    pub strategy: Option<StrategyInfo>,
}

/// A single optimization hole (`?name`) with optional type and range metadata.
#[derive(Debug, Clone)]
pub struct OptHole {
    /// The hole name (without the leading `?`).
    pub name: String,
    /// The inferred or declared type of this hole.
    pub hole_type: HoleType,
    /// Optional valid range `(lo, hi)` inclusive.
    pub range: Option<(i64, i64)>,
    /// Current concrete value, if one has been assigned.
    pub current_value: Option<Value>,
}

/// The type of an optimization hole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoleType {
    /// Unsigned 32-bit integer.
    U32,
    /// Signed 32-bit integer.
    I32,
    /// 64-bit floating-point number.
    F64,
    /// Boolean.
    Bool,
    /// An identifier (e.g., a loop variable name).
    Ident,
    /// A homogeneous array of the given element type.
    Array(Box<HoleType>),
}

impl fmt::Display for HoleType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HoleType::U32 => write!(f, "u32"),
            HoleType::I32 => write!(f, "i32"),
            HoleType::F64 => write!(f, "f64"),
            HoleType::Bool => write!(f, "bool"),
            HoleType::Ident => write!(f, "ident"),
            HoleType::Array(inner) => write!(f, "array[{inner}]"),
        }
    }
}

/// A concrete value that can fill an optimization hole.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Integer value.
    Int(i64),
    /// Floating-point value.
    Float(f64),
    /// Boolean value.
    Bool(bool),
    /// Identifier value (e.g., a loop variable name like `i`, `j`, `k`).
    Ident(String),
    /// Array of values.
    Array(Vec<Value>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Ident(v) => write!(f, "{v}"),
            Value::Array(vs) => {
                write!(f, "[")?;
                for (i, v) in vs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
        }
    }
}

/// Structured information extracted from a `@strategy { ... }` block.
#[derive(Debug, Clone)]
pub struct StrategyInfo {
    /// Named strategy entries: each key maps to either a hole name or a
    /// concrete value.
    pub entries: Vec<StrategyEntry>,
}

/// A single key-value entry inside a `@strategy` block.
#[derive(Debug, Clone)]
pub struct StrategyEntry {
    /// The key (e.g., `tiling`, `order`, `unroll`).
    pub key: String,
    /// The value — either a hole reference, a concrete value, or a sub-map.
    pub value: StrategyEntryValue,
}

/// The value side of a strategy entry.
#[derive(Debug, Clone)]
pub enum StrategyEntryValue {
    /// A direct hole reference (e.g., `?unroll_factor`).
    Hole(String),
    /// A concrete value.
    Concrete(Value),
    /// A sub-map of entries (e.g., `{ M: ?tile_m, N: ?tile_n }`).
    Map(Vec<StrategyEntry>),
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// Extract all optimization surfaces from an AXIOM source string.
///
/// The function parses the source, lowers it to HIR, then walks every function
/// looking for `@strategy` annotations and `?hole` expressions.
///
/// # Errors
///
/// Returns `Err(Vec<String>)` if the source fails to parse or lower.
pub fn extract_surfaces(source: &str) -> Result<Vec<OptSurface>, Vec<String>> {
    let parse_result = axiom_parser::parse(source);
    if parse_result.has_errors() {
        return Err(parse_result
            .errors
            .iter()
            .map(|e| format!("{e}"))
            .collect());
    }

    let hir_module = axiom_hir::lower(&parse_result.module).map_err(|errs| {
        errs.iter().map(|e| format!("{e}")).collect::<Vec<_>>()
    })?;

    Ok(extract_surfaces_from_hir(&hir_module))
}

/// Extract optimization surfaces directly from a pre-lowered HIR module.
///
/// This is useful when the caller already has a [`HirModule`] and does not
/// want to re-parse the source.
pub fn extract_surfaces_from_hir(module: &HirModule) -> Vec<OptSurface> {
    let mut surfaces = Vec::new();

    for func in &module.functions {
        let mut holes: Vec<OptHole> = Vec::new();
        let mut strategy_info: Option<StrategyInfo> = None;

        // 1. Look for @strategy annotations on the function itself
        extract_strategy_from_annotations(&func.annotations, &mut holes, &mut strategy_info);

        // 2. Look for @strategy annotations on the function body block
        extract_strategy_from_annotations(
            &func.body.annotations,
            &mut holes,
            &mut strategy_info,
        );

        // 3. Walk body for standalone ?hole expressions and nested blocks
        walk_block_for_holes(&func.body, &mut holes);

        // 4. Walk annotations on function for strategy blocks on inner blocks
        walk_function_inner_blocks(func, &mut holes, &mut strategy_info);

        // Deduplicate holes by name
        deduplicate_holes(&mut holes);

        if !holes.is_empty() || strategy_info.is_some() {
            surfaces.push(OptSurface {
                function_name: func.name.clone(),
                holes,
                strategy: strategy_info,
            });
        }
    }

    surfaces
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract strategy information from a list of HIR annotations.
fn extract_strategy_from_annotations(
    annotations: &[HirAnnotation],
    holes: &mut Vec<OptHole>,
    strategy_info: &mut Option<StrategyInfo>,
) {
    for ann in annotations {
        if let HirAnnotationKind::Strategy(ref sb) = ann.kind {
            let info = extract_strategy_block(sb, holes);
            *strategy_info = Some(info);
        }
    }
}

/// Convert a parsed [`StrategyBlock`] into a [`StrategyInfo`], collecting holes
/// along the way.
fn extract_strategy_block(block: &StrategyBlock, holes: &mut Vec<OptHole>) -> StrategyInfo {
    let mut entries = Vec::new();
    for (key, value) in &block.entries {
        entries.push(extract_strategy_entry(key, value, holes));
    }
    StrategyInfo { entries }
}

/// Convert a single strategy key-value pair into a [`StrategyEntry`].
fn extract_strategy_entry(
    key: &str,
    value: &StrategyValue,
    holes: &mut Vec<OptHole>,
) -> StrategyEntry {
    let entry_value = match value {
        StrategyValue::Hole(name) => {
            let hole_type = infer_hole_type_from_context(key, name);
            holes.push(OptHole {
                name: name.clone(),
                hole_type,
                range: infer_range_from_key(key),
                current_value: None,
            });
            StrategyEntryValue::Hole(name.clone())
        }
        StrategyValue::Map(sub_entries) => {
            let mut sub = Vec::new();
            for (sub_key, sub_value) in sub_entries {
                sub.push(extract_strategy_entry(sub_key, sub_value, holes));
            }
            StrategyEntryValue::Map(sub)
        }
        StrategyValue::Concrete(av) => {
            let value = annotation_value_to_value(av);
            StrategyEntryValue::Concrete(value)
        }
    };
    StrategyEntry {
        key: key.to_string(),
        value: entry_value,
    }
}

/// Infer the hole type from the strategy key and hole name context.
///
/// This uses simple heuristics based on common AXIOM strategy key and hole names:
/// - Keys/names containing `order` or `dims` → `Array(Ident)`
/// - Keys/names containing `factor`, `distance`, `prefetch`, `unroll` → `U32`
/// - Keys that are `parallel` → `Array(Ident)` (parallel dimensions)
/// - Otherwise → `U32`
fn infer_hole_type_from_context(key: &str, hole_name: &str) -> HoleType {
    let key_lower = key.to_lowercase();
    let name_lower = hole_name.to_lowercase();

    // Check both the key and the hole name for type hints
    if key_lower.contains("order")
        || name_lower.contains("order")
        || key_lower.contains("dims")
        || name_lower.contains("dims")
        || key_lower == "parallel"
    {
        HoleType::Array(Box::new(HoleType::Ident))
    } else if key_lower.contains("factor")
        || name_lower.contains("factor")
        || key_lower.contains("distance")
        || name_lower.contains("distance")
        || key_lower.contains("prefetch")
        || name_lower.contains("prefetch")
        || key_lower.contains("unroll")
        || name_lower.contains("unroll")
    {
        HoleType::U32
    } else {
        // For tiling sub-keys (M, N, K) or other numeric params, default to U32
        HoleType::U32
    }
}

/// Infer a sensible default range from the strategy key.
fn infer_range_from_key(key: &str) -> Option<(i64, i64)> {
    let key_lower = key.to_lowercase();
    if key_lower.contains("unroll") || key_lower.contains("factor") {
        Some((1, 32))
    } else if key_lower.contains("prefetch") || key_lower.contains("distance") {
        Some((0, 16))
    } else if key_lower == "m" || key_lower == "n" || key_lower == "k" {
        // Common tiling dimensions
        Some((1, 512))
    } else {
        None
    }
}

/// Convert an [`AnnotationValue`] to a [`Value`].
fn annotation_value_to_value(av: &AnnotationValue) -> Value {
    match av {
        AnnotationValue::Int(v) => Value::Int(*v),
        AnnotationValue::Float(v) => Value::Float(*v),
        AnnotationValue::Bool(v) => Value::Bool(*v),
        AnnotationValue::String(s) => Value::Ident(s.clone()),
        AnnotationValue::Ident(s) => Value::Ident(s.clone()),
        AnnotationValue::List(items) => {
            Value::Array(items.iter().map(annotation_value_to_value).collect())
        }
        AnnotationValue::Map(entries) => {
            // Flatten map entries into an array of ident keys for simplicity
            Value::Array(
                entries
                    .iter()
                    .map(|(k, _)| Value::Ident(k.clone()))
                    .collect(),
            )
        }
    }
}

/// Walk a HIR block looking for `?hole` expressions (standalone, not in strategy).
fn walk_block_for_holes(block: &HirBlock, holes: &mut Vec<OptHole>) {
    for stmt in &block.stmts {
        walk_stmt_for_holes(&stmt.kind, holes);
    }
}

/// Walk a HIR statement for `?hole` expressions.
fn walk_stmt_for_holes(kind: &HirStmtKind, holes: &mut Vec<OptHole>) {
    match kind {
        HirStmtKind::Let { value, .. } => {
            if let Some(value) = value {
                walk_expr_for_holes(value, holes);
            }
        }
        HirStmtKind::Assign { target, value } => {
            walk_expr_for_holes(target, holes);
            walk_expr_for_holes(value, holes);
        }
        HirStmtKind::Return { value } => {
            if let Some(value) = value {
                walk_expr_for_holes(value, holes);
            }
        }
        HirStmtKind::If {
            condition,
            then_block,
            else_block,
        } => {
            walk_expr_for_holes(condition, holes);
            walk_block_for_holes(then_block, holes);
            if let Some(eb) = else_block {
                walk_block_for_holes(eb, holes);
            }
        }
        HirStmtKind::For {
            iterable, body, ..
        } => {
            walk_expr_for_holes(iterable, holes);
            walk_block_for_holes(body, holes);
        }
        HirStmtKind::While { condition, body } => {
            walk_expr_for_holes(condition, holes);
            walk_block_for_holes(body, holes);
        }
        HirStmtKind::Break | HirStmtKind::Continue => {}
        HirStmtKind::Expr { expr } => walk_expr_for_holes(expr, holes),
    }
}

/// Walk a HIR expression tree for `?hole` nodes.
fn walk_expr_for_holes(expr: &HirExpr, holes: &mut Vec<OptHole>) {
    match &expr.kind {
        HirExprKind::OptHole { name } => {
            holes.push(OptHole {
                name: name.clone(),
                hole_type: HoleType::U32, // default for standalone holes
                range: None,
                current_value: None,
            });
        }
        HirExprKind::BinaryOp { lhs, rhs, .. } => {
            walk_expr_for_holes(lhs, holes);
            walk_expr_for_holes(rhs, holes);
        }
        HirExprKind::UnaryOp { operand, .. } => {
            walk_expr_for_holes(operand, holes);
        }
        HirExprKind::Call { func, args } => {
            walk_expr_for_holes(func, holes);
            for arg in args {
                walk_expr_for_holes(arg, holes);
            }
        }
        HirExprKind::Index { expr, indices } => {
            walk_expr_for_holes(expr, holes);
            for idx in indices {
                walk_expr_for_holes(idx, holes);
            }
        }
        HirExprKind::FieldAccess { expr, .. } => {
            walk_expr_for_holes(expr, holes);
        }
        HirExprKind::MethodCall { expr, args, .. } => {
            walk_expr_for_holes(expr, holes);
            for arg in args {
                walk_expr_for_holes(arg, holes);
            }
        }
        HirExprKind::StructLiteral { fields, .. } => {
            for (_, expr) in fields {
                walk_expr_for_holes(expr, holes);
            }
        }
        // Literals, idents, and array constructors have no holes
        HirExprKind::IntLiteral { .. }
        | HirExprKind::FloatLiteral { .. }
        | HirExprKind::StringLiteral { .. }
        | HirExprKind::BoolLiteral { .. }
        | HirExprKind::Ident { .. }
        | HirExprKind::ArrayZeros { .. } => {}
    }
}

/// Walk inner blocks inside a function (for/while/if bodies) looking for
/// `@strategy` annotations.
fn walk_function_inner_blocks(
    func: &HirFunction,
    holes: &mut Vec<OptHole>,
    strategy_info: &mut Option<StrategyInfo>,
) {
    walk_inner_blocks(&func.body, holes, strategy_info);
}

/// Recursively walk blocks for `@strategy` annotations on nested blocks.
fn walk_inner_blocks(
    block: &HirBlock,
    holes: &mut Vec<OptHole>,
    strategy_info: &mut Option<StrategyInfo>,
) {
    for stmt in &block.stmts {
        match &stmt.kind {
            HirStmtKind::If {
                then_block,
                else_block,
                ..
            } => {
                extract_strategy_from_annotations(
                    &then_block.annotations,
                    holes,
                    strategy_info,
                );
                walk_inner_blocks(then_block, holes, strategy_info);
                if let Some(eb) = else_block {
                    extract_strategy_from_annotations(&eb.annotations, holes, strategy_info);
                    walk_inner_blocks(eb, holes, strategy_info);
                }
            }
            HirStmtKind::For { body, .. } => {
                extract_strategy_from_annotations(&body.annotations, holes, strategy_info);
                walk_inner_blocks(body, holes, strategy_info);
            }
            HirStmtKind::While { body, .. } => {
                extract_strategy_from_annotations(&body.annotations, holes, strategy_info);
                walk_inner_blocks(body, holes, strategy_info);
            }
            _ => {}
        }
    }
}

/// Remove duplicate holes (by name), keeping the first occurrence.
fn deduplicate_holes(holes: &mut Vec<OptHole>) {
    let mut seen = std::collections::HashSet::new();
    holes.retain(|h| seen.insert(h.name.clone()));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_no_surfaces_from_simple_function() {
        let source = r#"
fn main() -> i32 {
    return 0;
}
"#;
        let surfaces = extract_surfaces(source).expect("should parse");
        assert!(surfaces.is_empty());
    }

    #[test]
    fn extract_single_strategy_block() {
        let source = r#"
fn add(a: i32, b: i32) -> i32 {
    @strategy {
        unroll: ?unroll_factor
    }
    return a + b;
}
"#;
        let surfaces = extract_surfaces(source).expect("should parse");
        assert_eq!(surfaces.len(), 1);

        let s = &surfaces[0];
        assert_eq!(s.function_name, "add");
        assert_eq!(s.holes.len(), 1);
        assert_eq!(s.holes[0].name, "unroll_factor");
        assert_eq!(s.holes[0].hole_type, HoleType::U32);
        // "unroll" key triggers default range (1, 32)
        assert_eq!(s.holes[0].range, Some((1, 32)));

        let strategy = s.strategy.as_ref().expect("should have strategy info");
        assert_eq!(strategy.entries.len(), 1);
        assert_eq!(strategy.entries[0].key, "unroll");
    }

    #[test]
    fn extract_nested_strategy_map() {
        let source = r#"
fn matmul(a: i32, b: i32) -> i32 {
    @strategy {
        tiling: { M: ?tile_m, N: ?tile_n, K: ?tile_k }
        unroll: ?unroll_factor
    }
    return a + b;
}
"#;
        let surfaces = extract_surfaces(source).expect("should parse");
        assert_eq!(surfaces.len(), 1);

        let s = &surfaces[0];
        assert_eq!(s.function_name, "matmul");

        // Should have 4 holes: tile_m, tile_n, tile_k, unroll_factor
        assert_eq!(s.holes.len(), 4);

        let hole_names: Vec<&str> = s.holes.iter().map(|h| h.name.as_str()).collect();
        assert!(hole_names.contains(&"tile_m"));
        assert!(hole_names.contains(&"tile_n"));
        assert!(hole_names.contains(&"tile_k"));
        assert!(hole_names.contains(&"unroll_factor"));

        // tile_m has M as sub-key → range (1, 512)
        let tile_m = s.holes.iter().find(|h| h.name == "tile_m").expect("tile_m");
        assert_eq!(tile_m.range, Some((1, 512)));

        let strategy = s.strategy.as_ref().expect("should have strategy info");
        assert_eq!(strategy.entries.len(), 2);
    }

    #[test]
    fn extract_multiple_functions() {
        let source = r#"
fn foo(x: i32) -> i32 {
    @strategy {
        unroll: ?uf
    }
    return x;
}

fn bar(y: i32) -> i32 {
    return y;
}

fn baz(z: i32) -> i32 {
    @strategy {
        prefetch: ?pd
    }
    return z;
}
"#;
        let surfaces = extract_surfaces(source).expect("should parse");
        assert_eq!(surfaces.len(), 2);
        assert_eq!(surfaces[0].function_name, "foo");
        assert_eq!(surfaces[1].function_name, "baz");
    }

    #[test]
    fn extract_surfaces_rejects_invalid_source() {
        let source = "this is not valid AXIOM at all @@@ {}{}{}";
        let result = extract_surfaces(source);
        assert!(result.is_err());
    }

    #[test]
    fn extract_with_strategy_containing_multiple_holes() {
        let source = r#"
fn compute(a: i32, b: i32) -> i32 {
    @strategy {
        tiling:   { M: ?tile_m, N: ?tile_n, K: ?tile_k }
        order:    ?loop_order
        parallel: ?parallel_dims
        unroll:   ?unroll_factor
        prefetch: ?prefetch_distance
    }
    return a + b;
}
"#;
        let surfaces = extract_surfaces(source).expect("should parse");
        assert_eq!(surfaces.len(), 1);

        let s = &surfaces[0];
        assert_eq!(s.holes.len(), 7);

        // Check types
        let order_hole = s
            .holes
            .iter()
            .find(|h| h.name == "loop_order")
            .expect("loop_order");
        assert_eq!(order_hole.hole_type, HoleType::Array(Box::new(HoleType::Ident)));

        let parallel_hole = s
            .holes
            .iter()
            .find(|h| h.name == "parallel_dims")
            .expect("parallel_dims");
        assert_eq!(
            parallel_hole.hole_type,
            HoleType::Array(Box::new(HoleType::Ident))
        );

        let unroll_hole = s
            .holes
            .iter()
            .find(|h| h.name == "unroll_factor")
            .expect("unroll_factor");
        assert_eq!(unroll_hole.hole_type, HoleType::U32);
        assert_eq!(unroll_hole.range, Some((1, 32)));

        let prefetch_hole = s
            .holes
            .iter()
            .find(|h| h.name == "prefetch_distance")
            .expect("prefetch_distance");
        assert_eq!(prefetch_hole.hole_type, HoleType::U32);
        assert_eq!(prefetch_hole.range, Some((0, 16)));
    }

    #[test]
    fn hole_type_display() {
        assert_eq!(format!("{}", HoleType::U32), "u32");
        assert_eq!(format!("{}", HoleType::Bool), "bool");
        assert_eq!(
            format!("{}", HoleType::Array(Box::new(HoleType::Ident))),
            "array[ident]"
        );
    }

    #[test]
    fn value_display() {
        assert_eq!(format!("{}", Value::Int(42)), "42");
        assert_eq!(format!("{}", Value::Float(3.14)), "3.14");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::Ident("i".into())), "i");
        assert_eq!(
            format!("{}", Value::Array(vec![Value::Ident("i".into()), Value::Ident("j".into())])),
            "[i, j]"
        );
    }

    #[test]
    fn deduplicate_holes_works() {
        let mut holes = vec![
            OptHole {
                name: "a".to_string(),
                hole_type: HoleType::U32,
                range: None,
                current_value: None,
            },
            OptHole {
                name: "b".to_string(),
                hole_type: HoleType::U32,
                range: None,
                current_value: None,
            },
            OptHole {
                name: "a".to_string(),
                hole_type: HoleType::I32,
                range: Some((1, 10)),
                current_value: None,
            },
        ];
        deduplicate_holes(&mut holes);
        assert_eq!(holes.len(), 2);
        assert_eq!(holes[0].name, "a");
        assert_eq!(holes[1].name, "b");
    }
}
