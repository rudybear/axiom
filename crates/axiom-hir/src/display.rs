//! HIR pretty-printer for `--emit=hir` output.
//!
//! Implements [`std::fmt::Display`] for [`HirModule`] and all HIR node types.
//! The output looks like AXIOM source code annotated with node IDs in comments.
//! This powers the `--emit=hir` flag in the CLI driver.

use std::fmt;

use crate::hir::*;

/// Convenience function that pretty-prints an [`HirModule`] to a string.
pub fn display_hir(module: &HirModule) -> String {
    format!("{module}")
}

impl fmt::Display for HirModule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Module header
        if let Some(ref name) = self.name {
            writeln!(f, "// HIR Module: {name}")?;
        } else {
            writeln!(f, "// HIR Module: <unnamed>")?;
        }

        // Module-level annotations
        for ann in &self.annotations {
            writeln!(f, "{ann}")?;
        }

        if !self.annotations.is_empty() {
            writeln!(f)?;
        }

        // Imports
        for imp in &self.imports {
            writeln!(f, "{imp}")?;
        }
        if !self.imports.is_empty() {
            writeln!(f)?;
        }

        // Type aliases
        for ta in &self.type_aliases {
            writeln!(f, "{ta}")?;
        }
        if !self.type_aliases.is_empty() {
            writeln!(f)?;
        }

        // Extern functions
        for ef in &self.extern_functions {
            write!(f, "{ef}")?;
            writeln!(f)?;
        }

        // Structs
        for (i, s) in self.structs.iter().enumerate() {
            write!(f, "{s}")?;
            if i + 1 < self.structs.len() || !self.functions.is_empty() {
                writeln!(f)?;
            }
        }

        // Functions
        for (i, func) in self.functions.iter().enumerate() {
            write!(f, "{func}")?;
            if i + 1 < self.functions.len() {
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

impl fmt::Display for HirFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for ann in &self.annotations {
            writeln!(f, "{ann}")?;
        }

        write!(f, "fn {}(", self.name)?;
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{param}")?;
        }
        write!(f, ") -> {}",  self.return_type)?;
        writeln!(f, " {{  // [node:{}]", self.id)?;
        write_block_contents(f, &self.body, 1)?;
        writeln!(f, "}}")?;
        Ok(())
    }
}

impl fmt::Display for HirExternFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for ann in &self.annotations {
            writeln!(f, "{ann}")?;
        }

        if self.convention != "C" {
            write!(f, "extern \"{}\" fn {}(", self.convention, self.name)?;
        } else {
            write!(f, "extern fn {}(", self.name)?;
        }
        for (i, param) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{param}")?;
        }
        if self.is_variadic {
            if !self.params.is_empty() {
                write!(f, ", ")?;
            }
            write!(f, "...")?;
        }
        writeln!(f, ") -> {};  // [node:{}]", self.return_type, self.id)?;
        Ok(())
    }
}

impl fmt::Display for HirParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for ann in &self.annotations {
            write!(f, "{ann} ")?;
        }
        write!(f, "{}: {}", self.name, self.ty)
    }
}

impl fmt::Display for HirStruct {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for ann in &self.annotations {
            writeln!(f, "{ann}")?;
        }
        writeln!(f, "struct {} {{  // [node:{}]", self.name, self.id)?;
        for field in &self.fields {
            write!(f, "    ")?;
            for ann in &field.annotations {
                write!(f, "{ann} ")?;
            }
            writeln!(f, "{}: {};  // [node:{}]", field.name, field.ty, field.id)?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}

impl fmt::Display for HirTypeAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "type {} = {};  // [node:{}]",
            self.name, self.ty, self.id
        )
    }
}

impl fmt::Display for HirImport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "import {}", self.path.join("::"))?;
        if let Some(ref alias) = self.alias {
            write!(f, " as {alias}")?;
        }
        write!(f, ";  // [node:{}]", self.id)
    }
}

/// Write the contents of a block (statements) at the given indentation level.
fn write_block_contents(f: &mut fmt::Formatter<'_>, block: &HirBlock, indent: usize) -> fmt::Result {
    let prefix = "    ".repeat(indent);

    for ann in &block.annotations {
        writeln!(f, "{prefix}{ann}")?;
    }

    for stmt in &block.stmts {
        write_stmt(f, stmt, indent)?;
    }
    Ok(())
}

