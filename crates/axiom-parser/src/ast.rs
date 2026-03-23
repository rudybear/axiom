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
}

/// Function definition
#[derive(Debug, Clone)]
pub struct Function {
    pub name: Spanned<String>,
    pub annotations: Vec<Spanned<Annotation>>,
    pub params: Vec<Param>,
    pub return_type: TypeExpr,
    pub body: Block,
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
        value: Expr,
        mutable: bool,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    Return(Expr),
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
    Custom(String, Vec<AnnotationValue>),
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
}
