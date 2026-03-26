use super::*;
use axiom_hir::{
    HirAnnotation, HirAnnotationKind, HirBlock, HirExpr, HirExprKind,
    HirExternFunction, HirFunction, HirModule, HirParam, HirStmt, HirStmtKind,
    HirType, NodeId, ParallelForConfig, PrimitiveType, SPAN_DUMMY,
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
        annotations: Vec::new(),
    }
}

/// Helper: create a statement with annotations (e.g., `@lifetime(scope)` on a let binding).
fn stmt_with_annotations(kind: HirStmtKind, annotations: Vec<HirAnnotation>) -> HirStmt {
    HirStmt {
        id: nid(0),
        kind,
        span: span(),
        annotations,
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

/// Helper: create a module with functions and struct definitions.
fn module_with_structs(
    name: Option<&str>,
    functions: Vec<HirFunction>,
    structs: Vec<HirStruct>,
) -> HirModule {
    HirModule {
        name: name.map(|s| s.to_string()),
        annotations: vec![],
        functions,
        extern_functions: vec![],
        structs,
        type_aliases: vec![],
        imports: vec![],
    }
}

/// Helper: create an HirStruct.
fn hir_struct(name: &str, fields: Vec<axiom_hir::HirStructField>) -> HirStruct {
    HirStruct {
        id: nid(0),
        name: name.to_string(),
        name_span: span(),
        annotations: vec![],
        fields,
        span: span(),
    }
}

/// Helper: create an HirStructField.
fn struct_field(name: &str, ty: HirType) -> axiom_hir::HirStructField {
    axiom_hir::HirStructField {
        id: nid(0),
        name: name.to_string(),
        name_span: span(),
        ty,
        annotations: vec![],
    }
}

/// Helper: create a field access expression.
fn field_access(base: HirExpr, field: &str) -> HirExpr {
    HirExpr {
        id: nid(0),
        kind: HirExprKind::FieldAccess {
            expr: Box::new(base),
            field: field.to_string(),
        },
        span: span(),
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
                value: Some(int_lit(0)),
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
                value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
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
    assert!(ir.contains("add nsw i32"), "should add with nsw");
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
                    value: Some(int_lit(42)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("x")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(ir.contains("alloca i32"), "should alloca x");
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
                    value: Some(int_lit(0)),
                    mutable: true,
                }),
                stmt(HirStmtKind::Assign {
                    target: ident("x"),
                    value: int_lit(42),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("x")),
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
                        value: Some(int_lit(1)),
                    })]),
                    else_block: None,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(1)),
                })]),
                else_block: Some(block(vec![stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(0)),
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
                    value: Some(ident("sum")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(ir.contains("alloca i32"), "should alloca loop var");
    assert!(ir.contains("icmp slt"), "should have loop comparison");
    assert!(ir.contains("for.cond."), "should have for.cond label");
    assert!(ir.contains("for.body."), "should have for.body label");
    assert!(ir.contains("for.end."), "should have for.end label");
    assert!(ir.contains("add nsw i32"), "should have nsw increment");
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
                    value: Some(int_lit(0)),
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
                        value: Some(call("fib", vec![int_lit(40)])),
                        mutable: false,
                    }),
                    stmt(HirStmtKind::Return {
                        value: Some(int_lit(0)),
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
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(42)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("print_i64", vec![ident("x")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(5)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "y".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("widen", vec![ident("x")])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(bool_lit(true)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "y".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::Bool),
                    value: Some(bool_lit(false)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(5)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "y".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(unaryop(UnaryOp::Neg, ident("x"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(bool_lit(true)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "y".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::Bool),
                    value: Some(unaryop(UnaryOp::Not, ident("x"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(float_lit(1.5)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "b".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(float_lit(2.5)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "c".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(0)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(ident("x")),
                })]),
            ),
            func(
                "main",
                vec![],
                HirType::Primitive(PrimitiveType::I32),
                block(vec![stmt(HirStmtKind::Return {
                    value: Some(call("helper", vec![int_lit(42)])),
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
                    value: Some(int_lit(0)),
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
                    value: Some(ident("x")),
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
                    value: Some(int_lit(1)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "b".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(int_lit(2)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "c".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(int_lit(3)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "result".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    // a + (b * c)  -- parser already handles precedence in AST
                    value: Some(binop(
                        BinOp::Add,
                        ident("a"),
                        binop(BinOp::Mul, ident("b"), ident("c")),
                    )),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // Mul should appear before add in the IR (with nsw flags).
    let mul_pos = ir.find("mul nsw i32").expect("should have mul nsw");
    let add_pos = ir
        .rfind("add nsw i32")
        .expect("should have add nsw");
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
                    value: Some(int_lit(-5)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "a".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("abs", vec![ident("x")])),
                    mutable: false,
                }),
                // abs_f64(x: f64) -> f64
                stmt(HirStmtKind::Let {
                    name: "fx".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(float_lit(-3.14)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "fa".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("abs_f64", vec![ident("fx")])),
                    mutable: false,
                }),
                // min(a: i32, b: i32) -> i32
                stmt(HirStmtKind::Let {
                    name: "mn".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("min", vec![int_lit(3), int_lit(7)])),
                    mutable: false,
                }),
                // max(a: i32, b: i32) -> i32
                stmt(HirStmtKind::Let {
                    name: "mx".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("max", vec![int_lit(3), int_lit(7)])),
                    mutable: false,
                }),
                // min_f64(a: f64, b: f64) -> f64
                stmt(HirStmtKind::Let {
                    name: "fmn".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("min_f64", vec![float_lit(1.5), float_lit(2.5)])),
                    mutable: false,
                }),
                // max_f64(a: f64, b: f64) -> f64
                stmt(HirStmtKind::Let {
                    name: "fmx".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("max_f64", vec![float_lit(1.5), float_lit(2.5)])),
                    mutable: false,
                }),
                // sqrt(x: f64) -> f64
                stmt(HirStmtKind::Let {
                    name: "sq".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("sqrt", vec![float_lit(4.0)])),
                    mutable: false,
                }),
                // pow(base: f64, exp: f64) -> f64
                stmt(HirStmtKind::Let {
                    name: "pw".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("pow", vec![float_lit(2.0), float_lit(3.0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(42)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "narrow_val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("narrow", vec![ident("wide")])),
                    mutable: false,
                }),
                // truncate(x: f64) -> i32
                stmt(HirStmtKind::Let {
                    name: "fval".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(float_lit(3.14)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "trunc_val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("truncate", vec![ident("fval")])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(42)),
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
                    value: Some(float_lit(3.14)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("print_f64", vec![ident("y")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                value: Some(call("clock", vec![])),
                mutable: false,
            }),
            stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
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
            value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
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
        ir.contains("alloca i32") && ir.contains("numbers"),
        "should have numbers counter: {ir}"
    );
    assert!(
        ir.contains("alloca i32") && ir.contains("operators"),
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
                    value: Some(int_lit(42)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "y".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("to_f64", vec![ident("x")])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
                    value: Some(int_lit(100)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "y".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("to_f64_i64", vec![ident("x")])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
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
    // Verify i64 subtraction (with nsw flag from @pure function).
    assert!(
        ir.contains("sub nsw i64"),
        "should have i64 subtraction with nsw: {ir}"
    );
    // Verify i64 addition (with nsw flag from @pure function).
    assert!(
        ir.contains("add nsw i64"),
        "should have i64 addition with nsw: {ir}"
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

// -----------------------------------------------------------------------
// Array support tests
// -----------------------------------------------------------------------

#[test]
fn test_array_type() {
    // Verify that array type generates the correct LLVM type string.
    let arr_ty = HirType::Array {
        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
        size: 100,
    };
    let llvm_ty = hir_type_to_llvm(&arr_ty).expect("array type should convert");
    assert_eq!(llvm_ty, "[100 x i32]");

    let arr_ty_f64 = HirType::Array {
        element: Box::new(HirType::Primitive(PrimitiveType::F64)),
        size: 50,
    };
    let llvm_ty_f64 = hir_type_to_llvm(&arr_ty_f64).expect("f64 array type should convert");
    assert_eq!(llvm_ty_f64, "[50 x double]");
}

#[test]
fn test_array_param_type() {
    // Verify that array params become ptr in function signatures.
    let arr_ty = HirType::Array {
        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
        size: 10,
    };
    let param_ty = hir_type_to_llvm_param(&arr_ty).expect("array param type should convert");
    assert_eq!(param_ty, "ptr");
}

#[test]
fn test_array_alloca() {
    // Test that array_zeros creates alloca + memset.
    let m = module(
        Some("arr_test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arr".to_string(),
                    name_span: span(),
                    ty: HirType::Array {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        size: 10,
                    },
                    value: Some(HirExpr {
                        id: nid(0),
                        kind: HirExprKind::ArrayZeros {
                            element_type: HirType::Primitive(PrimitiveType::I32),
                            size: 10,
                        },
                        span: span(),
                    }),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );
    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("alloca [10 x i32]"),
        "should have array alloca: {ir}"
    );
    assert!(
        ir.contains("i8 0, i64 40, i1 false"),
        "should have memset for 10 * 4 = 40 bytes: {ir}"
    );
    assert!(
        ir.contains("declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)"),
        "should declare memset intrinsic: {ir}"
    );
}

#[test]
fn test_array_index_read() {
    // Test array index read: arr[5].
    let m = module(
        Some("arr_read"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arr".to_string(),
                    name_span: span(),
                    ty: HirType::Array {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        size: 10,
                    },
                    value: Some(HirExpr {
                        id: nid(0),
                        kind: HirExprKind::ArrayZeros {
                            element_type: HirType::Primitive(PrimitiveType::I32),
                            size: 10,
                        },
                        span: span(),
                    }),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "x".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(HirExpr {
                        id: nid(0),
                        kind: HirExprKind::Index {
                            expr: Box::new(ident("arr")),
                            indices: vec![int_lit(5)],
                        },
                        span: span(),
                    }),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("x")),
                }),
            ]),
        )],
    );
    let ir = codegen(&m).expect("codegen should succeed");
    // Should have GEP + load for index read.
    assert!(
        ir.contains("getelementptr inbounds [10 x i32], ptr %arr"),
        "should have GEP for array index: {ir}"
    );
    assert!(
        ir.contains("load i32, ptr"),
        "should load element from GEP pointer: {ir}"
    );
}

#[test]
fn test_array_index_write() {
    // Test array index write: arr[5] = 42.
    let m = module(
        Some("arr_write"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arr".to_string(),
                    name_span: span(),
                    ty: HirType::Array {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        size: 10,
                    },
                    value: Some(HirExpr {
                        id: nid(0),
                        kind: HirExprKind::ArrayZeros {
                            element_type: HirType::Primitive(PrimitiveType::I32),
                            size: 10,
                        },
                        span: span(),
                    }),
                    mutable: false,
                }),
                stmt(HirStmtKind::Assign {
                    target: HirExpr {
                        id: nid(0),
                        kind: HirExprKind::Index {
                            expr: Box::new(ident("arr")),
                            indices: vec![int_lit(5)],
                        },
                        span: span(),
                    },
                    value: int_lit(42),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );
    let ir = codegen(&m).expect("codegen should succeed");
    // Should have GEP + store for index write.
    assert!(
        ir.contains("getelementptr inbounds [10 x i32], ptr %arr"),
        "should have GEP for array index write: {ir}"
    );
    assert!(
        ir.contains("store i32 42, ptr"),
        "should store value at GEP pointer: {ir}"
    );
}

#[test]
fn test_array_program() {
    // Full array program: create, fill with squares, sum them.
    let source = r#"
@module array_test;
fn main() -> i32 {
let arr: array[i32, 10] = array_zeros[i32, 10];
for i: i32 in range(0, 10) {
    arr[i] = i * i;
}
let sum: i32 = 0;
for i: i32 in range(0, 10) {
    sum = sum + arr[i];
}
print_i32(sum);
return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(
        !parse_result.has_errors(),
        "parse should succeed: {:?}",
        parse_result.errors
    );
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");

    // Verify key patterns in the generated IR.
    assert!(
        ir.contains("alloca [10 x i32]"),
        "should have array alloca: {ir}"
    );
    assert!(
        ir.contains("@llvm.memset.p0.i64"),
        "should use memset: {ir}"
    );
    assert!(
        ir.contains("getelementptr inbounds [10 x i32]"),
        "should have GEP: {ir}"
    );
    // Should have at least one store to array and one load from array.
    assert!(
        ir.contains("store i32"),
        "should have store to array: {ir}"
    );
    assert!(
        ir.contains("load i32"),
        "should have load from array: {ir}"
    );
}

// -----------------------------------------------------------------------
// LLVM optimization hint tests
// -----------------------------------------------------------------------

/// Helper: create a function with annotations.
fn func_with_annotations(
    name: &str,
    params: Vec<HirParam>,
    return_type: HirType,
    body: HirBlock,
    annotations: Vec<HirAnnotation>,
) -> HirFunction {
    HirFunction {
        id: nid(0),
        name: name.to_string(),
        name_span: span(),
        annotations,
        params,
        return_type,
        body,
        span: span(),
    }
}

/// Helper: create a @pure annotation.
fn pure_ann() -> HirAnnotation {
    HirAnnotation {
        kind: HirAnnotationKind::Pure,
        span: SPAN_DUMMY,
    }
}

/// Helper: create a @const annotation.
fn const_ann() -> HirAnnotation {
    HirAnnotation {
        kind: HirAnnotationKind::Const,
        span: SPAN_DUMMY,
    }
}

/// Helper: create a @vectorizable annotation.
fn vectorizable_ann() -> HirAnnotation {
    HirAnnotation {
        kind: HirAnnotationKind::Vectorizable(vec![]),
        span: SPAN_DUMMY,
    }
}

// --- Test #1: noalias on all pointer parameters ---

#[test]
fn test_noalias_params() {
    // Function with an array (ptr) parameter should get noalias.
    let sum_func = func_with_annotations(
        "sum_arr",
        vec![
            param(
                "arr",
                HirType::Array {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    size: 10,
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(int_lit(0)),
        })]),
        vec![],
    );

    let m = module(Some("test"), vec![sum_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // The ptr parameter should have noalias.
    assert!(
        ir.contains("ptr noalias %arr"),
        "ptr params should have noalias: {ir}"
    );
    // Non-ptr params should NOT have noalias.
    assert!(
        ir.contains("i32 %n"),
        "non-ptr params should not have noalias: {ir}"
    );
}

// --- Test #2: @pure function attributes (readnone/readonly) ---

#[test]
fn test_pure_function_attrs_readnone() {
    // @pure function with no pointer params -> memory(none).
    let fib_func = func_with_annotations(
        "fib",
        vec![param("n", HirType::Primitive(PrimitiveType::I64))],
        HirType::Primitive(PrimitiveType::I64),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(ident("n")),
        })]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![fib_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Should have function attribute group reference.
    assert!(
        ir.contains("#0"),
        "pure function should have attribute group ref: {ir}"
    );
    // Should have memory(none) in the attribute group.
    assert!(
        ir.contains("memory(none)"),
        "pure function without ptr params should get memory(none): {ir}"
    );
    // Should have nounwind.
    assert!(
        ir.contains("nounwind"),
        "pure function should have nounwind: {ir}"
    );
    // @pure should NOT have willreturn (cannot prove termination).
    assert!(
        !ir.contains("willreturn"),
        "@pure function should NOT have willreturn: {ir}"
    );
    // @pure should NOT have nosync (may be called from parallel workers).
    assert!(
        !ir.contains("nosync"),
        "@pure function should NOT have nosync: {ir}"
    );
}

#[test]
fn test_pure_function_attrs_argmem_read() {
    // @pure function with pointer params -> memory(argmem: read).
    let sum_func = func_with_annotations(
        "sum_arr",
        vec![
            param(
                "arr",
                HirType::Array {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    size: 10,
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(int_lit(0)),
        })]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![sum_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Should have memory(argmem: read) for @pure with ptr params.
    assert!(
        ir.contains("memory(argmem: read)"),
        "pure function with ptr params should get memory(argmem: read): {ir}"
    );
}

// --- Test #3: nsw/nuw flags on arithmetic ---

#[test]
fn test_nsw_arithmetic() {
    // Integer add/sub/mul should get nsw flag.
    let m = module(
        Some("test"),
        vec![func(
            "compute",
            vec![
                param("a", HirType::Primitive(PrimitiveType::I32)),
                param("b", HirType::Primitive(PrimitiveType::I32)),
            ],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "sum".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "diff".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(binop(BinOp::Sub, ident("a"), ident("b"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "prod".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(binop(BinOp::Mul, ident("a"), ident("b"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("sum")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("add nsw i32"),
        "integer add should have nsw: {ir}"
    );
    assert!(
        ir.contains("sub nsw i32"),
        "integer sub should have nsw: {ir}"
    );
    assert!(
        ir.contains("mul nsw i32"),
        "integer mul should have nsw: {ir}"
    );
}

#[test]
fn test_wrap_ops_no_nsw() {
    // AddWrap/SubWrap/MulWrap should NOT get nsw flag.
    let m = module(
        Some("test"),
        vec![func(
            "wrap_ops",
            vec![
                param("a", HirType::Primitive(PrimitiveType::I32)),
                param("b", HirType::Primitive(PrimitiveType::I32)),
            ],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "x".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(binop(BinOp::AddWrap, ident("a"), ident("b"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("x")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // AddWrap should produce plain `add` without nsw.
    // We check that it contains "add i32" but not "add nsw i32" at the same position.
    assert!(
        ir.contains("add i32"),
        "AddWrap should produce plain add: {ir}"
    );
    // The wrap add line should not have nsw.
    for line in ir.lines() {
        if line.contains("add i32") && line.contains("= add") {
            assert!(
                !line.contains("nsw"),
                "AddWrap should NOT have nsw: {line}"
            );
        }
    }
}

// --- Test #4: fast flag on float ops in @pure context ---

#[test]
fn test_fast_float_in_pure() {
    // Float operations in @pure function should get `fast` flag.
    let compute_func = func_with_annotations(
        "compute",
        vec![
            param("a", HirType::Primitive(PrimitiveType::F64)),
            param("b", HirType::Primitive(PrimitiveType::F64)),
        ],
        HirType::Primitive(PrimitiveType::F64),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "sum".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::F64),
                value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
                mutable: false,
            }),
            stmt(HirStmtKind::Let {
                name: "prod".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::F64),
                value: Some(binop(BinOp::Mul, ident("a"), ident("b"))),
                mutable: false,
            }),
            stmt(HirStmtKind::Return {
                value: Some(ident("sum")),
            }),
        ]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![compute_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("fadd fast double"),
        "float add in @pure should have fast flag: {ir}"
    );
    assert!(
        ir.contains("fmul fast double"),
        "float mul in @pure should have fast flag: {ir}"
    );
}

#[test]
fn test_no_fast_float_outside_pure() {
    // Float operations in non-@pure function should NOT get `fast` flag.
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
                    value: Some(float_lit(1.5)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "b".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(float_lit(2.5)),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "c".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // Should have plain fadd without fast.
    assert!(
        ir.contains("fadd double"),
        "should have fadd: {ir}"
    );
    assert!(
        !ir.contains("fadd fast"),
        "non-pure function should NOT have fast flag: {ir}"
    );
}

// --- Test #5: @const compile-time evaluation ---

#[test]
fn test_const_eval_simple() {
    // @const function called with all-literal args should be evaluated at compile time.
    let square_func = func_with_annotations(
        "square",
        vec![param("n", HirType::Primitive(PrimitiveType::I32))],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(binop(BinOp::Mul, ident("n"), ident("n"))),
        })]),
        vec![const_ann()],
    );

    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(call("square", vec![int_lit(42)])),
        })]),
    );

    let m = module(Some("test"), vec![square_func, main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // The call to square(42) should be replaced with the literal 1764.
    assert!(
        ir.contains("1764"),
        "const eval should compute square(42) = 1764: {ir}"
    );
    // Main should NOT contain a call to square.
    let main_section = ir.split("define i32 @main").nth(1).unwrap_or("");
    assert!(
        !main_section.contains("@square"),
        "const call should be eliminated from main: {ir}"
    );
}

#[test]
fn test_const_function_attributes() {
    // @const functions should get speculatable + memory(none).
    let square_func = func_with_annotations(
        "square",
        vec![param("n", HirType::Primitive(PrimitiveType::I32))],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(binop(BinOp::Mul, ident("n"), ident("n"))),
        })]),
        vec![const_ann()],
    );

    let m = module(Some("test"), vec![square_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("speculatable"),
        "@const should have speculatable: {ir}"
    );
    assert!(
        ir.contains("memory(none)"),
        "@const should have memory(none): {ir}"
    );
}

// --- Test #6: branch prediction hints ---

#[test]
fn test_branch_prediction_hints() {
    // @pure function with `if n <= 1` should get branch weight metadata.
    let fib_func = func_with_annotations(
        "fib",
        vec![param("n", HirType::Primitive(PrimitiveType::I64))],
        HirType::Primitive(PrimitiveType::I64),
        block(vec![
            stmt(HirStmtKind::If {
                condition: binop(BinOp::LtEq, ident("n"), int_lit(1)),
                then_block: block(vec![stmt(HirStmtKind::Return {
                    value: Some(ident("n")),
                })]),
                else_block: Some(block(vec![stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                })])),
            }),
        ]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![fib_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Should have !prof metadata on the branch.
    assert!(
        ir.contains("!prof !"),
        "base case branch should have !prof metadata: {ir}"
    );
    // Should have branch_weights metadata.
    assert!(
        ir.contains("branch_weights"),
        "should have branch_weights metadata: {ir}"
    );
    // Then-branch (base case) should be unlikely (weight 1).
    assert!(
        ir.contains("i32 1, i32 2000"),
        "base case should be unlikely: {ir}"
    );
}

#[test]
fn test_no_branch_hints_in_non_pure() {
    // Non-@pure function should NOT get branch prediction hints.
    let m = module(
        Some("test"),
        vec![func(
            "test_fn",
            vec![param("n", HirType::Primitive(PrimitiveType::I32))],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::If {
                    condition: binop(BinOp::LtEq, ident("n"), int_lit(1)),
                    then_block: block(vec![stmt(HirStmtKind::Return {
                        value: Some(int_lit(1)),
                    })]),
                    else_block: None,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // Should NOT have !prof metadata.
    assert!(
        !ir.contains("!prof"),
        "non-pure function should not have branch hints: {ir}"
    );
}

// --- Test #7: loop vectorization hints ---

#[test]
fn test_loop_vectorization_hints() {
    // @vectorizable function with a for loop should get vectorization metadata.
    let sum_func = func_with_annotations(
        "vec_sum",
        vec![
            param(
                "arr",
                HirType::Array {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    size: 100,
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "sum".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(int_lit(0)),
                mutable: true,
            }),
            stmt(HirStmtKind::For {
                var: "i".to_string(),
                var_span: span(),
                var_type: HirType::Primitive(PrimitiveType::I32),
                iterable: call("range", vec![int_lit(0), ident("n")]),
                body: block(vec![stmt(HirStmtKind::Assign {
                    target: ident("sum"),
                    value: binop(BinOp::Add, ident("sum"), int_lit(1)),
                })]),
            }),
            stmt(HirStmtKind::Return {
                value: Some(ident("sum")),
            }),
        ]),
        vec![vectorizable_ann()],
    );

    let m = module(Some("test"), vec![sum_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Should have loop vectorization metadata.
    assert!(
        ir.contains("!llvm.loop"),
        "vectorizable loop should have !llvm.loop metadata: {ir}"
    );
    assert!(
        ir.contains("llvm.loop.vectorize.enable"),
        "should have vectorize.enable metadata: {ir}"
    );
}

#[test]
fn test_no_vectorization_without_annotation() {
    // Regular function loops should NOT get vectorization metadata.
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
                    value: Some(int_lit(0)),
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
                    value: Some(ident("sum")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        !ir.contains("llvm.loop.vectorize"),
        "non-vectorizable function should not have vectorize metadata: {ir}"
    );
}

// --- Test: fibonacci benchmark generates all optimization hints ---

#[test]
fn test_fibonacci_optimizations() {
    // Full integration test: fibonacci with @pure generates all expected hints.
    let source = std::fs::read_to_string("../../benchmarks/fib/fib.axm")
        .expect("should read fib.axm");
    let parse_result = axiom_parser::parse(&source);
    assert!(
        !parse_result.has_errors(),
        "fib.axm should parse: {:?}",
        parse_result.errors
    );
    let hir_module = axiom_hir::lower(&parse_result.module)
        .expect("should lower");
    let ir = codegen(&hir_module).expect("should codegen");

    // 1. noalias is not applicable (no ptr params) -- that's correct.
    // 2. @pure attributes.
    assert!(
        ir.contains("memory(none)"),
        "fib should have memory(none): {ir}"
    );
    assert!(
        ir.contains("nounwind"),
        "fib should have nounwind: {ir}"
    );
    // 3. nsw on arithmetic.
    assert!(
        ir.contains("sub nsw i64"),
        "fib should have nsw on sub: {ir}"
    );
    assert!(
        ir.contains("add nsw i64"),
        "fib should have nsw on add: {ir}"
    );
    // 6. Branch prediction.
    assert!(
        ir.contains("!prof"),
        "fib base case should have branch prediction: {ir}"
    );
    assert!(
        ir.contains("branch_weights"),
        "fib should have branch_weights: {ir}"
    );
}

// --- Test: noalias on call-site arguments ---

#[test]
fn test_noalias_call_args() {
    // When calling a function with ptr params, the call-site should also
    // have noalias on the pointer arguments.
    let sum_func = func(
        "sum_arr",
        vec![
            param(
                "arr",
                HirType::Array {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    size: 10,
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(int_lit(0)),
        })]),
    );

    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "arr".to_string(),
                name_span: span(),
                ty: HirType::Array {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    size: 10,
                },
                value: Some(HirExpr {
                    id: nid(0),
                    kind: HirExprKind::ArrayZeros {
                        element_type: HirType::Primitive(PrimitiveType::I32),
                        size: 10,
                    },
                    span: span(),
                }),
                mutable: false,
            }),
            stmt(HirStmtKind::Let {
                name: "result".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(call("sum_arr", vec![ident("arr"), int_lit(10)])),
                mutable: false,
            }),
            stmt(HirStmtKind::Return {
                value: Some(ident("result")),
            }),
        ]),
    );

    let m = module(Some("test"), vec![sum_func, main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Call-site should have noalias on ptr arg.
    assert!(
        ir.contains("ptr noalias %arr"),
        "call-site should have noalias on ptr args: {ir}"
    );
}

// --- Test: loop increment has nsw ---

#[test]
fn test_loop_increment_nsw() {
    // The for-loop increment should have nsw flag.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::For {
                    var: "i".to_string(),
                    var_span: span(),
                    var_type: HirType::Primitive(PrimitiveType::I32),
                    iterable: call("range", vec![int_lit(0), int_lit(10)]),
                    body: block(vec![]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // The loop increment should have nsw.
    assert!(
        ir.contains("add nsw i32"),
        "loop increment should have nsw: {ir}"
    );
}

// --- Test: bitwise builtins ---

#[test]
fn test_bitwise_builtins() {
    // Test band, bor, bxor, shl, shr, lshr, bnot
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // band(0xFF, 0x0F) = 15
                stmt(HirStmtKind::Let {
                    name: "b_and".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("band", vec![int_lit(0xFF), int_lit(0x0F)])),
                    mutable: false,
                }),
                // bor(0xF0, 0x0F) = 255
                stmt(HirStmtKind::Let {
                    name: "b_or".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("bor", vec![int_lit(0xF0), int_lit(0x0F)])),
                    mutable: false,
                }),
                // bxor(0xFF, 0x0F) = 240
                stmt(HirStmtKind::Let {
                    name: "b_xor".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("bxor", vec![int_lit(0xFF), int_lit(0x0F)])),
                    mutable: false,
                }),
                // shl(1, 8) = 256
                stmt(HirStmtKind::Let {
                    name: "shifted_l".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("shl", vec![int_lit(1), int_lit(8)])),
                    mutable: false,
                }),
                // shr(256, 4) = 16
                stmt(HirStmtKind::Let {
                    name: "shifted_r".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("shr", vec![int_lit(256), int_lit(4)])),
                    mutable: false,
                }),
                // lshr(256, 4) = 16
                stmt(HirStmtKind::Let {
                    name: "shifted_lr".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("lshr", vec![int_lit(256), int_lit(4)])),
                    mutable: false,
                }),
                // bnot(0) = -1
                stmt(HirStmtKind::Let {
                    name: "b_not".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("bnot", vec![int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // band: LLVM `and`
    assert!(
        ir.contains("= and i32"),
        "band should emit LLVM `and`: {ir}"
    );
    // bor: LLVM `or`
    assert!(
        ir.contains("= or i32"),
        "bor should emit LLVM `or`: {ir}"
    );
    // bxor: LLVM `xor` (for values, not bnot)
    assert!(
        ir.contains("= xor i32") && ir.contains("255, 15"),
        "bxor should emit LLVM `xor`: {ir}"
    );
    // shl: LLVM `shl`
    assert!(
        ir.contains("= shl i32"),
        "shl should emit LLVM `shl`: {ir}"
    );
    // shr: LLVM `ashr`
    assert!(
        ir.contains("= ashr i32"),
        "shr should emit LLVM `ashr`: {ir}"
    );
    // lshr: LLVM `lshr`
    assert!(
        ir.contains("= lshr i32"),
        "lshr should emit LLVM `lshr`: {ir}"
    );
    // bnot: LLVM `xor %val, -1`
    assert!(
        ir.contains("xor i32"),
        "bnot should emit LLVM `xor %val, -1`: {ir}"
    );
}

#[test]
fn test_bitwise_rotate_builtins() {
    // Test rotl and rotr which use LLVM funnel shift intrinsics.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // rotl(0x80000001, 1) = 3
                stmt(HirStmtKind::Let {
                    name: "rot_l".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("rotl", vec![int_lit(0x80000001_u32 as i128), int_lit(1)])),
                    mutable: false,
                }),
                // rotr(3, 1) = 0x80000001
                stmt(HirStmtKind::Let {
                    name: "rot_r".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("rotr", vec![int_lit(3), int_lit(1)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // rotl: @llvm.fshl.i32
    assert!(
        ir.contains("call i32 @llvm.fshl.i32(i32"),
        "rotl should call llvm.fshl.i32: {ir}"
    );
    assert!(
        ir.contains("declare i32 @llvm.fshl.i32(i32, i32, i32)"),
        "should declare llvm.fshl.i32: {ir}"
    );

    // rotr: @llvm.fshr.i32
    assert!(
        ir.contains("call i32 @llvm.fshr.i32(i32"),
        "rotr should call llvm.fshr.i32: {ir}"
    );
    assert!(
        ir.contains("declare i32 @llvm.fshr.i32(i32, i32, i32)"),
        "should declare llvm.fshr.i32: {ir}"
    );
}

// -----------------------------------------------------------------------
// Heap allocation builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_heap_alloc() {
    // heap_alloc(100, 4) should emit sext + mul + call malloc with noalias
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(100), int_lit(4)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call noalias ptr @malloc(i64"),
        "heap_alloc should emit malloc call with noalias: {ir}"
    );
    assert!(
        ir.contains("declare noalias ptr @malloc(i64) #"),
        "should declare malloc: {ir}"
    );
}

#[test]
fn test_heap_alloc_zeroed() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc_zeroed", vec![int_lit(50), int_lit(4)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call noalias ptr @calloc(i64"),
        "heap_alloc_zeroed should emit calloc call: {ir}"
    );
    assert!(
        ir.contains("declare noalias ptr @calloc(i64, i64) #"),
        "should declare calloc: {ir}"
    );
}

#[test]
fn test_heap_free() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call void @free(ptr"),
        "heap_free should emit free call: {ir}"
    );
    assert!(
        ir.contains("declare void @free(ptr allocptr) #"),
        "should declare free: {ir}"
    );
}

#[test]
fn test_heap_realloc() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                    mutable: true,
                }),
                stmt(HirStmtKind::Assign {
                    target: ident("p"),
                    value: call(
                        "heap_realloc",
                        vec![ident("p"), int_lit(20), int_lit(4)],
                    ),
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call noalias ptr @realloc(ptr"),
        "heap_realloc should emit realloc call: {ir}"
    );
    assert!(
        ir.contains("declare noalias ptr @realloc(ptr, i64) #"),
        "should declare realloc: {ir}"
    );
}

#[test]
fn test_ptr_read_write_i32() {
    // ptr_write_i32(p, 0, 42); ptr_read_i32(p, 0) should emit GEP + store/load
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call(
                        "ptr_write_i32",
                        vec![ident("p"), int_lit(0), int_lit(42)],
                    ),
                }),
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("ptr_read_i32", vec![ident("p"), int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("val")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // ptr_write: GEP + store
    assert!(
        ir.contains("getelementptr i32, ptr"),
        "ptr_write should emit GEP: {ir}"
    );
    assert!(
        ir.contains("store i32"),
        "ptr_write should emit store: {ir}"
    );
    // ptr_read: GEP + load
    assert!(
        ir.contains("load i32, ptr"),
        "ptr_read should emit load: {ir}"
    );
}

#[test]
fn test_ptr_read_write_f64() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::F64)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(8)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call(
                        "ptr_write_f64",
                        vec![ident("p"), int_lit(0), float_lit(3.14)],
                    ),
                }),
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("ptr_read_f64", vec![ident("p"), int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("getelementptr double, ptr"),
        "ptr_write_f64 should emit GEP with double: {ir}"
    );
    assert!(
        ir.contains("store double"),
        "ptr_write_f64 should emit store double: {ir}"
    );
    assert!(
        ir.contains("load double, ptr"),
        "ptr_read_f64 should emit load double: {ir}"
    );
}

#[test]
fn test_ptr_read_write_i64() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I64)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(8)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call(
                        "ptr_write_i64",
                        vec![ident("p"), int_lit(0), int_lit(999)],
                    ),
                }),
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("ptr_read_i64", vec![ident("p"), int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("getelementptr i64, ptr"),
        "ptr_write_i64 should emit GEP with i64: {ir}"
    );
    assert!(
        ir.contains("store i64"),
        "ptr_write_i64 should emit store i64: {ir}"
    );
    assert!(
        ir.contains("load i64, ptr"),
        "ptr_read_i64 should emit load i64: {ir}"
    );
}

#[test]
fn test_heap_program_full_integration() {
    // A full program: alloc, write, read, sum, free -- mirrors heap_test.axm
    let m = module(
        Some("heap_int_test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let data: ptr[i32] = heap_alloc(10, 4);
                stmt(HirStmtKind::Let {
                    name: "data".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                    mutable: false,
                }),
                // for i in range(0,10) { ptr_write_i32(data, i, i*i); }
                stmt(HirStmtKind::For {
                    var: "i".to_string(),
                    var_span: span(),
                    var_type: HirType::Primitive(PrimitiveType::I32),
                    iterable: call("range", vec![int_lit(0), int_lit(10)]),
                    body: block(vec![stmt(HirStmtKind::Expr {
                        expr: call(
                            "ptr_write_i32",
                            vec![
                                ident("data"),
                                ident("i"),
                                binop(BinOp::Mul, ident("i"), ident("i")),
                            ],
                        ),
                    })]),
                }),
                // let sum: i64 = 0;
                stmt(HirStmtKind::Let {
                    name: "sum".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(int_lit(0)),
                    mutable: true,
                }),
                // for i in range(0,10) { sum = sum + widen(ptr_read_i32(data, i)); }
                stmt(HirStmtKind::For {
                    var: "i".to_string(),
                    var_span: span(),
                    var_type: HirType::Primitive(PrimitiveType::I32),
                    iterable: call("range", vec![int_lit(0), int_lit(10)]),
                    body: block(vec![stmt(HirStmtKind::Assign {
                        target: ident("sum"),
                        value: binop(
                            BinOp::Add,
                            ident("sum"),
                            call(
                                "widen",
                                vec![call("ptr_read_i32", vec![ident("data"), ident("i")])],
                            ),
                        ),
                    })]),
                }),
                // heap_free(data);
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("data")]),
                }),
                // print_i64(sum);
                stmt(HirStmtKind::Expr {
                    expr: call("print_i64", vec![ident("sum")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // Verify all key components are present.
    assert!(
        ir.contains("call noalias ptr @malloc(i64"),
        "should emit malloc: {ir}"
    );
    assert!(
        ir.contains("getelementptr i32, ptr"),
        "should emit GEP for ptr_write/read: {ir}"
    );
    assert!(
        ir.contains("call void @free(ptr"),
        "should emit free: {ir}"
    );
    assert!(
        ir.contains("sext i32"),
        "should widen i32 index to i64: {ir}"
    );
    assert!(
        ir.contains("declare noalias ptr @malloc(i64) #"),
        "should declare malloc: {ir}"
    );
    assert!(
        ir.contains("declare void @free(ptr allocptr) #"),
        "should declare free: {ir}"
    );
}

#[test]
fn test_no_malloc_decl_when_unused() {
    // A program that doesn't use heap builtins should NOT declare malloc/free.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            })]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        !ir.contains("@malloc"),
        "should not declare malloc when unused: {ir}"
    );
    assert!(
        !ir.contains("@free"),
        "should not declare free when unused: {ir}"
    );
    assert!(
        !ir.contains("@calloc"),
        "should not declare calloc when unused: {ir}"
    );
    assert!(
        !ir.contains("@realloc"),
        "should not declare realloc when unused: {ir}"
    );
}

// -----------------------------------------------------------------------
// LLVM allocator attribute tests
// -----------------------------------------------------------------------

#[test]
fn test_malloc_alloc_attrs() {
    // Verify that malloc/calloc/realloc/free declarations include
    // LLVM allockind and alloc-family attributes for optimizer integration.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                    mutable: true,
                }),
                stmt(HirStmtKind::Let {
                    name: "q".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc_zeroed", vec![int_lit(10), int_lit(4)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Assign {
                    target: ident("p"),
                    value: call(
                        "heap_realloc",
                        vec![ident("p"), int_lit(20), int_lit(4)],
                    ),
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("q")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // malloc declaration should have allockind("alloc,uninitialized") attribute group.
    assert!(
        ir.contains("declare noalias ptr @malloc(i64) #"),
        "malloc should have attribute group reference: {ir}"
    );
    assert!(
        ir.contains("allockind(\"alloc,uninitialized\")"),
        "malloc attribute group should have allockind(alloc,uninitialized): {ir}"
    );

    // calloc declaration should have allockind("alloc,zeroed") attribute group.
    assert!(
        ir.contains("declare noalias ptr @calloc(i64, i64) #"),
        "calloc should have attribute group reference: {ir}"
    );
    assert!(
        ir.contains("allockind(\"alloc,zeroed\")"),
        "calloc attribute group should have allockind(alloc,zeroed): {ir}"
    );

    // realloc declaration should have allockind("realloc") attribute group.
    assert!(
        ir.contains("declare noalias ptr @realloc(ptr, i64) #"),
        "realloc should have attribute group reference: {ir}"
    );
    assert!(
        ir.contains("allockind(\"realloc\")"),
        "realloc attribute group should have allockind(realloc): {ir}"
    );

    // free declaration should have allocptr parameter attribute and allockind("free").
    assert!(
        ir.contains("declare void @free(ptr allocptr) #"),
        "free should have allocptr param attribute and attr group ref: {ir}"
    );
    assert!(
        ir.contains("allockind(\"free\")"),
        "free attribute group should have allockind(free): {ir}"
    );

    // All allocator functions should be in the same alloc-family.
    let family_count = ir.matches("\"alloc-family\"=\"malloc\"").count();
    assert!(
        family_count >= 4,
        "all 4 allocator declarations should have alloc-family=malloc (got {family_count}): {ir}"
    );
}

#[test]
fn test_escape_analysis_hint() {
    // A @pure function that uses heap_alloc should get both @pure function
    // attributes (memory(none), nounwind, etc.) and the LLVM allocator
    // attributes on the malloc declaration. Together these enable LLVM's
    // HeapToStackPass to promote the allocation to the stack.
    let m = module(
        Some("test"),
        vec![func_with_annotations(
            "compute",
            vec![param("n", HirType::Primitive(PrimitiveType::I32))],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "buf".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![ident("n"), int_lit(4)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("buf")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(42)),
                }),
            ]),
            vec![pure_ann()],
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // The function should have @pure attributes (memory + nounwind).
    // Note: @pure with a ptr result from heap_alloc uses argmem:read if
    // there are ptr params. But compute takes only i32, so memory(none).
    assert!(
        ir.contains("memory(none)"),
        "@pure function should have memory(none): {ir}"
    );
    assert!(
        ir.contains("nounwind"),
        "@pure function should have nounwind: {ir}"
    );

    // The malloc declaration should have allocator attributes.
    assert!(
        ir.contains("allockind(\"alloc,uninitialized\")"),
        "malloc should have allockind for heap-to-stack optimization: {ir}"
    );
    assert!(
        ir.contains("\"alloc-family\"=\"malloc\""),
        "should have alloc-family for malloc/free pairing: {ir}"
    );

    // The free declaration should have allocator attributes.
    assert!(
        ir.contains("allockind(\"free\")"),
        "free should have allockind for dead-free elimination: {ir}"
    );
}

#[test]
fn test_lifetime_scope() {
    // @lifetime(scope) on a let binding with heap_alloc should emit alloca
    // instead of malloc, promoting the heap allocation to the stack.
    let lifetime_scope_ann = HirAnnotation {
        kind: HirAnnotationKind::Lifetime("scope".to_string()),
        span: SPAN_DUMMY,
    };

    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt_with_annotations(
                    HirStmtKind::Let {
                        name: "buf".to_string(),
                        name_span: span(),
                        ty: HirType::Ptr {
                            element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                        },
                        value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                        mutable: false,
                    },
                    vec![lifetime_scope_ann],
                ),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // The allocation should be promoted to alloca (stack).
    assert!(
        ir.contains("alloca i8, i64"),
        "@lifetime(scope) should promote heap_alloc to alloca: {ir}"
    );

    // No malloc should be called — the allocation is on the stack.
    assert!(
        !ir.contains("call noalias ptr @malloc"),
        "@lifetime(scope) should NOT call malloc: {ir}"
    );

    // No malloc declaration needed since it was promoted.
    assert!(
        !ir.contains("@malloc"),
        "@lifetime(scope) should not need malloc declaration: {ir}"
    );
}

// -----------------------------------------------------------------------
// Arena (bump) allocator builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_arena_create() {
    // arena_create(1048576) should emit two malloc calls + stores for struct fields.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arena".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("arena_create", vec![int_lit(1048576)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // Should allocate 24 bytes for the arena struct.
    assert!(
        ir.contains("call noalias ptr @malloc(i64 24)"),
        "arena_create should malloc 24 bytes for arena struct: {ir}"
    );
    // Should allocate the backing buffer.
    assert!(
        ir.contains("call noalias ptr @malloc(i64 %"),
        "arena_create should malloc the backing buffer: {ir}"
    );
    // Should store offset = 0.
    assert!(
        ir.contains("store i64 0, ptr %"),
        "arena_create should store offset = 0: {ir}"
    );
    // Should emit GEP for offset field at +8.
    assert!(
        ir.contains("getelementptr i8, ptr %"),
        "arena_create should emit GEP for struct fields: {ir}"
    );
    // Should declare malloc.
    assert!(
        ir.contains("declare noalias ptr @malloc(i64) #"),
        "arena_create should declare malloc: {ir}"
    );
}

#[test]
fn test_arena_alloc() {
    // arena_alloc(arena, 100, 4) should emit GEP + load offset + bump + store.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arena".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("arena_create", vec![int_lit(4096)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "data".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call(
                        "arena_alloc",
                        vec![ident("arena"), int_lit(100), int_lit(4)],
                    )),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // Should load the base pointer.
    assert!(
        ir.contains("load ptr, ptr %"),
        "arena_alloc should load base pointer: {ir}"
    );
    // Should load the offset.
    assert!(
        ir.contains("load i64, ptr %"),
        "arena_alloc should load offset: {ir}"
    );
    // Should compute total = count * elem_size.
    assert!(
        ir.contains("mul i64"),
        "arena_alloc should compute total bytes: {ir}"
    );
    // Should compute new_offset = offset + total.
    assert!(
        ir.contains("add i64"),
        "arena_alloc should bump the offset: {ir}"
    );
    // Should compute result pointer = base + offset via GEP.
    let gep_count = ir.matches("getelementptr i8, ptr %").count();
    assert!(
        gep_count >= 2,
        "arena_alloc should emit GEP for struct fields and result pointer (got {gep_count}): {ir}"
    );
}

#[test]
fn test_arena_reset() {
    // arena_reset(arena) should store 0 to the offset field.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arena".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("arena_create", vec![int_lit(4096)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("arena_reset", vec![ident("arena")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // The reset emits a GEP to arena+8 then store i64 0.
    // Count "store i64 0" -- arena_create stores one (offset=0), arena_reset stores another.
    let store_zero_count = ir.matches("store i64 0, ptr %").count();
    assert!(
        store_zero_count >= 2,
        "arena_reset should store 0 to offset (found {store_zero_count} store-zero ops): {ir}"
    );
}

#[test]
fn test_arena_destroy() {
    // arena_destroy(arena) should free the base buffer and the arena struct.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arena".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("arena_create", vec![int_lit(4096)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("arena_destroy", vec![ident("arena")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // Should emit two free calls (one for base, one for arena struct).
    let free_count = ir.matches("call void @free(ptr").count();
    assert!(
        free_count == 2,
        "arena_destroy should emit exactly 2 free calls (got {free_count}): {ir}"
    );
    // Should declare free.
    assert!(
        ir.contains("declare void @free(ptr allocptr) #"),
        "arena_destroy should declare free: {ir}"
    );
}

#[test]
fn test_arena_program() {
    // Full integration: create arena, alloc, write, read, reset, reuse, destroy.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let arena: ptr[i32] = arena_create(4096);
                stmt(HirStmtKind::Let {
                    name: "arena".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("arena_create", vec![int_lit(4096)])),
                    mutable: false,
                }),
                // let data: ptr[i32] = arena_alloc(arena, 10, 4);
                stmt(HirStmtKind::Let {
                    name: "data".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call(
                        "arena_alloc",
                        vec![ident("arena"), int_lit(10), int_lit(4)],
                    )),
                    mutable: false,
                }),
                // ptr_write_i32(data, 0, 42);
                stmt(HirStmtKind::Expr {
                    expr: call(
                        "ptr_write_i32",
                        vec![ident("data"), int_lit(0), int_lit(42)],
                    ),
                }),
                // let val: i32 = ptr_read_i32(data, 0);
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("ptr_read_i32", vec![ident("data"), int_lit(0)])),
                    mutable: false,
                }),
                // arena_reset(arena);
                stmt(HirStmtKind::Expr {
                    expr: call("arena_reset", vec![ident("arena")]),
                }),
                // let reused: ptr[i32] = arena_alloc(arena, 5, 4);
                stmt(HirStmtKind::Let {
                    name: "reused".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call(
                        "arena_alloc",
                        vec![ident("arena"), int_lit(5), int_lit(4)],
                    )),
                    mutable: false,
                }),
                // arena_destroy(arena);
                stmt(HirStmtKind::Expr {
                    expr: call("arena_destroy", vec![ident("arena")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // Verify all key components are present.
    assert!(
        ir.contains("call noalias ptr @malloc(i64 24)"),
        "should emit malloc for arena struct: {ir}"
    );
    assert!(
        ir.contains("getelementptr i8, ptr"),
        "should emit GEP for arena struct fields: {ir}"
    );
    assert!(
        ir.contains("load ptr, ptr"),
        "should load base pointer: {ir}"
    );
    assert!(
        ir.contains("load i64, ptr"),
        "should load offset: {ir}"
    );
    assert!(
        ir.contains("mul i64"),
        "should compute total bytes: {ir}"
    );
    assert!(
        ir.contains("add i64"),
        "should bump offset: {ir}"
    );
    let free_count = ir.matches("call void @free(ptr").count();
    assert!(
        free_count == 2,
        "should emit 2 free calls for destroy (got {free_count}): {ir}"
    );
    assert!(
        ir.contains("declare noalias ptr @malloc(i64) #"),
        "should declare malloc: {ir}"
    );
    assert!(
        ir.contains("declare void @free(ptr allocptr) #"),
        "should declare free: {ir}"
    );
}

// -----------------------------------------------------------------------
// Struct codegen tests
// -----------------------------------------------------------------------

#[test]
fn test_struct_type_definition() {
    let vec3 = hir_struct(
        "Vec3",
        vec![
            struct_field("x", HirType::Primitive(PrimitiveType::F64)),
            struct_field("y", HirType::Primitive(PrimitiveType::F64)),
            struct_field("z", HirType::Primitive(PrimitiveType::F64)),
        ],
    );
    let m = module_with_structs(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            })]),
        )],
        vec![vec3],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("%struct.Vec3 = type { double, double, double }"),
        "should emit struct type definition: {ir}"
    );
}

