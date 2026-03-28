//! AST node definitions for AXIOM.
//!
//! Every node carries a Span. Annotations are first-class typed data.

use axiom_lexer::Span;

/// Top-level module
#[derive(Debug, Clone)]
pub struct Module {
    pub name: Option<Spanned<String>>,
    pub annotations: Vec<Spanned<Annotation>>,
    pub items: Vec<Spanned<Item>>,
}

/// A node with source location
#[derive(Debug, Clone)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

/// Top-level items
#[derive(Debug, Clone)]
pub enum Item {
    Function(Function),
    ExternFunction(ExternFunction),
    Struct(StructDef),
    TypeAlias(TypeAlias),
    Import(ImportDecl),
}

/// External function declaration (no body).
#[derive(Debug, Clone)]
pub struct ExternFunction {
    pub name: Spanned<String>,
    pub annotations: Vec<Spanned<Annotation>>,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    /// Calling convention string (e.g., `"C"`, `"fastcall"`, `"stdcall"`, `"win64"`).
    /// `None` means the default C calling convention.
    pub convention: Option<String>,
    /// Whether this extern function accepts variadic arguments (`...`).
    pub is_variadic: bool,
    /// Whether this extern function is marked `pub` (visible when imported).
    pub is_public: bool,
}

/// Function definition
#[derive(Debug, Clone)]
pub struct Function {
    pub name: Spanned<String>,
    pub annotations: Vec<Spanned<Annotation>>,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub body: Block,
    /// Whether this function is marked `pub` (visible when imported).
    pub is_public: bool,
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct Param {
    pub name: Spanned<String>,
    pub ty: TypeExpr,
    pub annotations: Vec<Spanned<Annotation>>,
}

/// Type expression
#[derive(Debug, Clone)]
pub enum TypeExpr {
    Named(String),                           // i32, MyStruct
    Tensor(Box<TypeExpr>, Vec<DimExpr>),     // tensor[f32, M, N]
    Array(Box<TypeExpr>, Box<Expr>),         // array[f32, 1024]
    Slice(Box<TypeExpr>),                    // slice[f32]
    Ptr(Box<TypeExpr>),                      // ptr[f32]
    ReadonlyPtr(Box<TypeExpr>),              // readonly_ptr[f32]
    WriteonlyPtr(Box<TypeExpr>),             // writeonly_ptr[f32]
    Tuple(Vec<TypeExpr>),                    // (i32, f64)
    Fn(Vec<TypeExpr>, Box<TypeExpr>),        // fn(i32) -> i64
}

/// Dimension expression in tensor types
#[derive(Debug, Clone)]
pub enum DimExpr {
    Const(i64),
    Named(String),       // M, N — generic dimensions
    Dynamic,             // ? — runtime-determined
}

/// Block of statements
#[derive(Debug, Clone)]
pub struct Block {
    pub annotations: Vec<Spanned<Annotation>>,
    pub stmts: Vec<Spanned<Stmt>>,
}

/// Statements
#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: Spanned<String>,
        ty: TypeExpr,
        value: Option<Expr>,
        mutable: bool,
        /// Annotations attached to this let binding (e.g., `@lifetime(scope)`).
        annotations: Vec<Spanned<Annotation>>,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    Return(Option<Expr>),
    If {
        condition: Expr,
        then_block: Block,
        else_block: Option<Block>,
    },
    For {
        var: Spanned<String>,
        var_type: TypeExpr,
        iterable: Expr,
        body: Block,
    },
    While {
        condition: Expr,
        body: Block,
    },
    Break,
    Continue,
    Expr(Expr),
}

