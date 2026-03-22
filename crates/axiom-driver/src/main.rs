//! AXIOM compiler CLI driver.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "axiom", about = "AXIOM — AI eXchange Intermediate Optimization Medium")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile an AXIOM source file
    Compile {
        /// Input .axm file
        input: String,

        /// Output file path
        #[arg(short, long)]
        output: Option<String>,

        /// Emit intermediate representation instead of binary
        #[arg(long, value_parser = ["tokens", "ast", "hir", "mir", "mlir", "llvm-ir"])]
        emit: Option<String>,
    },

    /// Tokenize an AXIOM source file (debug tool)
    Lex {
        /// Input .axm file
        input: String,
    },
}

fn main() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Lex { input } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;
            let (tokens, errors) = axiom_lexer::Lexer::new(&source).tokenize();

            for tok in &tokens {
                let text = &source[tok.span.start as usize..tok.span.end as usize];
                println!("{:4}..{:<4} {:?}  {:?}", tok.span.start, tok.span.end, tok.kind, text);
            }

            if !errors.is_empty() {
                eprintln!("\n--- Errors ---");
                for err in &errors {
                    eprintln!("  {:?}", err);
                }
            }

            Ok(())
        }

        Commands::Compile { input, output: _, emit } => {
            let source = std::fs::read_to_string(&input)
                .map_err(|e| miette::miette!("Failed to read {}: {}", input, e))?;

            match emit.as_deref() {
                Some("tokens") => {
                    let (tokens, _) = axiom_lexer::Lexer::new(&source).tokenize();
                    for tok in &tokens {
                        println!("{:?}", tok);
                    }
                }
                Some("ast") => {
                    let result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                    }
                    println!("{:#?}", result.module);
                }
                Some("hir") => {
                    let result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                        return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
                    }
                    match axiom_hir::lower(&result.module) {
                        Ok(hir_module) => {
                            println!("{hir_module}");
                        }
                        Err(errors) => {
                            eprintln!("--- HIR Lowering Errors ---");
                            for err in &errors {
                                eprintln!("  {err}");
                            }
                            return Err(miette::miette!("HIR lowering failed with {} error(s)", errors.len()));
                        }
                    }
                }
                Some("llvm-ir") => {
                    let result = axiom_parser::parse(&source);
                    if result.has_errors() {
                        eprintln!("--- Parse Errors ---");
                        for err in &result.errors {
                            eprintln!("  {err}");
                        }
                        return Err(miette::miette!("parsing failed with {} error(s)", result.errors.len()));
                    }
                    let hir_module = axiom_hir::lower(&result.module).map_err(|errors| {
                        for err in &errors {
                            eprintln!("  {err}");
                        }
                        miette::miette!("HIR lowering failed with {} error(s)", errors.len())
                    })?;
                    let llvm_ir = axiom_codegen::codegen(&hir_module).map_err(|errors| {
                        for err in &errors {
                            eprintln!("  {err}");
                        }
                        miette::miette!("codegen failed with {} error(s)", errors.len())
                    })?;
                    println!("{llvm_ir}");
                }
                Some(level) => {
                    eprintln!("TODO: --emit={level} not yet implemented");
                }
                None => {
                    eprintln!("TODO: full compilation not yet implemented");
                }
            }

            Ok(())
        }
    }
}