#[test]
fn test_struct_field_access() {
    let point = hir_struct(
        "Point",
        vec![
            struct_field("x", HirType::Primitive(PrimitiveType::F64)),
            struct_field("y", HirType::Primitive(PrimitiveType::F64)),
        ],
    );
    let m = module_with_structs(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let p: Point;
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::UserDefined("Point".to_string()),
                    value: None,
                    mutable: false,
                }),
                // p.x = 1.0;
                stmt(HirStmtKind::Assign {
                    target: field_access(ident("p"), "x"),
                    value: float_lit(1.0),
                }),
                // p.y = 2.0;
                stmt(HirStmtKind::Assign {
                    target: field_access(ident("p"), "y"),
                    value: float_lit(2.0),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
        vec![point],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // Struct type should be emitted.
    assert!(
        ir.contains("%struct.Point = type { double, double }"),
        "should emit struct type: {ir}"
    );
    // Alloca for struct.
    assert!(
        ir.contains("alloca %struct.Point"),
        "should alloca struct: {ir}"
    );
    // Memset zero-init.
    assert!(
        ir.contains("call void @llvm.memset.p0.i64(ptr %p.0, i8 0, i64 16, i1 false)"),
        "should zero-init struct: {ir}"
    );
    // GEP for field x (index 0).
    assert!(
        ir.contains("getelementptr inbounds %struct.Point, ptr %p.0, i32 0, i32 0"),
        "should GEP field x (index 0): {ir}"
    );
    // GEP for field y (index 1).
    assert!(
        ir.contains("getelementptr inbounds %struct.Point, ptr %p.0, i32 0, i32 1"),
        "should GEP field y (index 1): {ir}"
    );
    // Store values.
    assert!(
        ir.contains("store double 1.0"),
        "should store field x: {ir}"
    );
    assert!(
        ir.contains("store double 2.0"),
        "should store field y: {ir}"
    );
}

#[test]
fn test_struct_as_param() {
    let vec2 = hir_struct(
        "Vec2",
        vec![
            struct_field("x", HirType::Primitive(PrimitiveType::F64)),
            struct_field("y", HirType::Primitive(PrimitiveType::F64)),
        ],
    );
    let m = module_with_structs(
        Some("test"),
        vec![func(
            "get_x",
            vec![param("v", HirType::UserDefined("Vec2".to_string()))],
            HirType::Primitive(PrimitiveType::F64),
            block(vec![stmt(HirStmtKind::Return {
                value: Some(field_access(ident("v"), "x")),
            })]),
        )],
        vec![vec2],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // Struct param should be ptr.
    assert!(
        ir.contains("@get_x(ptr noalias %v)"),
        "should pass struct as ptr noalias: {ir}"
    );
    // Should alloca ptr for param.
    assert!(
        ir.contains("%v.addr = alloca ptr"),
        "should alloca ptr for struct param: {ir}"
    );
    // Should store param ptr.
    assert!(
        ir.contains("store ptr %v, ptr %v.addr"),
        "should store struct param ptr: {ir}"
    );
    // Should GEP for field access on param.
    assert!(
        ir.contains("getelementptr inbounds %struct.Vec2"),
        "should GEP for field access: {ir}"
    );
}

#[test]
fn test_struct_program() {
    // Full integration test using parse → lower → codegen pipeline.
    let source = r#"
@module struct_prog;

struct Vec3 {
x: f64,
y: f64,
z: f64,
}

@pure
fn vec3_dot(a: Vec3, b: Vec3) -> f64 {
return a.x * b.x + a.y * b.y + a.z * b.z;
}

fn main() -> i32 {
let a: Vec3;
a.x = 3.0;
a.y = 4.0;
a.z = 0.0;

let dot: f64 = vec3_dot(a, a);
print_f64(dot);
return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(
        parse_result.errors.is_empty(),
        "parse errors: {:?}",
        parse_result.errors
    );
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");

    // Struct type definition.
    assert!(
        ir.contains("%struct.Vec3 = type { double, double, double }"),
        "should define Vec3: {ir}"
    );
    // Struct alloca + memset.
    assert!(
        ir.contains("alloca %struct.Vec3"),
        "should alloca Vec3: {ir}"
    );
    assert!(
        ir.contains("call void @llvm.memset.p0.i64"),
        "should zero-init: {ir}"
    );
    // Pure function with struct params gets memory(argmem: read).
    assert!(
        ir.contains("memory(argmem: read)"),
        "should have argmem read for @pure with struct params: {ir}"
    );
    // Field stores.
    assert!(ir.contains("store double 3.0"), "should store x: {ir}");
    assert!(ir.contains("store double 4.0"), "should store y: {ir}");
    // Dot product call.
    assert!(
        ir.contains("call fastcc double @vec3_dot"),
        "should call vec3_dot: {ir}"
    );
}

#[test]
fn test_struct_field_read() {
    let point = hir_struct(
        "Point",
        vec![
            struct_field("x", HirType::Primitive(PrimitiveType::F64)),
            struct_field("y", HirType::Primitive(PrimitiveType::F64)),
        ],
    );
    let m = module_with_structs(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let p: Point;
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::UserDefined("Point".to_string()),
                    value: None,
                    mutable: false,
                }),
                // p.x = 5.0;
                stmt(HirStmtKind::Assign {
                    target: field_access(ident("p"), "x"),
                    value: float_lit(5.0),
                }),
                // let val: f64 = p.x;
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(field_access(ident("p"), "x")),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
        vec![point],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // Should load field value for the let binding.
    assert!(
        ir.contains("load double, ptr"),
        "should load field value: {ir}"
    );
}

#[test]
fn test_let_without_initializer() {
    // Test that `let x: i32;` (no initializer) works with zero-init.
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
                    value: None,
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("x")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("alloca i32"),
        "should alloca: {ir}"
    );
    assert!(
        ir.contains("store i32 0, ptr"),
        "should zero-init: {ir}"
    );
}

// -----------------------------------------------------------------------
// I/O runtime builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_clock_ns_builtin() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "t".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("clock_ns", vec![])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call i64 @axiom_clock_ns()"),
        "should call axiom_clock_ns: {ir}"
    );
    assert!(
        ir.contains("declare i64 @axiom_clock_ns()"),
        "should declare axiom_clock_ns: {ir}"
    );
    assert!(needs_runtime(&ir), "IR using clock_ns should need runtime");
}

