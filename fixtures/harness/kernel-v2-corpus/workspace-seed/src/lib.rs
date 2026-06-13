//! minilang — a tiny tree-walking interpreter.
//!
//! Corpus seed for kernel-v2 benchmarks; small on purpose.

pub mod eval;
pub mod lexer;
pub mod parser;
pub mod util;

pub use eval::{Env, Value, eval_program};
pub use lexer::{Token, tokenize};
pub use parser::{Ast, parse_program};
pub use util::{Span, render_span};

/// Convenience entry point: tokenize, parse, and evaluate `source`.
///
/// TODO: accept a pre-seeded `Env` so REPL sessions can share state.
pub fn run(source: &str) -> Result<Value, String> {
    let tokens = tokenize(source)?;
    let ast = parse_program(&tokens)?;
    let mut env = Env::root();
    eval_program(&ast, &mut env)
}

/// Library version string surfaced by the `version()` built-in.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_arithmetic() {
        assert_eq!(run("1 + 2 * 3"), Ok(Value::Int(7)));
    }

    #[test]
    fn run_let_binding() {
        assert_eq!(run("let x = 4; x + 1"), Ok(Value::Int(5)));
    }
}
