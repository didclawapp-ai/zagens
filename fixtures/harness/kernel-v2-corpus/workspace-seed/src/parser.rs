//! Recursive-descent parser producing an AST from a token stream.

use crate::lexer::Token;
use crate::util::Span;

/// Expression / statement nodes. A program is a `Vec<Ast>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ast {
    Int(i64),
    Ident(String),
    Let {
        name: String,
        value: Box<Ast>,
    },
    BinOp {
        op: BinOp,
        lhs: Box<Ast>,
        rhs: Box<Ast>,
    },
    If {
        cond: Box<Ast>,
        then_branch: Vec<Ast>,
        else_branch: Vec<Ast>,
    },
    Fn {
        params: Vec<String>,
        body: Vec<Ast>,
    },
    Call {
        callee: Box<Ast>,
        args: Vec<Ast>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

/// Parse a full program (sequence of `;`-separated statements).
///
/// TODO: error recovery — on a parse error we abort instead of skipping to
/// the next statement boundary, which makes multi-error diagnostics impossible.
pub fn parse_program(tokens: &[(Token, Span)]) -> Result<Vec<Ast>, String> {
    let mut parser = Parser { tokens, pos: 0 };
    let mut program = Vec::new();
    while !parser.at_eof() {
        program.push(parser.parse_statement()?);
        parser.eat_if(&Token::Semicolon);
    }
    Ok(program)
}

struct Parser<'a> {
    tokens: &'a [(Token, Span)],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn at_eof(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|(t, _)| t)
            .unwrap_or(&Token::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.peek().clone();
        self.pos += 1;
        token
    }

    fn eat_if(&mut self, expected: &Token) -> bool {
        if self.peek() == expected {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        if self.eat_if(expected) {
            Ok(())
        } else {
            Err(format!("expected {expected:?}, found {:?}", self.peek()))
        }
    }

    fn parse_statement(&mut self) -> Result<Ast, String> {
        if self.eat_if(&Token::Let) {
            let name = match self.bump() {
                Token::Ident(name) => name,
                other => return Err(format!("expected identifier after let, found {other:?}")),
            };
            self.expect(&Token::Assign)?;
            let value = self.parse_expr()?;
            return Ok(Ast::Let {
                name,
                value: Box::new(value),
            });
        }
        self.parse_expr()
    }

    fn parse_expr(&mut self) -> Result<Ast, String> {
        let mut lhs = self.parse_term()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_term()?;
            lhs = Ast::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_term(&mut self) -> Result<Ast, String> {
        let mut lhs = self.parse_atom()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_atom()?;
            lhs = Ast::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_atom(&mut self) -> Result<Ast, String> {
        match self.bump() {
            Token::Int(value) => Ok(Ast::Int(value)),
            Token::Ident(name) => {
                if self.peek() == &Token::LParen {
                    self.pos += 1;
                    let mut args = Vec::new();
                    while self.peek() != &Token::RParen {
                        args.push(self.parse_expr()?);
                        if !self.eat_if(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect(&Token::RParen)?;
                    return Ok(Ast::Call {
                        callee: Box::new(Ast::Ident(name)),
                        args,
                    });
                }
                Ok(Ast::Ident(name))
            }
            Token::LParen => {
                let inner = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(inner)
            }
            other => Err(format!("unexpected token in expression: {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    #[test]
    fn parses_precedence() {
        let tokens = tokenize("1 + 2 * 3").unwrap();
        let program = parse_program(&tokens).unwrap();
        assert!(matches!(
            &program[0],
            Ast::BinOp {
                op: BinOp::Add,
                ..
            }
        ));
    }
}