#[test]
fn test_file_read_builtin() {
    // file_read takes two ptr args; we use heap_alloc to get a ptr for out_size.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "sz_ptr".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I8)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(1), int_lit(8)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "buf".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I8)),
                    },
                    value: Some(call(
                        "file_read",
                        vec![str_lit("test.txt"), ident("sz_ptr")],
                    )),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call ptr @axiom_file_read(ptr"),
        "should call axiom_file_read: {ir}"
    );
    assert!(
        ir.contains("declare ptr @axiom_file_read(ptr, ptr)"),
        "should declare axiom_file_read: {ir}"
    );
}

#[test]
fn test_file_size_builtin() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "sz".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("file_size", vec![str_lit("test.txt")])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call i64 @axiom_file_size(ptr"),
        "should call axiom_file_size: {ir}"
    );
    assert!(
        ir.contains("declare i64 @axiom_file_size(ptr)"),
        "should declare axiom_file_size: {ir}"
    );
}

#[test]
fn test_get_argc_builtin() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "n".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("get_argc", vec![])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call i32 @axiom_get_argc()"),
        "should call axiom_get_argc: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_get_argc()"),
        "should declare axiom_get_argc: {ir}"
    );
}

#[test]
fn test_get_argv_builtin() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "arg0".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I8)),
                    },
                    value: Some(call("get_argv", vec![int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call ptr @axiom_get_argv(i32"),
        "should call axiom_get_argv: {ir}"
    );
    assert!(
        ir.contains("declare ptr @axiom_get_argv(i32)"),
        "should declare axiom_get_argv: {ir}"
    );
}

#[test]
fn test_runtime_declarations_only_when_needed() {
    // A module that doesn't use I/O builtins should not emit runtime declarations.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            })]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        !ir.contains("@axiom_"),
        "should NOT contain runtime declarations: {ir}"
    );
    assert!(
        !needs_runtime(&ir),
        "IR without I/O builtins should not need runtime"
    );
}

#[test]
fn test_file_write_builtin() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Expr {
                    expr: call(
                        "file_write",
                        vec![str_lit("out.bin"), str_lit("data"), int_lit(4)],
                    ),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call void @axiom_file_write(ptr"),
        "should call axiom_file_write: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_file_write(ptr, ptr, i64)"),
        "should declare axiom_file_write: {ir}"
    );
}

