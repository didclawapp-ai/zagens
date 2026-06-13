//! Tree-walking evaluator for minilang.

use std::collections::HashMap;

use crate::parser::{Ast, BinOp};

/// Runtime values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Unit,
}

/// Lexical environment: a chain of scopes.
///
/// TODO: environments are cloned on function call; switch to Rc<RefCell<..>>
/// so closures can mutate captured bindings.
#[derive(Debug, Clone, Default)]
pub struct Env {
    bindings: HashMap<String, Value>,
}

impl Env {
    /// Root scope with no parent.
    #[must_use]
    pub fn root() -> Self {
        Self::default()
    }

    pub fn define(&mut self, name: &str, value: Value) {
        self.bindings.insert(name.to_string(), value);
    }

    #[must_use]
    pub fn lookup(&self, name: &str) -> Option<&Value> {
        self.bindings.get(name)
    }
}

/// Evaluate a parsed program, returning the value of the last statement.
pub fn eval_program(program: &[Ast], env: &mut Env) -> Result<Value, String> {
    let mut last = Value::Unit;
    for node in program {
        last = eval(node, env)?;
    }
    Ok(last)
}

fn eval(node: &Ast, env: &mut Env) -> Result<Value, String> {
    match node {
        Ast::Int(value) => Ok(Value::Int(*value)),
        Ast::Ident(name) => env
            .lookup(name)
            .cloned()
            .ok_or_else(|| format!("undefined variable: {name}")),
        Ast::Let { name, value } => {
            let value = eval(value, env)?;
            env.define(name, value);
            Ok(Value::Unit)
        }
        Ast::BinOp { op, lhs, rhs } => {
            let lhs = eval_int(lhs, env)?;
            let rhs = eval_int(rhs, env)?;
            // NOTE: wrapping arithmetic — overflow wraps silently by design
            // for the corpus; a real implementation should surface an error.
            let result = match op {
                BinOp::Add => lhs.wrapping_add(rhs),
                BinOp::Sub => lhs.wrapping_sub(rhs),
                BinOp::Mul => lhs.wrapping_mul(rhs),
                BinOp::Div => {
                    if rhs == 0 {
                        return Err("division by zero".to_string());
                    }
                    lhs / rhs
                }
                BinOp::Mod => {
                    if rhs == 0 {
                        return Err("modulo by zero".to_string());
                    }
                    lhs % rhs
                }
            };
            Ok(Value::Int(result))
        }
        Ast::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let cond = eval_int(cond, env)?;
            if cond != 0 {
                eval_program(then_branch, env)
            } else {
                eval_program(else_branch, env)
            }
        }
        // FIXME: first-class functions are parsed but not yet evaluated;
        // calling a user-defined fn currently returns an error.
        Ast::Fn { .. } => Err("function values are not implemented yet".to_string()),
        Ast::Call { callee, args } => {
            let Ast::Ident(name) = callee.as_ref() else {
                return Err("only named functions can be called".to_string());
            };
            match name.as_str() {
                "print" => {
                    for arg in args {
                        let value = eval(arg, env)?;
                        println!("{value:?}");
                    }
                    Ok(Value::Unit)
                }
                "abs" => {
                    let [arg] = args.as_slice() else {
                        return Err("abs expects exactly one argument".to_string());
                    };
                    Ok(Value::Int(eval_int(arg, env)?.wrapping_abs()))
                }
                other => Err(format!("unknown function: {other}")),
            }
        }
    }
}

fn eval_int(node: &Ast, env: &mut Env) -> Result<i64, String> {
    match eval(node, env)? {
        Value::Int(value) => Ok(value),
        Value::Unit => Err("expected integer, found unit".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::parser::parse_program;

    fn run(source: &str) -> Result<Value, String> {
        let tokens = tokenize(source)?;
        let program = parse_program(&tokens)?;
        eval_program(&program, &mut Env::root())
    }

    #[test]
    fn modulo_works() {
        assert_eq!(run("10 % 3"), Ok(Value::Int(1)));
    }

    #[test]
    fn division_by_zero_errors() {
        assert!(run("1 / 0").is_err());
    }
}