/// Write a single statement at the given indentation level.
fn write_stmt(f: &mut fmt::Formatter<'_>, stmt: &HirStmt, indent: usize) -> fmt::Result {
    let prefix = "    ".repeat(indent);

    // Display any annotations on this statement.
    for ann in &stmt.annotations {
        writeln!(f, "{prefix}{ann}")?;
    }

    match &stmt.kind {
        HirStmtKind::Let {
            name,
            ty,
            value,
            mutable,
            ..
        } => {
            let mut_kw = if *mutable { "mut " } else { "" };
            if let Some(val) = value {
                writeln!(
                    f,
                    "{prefix}let {mut_kw}{name}: {ty} = {val};  // [node:{}]",
                    stmt.id
                )?;
            } else {
                writeln!(
                    f,
                    "{prefix}let {mut_kw}{name}: {ty};  // [node:{}]",
                    stmt.id
                )?;
            }
        }
        HirStmtKind::Assign { target, value } => {
            writeln!(
                f,
                "{prefix}{target} = {value};  // [node:{}]",
                stmt.id
            )?;
        }
        HirStmtKind::Return { value } => {
            if let Some(value) = value {
                writeln!(f, "{prefix}return {value};  // [node:{}]", stmt.id)?;
            } else {
                writeln!(f, "{prefix}return;  // [node:{}]", stmt.id)?;
            }
        }
        HirStmtKind::If {
            condition,
            then_block,
            else_block,
        } => {
            writeln!(f, "{prefix}if {condition} {{  // [node:{}]", stmt.id)?;
            write_block_contents(f, then_block, indent + 1)?;
            if let Some(else_blk) = else_block {
                writeln!(f, "{prefix}}} else {{")?;
                write_block_contents(f, else_blk, indent + 1)?;
            }
            writeln!(f, "{prefix}}}")?;
        }
        HirStmtKind::For {
            var,
            var_type,
            iterable,
            body,
            ..
        } => {
            writeln!(
                f,
                "{prefix}for {var}: {var_type} in {iterable} {{  // [node:{}]",
                stmt.id
            )?;
            write_block_contents(f, body, indent + 1)?;
            writeln!(f, "{prefix}}}")?;
        }
        HirStmtKind::While {
            condition, body, ..
        } => {
            writeln!(f, "{prefix}while {condition} {{  // [node:{}]", stmt.id)?;
            write_block_contents(f, body, indent + 1)?;
            writeln!(f, "{prefix}}}")?;
        }
        HirStmtKind::Break => {
            writeln!(f, "{prefix}break;  // [node:{}]", stmt.id)?;
        }
        HirStmtKind::Continue => {
            writeln!(f, "{prefix}continue;  // [node:{}]", stmt.id)?;
        }
        HirStmtKind::Expr { expr } => {
            writeln!(f, "{prefix}{expr};  // [node:{}]", stmt.id)?;
        }
    }
    Ok(())
}

impl fmt::Display for HirExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            HirExprKind::IntLiteral { value } => write!(f, "{value}"),
            HirExprKind::FloatLiteral { value } => {
                if value.fract() == 0.0 && !value.is_nan() && !value.is_infinite() {
                    write!(f, "{value:.1}")
                } else {
                    write!(f, "{value}")
                }
            }
            HirExprKind::StringLiteral { value } => write!(f, "\"{value}\""),
            HirExprKind::BoolLiteral { value } => write!(f, "{value}"),
            HirExprKind::Ident { name } => write!(f, "{name}"),
            HirExprKind::OptHole { name } => write!(f, "?{name}"),
            HirExprKind::BinaryOp { op, lhs, rhs } => {
                write!(f, "{lhs} {} {rhs}", fmt_binop(op))
            }
            HirExprKind::UnaryOp { op, operand } => {
                write!(f, "{}{operand}", fmt_unaryop(op))
            }
            HirExprKind::Call { func, args } => {
                write!(f, "{func}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
            HirExprKind::Index { expr, indices } => {
                write!(f, "{expr}[")?;
                for (i, idx) in indices.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{idx}")?;
                }
                write!(f, "]")
            }
            HirExprKind::FieldAccess { expr, field } => {
                write!(f, "{expr}.{field}")
            }
            HirExprKind::MethodCall { expr, method, args } => {
                write!(f, "{expr}.{method}(")?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
            HirExprKind::ArrayZeros {
                element_type,
                size,
            } => {
                write!(f, "array_zeros[{element_type}, {size}]")
            }
            HirExprKind::StructLiteral { type_name, fields } => {
                write!(f, "{type_name} {{ ")?;
                for (i, (name, expr)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {expr}")?;
                }
                write!(f, " }}")
            }
            HirExprKind::TupleLiteral { elements } => {
                write!(f, "(")?;
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{elem}")?;
                }
                write!(f, ")")
            }
        }
    }
}

