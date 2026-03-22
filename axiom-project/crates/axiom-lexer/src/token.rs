//! Token types for the AXIOM language.

/// Byte offset span in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A token with its kind and location in source.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Literals ────────────────────────────────────────
    IntLiteral(i128),
    FloatLiteral(f64),
    StringLiteral(String),
    BoolLiteral(bool),

    // ── Identifiers & Special Prefixes ──────────────────
    Ident(String),
    /// @annotation_name
    Annotation(String),
    /// ?optimization_hole
    OptHole(String),

    // ── Keywords ────────────────────────────────────────
    Fn,
    Let,
    Mut,
    Return,
    If,
    Else,
    For,
    While,
    In,
    Struct,
    Type,
    Module,
    Import,
    Pub,
    Unsafe,
    And,
    Or,
    Not,

    // ── Type keywords ───────────────────────────────────
    I8, I16, I32, I64, I128,
    U8, U16, U32, U64, U128,
    F16, Bf16, F32, F64,
    Bool,
    Tensor,
    Array,
    Slice,
    Ptr,

    // ── Conversion keywords ─────────────────────────────
    Widen,
    Narrow,
    Truncate,

    // ── Operators ───────────────────────────────────────
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    PlusWrap,       // +%  (wrapping add)
    PlusSat,        // +|  (saturating add)
    MinusWrap,      // -%
    MinusSat,       // -|
    StarWrap,       // *%
    Eq,             // ==
    NotEq,          // !=
    Lt,             // <
    Gt,             // >
    LtEq,          // <=
    GtEq,          // >=
    Assign,         // =
    Arrow,          // ->
    FatArrow,       // =>
    Dot,            // .
    DotDot,         // ..
    Colon,          // :
    ColonColon,     // ::
    Comma,          // ,
    Semicolon,      // ;
    Pipe,           // |  (used in sum types)

    // ── Delimiters ──────────────────────────────────────
    LParen,         // (
    RParen,         // )
    LBracket,       // [
    RBracket,       // ]
    LBrace,         // {
    RBrace,         // }

    // ── Special ─────────────────────────────────────────
    Eof,
    /// Error token — lexer recovered and continued
    Error(String),
}

impl TokenKind {
    /// Try to match an identifier string to a keyword.
    pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
        match s {
            "fn" => Some(TokenKind::Fn),
            "let" => Some(TokenKind::Let),
            "mut" => Some(TokenKind::Mut),
            "return" => Some(TokenKind::Return),
            "if" => Some(TokenKind::If),
            "else" => Some(TokenKind::Else),
            "for" => Some(TokenKind::For),
            "while" => Some(TokenKind::While),
            "in" => Some(TokenKind::In),
            "struct" => Some(TokenKind::Struct),
            "type" => Some(TokenKind::Type),
            "module" => Some(TokenKind::Module),
            "import" => Some(TokenKind::Import),
            "pub" => Some(TokenKind::Pub),
            "unsafe" => Some(TokenKind::Unsafe),
            "and" => Some(TokenKind::And),
            "or" => Some(TokenKind::Or),
            "not" => Some(TokenKind::Not),
            "true" => Some(TokenKind::BoolLiteral(true)),
            "false" => Some(TokenKind::BoolLiteral(false)),
            "i8" => Some(TokenKind::I8),
            "i16" => Some(TokenKind::I16),
            "i32" => Some(TokenKind::I32),
            "i64" => Some(TokenKind::I64),
            "i128" => Some(TokenKind::I128),
            "u8" => Some(TokenKind::U8),
            "u16" => Some(TokenKind::U16),
            "u32" => Some(TokenKind::U32),
            "u64" => Some(TokenKind::U64),
            "u128" => Some(TokenKind::U128),
            "f16" => Some(TokenKind::F16),
            "bf16" => Some(TokenKind::Bf16),
            "f32" => Some(TokenKind::F32),
            "f64" => Some(TokenKind::F64),
            "bool" => Some(TokenKind::Bool),
            "tensor" => Some(TokenKind::Tensor),
            "array" => Some(TokenKind::Array),
            "slice" => Some(TokenKind::Slice),
            "ptr" => Some(TokenKind::Ptr),
            "widen" => Some(TokenKind::Widen),
            "narrow" => Some(TokenKind::Narrow),
            "truncate" => Some(TokenKind::Truncate),
            _ => None,
        }
    }
}