// -----------------------------------------------------------------------
// Coroutine builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_coro_resume_builtin() {
    // coro_resume(handle: i32) -> i32
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("coro_resume", vec![int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call i32 @axiom_coro_resume(i32"),
        "should call axiom_coro_resume: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_coro_resume(i32)"),
        "should declare axiom_coro_resume: {ir}"
    );
    assert!(
        needs_runtime(&ir),
        "IR using coro_resume should need runtime"
    );
}

#[test]
fn test_coro_yield_builtin() {
    // coro_yield(value: i32)
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Expr {
                    expr: call("coro_yield", vec![int_lit(42)]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call void @axiom_coro_yield(i32"),
        "should call axiom_coro_yield: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_coro_yield(i32)"),
        "should declare axiom_coro_yield: {ir}"
    );
}

#[test]
fn test_coro_is_done_builtin() {
    // coro_is_done(handle: i32) -> i32
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "done".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("coro_is_done", vec![int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call i32 @axiom_coro_is_done(i32"),
        "should call axiom_coro_is_done: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_coro_is_done(i32)"),
        "should declare axiom_coro_is_done: {ir}"
    );
}

#[test]
fn test_coro_destroy_builtin() {
    // coro_destroy(handle: i32)
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Expr {
                    expr: call("coro_destroy", vec![int_lit(0)]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("call void @axiom_coro_destroy(i32"),
        "should call axiom_coro_destroy: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_coro_destroy(i32)"),
        "should declare axiom_coro_destroy: {ir}"
    );
}

#[test]
fn test_coro_all_declarations_emitted() {
    // When any coroutine builtin is used, all 5 extern declarations should be emitted.
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Expr {
                    expr: call("coro_yield", vec![int_lit(1)]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("declare i32 @axiom_coro_create(ptr, i32)"),
        "should declare axiom_coro_create: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_coro_resume(i32)"),
        "should declare axiom_coro_resume: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_coro_yield(i32)"),
        "should declare axiom_coro_yield: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_coro_is_done(i32)"),
        "should declare axiom_coro_is_done: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_coro_destroy(i32)"),
        "should declare axiom_coro_destroy: {ir}"
    );
}