impl fmt::Display for HirType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Primitive(p) => write!(f, "{p}"),
            Self::UserDefined(name) => write!(f, "{name}"),
            Self::Tensor { element, dims } => {
                write!(f, "tensor[{element}")?;
                for dim in dims {
                    write!(f, ", {dim}")?;
                }
                write!(f, "]")
            }
            Self::Array { element, size } => {
                write!(f, "array[{element}, {size}]")
            }
            Self::Slice { element } => write!(f, "slice[{element}]"),
            Self::Ptr { element } => write!(f, "ptr[{element}]"),
            Self::ReadonlyPtr { element } => write!(f, "readonly_ptr[{element}]"),
            Self::WriteonlyPtr { element } => write!(f, "writeonly_ptr[{element}]"),
            Self::Tuple { elements } => {
                write!(f, "(")?;
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{elem}")?;
                }
                write!(f, ")")
            }
            Self::Fn { params, ret } => {
                write!(f, "fn(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{param}")?;
                }
                write!(f, ") -> {ret}")
            }
            Self::Unknown(name) => write!(f, "?{name}"),
        }
    }
}

impl fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::F16 => "f16",
            Self::Bf16 => "bf16",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Bool => "bool",
            Self::Vec2 => "vec2",
            Self::Vec3 => "vec3",
            Self::Vec4 => "vec4",
            Self::IVec2 => "ivec2",
            Self::IVec3 => "ivec3",
            Self::IVec4 => "ivec4",
            Self::FVec2 => "fvec2",
            Self::FVec3 => "fvec3",
            Self::FVec4 => "fvec4",
            Self::Mat3 => "mat3",
            Self::Mat4 => "mat4",
        };
        write!(f, "{s}")
    }
}

impl fmt::Display for HirDimExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Const(v) => write!(f, "{v}"),
            Self::Named(name) => write!(f, "{name}"),
            Self::Dynamic => write!(f, "?"),
        }
    }
}

/// Format a binary operator as its AXIOM source representation.
fn fmt_binop(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::AddWrap => "+%",
        BinOp::AddSat => "+|",
        BinOp::SubWrap => "-%",
        BinOp::SubSat => "-|",
        BinOp::MulWrap => "*%",
        BinOp::Eq => "==",
        BinOp::NotEq => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::LtEq => "<=",
        BinOp::GtEq => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

/// Format a unary operator as its AXIOM source representation.
fn fmt_unaryop(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "not ",
    }
}

impl fmt::Display for HirAnnotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            HirAnnotationKind::Pure => write!(f, "@pure"),
            HirAnnotationKind::Const => write!(f, "@const"),
            HirAnnotationKind::Inline(hint) => {
                let h = match hint {
                    InlineHint::Always => "always",
                    InlineHint::Never => "never",
                    InlineHint::Hint => "hint",
                };
                write!(f, "@inline({h})")
            }
            HirAnnotationKind::Complexity(expr) => write!(f, "@complexity {expr}"),
            HirAnnotationKind::Intent(desc) => write!(f, "@intent(\"{desc}\")"),
            HirAnnotationKind::Module(name) => write!(f, "@module({name})"),
            HirAnnotationKind::Constraint(entries) => {
                write!(f, "@constraint {{ ")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: ")?;
                    write_annotation_value(f, v)?;
                }
                write!(f, " }}")
            }
            HirAnnotationKind::Target(targets) => {
                write!(f, "@target(")?;
                write!(f, "{}", targets.join(", "))?;
                write!(f, ")")
            }
            HirAnnotationKind::Strategy(_block) => {
                write!(f, "@strategy {{ ... }}")
            }
            HirAnnotationKind::Transfer(_block) => {
                write!(f, "@transfer {{ ... }}")
            }
            HirAnnotationKind::Vectorizable(dims) => {
                write!(f, "@vectorizable(")?;
                write!(f, "{}", dims.join(", "))?;
                write!(f, ")")
            }
            HirAnnotationKind::Parallel(dims) => {
                write!(f, "@parallel(")?;
                write!(f, "{}", dims.join(", "))?;
                write!(f, ")")
            }
            HirAnnotationKind::Layout(kind) => {
                let k = match kind {
                    LayoutKind::RowMajor => "row_major",
                    LayoutKind::ColMajor => "col_major",
                    LayoutKind::Custom(s) => s.as_str(),
                };
                write!(f, "@layout({k})")
            }
            HirAnnotationKind::Align(bytes) => write!(f, "@align({bytes})"),
            HirAnnotationKind::OptimizationLog(_) => {
                write!(f, "@optimization_log {{ ... }}")
            }
            HirAnnotationKind::Export => write!(f, "@export"),
            HirAnnotationKind::Lifetime(scope) => write!(f, "@lifetime({scope})"),
            HirAnnotationKind::ParallelFor(config) => {
                write!(f, "@parallel_for(")?;
                let mut first = true;
                if !config.shared_read.is_empty() {
                    write!(f, "shared_read: [{}]", config.shared_read.join(", "))?;
                    first = false;
                }
                if !config.shared_write.is_empty() {
                    if !first { write!(f, ", ")?; }
                    write!(f, "shared_write: [{}]", config.shared_write.join(", "))?;
                    first = false;
                }
                for (op, var) in &config.reductions {
                    if !first { write!(f, ", ")?; }
                    write!(f, "reduction({op}: {var})")?;
                    first = false;
                }
                if !config.private.is_empty() {
                    if !first { write!(f, ", ")?; }
                    write!(f, "private: [{}]", config.private.join(", "))?;
                }
                write!(f, ")")
            }
            HirAnnotationKind::Strict => write!(f, "@strict"),
            HirAnnotationKind::Precondition(expr) => write!(f, "@precondition({expr})"),
            HirAnnotationKind::Postcondition(expr) => write!(f, "@postcondition({expr})"),
            HirAnnotationKind::Test(tc) => {
                write!(f, "@test {{ input: (")?;
                for (i, input) in tc.inputs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{input}")?;
                }
                write!(f, "), expect: {} }}", tc.expected)
            }
            HirAnnotationKind::Link { library, kind } => {
                write!(f, "@link(library: \"{library}\", kind: \"{kind}\")")
            }
            HirAnnotationKind::Trace => write!(f, "@trace"),
            HirAnnotationKind::Cfg(target) => write!(f, "@cfg(\"{target}\")"),
            HirAnnotationKind::Requires(expr) => write!(f, "@requires({expr})"),
            HirAnnotationKind::Ensures(expr) => write!(f, "@ensures({expr})"),
            HirAnnotationKind::Invariant(expr) => write!(f, "@invariant({expr})"),
            HirAnnotationKind::Custom(name, args) => {
                write!(f, "@{name}")?;
                if !args.is_empty() {
                    write!(f, "(")?;
                    for (i, arg) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write_annotation_value(f, arg)?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
        }
    }
}

