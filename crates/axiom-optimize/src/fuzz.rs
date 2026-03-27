//! Fuzz testing support: extract value ranges from `@precondition` annotations
//! and generate test inputs that explore boundary conditions.
//!
//! When a function has `@precondition(x >= 0 and x < 100)`, the fuzzer extracts
//! the range `[0, 99]` for parameter `x` and generates boundary values (min, max,
//! min+1, max-1, midpoint) plus evenly-spaced samples.

use std::collections::HashMap;

use axiom_hir::{BinOp, HirExpr, HirExprKind, HirParam};

/// A range of valid values for a single function parameter, extracted from
/// `@precondition` constraints.
#[derive(Debug, Clone)]
pub struct FuzzRange {
    /// The parameter name this range applies to.
    pub param_name: String,
    /// Minimum integer value (inclusive), if constrained.
    pub min: Option<i64>,
    /// Maximum integer value (inclusive), if constrained.
    pub max: Option<i64>,
    /// Minimum float value (inclusive), if constrained.
    pub min_f: Option<f64>,
    /// Maximum float value (inclusive), if constrained.
    pub max_f: Option<f64>,
}

impl FuzzRange {
    /// Create a new unconstrained range for the given parameter.
    pub fn new(name: &str) -> Self {
        Self {
            param_name: name.to_string(),
            min: None,
            max: None,
            min_f: None,
            max_f: None,
        }
    }
}

/// Extract value ranges from `@precondition` expressions for the given parameters.
///
/// Handles comparisons like `x >= 0`, `x < 100`, `x > 0 and x <= 50`, and
/// logical `and` conjunctions.
pub fn extract_fuzz_ranges(preconditions: &[HirExpr], params: &[HirParam]) -> Vec<FuzzRange> {
    let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    let mut ranges: HashMap<String, FuzzRange> = HashMap::new();

    // Initialize ranges for all parameters.
    for name in &param_names {
        ranges.insert(name.clone(), FuzzRange::new(name));
    }

    for pre in preconditions {
        walk_precondition_for_ranges(pre, &mut ranges);
    }

    // Return ranges only for parameters that exist.
    param_names
        .iter()
        .filter_map(|name| ranges.remove(name))
        .collect()
}

/// Walk a precondition expression tree and populate ranges.
fn walk_precondition_for_ranges(expr: &HirExpr, ranges: &mut HashMap<String, FuzzRange>) {
    match &expr.kind {
        // x >= N -> min = N
        HirExprKind::BinaryOp {
            op: BinOp::GtEq,
            lhs,
            rhs,
        } => {
            if let (HirExprKind::Ident { name }, HirExprKind::IntLiteral { value }) =
                (&lhs.kind, &rhs.kind)
            {
                ranges
                    .entry(name.clone())
                    .or_insert_with(|| FuzzRange::new(name))
                    .min = Some(*value as i64);
            }
            // N <= x  (same as x >= N)
            if let (HirExprKind::IntLiteral { value }, HirExprKind::Ident { name }) =
                (&lhs.kind, &rhs.kind)
            {
                ranges
                    .entry(name.clone())
                    .or_insert_with(|| FuzzRange::new(name))
                    .max = Some(*value as i64);
            }
        }
        // x > N -> min = N + 1
        HirExprKind::BinaryOp {
            op: BinOp::Gt,
            lhs,
            rhs,
        } => {
            if let (HirExprKind::Ident { name }, HirExprKind::IntLiteral { value }) =
                (&lhs.kind, &rhs.kind)
            {
                ranges
                    .entry(name.clone())
                    .or_insert_with(|| FuzzRange::new(name))
                    .min = Some(*value as i64 + 1);
            }
        }
        // x < N -> max = N - 1
        HirExprKind::BinaryOp {
            op: BinOp::Lt,
            lhs,
            rhs,
        } => {
            if let (HirExprKind::Ident { name }, HirExprKind::IntLiteral { value }) =
                (&lhs.kind, &rhs.kind)
            {
                ranges
                    .entry(name.clone())
                    .or_insert_with(|| FuzzRange::new(name))
                    .max = Some(*value as i64 - 1);
            }
        }
        // x <= N -> max = N
        HirExprKind::BinaryOp {
            op: BinOp::LtEq,
            lhs,
            rhs,
        } => {
            if let (HirExprKind::Ident { name }, HirExprKind::IntLiteral { value }) =
                (&lhs.kind, &rhs.kind)
            {
                ranges
                    .entry(name.clone())
                    .or_insert_with(|| FuzzRange::new(name))
                    .max = Some(*value as i64);
            }
        }
        // and: recurse into both sides
        HirExprKind::BinaryOp {
            op: BinOp::And,
            lhs,
            rhs,
        } => {
            walk_precondition_for_ranges(lhs, ranges);
            walk_precondition_for_ranges(rhs, ranges);
        }
        _ => {}
    }
}

