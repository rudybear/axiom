//! Token types for the AXIOM language.

/// Byte offset span in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// Create a new span from start (inclusive) to end (exclusive) byte offsets.
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Returns the length of the span in bytes.
    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Returns true if the span covers zero bytes.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Merge two spans into the smallest span that covers both.
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
    /// Create a new token with the given kind and span.
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// The base (radix) of an integer literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntBase {
    /// Decimal (base 10), e.g. `42`
    Decimal,
    /// Hexadecimal (base 16), e.g. `0xFF`
    Hex,
    /// Binary (base 2), e.g. `0b1010`
    Binary,
    /// Octal (base 8), e.g. `0o77`
    Octal,
}

/// Width suffix for integer literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntSuffix {
    /// `i8`
    I8,
    /// `i16`
    I16,
    /// `i32`
    I32,
    /// `i64`
    I64,
    /// `i128`
    I128,
    /// `u8`
    U8,
    /// `u16`
    U16,
    /// `u32`
    U32,
    /// `u64`
    U64,
    /// `u128`
    U128,
}

/// Width suffix for float literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatSuffix {
    /// `f16`
    F16,
    /// `bf16`
    Bf16,
    /// `f32`
    F32,
    /// `f64`
    F64,
}

/// Lookup table that maps byte offsets to line/column positions.
///
/// Constructed from a source string and used to translate byte offsets
/// (from `Span`) into human-readable line and column numbers on demand.
/// This avoids per-token overhead while still providing line/col when
/// needed for error messages.
pub struct LineIndex {
    /// Byte offsets of the start of each line (0-indexed).
    line_starts: Vec<u32>,
}

impl LineIndex {
    /// Build a line index from source text.
    ///
    /// # Examples
    /// ```
    /// use axiom_lexer::LineIndex;
    /// let idx = LineIndex::new("hello\nworld\n");
    /// assert_eq!(idx.line_col(0), (0, 0));
    /// assert_eq!(idx.line_col(6), (1, 0));
    /// ```
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0u32];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push((i + 1) as u32);
            }
        }
        Self { line_starts }
    }

    /// Convert a byte offset to a 0-based (line, column) pair.
    ///
    /// If the offset is past the end of the source, returns the last
    /// valid position.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(insertion) => insertion.saturating_sub(1),
        };
        let col = offset.saturating_sub(self.line_starts[line]);
        (line as u32, col)
    }

    /// Returns the total number of lines in the source.
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Literals ────────────────────────────────────────
    /// Integer literal with value, optional width suffix, and base.
    IntLiteral {
        value: i128,
        suffix: Option<IntSuffix>,
        base: IntBase,
    },
    /// Float literal with value and optional width suffix.
    FloatLiteral {
        value: f64,
        suffix: Option<FloatSuffix>,
    },
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
    Extern,
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
    ReadonlyPtr,
    WriteonlyPtr,
    Vec2,
    Vec3,
    Vec4,

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
    Caret,          // ^
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
            "extern" => Some(TokenKind::Extern),
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
            "readonly_ptr" => Some(TokenKind::ReadonlyPtr),
            "writeonly_ptr" => Some(TokenKind::WriteonlyPtr),
            "vec2" => Some(TokenKind::Vec2),
            "vec3" => Some(TokenKind::Vec3),
            "vec4" => Some(TokenKind::Vec4),
            "widen" => Some(TokenKind::Widen),
            "narrow" => Some(TokenKind::Narrow),
            "truncate" => Some(TokenKind::Truncate),
            _ => None,
        }
    }

    /// Helper to create a decimal integer literal with no suffix.
    pub fn int(value: i128) -> Self {
        TokenKind::IntLiteral {
            value,
            suffix: None,
            base: IntBase::Decimal,
        }
    }

    /// Helper to create a float literal with no suffix.
    pub fn float(value: f64) -> Self {
        TokenKind::FloatLiteral {
            value,
            suffix: None,
        }
    }
}