/// Write an annotation value.
fn write_annotation_value(f: &mut fmt::Formatter<'_>, val: &AnnotationValue) -> fmt::Result {
    match val {
        AnnotationValue::String(s) => write!(f, "\"{s}\""),
        AnnotationValue::Int(v) => write!(f, "{v}"),
        AnnotationValue::Float(v) => write!(f, "{v}"),
        AnnotationValue::Bool(v) => write!(f, "{v}"),
        AnnotationValue::Ident(name) => write!(f, "{name}"),
        AnnotationValue::List(items) => {
            write!(f, "[")?;
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write_annotation_value(f, item)?;
            }
            write!(f, "]")
        }
        AnnotationValue::Map(entries) => {
            write!(f, "{{ ")?;
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{k}: ")?;
                write_annotation_value(f, v)?;
            }
            write!(f, " }}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::lower;

    fn parse_and_display(source: &str) -> String {
        let parse_result = axiom_parser::parse(source);
        assert!(
            !parse_result.has_errors(),
            "Parse errors: {:?}",
            parse_result.errors
        );
        let hir = lower(&parse_result.module).expect("lowering should succeed");
        display_hir(&hir)
    }

    #[test]
    fn test_display_hello() {
        let source = r#"
@module hello;
@intent("Print greeting to stdout");

fn main() -> i32 {
    print("Hello from AXIOM!");
    return 0;
}
"#;
        let output = parse_and_display(source);
        assert!(output.contains("fn main"), "Output should contain 'fn main': {output}");
        assert!(
            output.contains("@module(hello)") || output.contains("@module hello"),
            "Output should contain module annotation: {output}"
        );
        assert!(output.contains("@intent"), "Output should contain @intent: {output}");
        assert!(output.contains("return 0"), "Output should contain 'return 0': {output}");
    }

    #[test]
    fn test_display_fibonacci() {
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
        let output = parse_and_display(source);
        assert!(output.contains("fn fib"), "Output should contain 'fn fib': {output}");
        assert!(output.contains("fn main"), "Output should contain 'fn main': {output}");
        assert!(output.contains("@pure"), "Output should contain '@pure': {output}");
        assert!(
            output.contains("@complexity"),
            "Output should contain '@complexity': {output}"
        );
        assert!(output.contains("i32"), "Output should contain 'i32': {output}");
        assert!(output.contains("i64"), "Output should contain 'i64': {output}");
    }
}