#[test]
fn test_coro_needs_runtime() {
    // Coroutine builtins should trigger needs_runtime detection.
    assert!(needs_runtime("declare i32 @axiom_coro_create(ptr, i32)"));
    assert!(needs_runtime("declare i32 @axiom_coro_resume(i32)"));
    assert!(needs_runtime("declare void @axiom_coro_yield(i32)"));
    assert!(needs_runtime("declare i32 @axiom_coro_is_done(i32)"));
    assert!(needs_runtime("declare void @axiom_coro_destroy(i32)"));
    // Non-coroutine, non-runtime IR should not trigger.
    assert!(!needs_runtime("define i32 @main() { ret i32 0 }"));
}

// ======================================================================
// MT-1: Memory safety and correctness tests
// ======================================================================

// T1: @pure with ptr params AND ptr_write_* calls -> memory(argmem: readwrite)
#[test]
fn test_pure_with_ptr_writes_gets_argmem_readwrite() {
    let compute_func = func_with_annotations(
        "compute_chunk",
        vec![
            param(
                "data",
                HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
            ),
            param("start", HirType::Primitive(PrimitiveType::I32)),
            param("end", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Unknown("void".to_string()),
        block(vec![stmt(HirStmtKind::Expr {
            expr: call(
                "ptr_write_i32",
                vec![ident("data"), ident("start"), int_lit(42)],
            ),
        })]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![compute_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("memory(argmem: readwrite)"),
        "@pure with ptr_write should get memory(argmem: readwrite): {ir}"
    );
    assert!(
        !ir.contains("memory(argmem: read)\"") && !ir.contains("memory(argmem: read) "),
        "@pure with ptr_write should NOT get memory(argmem: read) alone: {ir}"
    );
}

// T2: @pure with ptr params but NO ptr_write_* calls -> memory(argmem: read)
#[test]
fn test_pure_with_ptr_readonly_gets_argmem_read() {
    let sum_func = func_with_annotations(
        "sum_arr",
        vec![
            param(
                "arr",
                HirType::Array {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    size: 10,
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(int_lit(0)),
        })]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![sum_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("memory(argmem: read)"),
        "@pure with ptr params but no writes should get memory(argmem: read): {ir}"
    );
    assert!(
        !ir.contains("memory(argmem: readwrite)"),
        "@pure without writes should NOT get readwrite: {ir}"
    );
}

// T3: @pure with ptr params still gets noalias (language rule)
#[test]
fn test_noalias_on_pure_with_ptr() {
    let sum_func = func_with_annotations(
        "sum_arr",
        vec![
            param(
                "arr",
                HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(int_lit(0)),
        })]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![sum_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("ptr noalias %arr"),
        "@pure function ptr params should have noalias (language rule): {ir}"
    );
}

// T4: Non-@pure with ptr params still gets noalias (language rule)
#[test]
fn test_noalias_on_non_pure_with_ptr() {
    let write_func = func_with_annotations(
        "write_arr",
        vec![
            param(
                "arr",
                HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Unknown("void".to_string()),
        block(vec![stmt(HirStmtKind::Expr {
            expr: call(
                "ptr_write_i32",
                vec![ident("arr"), int_lit(0), int_lit(42)],
            ),
        })]),
        vec![], // No @pure annotation
    );

    let m = module(Some("test"), vec![write_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("ptr noalias %arr"),
        "non-@pure function ptr params should have noalias (language rule): {ir}"
    );
}

// T5: fence acquire after job_wait
#[test]
fn test_fence_acquire_after_job_wait() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Expr {
                    expr: call("job_wait", vec![]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // Find the positions: fence acquire must come AFTER call void @axiom_job_wait()
    let wait_pos = ir.find("call void @axiom_job_wait()");
    let fence_pos = ir.find("fence acquire");
    assert!(
        wait_pos.is_some(),
        "should emit call void @axiom_job_wait(): {ir}"
    );
    assert!(
        fence_pos.is_some(),
        "should emit fence acquire after job_wait: {ir}"
    );
    assert!(
        fence_pos.unwrap() > wait_pos.unwrap(),
        "fence acquire should come AFTER job_wait call: {ir}"
    );
}

// T6: fence release before job_dispatch
#[test]
fn test_fence_release_before_job_dispatch() {
    // job_dispatch(func, data, total_items) needs ptr args.
    // We use heap_alloc for both func_ptr and data to avoid undefined variable errors.
    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "func_ptr".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("heap_alloc", vec![int_lit(1), int_lit(4)])),
                mutable: false,
            }),
            stmt(HirStmtKind::Let {
                name: "data".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                mutable: false,
            }),
            stmt(HirStmtKind::Expr {
                expr: call(
                    "job_dispatch",
                    vec![ident("func_ptr"), ident("data"), int_lit(10)],
                ),
            }),
            stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            }),
        ]),
    );

    let m = module(Some("test"), vec![main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Find the positions: fence release must come BEFORE call void @axiom_job_dispatch(...)
    let fence_pos = ir.find("fence release");
    let dispatch_pos = ir.find("call void @axiom_job_dispatch(");
    assert!(
        fence_pos.is_some(),
        "should emit fence release before job_dispatch: {ir}"
    );
    assert!(
        dispatch_pos.is_some(),
        "should emit call void @axiom_job_dispatch: {ir}"
    );
    assert!(
        fence_pos.unwrap() < dispatch_pos.unwrap(),
        "fence release should come BEFORE job_dispatch call: {ir}"
    );
}

// T7: @pure does NOT get nosync
#[test]
fn test_no_nosync_on_pure() {
    let fib_func = func_with_annotations(
        "fib",
        vec![param("n", HirType::Primitive(PrimitiveType::I64))],
        HirType::Primitive(PrimitiveType::I64),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(ident("n")),
        })]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![fib_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        !ir.contains("nosync"),
        "@pure function should NOT have nosync: {ir}"
    );
}

// T8: @const still gets nosync
#[test]
fn test_nosync_on_const() {
    let square_func = func_with_annotations(
        "square",
        vec![param("n", HirType::Primitive(PrimitiveType::I32))],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(binop(BinOp::Mul, ident("n"), ident("n"))),
        })]),
        vec![const_ann()],
    );

    let m = module(Some("test"), vec![square_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("nosync"),
        "@const function should have nosync: {ir}"
    );
}

// T9: @pure with scalar only -> memory(none)
#[test]
fn test_pure_scalar_only_memory_none() {
    let add_func = func_with_annotations(
        "add",
        vec![
            param("a", HirType::Primitive(PrimitiveType::I32)),
            param("b", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(binop(BinOp::Add, ident("a"), ident("b"))),
        })]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![add_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("memory(none)"),
        "@pure with scalar params only should get memory(none): {ir}"
    );
    assert!(
        ir.contains("nounwind"),
        "@pure should have nounwind: {ir}"
    );
    assert!(
        !ir.contains("willreturn"),
        "@pure should NOT have willreturn: {ir}"
    );
    assert!(
        !ir.contains("nosync"),
        "@pure should NOT have nosync: {ir}"
    );
}

// T10: @pure does NOT get willreturn
#[test]
fn test_no_willreturn_on_pure() {
    // @pure function with a loop (potential non-termination)
    let loop_func = func_with_annotations(
        "loop_fn",
        vec![param("n", HirType::Primitive(PrimitiveType::I32))],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::While {
                condition: binop(BinOp::Gt, ident("n"), int_lit(0)),
                body: block(vec![stmt(HirStmtKind::Return {
                    value: Some(ident("n")),
                })]),
            }),
            stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            }),
        ]),
        vec![pure_ann()],
    );

    let m = module(Some("test"), vec![loop_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        !ir.contains("willreturn"),
        "@pure function should NOT have willreturn: {ir}"
    );
}

// T11: @const still gets willreturn
#[test]
fn test_willreturn_on_const() {
    let square_func = func_with_annotations(
        "square",
        vec![param("n", HirType::Primitive(PrimitiveType::I32))],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(binop(BinOp::Mul, ident("n"), ident("n"))),
        })]),
        vec![const_ann()],
    );

    let m = module(Some("test"), vec![square_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("willreturn"),
        "@const function should have willreturn: {ir}"
    );
}

// T12: Call-site ptr args get noalias (language rule)
#[test]
fn test_noalias_on_callsite_ptr_args() {
    let write_func = func_with_annotations(
        "write_arr",
        vec![
            param(
                "arr",
                HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Unknown("void".to_string()),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(int_lit(0)),
        })]),
        vec![],
    );

    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "data".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                mutable: false,
            }),
            stmt(HirStmtKind::Expr {
                expr: call("write_arr", vec![ident("data"), int_lit(10)]),
            }),
            stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            }),
        ]),
    );

    let m = module(Some("test"), vec![write_func, main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Check that the call site has noalias on the ptr argument.
    assert!(
        ir.contains("ptr noalias"),
        "call-site ptr args should get noalias: {ir}"
    );
}

// T-extra: @const gets all expected attributes
#[test]
fn test_const_attrs_complete() {
    let square_func = func_with_annotations(
        "square",
        vec![param("n", HirType::Primitive(PrimitiveType::I32))],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(binop(BinOp::Mul, ident("n"), ident("n"))),
        })]),
        vec![const_ann()],
    );

    let m = module(Some("test"), vec![square_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("memory(none)"),
        "@const should have memory(none): {ir}"
    );
    assert!(
        ir.contains("nounwind"),
        "@const should have nounwind: {ir}"
    );
    assert!(
        ir.contains("willreturn"),
        "@const should have willreturn: {ir}"
    );
    assert!(
        ir.contains("nosync"),
        "@const should have nosync: {ir}"
    );
    assert!(
        ir.contains("speculatable"),
        "@const should have speculatable: {ir}"
    );
}