/// Expressions
#[derive(Debug, Clone)]
pub enum Expr {
    IntLiteral(i128),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),
    Ident(String),
    OptHole(String),          // ?param — optimization hole
    BinaryOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        expr: Box<Expr>,
        indices: Vec<Expr>,
    },
    FieldAccess {
        expr: Box<Expr>,
        field: String,
    },
    MethodCall {
        expr: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    /// `array_zeros[T, N]` — zero-initialized fixed-size array literal.
    ArrayZeros {
        element_type: TypeExpr,
        size: Box<Expr>,
    },
    /// Struct literal: `Point { x: 1.0, y: 2.0 }`.
    StructLiteral {
        type_name: String,
        fields: Vec<(String, Expr)>,
    },
    /// Tuple literal: `(a, b, c)`.
    TupleLiteral {
        elements: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    AddWrap, AddSat,
    SubWrap, SubSat,
    MulWrap,
    Eq, NotEq, Lt, Gt, LtEq, GtEq,
    And, Or,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
}

/// Structured annotations — NOT strings
#[derive(Debug, Clone)]
pub enum Annotation {
    Pure,
    Const,
    Inline(InlineHint),
    Complexity(String),          // O(n^2) as string for now, structured later
    Intent(String),
    Module(String),
    Constraint(Vec<(String, AnnotationValue)>),
    Target(Vec<String>),
    Strategy(StrategyBlock),
    Transfer(TransferBlock),
    Vectorizable(Vec<String>),
    Parallel(Vec<String>),
    Layout(LayoutKind),
    Align(u64),
    OptimizationLog(Vec<OptLogEntry>),
    Export,
    /// `@lifetime(scope|static|manual)` — declares allocation lifetime.
    Lifetime(String),
    /// `@parallel_for(shared_read: [...], shared_write: [...], reduction(+: var), private: [...])`
    /// — marks a for loop for parallel execution with data sharing clauses.
    ParallelFor(ParallelForConfig),
    /// `@strict` — module-level annotation requiring all functions to have @intent and contracts.
    Strict,
    /// `@precondition(expr)` — function precondition (checked at runtime in debug mode).
    Precondition(Box<Expr>),
    /// `@postcondition(expr)` — function postcondition (checked at runtime in debug mode).
    Postcondition(Box<Expr>),
    /// `@test { input: (...), expect: value }` — inline test case for a function.
    Test(TestCase),
    /// `@link("library_name")` or `@link("library_name", "kind")` — specifies a library to link.
    Link { library: String, kind: Option<String> },
    /// `@trace` — emit ENTER/EXIT printf calls for function tracing.
    Trace,
    /// `@cfg("windows"|"linux"|"macos")` — conditional compilation: include/exclude item by platform.
    Cfg(String),
    /// `@requires(expr)` — alias for `@precondition`, signals formal verification intent.
    Requires(Box<Expr>),
    /// `@ensures(expr)` — alias for `@postcondition`, signals formal verification intent.
    Ensures(Box<Expr>),
    /// `@invariant(expr)` — loop invariant (checked at runtime in debug mode).
    Invariant(Box<Expr>),
    Custom(String, Vec<AnnotationValue>),
}

/// Inline test case attached to a function via `@test { input: (...), expect: value }`.
#[derive(Debug, Clone)]
pub struct TestCase {
    /// The input argument expressions.
    pub inputs: Vec<Expr>,
    /// The expected return value expression.
    pub expected: Expr,
}

/// Configuration for `@parallel_for` data sharing clauses.
#[derive(Debug, Clone)]
pub struct ParallelForConfig {
    /// Variables that are read (but never written) inside the loop body.
    pub shared_read: Vec<String>,
    /// Variables that are written (with disjoint index access) inside the loop body.
    pub shared_write: Vec<String>,
    /// Reduction operations: (operator, variable) — e.g., ("+", "total_energy").
    pub reductions: Vec<(String, String)>,
    /// Variables that are private to each iteration (thread-local copy).
    pub private: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum InlineHint {
    Always,
    Never,
    Hint,
}

#[derive(Debug, Clone)]
pub enum LayoutKind {
    RowMajor,
    ColMajor,
    Custom(String),
}

#[derive(Debug, Clone)]
pub enum AnnotationValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Ident(String),
    List(Vec<AnnotationValue>),
    Map(Vec<(String, AnnotationValue)>),
}

/// Strategy block — the optimization surface
#[derive(Debug, Clone)]
pub struct StrategyBlock {
    pub entries: Vec<(String, StrategyValue)>,
}

#[derive(Debug, Clone)]
pub enum StrategyValue {
    Hole(String),                           // ?param_name
    Map(Vec<(String, StrategyValue)>),      // { M: ?tile_m, N: ?tile_n }
    Concrete(AnnotationValue),              // Fixed value
}

/// Transfer block — inter-agent handoff
#[derive(Debug, Clone)]
pub struct TransferBlock {
    pub source_agent: Option<String>,
    pub target_agent: Option<String>,
    pub context: Option<String>,
    pub open_questions: Vec<String>,
    pub confidence: Option<(f64, f64)>,     // (correctness, optimality)
}

/// Optimization log entry
#[derive(Debug, Clone)]
pub struct OptLogEntry {
    pub version: String,
    pub params: Vec<(String, AnnotationValue)>,
    pub metrics: Vec<(String, f64)>,
    pub agent: Option<String>,
    pub target: Option<String>,
    pub date: Option<String>,
}

/// Struct definition
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: Spanned<String>,
    pub annotations: Vec<Spanned<Annotation>>,
    pub fields: Vec<StructField>,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: Spanned<String>,
    pub ty: TypeExpr,
    pub annotations: Vec<Spanned<Annotation>>,
}

/// Type alias
#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub name: Spanned<String>,
    pub ty: TypeExpr,
}

/// Import declaration
#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub alias: Option<String>,
    /// Whether this import is marked `pub` (re-export).
    pub is_public: bool,
}
