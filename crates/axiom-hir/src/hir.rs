//! HIR node type definitions for AXIOM.
//!
//! The HIR mirrors the AST structure but adds unique node IDs on every node,
//! validated annotation placement, and validated type references. It is produced
//! by lowering an [`axiom_parser::ast::Module`] via [`crate::lower::lower`].

use axiom_lexer::Span;

// Re-export AST types that are identical between AST and HIR.
pub use axiom_parser::ast::{
    AnnotationValue, BinOp, InlineHint, LayoutKind, OptLogEntry, ParallelForConfig,
    StrategyBlock, StrategyValue, TransferBlock, UnaryOp,
};

/// Dummy span used for expressions that lack source location information.
///
/// The AST `Expr` enum does not carry its own span, so sub-expressions
/// use this sentinel value. Phase 2 should add span tracking to AST expressions.
pub const SPAN_DUMMY: Span = Span { start: 0, end: 0 };

/// Unique identifier for every HIR node. Monotonically increasing, assigned during lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Index of a function within `HirModule.functions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FuncId(pub u32);

/// Index into the expression arena within a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

/// Index into the statement arena within a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StmtId(pub u32);

/// Monotonic ID generator for HIR lowering.
#[derive(Debug)]
pub struct NodeIdGen {
    next: u32,
}

impl NodeIdGen {
    /// Create a new ID generator starting from 0.
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// Generate the next unique [`NodeId`].
    pub fn next_id(&mut self) -> NodeId {
        let id = NodeId(self.next);
        self.next += 1;
        id
    }
}