/// Generate a set of test inputs from the extracted ranges.
///
/// For each parameter, generates boundary values (min, max, min+1, max-1,
/// midpoint) plus evenly-spaced samples within the range. For multi-parameter
/// functions, computes a bounded Cartesian product.
pub fn generate_fuzz_inputs(ranges: &[FuzzRange], count: usize) -> Vec<Vec<i64>> {
    if ranges.is_empty() {
        return vec![];
    }

    // For each parameter range, collect candidate values.
    let mut param_values: Vec<Vec<i64>> = Vec::new();
    for range in ranges {
        let mut vals = Vec::new();
        let min = range.min.unwrap_or(0);
        let max = range.max.unwrap_or(100);
        vals.push(min);
        vals.push(max);
        if max > min {
            vals.push(min + 1);
            vals.push(max - 1);
            vals.push((min + max) / 2);
        }
        // Add evenly-spaced samples.
        let step = std::cmp::max(1, (max - min) / (count as i64));
        let mut v = min;
        while v <= max && vals.len() < count {
            if !vals.contains(&v) {
                vals.push(v);
            }
            v += step;
        }
        param_values.push(vals);
    }

    // Cartesian product (bounded by `count`).
    let mut inputs = Vec::new();
    match param_values.len() {
        0 => {}
        1 => {
            for v in &param_values[0] {
                inputs.push(vec![*v]);
                if inputs.len() >= count {
                    break;
                }
            }
        }
        2 => {
            'outer2: for a in &param_values[0] {
                for b in &param_values[1] {
                    inputs.push(vec![*a, *b]);
                    if inputs.len() >= count {
                        break 'outer2;
                    }
                }
            }
        }
        _ => {
            // For 3+ parameters, use the first values from each range.
            let max_per_param = std::cmp::max(2, (count as f64).powf(1.0 / param_values.len() as f64) as usize);
            let mut indices: Vec<usize> = vec![0; param_values.len()];
            loop {
                let input: Vec<i64> = indices
                    .iter()
                    .enumerate()
                    .map(|(i, &idx)| param_values[i][idx.min(param_values[i].len() - 1)])
                    .collect();
                inputs.push(input);
                if inputs.len() >= count {
                    break;
                }
                // Increment indices (odometer style).
                let mut carry = true;
                for i in (0..indices.len()).rev() {
                    if carry {
                        indices[i] += 1;
                        if indices[i] >= max_per_param.min(param_values[i].len()) {
                            indices[i] = 0;
                        } else {
                            carry = false;
                        }
                    }
                }
                if carry {
                    break; // All combinations exhausted.
                }
            }
        }
    }

    inputs.truncate(count);
    inputs
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_hir::{HirExpr, HirExprKind, NodeId, SPAN_DUMMY};

    fn int_lit(v: i128) -> HirExpr {
        HirExpr {
            id: NodeId(0),
            kind: HirExprKind::IntLiteral { value: v },
            span: SPAN_DUMMY,
        }
    }

    fn ident(name: &str) -> HirExpr {
        HirExpr {
            id: NodeId(0),
            kind: HirExprKind::Ident {
                name: name.to_string(),
            },
            span: SPAN_DUMMY,
        }
    }

    fn binop(op: BinOp, lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr {
            id: NodeId(0),
            kind: HirExprKind::BinaryOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span: SPAN_DUMMY,
        }
    }

    fn make_param(name: &str) -> HirParam {
        HirParam {
            id: NodeId(0),
            name: name.to_string(),
            name_span: SPAN_DUMMY,
            ty: axiom_hir::HirType::Primitive(axiom_hir::PrimitiveType::I32),
            annotations: vec![],
        }
    }

    #[test]
    fn test_extract_range_gteq_lt() {
        // x >= 0 and x < 100
        let pre = binop(
            BinOp::And,
            binop(BinOp::GtEq, ident("x"), int_lit(0)),
            binop(BinOp::Lt, ident("x"), int_lit(100)),
        );
        let params = vec![make_param("x")];
        let ranges = extract_fuzz_ranges(&[pre], &params);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].param_name, "x");
        assert_eq!(ranges[0].min, Some(0));
        assert_eq!(ranges[0].max, Some(99));
    }

    #[test]
    fn test_extract_range_gt_lteq() {
        // x > 0 and x <= 50
        let pre = binop(
            BinOp::And,
            binop(BinOp::Gt, ident("x"), int_lit(0)),
            binop(BinOp::LtEq, ident("x"), int_lit(50)),
        );
        let params = vec![make_param("x")];
        let ranges = extract_fuzz_ranges(&[pre], &params);
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].min, Some(1));
        assert_eq!(ranges[0].max, Some(50));
    }

    #[test]
    fn test_generate_fuzz_inputs_single_param() {
        let ranges = vec![FuzzRange {
            param_name: "x".to_string(),
            min: Some(0),
            max: Some(99),
            min_f: None,
            max_f: None,
        }];
        let inputs = generate_fuzz_inputs(&ranges, 10);
        assert!(!inputs.is_empty());
        assert!(inputs.len() <= 10);
        // All values should be in range [0, 99].
        for input in &inputs {
            assert_eq!(input.len(), 1);
            assert!(input[0] >= 0 && input[0] <= 99);
        }
        // Boundary values should be present.
        assert!(inputs.iter().any(|i| i[0] == 0));
        assert!(inputs.iter().any(|i| i[0] == 99));
    }

    #[test]
    fn test_generate_fuzz_inputs_two_params() {
        let ranges = vec![
            FuzzRange {
                param_name: "a".to_string(),
                min: Some(0),
                max: Some(10),
                min_f: None,
                max_f: None,
            },
            FuzzRange {
                param_name: "b".to_string(),
                min: Some(1),
                max: Some(5),
                min_f: None,
                max_f: None,
            },
        ];
        let inputs = generate_fuzz_inputs(&ranges, 20);
        assert!(!inputs.is_empty());
        for input in &inputs {
            assert_eq!(input.len(), 2);
            assert!(input[0] >= 0 && input[0] <= 10);
            assert!(input[1] >= 1 && input[1] <= 5);
        }
    }

    #[test]
    fn test_generate_fuzz_inputs_empty_ranges() {
        let inputs = generate_fuzz_inputs(&[], 10);
        assert!(inputs.is_empty());
    }

    #[test]
    fn test_extract_no_preconditions() {
        let params = vec![make_param("x")];
        let ranges = extract_fuzz_ranges(&[], &params);
        assert_eq!(ranges.len(), 1);
        // Unconstrained: min/max should be None.
        assert_eq!(ranges[0].min, None);
        assert_eq!(ranges[0].max, None);
    }
}