// T-alias: Aliasing warning detection
#[test]
fn test_aliasing_warning_on_same_ptr_arg() {
    let swap_func = func_with_annotations(
        "swap",
        vec![
            param(
                "a",
                HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
            ),
            param(
                "b",
                HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
            ),
        ],
        HirType::Unknown("void".to_string()),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(int_lit(0)),
        })]),
        vec![],
    );

    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "data".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                mutable: false,
            }),
            // swap(data, data) -- same pointer as both args, should trigger warning
            stmt(HirStmtKind::Expr {
                expr: call("swap", vec![ident("data"), ident("data")]),
            }),
            stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            }),
        ]),
    );

    let m = module(Some("test"), vec![swap_func, main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // The warning should appear as an IR comment.
    assert!(
        ir.contains("warning: pointer argument 'data' passed as both param 0 and param 1 to 'swap'"),
        "should emit aliasing warning for swap(data, data): {ir}"
    );
}

// --- @parallel_for annotation tests ---

#[test]
fn test_parallel_for_loop_metadata() {
    // A for-loop with @parallel_for should get parallel loop metadata.
    let pf_config = ParallelForConfig {
        shared_read: vec!["data".to_string()],
        shared_write: vec!["results".to_string()],
        reductions: vec![],
        private: vec![],
    };
    let pf_ann = HirAnnotation {
        kind: HirAnnotationKind::ParallelFor(pf_config),
        span: SPAN_DUMMY,
    };

    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "sum".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(int_lit(0)),
                mutable: true,
            }),
            stmt_with_annotations(
                HirStmtKind::For {
                    var: "i".to_string(),
                    var_span: span(),
                    var_type: HirType::Primitive(PrimitiveType::I32),
                    iterable: call("range", vec![int_lit(0), int_lit(100)]),
                    body: block(vec![stmt(HirStmtKind::Assign {
                        target: ident("sum"),
                        value: binop(BinOp::Add, ident("sum"), int_lit(1)),
                    })]),
                },
                vec![pf_ann],
            ),
            stmt(HirStmtKind::Return {
                value: Some(ident("sum")),
            }),
        ]),
    );

    let m = module(Some("test"), vec![main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Should have parallel access metadata.
    assert!(
        ir.contains("llvm.loop.parallel_accesses"),
        "parallel_for loop should have parallel_accesses metadata: {ir}"
    );
    // Should have vectorize enable hint.
    assert!(
        ir.contains("llvm.loop.vectorize.enable"),
        "parallel_for loop should have vectorize.enable metadata: {ir}"
    );
    // Should have distribute enable hint.
    assert!(
        ir.contains("llvm.loop.distribute.enable"),
        "parallel_for loop should have distribute.enable metadata: {ir}"
    );
    // Should have !llvm.loop on backedge branch.
    assert!(
        ir.contains("!llvm.loop"),
        "parallel_for loop should have !llvm.loop on backedge: {ir}"
    );
}

#[test]
fn test_non_parallel_for_no_parallel_metadata() {
    // A regular for-loop (no @parallel_for) should NOT get parallel metadata.
    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "sum".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(int_lit(0)),
                mutable: true,
            }),
            stmt(HirStmtKind::For {
                var: "i".to_string(),
                var_span: span(),
                var_type: HirType::Primitive(PrimitiveType::I32),
                iterable: call("range", vec![int_lit(0), int_lit(100)]),
                body: block(vec![stmt(HirStmtKind::Assign {
                    target: ident("sum"),
                    value: binop(BinOp::Add, ident("sum"), int_lit(1)),
                })]),
            }),
            stmt(HirStmtKind::Return {
                value: Some(ident("sum")),
            }),
        ]),
    );

    let m = module(Some("test"), vec![main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Should NOT have parallel access metadata.
    assert!(
        !ir.contains("llvm.loop.parallel_accesses"),
        "regular loop should not have parallel_accesses metadata: {ir}"
    );
}

// ── MT-4: Ownership slices tests ──────────────────────────────────

#[test]
fn test_readonly_ptr_type() {
    // fn read_only(data: readonly_ptr[f64], n: i32) -> f64 {
    //     return ptr_read_f64(data, 0);
    // }
    let read_only_func = func(
        "read_only",
        vec![
            param(
                "data",
                HirType::ReadonlyPtr {
                    element: Box::new(HirType::Primitive(PrimitiveType::F64)),
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::F64),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(call("ptr_read_f64", vec![ident("data"), int_lit(0)])),
        })]),
    );

    let m = module(Some("test"), vec![read_only_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // readonly_ptr should emit `ptr noalias readonly`.
    assert!(
        ir.contains("ptr noalias readonly %data"),
        "readonly_ptr should emit 'ptr noalias readonly': {ir}"
    );
    // Should NOT have just `ptr noalias %data` (without readonly).
    assert!(
        !ir.contains("ptr noalias %data,") || ir.contains("ptr noalias readonly %data"),
        "should use readonly attribute: {ir}"
    );
}

#[test]
fn test_writeonly_ptr_type() {
    // fn write_only(data: writeonly_ptr[f64], n: i32) {
    //     ptr_write_f64(data, 0, 3.14);
    // }
    let write_only_func = func(
        "write_only",
        vec![
            param(
                "data",
                HirType::WriteonlyPtr {
                    element: Box::new(HirType::Primitive(PrimitiveType::F64)),
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Unknown("void".to_string()),
        block(vec![stmt(HirStmtKind::Expr {
            expr: call(
                "ptr_write_f64",
                vec![ident("data"), int_lit(0), float_lit(3.14)],
            ),
        })]),
    );

    let m = module(Some("test"), vec![write_only_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // writeonly_ptr should emit `ptr noalias writeonly`.
    assert!(
        ir.contains("ptr noalias writeonly %data"),
        "writeonly_ptr should emit 'ptr noalias writeonly': {ir}"
    );
}

#[test]
fn test_readonly_ptr_write_error() {
    // fn bad(data: readonly_ptr[i32]) {
    //     ptr_write_i32(data, 0, 42);  // ERROR: cannot write to readonly_ptr
    // }
    let bad_func = func(
        "bad",
        vec![param(
            "data",
            HirType::ReadonlyPtr {
                element: Box::new(HirType::Primitive(PrimitiveType::I32)),
            },
        )],
        HirType::Unknown("void".to_string()),
        block(vec![stmt(HirStmtKind::Expr {
            expr: call(
                "ptr_write_i32",
                vec![ident("data"), int_lit(0), int_lit(42)],
            ),
        })]),
    );

    let m = module(Some("test"), vec![bad_func]);
    let result = codegen(&m);
    assert!(
        result.is_err(),
        "writing to readonly_ptr should be a compile error"
    );
    let errs = result.unwrap_err();
    let msg = format!("{:?}", errs);
    assert!(
        msg.contains("readonly_ptr"),
        "error should mention readonly_ptr: {msg}"
    );
}

#[test]
fn test_writeonly_ptr_read_error() {
    // fn bad(data: writeonly_ptr[i32]) -> i32 {
    //     return ptr_read_i32(data, 0);  // ERROR: cannot read from writeonly_ptr
    // }
    let bad_func = func(
        "bad",
        vec![param(
            "data",
            HirType::WriteonlyPtr {
                element: Box::new(HirType::Primitive(PrimitiveType::I32)),
            },
        )],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![stmt(HirStmtKind::Return {
            value: Some(call("ptr_read_i32", vec![ident("data"), int_lit(0)])),
        })]),
    );

    let m = module(Some("test"), vec![bad_func]);
    let result = codegen(&m);
    assert!(
        result.is_err(),
        "reading from writeonly_ptr should be a compile error"
    );
    let errs = result.unwrap_err();
    let msg = format!("{:?}", errs);
    assert!(
        msg.contains("writeonly_ptr"),
        "error should mention writeonly_ptr: {msg}"
    );
}

// ── MT-5: Job dependency graph tests ──────────────────────────────

#[test]
fn test_job_handle() {
    // let func_ptr: ptr[i32] = heap_alloc(1, 4);
    // let data: ptr[i32] = heap_alloc(10, 4);
    // let h: i32 = job_dispatch_handle(func_ptr, data, 10);
    // job_wait_handle(h);
    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "func_ptr".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("heap_alloc", vec![int_lit(1), int_lit(4)])),
                mutable: false,
            }),
            stmt(HirStmtKind::Let {
                name: "data".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                mutable: false,
            }),
            stmt(HirStmtKind::Let {
                name: "h".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(call(
                    "job_dispatch_handle",
                    vec![ident("func_ptr"), ident("data"), int_lit(10)],
                )),
                mutable: false,
            }),
            stmt(HirStmtKind::Expr {
                expr: call("job_wait_handle", vec![ident("h")]),
            }),
            stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            }),
        ]),
    );

    let m = module(Some("test"), vec![main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("call i32 @axiom_job_dispatch_handle("),
        "should emit call to axiom_job_dispatch_handle: {ir}"
    );
    assert!(
        ir.contains("call void @axiom_job_wait_handle("),
        "should emit call to axiom_job_wait_handle: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_job_dispatch_handle(ptr, ptr, i32)"),
        "should declare axiom_job_dispatch_handle: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_job_wait_handle(i32)"),
        "should declare axiom_job_wait_handle: {ir}"
    );
    // Fence release before dispatch, fence acquire after wait.
    let dispatch_pos = ir.find("call i32 @axiom_job_dispatch_handle(");
    let fence_rel_pos = ir.find("fence release");
    assert!(
        fence_rel_pos.is_some() && dispatch_pos.is_some(),
        "should have fence release before dispatch_handle: {ir}"
    );
    assert!(
        fence_rel_pos.unwrap() < dispatch_pos.unwrap(),
        "fence release should come before dispatch_handle: {ir}"
    );
}

#[test]
fn test_job_dependency() {
    // let func_ptr: ptr[i32] = heap_alloc(1, 4);
    // let data: ptr[i32] = heap_alloc(10, 4);
    // let h1: i32 = job_dispatch_handle(func_ptr, data, 10);
    // let h2: i32 = job_dispatch_after(func_ptr, data, 10, h1);
    // job_wait_handle(h2);
    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "func_ptr".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("heap_alloc", vec![int_lit(1), int_lit(4)])),
                mutable: false,
            }),
            stmt(HirStmtKind::Let {
                name: "data".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                mutable: false,
            }),
            stmt(HirStmtKind::Let {
                name: "h1".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(call(
                    "job_dispatch_handle",
                    vec![ident("func_ptr"), ident("data"), int_lit(10)],
                )),
                mutable: false,
            }),
            stmt(HirStmtKind::Let {
                name: "h2".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(call(
                    "job_dispatch_after",
                    vec![ident("func_ptr"), ident("data"), int_lit(10), ident("h1")],
                )),
                mutable: false,
            }),
            stmt(HirStmtKind::Expr {
                expr: call("job_wait_handle", vec![ident("h2")]),
            }),
            stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            }),
        ]),
    );

    let m = module(Some("test"), vec![main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("call i32 @axiom_job_dispatch_handle("),
        "should emit call to axiom_job_dispatch_handle: {ir}"
    );
    assert!(
        ir.contains("call i32 @axiom_job_dispatch_after("),
        "should emit call to axiom_job_dispatch_after: {ir}"
    );
    assert!(
        ir.contains("call void @axiom_job_wait_handle("),
        "should emit call to axiom_job_wait_handle: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_job_dispatch_after(ptr, ptr, i32, i32)"),
        "should declare axiom_job_dispatch_after: {ir}"
    );
}

// -----------------------------------------------------------------------
// F1: Option (sum type) builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_option_builtins() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let none_val: i64 = option_none();
                stmt(HirStmtKind::Let {
                    name: "none_val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("option_none", vec![])),
                    mutable: false,
                }),
                // let some_val: i64 = option_some(42);
                stmt(HirStmtKind::Let {
                    name: "some_val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("option_some", vec![int_lit(42)])),
                    mutable: false,
                }),
                // let is_some: i32 = option_is_some(some_val);
                stmt(HirStmtKind::Let {
                    name: "is_some".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("option_is_some", vec![ident("some_val")])),
                    mutable: false,
                }),
                // let is_none: i32 = option_is_none(none_val);
                stmt(HirStmtKind::Let {
                    name: "is_none".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("option_is_none", vec![ident("none_val")])),
                    mutable: false,
                }),
                // let val: i32 = option_unwrap(some_val);
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("option_unwrap", vec![ident("some_val")])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // option_some: should pack tag=1 with value using or + and
    assert!(
        ir.contains("zext i32"),
        "option_some should zero-extend i32 to i64: {ir}"
    );
    assert!(
        ir.contains("or i64"),
        "option_some should use or to pack tag: {ir}"
    );
    assert!(
        ir.contains("and i64"),
        "option_some should mask value: {ir}"
    );

    // option_is_some: should use lshr + icmp ne
    assert!(
        ir.contains("lshr i64"),
        "option_is_some should shift right to get tag: {ir}"
    );
    assert!(
        ir.contains("icmp ne i32"),
        "option_is_some should compare tag != 0: {ir}"
    );

    // option_is_none: should use icmp eq
    assert!(
        ir.contains("icmp eq i32"),
        "option_is_none should compare tag == 0: {ir}"
    );

    // option_unwrap: should truncate i64 to i32
    assert!(
        ir.contains("trunc i64"),
        "option_unwrap should truncate to i32: {ir}"
    );
}