impl Default for NodeIdGen {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-level HIR module, the result of lowering an `ast::Module`.
#[derive(Debug, Clone)]
pub struct HirModule {
    /// Module name from `@module` annotation (if present).
    pub name: Option<String>,
    /// Module-level annotations (e.g., `@intent`, `@module`).
    pub annotations: Vec<HirAnnotation>,
    /// All functions in the module.
    pub functions: Vec<HirFunction>,
    /// All extern function declarations in the module.
    pub extern_functions: Vec<HirExternFunction>,
    /// All struct definitions in the module.
    pub structs: Vec<HirStruct>,
    /// All type aliases in the module.
    pub type_aliases: Vec<HirTypeAlias>,
    /// All import declarations in the module.
    pub imports: Vec<HirImport>,
}

/// An external function declaration (no body).
#[derive(Debug, Clone)]
pub struct HirExternFunction {
    /// Unique node ID.
    pub id: NodeId,
    /// Function name.
    pub name: String,
    /// Span of the function name in source.
    pub name_span: Span,
    /// Validated annotations.
    pub annotations: Vec<HirAnnotation>,
    /// Function parameters.
    pub params: Vec<HirParam>,
    /// Return type.
    pub return_type: HirType,
    /// Span covering the entire extern function declaration.
    pub span: Span,
    /// Calling convention (e.g., `"C"`, `"fastcall"`, `"stdcall"`, `"win64"`).
    /// Defaults to `"C"`.
    pub convention: String,
}

/// A function with validated annotations.
///
/// Valid annotations on functions: `@pure`, `@const`, `@inline`, `@complexity`,
/// `@intent`, `@vectorizable`, `@parallel`, `@strategy`, `@constraint`,
/// `@target`, `@transfer`, `@optimization_log`, `@custom`.
#[derive(Debug, Clone)]
pub struct HirFunction {
    /// Unique node ID.
    pub id: NodeId,
    /// Function name.
    pub name: String,
    /// Span of the function name in source.
    pub name_span: Span,
    /// Validated annotations.
    pub annotations: Vec<HirAnnotation>,
    /// Function parameters.
    pub params: Vec<HirParam>,
    /// Return type.
    pub return_type: HirType,
    /// Function body.
    pub body: HirBlock,
    /// Span covering the entire function definition.
    pub span: Span,
}

/// Function parameter with validated annotations.
///
/// Valid annotations on params: `@layout`, `@align`, `@custom`.
#[derive(Debug, Clone)]
pub struct HirParam {
    /// Unique node ID.
    pub id: NodeId,
    /// Parameter name.
    pub name: String,
    /// Span of the parameter name in source.
    pub name_span: Span,
    /// Parameter type.
    pub ty: HirType,
    /// Validated annotations.
    pub annotations: Vec<HirAnnotation>,
}

/// Struct definition with validated annotations.
#[derive(Debug, Clone)]
pub struct HirStruct {
    /// Unique node ID.
    pub id: NodeId,
    /// Struct name.
    pub name: String,
    /// Span of the struct name in source.
    pub name_span: Span,
    /// Validated annotations.
    pub annotations: Vec<HirAnnotation>,
    /// Struct fields.
    pub fields: Vec<HirStructField>,
    /// Span covering the entire struct definition.
    pub span: Span,
}

/// Struct field with validated annotations.
///
/// Valid annotations on fields: `@layout`, `@align`, `@custom`.
#[derive(Debug, Clone)]
pub struct HirStructField {
    /// Unique node ID.
    pub id: NodeId,
    /// Field name.
    pub name: String,
    /// Span of the field name in source.
    pub name_span: Span,
    /// Field type.
    pub ty: HirType,
    /// Validated annotations.
    pub annotations: Vec<HirAnnotation>,
}

/// Type alias: `type Name = SomeType;`
#[derive(Debug, Clone)]
pub struct HirTypeAlias {
    /// Unique node ID.
    pub id: NodeId,
    /// Alias name.
    pub name: String,
    /// Span of the alias name in source.
    pub name_span: Span,
    /// The aliased type.
    pub ty: HirType,
    /// Span covering the entire type alias declaration.
    pub span: Span,
}

/// Import declaration.
#[derive(Debug, Clone)]
pub struct HirImport {
    /// Unique node ID.
    pub id: NodeId,
    /// Import path segments.
    pub path: Vec<String>,
    /// Optional alias (`import foo::bar as baz`).
    pub alias: Option<String>,
    /// Span covering the entire import declaration.
    pub span: Span,
}

/// Block of statements. Blocks inside function bodies can carry `@strategy` annotations.
#[derive(Debug, Clone)]
pub struct HirBlock {
    /// Unique node ID.
    pub id: NodeId,
    /// Block-level annotations.
    pub annotations: Vec<HirAnnotation>,
    /// Statements in the block.
    pub stmts: Vec<HirStmt>,
    /// Span covering the entire block.
    pub span: Span,
}

/// Statement with unique ID and span.
#[derive(Debug, Clone)]
pub struct HirStmt {
    /// Unique node ID.
    pub id: NodeId,
    /// Statement kind.
    pub kind: HirStmtKind,
    /// Span covering the entire statement.
    pub span: Span,
    /// Annotations on this statement (e.g., `@lifetime(scope)` on a let binding).
    pub annotations: Vec<HirAnnotation>,
}

/// Statement variants.
#[derive(Debug, Clone)]
pub enum HirStmtKind {
    /// `let [mut] name: Type [= value];`
    Let {
        /// Variable name.
        name: String,
        /// Span of the variable name.
        name_span: Span,
        /// Variable type.
        ty: HirType,
        /// Initializer expression (None for declarations like `let v: Vec3;`).
        value: Option<HirExpr>,
        /// Whether the binding is mutable.
        mutable: bool,
    },
    /// `target = value;`
    Assign {
        /// Assignment target (lvalue).
        target: HirExpr,
        /// Value being assigned.
        value: HirExpr,
    },
    /// `return value;` or `return;` (bare return for void functions).
    Return {
        /// Return value expression. None for bare return.
        value: Option<HirExpr>,
    },
    /// `if condition { then_block } [else { else_block }]`
    If {
        /// Condition expression.
        condition: HirExpr,
        /// Then branch.
        then_block: HirBlock,
        /// Optional else branch.
        else_block: Option<HirBlock>,
    },
    /// `for var: Type in iterable { body }`
    For {
        /// Loop variable name.
        var: String,
        /// Span of the loop variable name.
        var_span: Span,
        /// Loop variable type.
        var_type: HirType,
        /// Iterable expression.
        iterable: HirExpr,
        /// Loop body.
        body: HirBlock,
    },
    /// `while condition { body }`
    While {
        /// Condition expression.
        condition: HirExpr,
        /// Loop body.
        body: HirBlock,
    },
    /// `break;` — exit the innermost loop.
    Break,
    /// `continue;` — skip to the next iteration of the innermost loop.
    Continue,
    /// Expression statement.
    Expr {
        /// The expression.
        expr: HirExpr,
    },
}

/// Expression with unique ID and span.
///
/// Span is inherited from the AST where available. For sub-expressions
/// without explicit spans, [`SPAN_DUMMY`] is used.
#[derive(Debug, Clone)]
pub struct HirExpr {
    /// Unique node ID.
    pub id: NodeId,
    /// Expression kind.
    pub kind: HirExprKind,
    /// Source span (may be [`SPAN_DUMMY`] for sub-expressions).
    pub span: Span,
}

/// Expression variants.
#[derive(Debug, Clone)]
pub enum HirExprKind {
    /// Integer literal.
    IntLiteral { value: i128 },
    /// Float literal.
    FloatLiteral { value: f64 },
    /// String literal.
    StringLiteral { value: String },
    /// Boolean literal.
    BoolLiteral { value: bool },
    /// Identifier reference.
    Ident { name: String },
    /// Optimization hole (`?name`).
    OptHole { name: String },
    /// Binary operation.
    BinaryOp {
        /// Operator.
        op: BinOp,
        /// Left-hand side.
        lhs: Box<HirExpr>,
        /// Right-hand side.
        rhs: Box<HirExpr>,
    },
    /// Unary operation.
    UnaryOp {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        operand: Box<HirExpr>,
    },
    /// Function call.
    Call {
        /// Callee expression.
        func: Box<HirExpr>,
        /// Arguments.
        args: Vec<HirExpr>,
    },
    /// Index operation (`expr[indices]`).
    Index {
        /// Expression being indexed.
        expr: Box<HirExpr>,
        /// Index expressions.
        indices: Vec<HirExpr>,
    },
    /// Field access (`expr.field`).
    FieldAccess {
        /// Expression being accessed.
        expr: Box<HirExpr>,
        /// Field name.
        field: String,
    },
    /// Method call (`expr.method(args)`).
    MethodCall {
        /// Receiver expression.
        expr: Box<HirExpr>,
        /// Method name.
        method: String,
        /// Arguments.
        args: Vec<HirExpr>,
    },
    /// `array_zeros[T, N]` — zero-initialized fixed-size array.
    ArrayZeros {
        /// Element type.
        element_type: HirType,
        /// Fixed array size.
        size: usize,
    },
    /// Struct literal: `Point { x: 1.0, y: 2.0 }`.
    StructLiteral {
        /// The struct type name.
        type_name: String,
        /// Field name and value pairs.
        fields: Vec<(String, HirExpr)>,
    },
}

/// Validated type reference.
///
/// During lowering, `Named` types are checked against the set of known primitives
/// and user-defined struct names. Unknown types produce a [`crate::LowerError`]
/// but lowering continues using [`HirType::Unknown`] for error recovery.
#[derive(Debug, Clone, PartialEq)]
pub enum HirType {
    /// Resolved primitive type (e.g., `i32`, `f64`, `bool`).
    Primitive(PrimitiveType),
    /// Resolved reference to a user-defined struct or type alias.
    UserDefined(String),
    /// Tensor type with element type and dimension expressions.
    Tensor {
        element: Box<HirType>,
        dims: Vec<HirDimExpr>,
    },
    /// Fixed-size array type.
    Array {
        element: Box<HirType>,
        size: usize,
    },
    /// Slice type (fat pointer).
    Slice {
        element: Box<HirType>,
    },
    /// Raw pointer type.
    Ptr {
        element: Box<HirType>,
    },
    /// Readonly pointer type — can only be read from, not written to.
    ReadonlyPtr {
        element: Box<HirType>,
    },
    /// Writeonly pointer type — can only be written to, not read from.
    WriteonlyPtr {
        element: Box<HirType>,
    },
    /// Tuple type.
    Tuple {
        elements: Vec<HirType>,
    },
    /// Function type.
    Fn {
        params: Vec<HirType>,
        ret: Box<HirType>,
    },
    /// Placeholder for types that failed validation.
    ///
    /// Allows lowering to continue despite errors so that all errors
    /// can be collected in a single pass.
    Unknown(String),
}


/// All primitive types from the AXIOM spec.
///
/// Resolved from string names during lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    /// 8-bit signed integer.
    I8,
    /// 16-bit signed integer.
    I16,
    /// 32-bit signed integer.
    I32,
    /// 64-bit signed integer.
    I64,
    /// 128-bit signed integer.
    I128,
    /// 8-bit unsigned integer.
    U8,
    /// 16-bit unsigned integer.
    U16,
    /// 32-bit unsigned integer.
    U32,
    /// 64-bit unsigned integer.
    U64,
    /// 128-bit unsigned integer.
    U128,
    /// 16-bit IEEE 754 float.
    F16,
    /// 16-bit brain float.
    Bf16,
    /// 32-bit IEEE 754 float.
    F32,
    /// 64-bit IEEE 754 float.
    F64,
    /// Boolean.
    Bool,
    /// 2-component f64 vector (SIMD).
    Vec2,
    /// 3-component f64 vector (SIMD, padded to 4 lanes with w=0).
    Vec3,
    /// 4-component f64 vector (SIMD).
    Vec4,
}

/// Tensor dimension expression, mirrors `ast::DimExpr`.
#[derive(Debug, Clone, PartialEq)]
pub enum HirDimExpr {
    /// Constant dimension.
    Const(i64),
    /// Named generic dimension (e.g., `M`, `N`).
    Named(String),
    /// Runtime-determined dimension (`?`).
    Dynamic,
}

/// Annotation that has passed target validation.
#[derive(Debug, Clone)]
pub struct HirAnnotation {
    /// Annotation kind.
    pub kind: HirAnnotationKind,
    /// Source span of the annotation.
    pub span: Span,
}

/// Annotation variants. Types like [`InlineHint`], [`LayoutKind`], [`AnnotationValue`],
/// [`StrategyBlock`], [`TransferBlock`], and [`OptLogEntry`] are re-exported from
/// `axiom_parser::ast` to avoid duplication.
#[derive(Debug, Clone)]
pub enum HirAnnotationKind {
    /// `@pure` - no side effects.
    Pure,
    /// `@const` - compile-time evaluable.
    Const,
    /// `@inline(always|never|hint)` - inlining guidance.
    Inline(InlineHint),
    /// `@complexity(expr)` - algorithmic complexity class.
    Complexity(String),
    /// `@intent("description")` - semantic intent.
    Intent(String),
    /// `@module(name)` - module name.
    Module(String),
    /// `@constraint { key: value, ... }` - hard constraints.
    Constraint(Vec<(String, AnnotationValue)>),
    /// `@target(targets)` - target hardware class.
    Target(Vec<String>),
    /// `@strategy { ... }` - optimization surface declaration.
    Strategy(StrategyBlock),
    /// `@transfer { ... }` - agent handoff metadata.
    Transfer(TransferBlock),
    /// `@vectorizable(dims)` - vectorization hints.
    Vectorizable(Vec<String>),
    /// `@parallel(dims)` - parallelization hints.
    Parallel(Vec<String>),
    /// `@layout(kind)` - memory layout.
    Layout(LayoutKind),
    /// `@align(bytes)` - alignment requirement.
    Align(u64),
    /// `@optimization_log { ... }` - history of optimization attempts.
    OptimizationLog(Vec<OptLogEntry>),
    /// `@export` - C calling convention, externally visible.
    Export,
    /// `@lifetime(scope|static|manual)` - declares allocation lifetime for escape analysis.
    Lifetime(String),
    /// `@parallel_for(shared_read: [...], shared_write: [...], reduction(...: ...), private: [...])`
    /// — marks a for loop for parallel execution with data sharing clauses.
    ParallelFor(ParallelForConfig),
    /// `@strict` — module-level annotation requiring all functions to have @intent and contracts.
    Strict,
    /// `@precondition(expr)` — function precondition (checked at runtime in debug mode).
    Precondition(Box<HirExpr>),
    /// `@postcondition(expr)` — function postcondition (checked at runtime in debug mode).
    Postcondition(Box<HirExpr>),
    /// `@test { input: (...), expect: value }` — inline test case for a function.
    Test(HirTestCase),
    /// `@link("library", "kind")` - link against a native library.
    Link { library: String, kind: String },
    /// `@custom(name, args)` - extensibility.
    Custom(String, Vec<AnnotationValue>),
}

/// Inline test case attached to a function via `@test`.
#[derive(Debug, Clone)]
pub struct HirTestCase {
    /// The input argument expressions.
    pub inputs: Vec<HirExpr>,
    /// The expected return value expression.
    pub expected: HirExpr,
}