// -----------------------------------------------------------------------
// F2: String builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_string_builtins() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let s: i64 = string_from_literal("hello");
                stmt(HirStmtKind::Let {
                    name: "s".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("string_from_literal", vec![str_lit("hello")])),
                    mutable: false,
                }),
                // let len: i32 = string_len(s);
                stmt(HirStmtKind::Let {
                    name: "len".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("string_len", vec![ident("s")])),
                    mutable: false,
                }),
                // let p: ptr = string_ptr(s);
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("string_ptr", vec![ident("s")])),
                    mutable: false,
                }),
                // let s2: i64 = string_from_literal("hello");
                stmt(HirStmtKind::Let {
                    name: "s2".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("string_from_literal", vec![str_lit("hello")])),
                    mutable: false,
                }),
                // let eq: i32 = string_eq(s, s2);
                stmt(HirStmtKind::Let {
                    name: "eq".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("string_eq", vec![ident("s"), ident("s2")])),
                    mutable: false,
                }),
                // string_print(s);
                stmt(HirStmtKind::Expr {
                    expr: call("string_print", vec![ident("s")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // Should call runtime functions
    assert!(
        ir.contains("call i64 @axiom_string_from_literal(ptr"),
        "should call axiom_string_from_literal: {ir}"
    );
    assert!(
        ir.contains("call i32 @axiom_string_len(i64"),
        "should call axiom_string_len: {ir}"
    );
    assert!(
        ir.contains("call ptr @axiom_string_ptr(i64"),
        "should call axiom_string_ptr: {ir}"
    );
    assert!(
        ir.contains("call i32 @axiom_string_eq(i64"),
        "should call axiom_string_eq: {ir}"
    );
    assert!(
        ir.contains("call void @axiom_string_print(i64"),
        "should call axiom_string_print: {ir}"
    );

    // Should declare extern functions
    assert!(
        ir.contains("declare i64 @axiom_string_from_literal(ptr)"),
        "should declare axiom_string_from_literal: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_string_len(i64)"),
        "should declare axiom_string_len: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_string_print(i64)"),
        "should declare axiom_string_print: {ir}"
    );
}

// -----------------------------------------------------------------------
// F3: Vec (dynamic array) builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_vec_builtins() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let v: ptr = vec_new(4);
                stmt(HirStmtKind::Let {
                    name: "v".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("vec_new", vec![int_lit(4)])),
                    mutable: false,
                }),
                // vec_push_i32(v, 10);
                stmt(HirStmtKind::Expr {
                    expr: call("vec_push_i32", vec![ident("v"), int_lit(10)]),
                }),
                // vec_push_i32(v, 20);
                stmt(HirStmtKind::Expr {
                    expr: call("vec_push_i32", vec![ident("v"), int_lit(20)]),
                }),
                // let val: i32 = vec_get_i32(v, 0);
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("vec_get_i32", vec![ident("v"), int_lit(0)])),
                    mutable: false,
                }),
                // vec_set_i32(v, 0, 99);
                stmt(HirStmtKind::Expr {
                    expr: call("vec_set_i32", vec![ident("v"), int_lit(0), int_lit(99)]),
                }),
                // let len: i32 = vec_len(v);
                stmt(HirStmtKind::Let {
                    name: "len".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("vec_len", vec![ident("v")])),
                    mutable: false,
                }),
                // vec_free(v);
                stmt(HirStmtKind::Expr {
                    expr: call("vec_free", vec![ident("v")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // Should call runtime functions
    assert!(
        ir.contains("call ptr @axiom_vec_new(i32"),
        "should call axiom_vec_new: {ir}"
    );
    assert!(
        ir.contains("call void @axiom_vec_push_i32(ptr"),
        "should call axiom_vec_push_i32: {ir}"
    );
    assert!(
        ir.contains("call i32 @axiom_vec_get_i32(ptr"),
        "should call axiom_vec_get_i32: {ir}"
    );
    assert!(
        ir.contains("call void @axiom_vec_set_i32(ptr"),
        "should call axiom_vec_set_i32: {ir}"
    );
    assert!(
        ir.contains("call i32 @axiom_vec_len(ptr"),
        "should call axiom_vec_len: {ir}"
    );
    assert!(
        ir.contains("call void @axiom_vec_free(ptr"),
        "should call axiom_vec_free: {ir}"
    );

    // Should declare extern functions
    assert!(
        ir.contains("declare ptr @axiom_vec_new(i32)"),
        "should declare axiom_vec_new: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_vec_push_i32(ptr, i32)"),
        "should declare axiom_vec_push_i32: {ir}"
    );
    assert!(
        ir.contains("declare i32 @axiom_vec_get_i32(ptr, i32)"),
        "should declare axiom_vec_get_i32: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_vec_free(ptr)"),
        "should declare axiom_vec_free: {ir}"
    );
}

#[test]
fn test_vec_f64_builtins() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let v: ptr = vec_new(8);
                stmt(HirStmtKind::Let {
                    name: "v".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::F64)),
                    },
                    value: Some(call("vec_new", vec![int_lit(8)])),
                    mutable: false,
                }),
                // vec_push_f64(v, 3.14);
                stmt(HirStmtKind::Expr {
                    expr: call("vec_push_f64", vec![ident("v"), float_lit(3.14)]),
                }),
                // let val: f64 = vec_get_f64(v, 0);
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("vec_get_f64", vec![ident("v"), int_lit(0)])),
                    mutable: false,
                }),
                // vec_set_f64(v, 0, 2.71);
                stmt(HirStmtKind::Expr {
                    expr: call(
                        "vec_set_f64",
                        vec![ident("v"), int_lit(0), float_lit(2.71)],
                    ),
                }),
                // vec_free(v);
                stmt(HirStmtKind::Expr {
                    expr: call("vec_free", vec![ident("v")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    assert!(
        ir.contains("call void @axiom_vec_push_f64(ptr"),
        "should call axiom_vec_push_f64: {ir}"
    );
    assert!(
        ir.contains("call double @axiom_vec_get_f64(ptr"),
        "should call axiom_vec_get_f64: {ir}"
    );
    assert!(
        ir.contains("call void @axiom_vec_set_f64(ptr"),
        "should call axiom_vec_set_f64: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_vec_push_f64(ptr, double)"),
        "should declare axiom_vec_push_f64: {ir}"
    );
    assert!(
        ir.contains("declare double @axiom_vec_get_f64(ptr, i32)"),
        "should declare axiom_vec_get_f64: {ir}"
    );
    assert!(
        ir.contains("declare void @axiom_vec_set_f64(ptr, i32, double)"),
        "should declare axiom_vec_set_f64: {ir}"
    );
}

// -----------------------------------------------------------------------
// F5: Function pointer builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_fn_ptr_builtins() {
    // Define a helper function first, then use fn_ptr / call_fn_ptr_i32
    let helper = HirFunction {
        id: nid(0),
        name: "double_it".to_string(),
        name_span: span(),
        annotations: vec![],
        params: vec![param("x", HirType::Primitive(PrimitiveType::I32))],
        return_type: HirType::Primitive(PrimitiveType::I32),
        body: block(vec![stmt(HirStmtKind::Return {
            value: Some(binop(BinOp::Mul, ident("x"), int_lit(2))),
        })]),
        span: span(),
    };

    let main_func = func(
        "main",
        vec![],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            // let fptr: ptr = fn_ptr(double_it);
            stmt(HirStmtKind::Let {
                name: "fptr".to_string(),
                name_span: span(),
                ty: HirType::Ptr {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                },
                value: Some(call("fn_ptr", vec![ident("double_it")])),
                mutable: false,
            }),
            // let result: i32 = call_fn_ptr_i32(fptr, 21);
            stmt(HirStmtKind::Let {
                name: "result".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(call(
                    "call_fn_ptr_i32",
                    vec![ident("fptr"), int_lit(21)],
                )),
                mutable: false,
            }),
            stmt(HirStmtKind::Return {
                value: Some(ident("result")),
            }),
        ]),
    );

    let m = module(Some("test"), vec![helper, main_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // fn_ptr should produce @double_it reference
    assert!(
        ir.contains("@double_it"),
        "fn_ptr should reference @double_it: {ir}"
    );

    // call_fn_ptr_i32 should do an indirect call
    assert!(
        ir.contains("call i32 "),
        "call_fn_ptr_i32 should emit indirect call: {ir}"
    );
}

// -----------------------------------------------------------------------
// F7: Result (error handling) builtin tests
// -----------------------------------------------------------------------

#[test]
fn test_result_builtins() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                // let ok_val: i64 = result_ok(42);
                stmt(HirStmtKind::Let {
                    name: "ok_val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("result_ok", vec![int_lit(42)])),
                    mutable: false,
                }),
                // let err_val: i64 = result_err(1);
                stmt(HirStmtKind::Let {
                    name: "err_val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I64),
                    value: Some(call("result_err", vec![int_lit(1)])),
                    mutable: false,
                }),
                // let is_ok: i32 = result_is_ok(ok_val);
                stmt(HirStmtKind::Let {
                    name: "is_ok".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("result_is_ok", vec![ident("ok_val")])),
                    mutable: false,
                }),
                // let is_err: i32 = result_is_err(err_val);
                stmt(HirStmtKind::Let {
                    name: "is_err".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("result_is_err", vec![ident("err_val")])),
                    mutable: false,
                }),
                // let unwrapped: i32 = result_unwrap(ok_val);
                stmt(HirStmtKind::Let {
                    name: "unwrapped".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("result_unwrap", vec![ident("ok_val")])),
                    mutable: false,
                }),
                // let err_code: i32 = result_err_code(err_val);
                stmt(HirStmtKind::Let {
                    name: "err_code".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("result_err_code", vec![ident("err_val")])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // result_ok: should pack with tag=1
    assert!(
        ir.contains("or i64") && ir.contains("4294967296"),
        "result_ok should pack tag 1 << 32: {ir}"
    );

    // result_err: should zero-extend (tag=0, just the code)
    assert!(
        ir.contains("zext i32"),
        "result_err should zero-extend code to i64: {ir}"
    );

    // result_is_ok: should check tag == 1
    assert!(
        ir.contains("icmp eq i32") && ir.contains(", 1"),
        "result_is_ok should compare tag to 1: {ir}"
    );

    // result_is_err: should check tag == 0
    assert!(
        ir.contains("icmp eq i32") && ir.contains(", 0"),
        "result_is_err should compare tag to 0: {ir}"
    );

    // result_unwrap / result_err_code: should trunc i64 to i32
    // Count trunc instructions (at least 4: is_ok, is_err, unwrap, err_code)
    let trunc_count = ir.matches("trunc i64").count();
    assert!(
        trunc_count >= 4,
        "should have at least 4 trunc i64 instructions for tag extraction and value extraction, got {trunc_count}: {ir}"
    );
}

// -----------------------------------------------------------------------
// P2: CPUID feature detection test
// -----------------------------------------------------------------------

#[test]
fn test_cpu_features_builtin() {
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "features".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("cpu_features", vec![])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("features")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // Should call axiom_cpu_features
    assert!(
        ir.contains("call i32 @axiom_cpu_features()"),
        "should call axiom_cpu_features: {ir}"
    );

    // Should declare the runtime function
    assert!(
        ir.contains("declare i32 @axiom_cpu_features()"),
        "should declare axiom_cpu_features: {ir}"
    );

    // Should need runtime
    assert!(
        needs_runtime(&ir),
        "cpu_features should trigger runtime linking"
    );
}

// -----------------------------------------------------------------------
// P3: vectorize.width metadata test
// -----------------------------------------------------------------------

#[test]
fn test_vectorize_width_metadata() {
    // @vectorizable function with a for loop should get vectorize.width metadata.
    let sum_func = func_with_annotations(
        "vec_sum_width",
        vec![
            param(
                "arr",
                HirType::Array {
                    element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    size: 100,
                },
            ),
            param("n", HirType::Primitive(PrimitiveType::I32)),
        ],
        HirType::Primitive(PrimitiveType::I32),
        block(vec![
            stmt(HirStmtKind::Let {
                name: "sum".to_string(),
                name_span: span(),
                ty: HirType::Primitive(PrimitiveType::I32),
                value: Some(int_lit(0)),
                mutable: true,
            }),
            stmt(HirStmtKind::For {
                var: "i".to_string(),
                var_span: span(),
                var_type: HirType::Primitive(PrimitiveType::I32),
                iterable: call("range", vec![int_lit(0), ident("n")]),
                body: block(vec![stmt(HirStmtKind::Assign {
                    target: ident("sum"),
                    value: binop(BinOp::Add, ident("sum"), int_lit(1)),
                })]),
            }),
            stmt(HirStmtKind::Return {
                value: Some(ident("sum")),
            }),
        ]),
        vec![vectorizable_ann()],
    );

    let m = module(Some("test"), vec![sum_func]);
    let ir = codegen(&m).expect("codegen should succeed");

    // Should have vectorize.width metadata with width 8
    assert!(
        ir.contains("llvm.loop.vectorize.width"),
        "should have vectorize.width metadata: {ir}"
    );
    assert!(
        ir.contains("!\"llvm.loop.vectorize.width\", i32 8"),
        "should have vectorize.width = 8: {ir}"
    );
}

// -----------------------------------------------------------------------
// E2: DWARF debug info test
// -----------------------------------------------------------------------

#[test]
fn test_dwarf_debug_info() {
    let m = module(
        Some("myprogram"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![stmt(HirStmtKind::Return {
                value: Some(int_lit(0)),
            })]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");

    // Should have debug compile unit
    assert!(
        ir.contains("!llvm.dbg.cu"),
        "should have !llvm.dbg.cu metadata: {ir}"
    );

    // Should have DICompileUnit
    assert!(
        ir.contains("DICompileUnit"),
        "should have DICompileUnit: {ir}"
    );

    // Should have DIFile with the module name
    assert!(
        ir.contains("DIFile(filename: \"myprogram.axm\""),
        "should have DIFile with module name: {ir}"
    );

    // Should have Debug Info Version flag
    assert!(
        ir.contains("Debug Info Version"),
        "should have Debug Info Version module flag: {ir}"
    );

    // Should have producer = axiom
    assert!(
        ir.contains("producer: \"axiom\""),
        "should have producer axiom: {ir}"
    );
}

// ── vec2/vec3/vec4 SIMD vector type tests ────────────────────────────

#[test]
fn test_vec3_constructor_and_field_access() {
    let source = r#"
@module vec_test;
fn main() -> i32 {
    let v: vec3 = vec3(1.0, 2.0, 3.0);
    let x: f64 = v.x;
    let y: f64 = v.y;
    let z: f64 = v.z;
    print_f64(x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("<4 x double>"), "should use <4 x double> for vec3: {ir}");
    assert!(ir.contains("insertelement"), "should emit insertelement for constructor: {ir}");
    assert!(ir.contains("extractelement"), "should emit extractelement for field access: {ir}");
}

#[test]
fn test_vec3_arithmetic() {
    let source = r#"
@module vec_arith;
@pure
fn add_vecs(a: vec3, b: vec3) -> vec3 {
    return a + b;
}
fn main() -> i32 {
    let a: vec3 = vec3(1.0, 2.0, 3.0);
    let b: vec3 = vec3(4.0, 5.0, 6.0);
    let c: vec3 = add_vecs(a, b);
    print_f64(c.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("fadd fast <4 x double>"), "should emit fadd fast for @pure vec3 add: {ir}");
    assert!(ir.contains("ret <4 x double>"), "should return <4 x double>: {ir}");
}

#[test]
fn test_vec3_scalar_multiply() {
    let source = r#"
@module vec_scale;
fn main() -> i32 {
    let v: vec3 = vec3(1.0, 2.0, 3.0);
    let s: vec3 = v * 2.0;
    print_f64(s.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("shufflevector"), "should broadcast scalar: {ir}");
    assert!(ir.contains("zeroinitializer"), "should use zeroinitializer for broadcast: {ir}");
    assert!(ir.contains("fmul <4 x double>"), "should emit fmul for vec3 * scalar: {ir}");
}

#[test]
fn test_vec3_dot_product() {
    let source = r#"
@module vec_dot;
fn main() -> i32 {
    let a: vec3 = vec3(1.0, 2.0, 3.0);
    let b: vec3 = vec3(4.0, 5.0, 6.0);
    let d: f64 = dot(a, b);
    print_f64(d);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("fmul <4 x double>"), "should emit fmul for dot: {ir}");
    assert!(ir.contains("extractelement"), "should extract lanes for horizontal sum: {ir}");
}

#[test]
fn test_vec3_cross_product() {
    let source = r#"
@module vec_cross;
fn main() -> i32 {
    let a: vec3 = vec3(1.0, 0.0, 0.0);
    let b: vec3 = vec3(0.0, 1.0, 0.0);
    let c: vec3 = cross(a, b);
    print_f64(c.z);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("shufflevector"), "should emit shufflevector for cross: {ir}");
    assert!(ir.contains("fsub <4 x double>"), "should emit fsub in cross: {ir}");
}

#[test]
fn test_vec3_length_and_normalize() {
    let source = r#"
@module vec_len;
fn main() -> i32 {
    let v: vec3 = vec3(3.0, 4.0, 0.0);
    let len: f64 = length(v);
    let n: vec3 = normalize(v);
    print_f64(len);
    print_f64(n.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("@llvm.sqrt.f64"), "should use sqrt for length/normalize: {ir}");
    assert!(ir.contains("fdiv"), "should use fdiv for 1/length in normalize: {ir}");
}

#[test]
fn test_vec3_reflect() {
    let source = r#"
@module vec_reflect;
fn main() -> i32 {
    let i: vec3 = vec3(1.0, -1.0, 0.0);
    let n: vec3 = vec3(0.0, 1.0, 0.0);
    let r: vec3 = reflect(i, n);
    print_f64(r.y);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("fsub <4 x double>"), "should emit fsub for reflect: {ir}");
    assert!(ir.contains("shufflevector"), "should broadcast dot in reflect: {ir}");
}

#[test]
fn test_vec3_lerp() {
    let source = r#"
@module vec_lerp;
fn main() -> i32 {
    let a: vec3 = vec3(0.0, 0.0, 0.0);
    let b: vec3 = vec3(10.0, 20.0, 30.0);
    let m: vec3 = lerp(a, b, 0.5);
    print_f64(m.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("fsub <4 x double>"), "should emit fsub for b-a in lerp: {ir}");
    assert!(ir.contains("fadd <4 x double>"), "should emit fadd for a + t*(b-a) in lerp: {ir}");
}

#[test]
fn test_vec2_operations() {
    let source = r#"
@module vec2_test;
fn main() -> i32 {
    let a: vec2 = vec2(3.0, 4.0);
    let b: vec2 = vec2(1.0, 2.0);
    let c: vec2 = a + b;
    let d: f64 = dot(a, b);
    print_f64(c.x);
    print_f64(d);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("<2 x double>"), "should use <2 x double> for vec2: {ir}");
    assert!(ir.contains("fadd <2 x double>"), "should emit fadd <2 x double>: {ir}");
}

#[test]
fn test_vec4_operations() {
    let source = r#"
@module vec4_test;
fn main() -> i32 {
    let a: vec4 = vec4(1.0, 2.0, 3.0, 4.0);
    let b: vec4 = vec4(5.0, 6.0, 7.0, 8.0);
    let c: vec4 = a + b;
    let w: f64 = c.w;
    print_f64(w);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("<4 x double>"), "should use <4 x double> for vec4: {ir}");
    assert!(ir.contains("fadd <4 x double>"), "should emit fadd for vec4: {ir}");
}

#[test]
fn test_vec3_field_assignment() {
    let source = r#"
@module vec_assign;
fn main() -> i32 {
    let v: vec3 = vec3(0.0, 0.0, 0.0);
    v.x = 5.0;
    v.y = 10.0;
    print_f64(v.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("insertelement <4 x double>"), "should emit insertelement for field assign: {ir}");
    assert!(ir.contains("store <4 x double>"), "should store updated vector: {ir}");
}

#[test]
fn test_vec3_pass_and_return_by_value() {
    let source = r#"
@module vec_passret;
@pure
fn scale(v: vec3, s: f64) -> vec3 {
    return v * s;
}
fn main() -> i32 {
    let v: vec3 = vec3(1.0, 2.0, 3.0);
    let r: vec3 = scale(v, 3.0);
    print_f64(r.z);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    // Function should take <4 x double> param and return <4 x double>
    assert!(ir.contains("@scale(<4 x double>"), "should pass vec3 by value: {ir}");
    assert!(ir.contains("ret <4 x double>"), "should return vec3 by value: {ir}");
}

#[test]
fn test_swizzle_reorder() {
    let source = r#"
@module swiz;
fn main() -> i32 {
    let v: vec3 = vec3(1.0, 2.0, 3.0);
    let r: vec3 = v.zyx;
    print_f64(r.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("shufflevector"), "should emit shufflevector for swizzle: {ir}");
}

#[test]
fn test_swizzle_broadcast() {
    let source = r#"
@module swiz_bcast;
fn main() -> i32 {
    let v: vec3 = vec3(1.0, 2.0, 3.0);
    let b: vec3 = v.xxx;
    print_f64(b.y);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("shufflevector"), "should emit shufflevector: {ir}");
    assert!(ir.contains("i32 0, i32 0, i32 0"), "should broadcast x: {ir}");
}

#[test]
fn test_swizzle_narrow_to_vec2() {
    let source = r#"
@module swiz_narrow;
fn main() -> i32 {
    let v: vec3 = vec3(1.0, 2.0, 3.0);
    let xy: vec2 = v.xy;
    print_f64(xy.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("shufflevector"), "should emit shufflevector: {ir}");
    assert!(ir.contains("<2 x i32>"), "should produce vec2 mask: {ir}");
}

#[test]
fn test_swizzle_rgba_notation() {
    let source = r#"
@module swiz_rgba;
fn main() -> i32 {
    let color: vec3 = vec3(1.0, 0.5, 0.0);
    let rg: vec2 = color.rg;
    print_f64(rg.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(parse_result.errors.is_empty(), "parse errors: {:?}", parse_result.errors);
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(ir.contains("shufflevector"), "should emit shufflevector for rgba swizzle: {ir}");
}

// ======================================================================
// BSP binary format builtins: ptr_read_f32, ptr_write_f32, ptr_read_i16, ptr_read_u8
// ======================================================================

#[test]
fn test_ptr_read_write_f32() {
    // ptr_write_f32(p, 0, 3.14) should emit fptrunc + GEP float + store float
    // ptr_read_f32(p, 0) should emit GEP float + load float + fpext to double
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::F64)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(4)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call(
                        "ptr_write_f32",
                        vec![ident("p"), int_lit(0), float_lit(3.14)],
                    ),
                }),
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::F64),
                    value: Some(call("ptr_read_f32", vec![ident("p"), int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(int_lit(0)),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    // ptr_write_f32: fptrunc + GEP float + store float
    assert!(
        ir.contains("fptrunc double"),
        "ptr_write_f32 should emit fptrunc: {ir}"
    );
    assert!(
        ir.contains("getelementptr float, ptr"),
        "ptr_write_f32 should emit GEP float: {ir}"
    );
    assert!(
        ir.contains("store float"),
        "ptr_write_f32 should emit store float: {ir}"
    );
    // ptr_read_f32: load float + fpext
    assert!(
        ir.contains("load float, ptr"),
        "ptr_read_f32 should emit load float: {ir}"
    );
    assert!(
        ir.contains("fpext float"),
        "ptr_read_f32 should emit fpext float to double: {ir}"
    );
}

#[test]
fn test_ptr_read_i16() {
    // ptr_read_i16(p, 0) should emit GEP i16 + load i16 + sext i16 to i32
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(2)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("ptr_read_i16", vec![ident("p"), int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("val")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("getelementptr i16, ptr"),
        "ptr_read_i16 should emit GEP i16: {ir}"
    );
    assert!(
        ir.contains("load i16, ptr"),
        "ptr_read_i16 should emit load i16: {ir}"
    );
    assert!(
        ir.contains("sext i16"),
        "ptr_read_i16 should emit sext i16 to i32: {ir}"
    );
}

#[test]
fn test_ptr_read_u8() {
    // ptr_read_u8(p, 0) should emit GEP i8 + load i8 + zext i8 to i32
    let m = module(
        Some("test"),
        vec![func(
            "main",
            vec![],
            HirType::Primitive(PrimitiveType::I32),
            block(vec![
                stmt(HirStmtKind::Let {
                    name: "p".to_string(),
                    name_span: span(),
                    ty: HirType::Ptr {
                        element: Box::new(HirType::Primitive(PrimitiveType::I32)),
                    },
                    value: Some(call("heap_alloc", vec![int_lit(10), int_lit(1)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Let {
                    name: "val".to_string(),
                    name_span: span(),
                    ty: HirType::Primitive(PrimitiveType::I32),
                    value: Some(call("ptr_read_u8", vec![ident("p"), int_lit(0)])),
                    mutable: false,
                }),
                stmt(HirStmtKind::Expr {
                    expr: call("heap_free", vec![ident("p")]),
                }),
                stmt(HirStmtKind::Return {
                    value: Some(ident("val")),
                }),
            ]),
        )],
    );

    let ir = codegen(&m).expect("codegen should succeed");
    assert!(
        ir.contains("getelementptr i8, ptr"),
        "ptr_read_u8 should emit GEP i8: {ir}"
    );
    assert!(
        ir.contains("load i8, ptr"),
        "ptr_read_u8 should emit load i8: {ir}"
    );
    assert!(
        ir.contains("zext i8"),
        "ptr_read_u8 should emit zext i8 to i32: {ir}"
    );
}

#[test]
fn test_struct_literal_basic() {
    let source = r#"
@module struct_lit;
struct Point { x: f64, y: f64, z: f64 }
fn main() -> i32 {
    let p: Point = Point { x: 1.0, y: 2.0, z: 3.0 };
    print_f64(p.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(
        parse_result.errors.is_empty(),
        "parse errors: {:?}",
        parse_result.errors
    );
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(
        ir.contains("%struct.Point"),
        "should define struct: {ir}"
    );
    assert!(
        ir.contains("getelementptr inbounds %struct.Point"),
        "should GEP for fields: {ir}"
    );
    // Check that field values are stored
    assert!(
        ir.contains("store double 1.0"),
        "should store field x: {ir}"
    );
    assert!(
        ir.contains("store double 2.0"),
        "should store field y: {ir}"
    );
    assert!(
        ir.contains("store double 3.0"),
        "should store field z: {ir}"
    );
    // Should have memcpy from struct literal alloca to variable alloca
    assert!(
        ir.contains("@llvm.memcpy.p0.p0.i64"),
        "should memcpy struct literal to variable: {ir}"
    );
}

#[test]
fn test_struct_literal_with_vec3_fields() {
    let source = r#"
@module struct_vec;
struct Particle { pos: vec3, vel: vec3, mass: f64 }
fn main() -> i32 {
    let p: Particle = Particle { pos: vec3(1.0, 2.0, 3.0), vel: vec3(0.0, 0.0, 0.0), mass: 1.5 };
    print_f64(p.mass);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(
        parse_result.errors.is_empty(),
        "parse errors: {:?}",
        parse_result.errors
    );
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(
        ir.contains("<4 x double>"),
        "should have vec3 type in struct: {ir}"
    );
    assert!(
        ir.contains("%struct.Particle"),
        "should define struct: {ir}"
    );
    assert!(
        ir.contains("getelementptr inbounds %struct.Particle"),
        "should GEP: {ir}"
    );
}

#[test]
fn test_struct_literal_nested_field_access() {
    // Test s.center.x access pattern (FieldAccess on FieldAccess)
    let source = r#"
@module nested_field;
struct Sphere { center: vec3, radius: f64 }
fn main() -> i32 {
    let s: Sphere = Sphere { center: vec3(1.0, 2.0, 3.0), radius: 1.5 };
    print_f64(s.radius);
    print_f64(s.center.x);
    return 0;
}
"#;
    let parse_result = axiom_parser::parse(source);
    assert!(
        parse_result.errors.is_empty(),
        "parse errors: {:?}",
        parse_result.errors
    );
    let hir = axiom_hir::lower(&parse_result.module).expect("lowering should succeed");
    let ir = codegen(&hir).expect("codegen should succeed");
    assert!(
        ir.contains("%struct.Sphere"),
        "should define Sphere: {ir}"
    );
    // Should have extractelement for s.center.x
    assert!(
        ir.contains("extractelement"),
        "should extractelement for nested vec3 field access: {ir}"
    );
}
