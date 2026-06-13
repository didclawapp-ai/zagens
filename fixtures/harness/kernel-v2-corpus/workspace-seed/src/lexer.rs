//! Tokenizer for minilang source text.

use crate::util::Span;

/// A lexical token with its source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Int(i64),
    Ident(String),
    Let,
    Fn,
    If,
    Else,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semicolon,
    Eof,
}

/// Tokenize `source` into a flat token stream.
///
/// FIXME: identifiers currently reject digits entirely; `counter1` should
/// lex as a single identifier (first char alphabetic, rest alphanumeric).
pub fn tokenize(source: &str) -> Result<Vec<(Token, Span)>, String> {
    let mut tokens = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0usize;

    while pos < bytes.len() {
        let start = pos;
        let ch = bytes[pos] as char;
        match ch {
            c if c.is_ascii_whitespace() => {
                pos += 1;
                continue;
            }
            c if c.is_ascii_digit() => {
                while pos < bytes.len() && (bytes[pos] as char).is_ascii_digit() {
                    pos += 1;
                }
                let text = &source[start..pos];
                let value: i64 = text
                    .parse()
                    .map_err(|_| format!("integer literal out of range: {text}"))?;
                tokens.push((Token::Int(value), Span::new(start, pos)));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                while pos < bytes.len() && {
                    let c = bytes[pos] as char;
                    c.is_ascii_alphabetic() || c == '_'
                } {
                    pos += 1;
                }
                let text = &source[start..pos];
                let token = match text {
                    "let" => Token::Let,
                    "fn" => Token::Fn,
                    "if" => Token::If,
                    "else" => Token::Else,
                    _ => Token::Ident(text.to_string()),
                };
                tokens.push((token, Span::new(start, pos)));
            }
            '+' => push_single(&mut tokens, Token::Plus, &mut pos, start),
            '-' => push_single(&mut tokens, Token::Minus, &mut pos, start),
            '*' => push_single(&mut tokens, Token::Star, &mut pos, start),
            '/' => push_single(&mut tokens, Token::Slash, &mut pos, start),
            '%' => push_single(&mut tokens, Token::Percent, &mut pos, start),
            '=' => push_single(&mut tokens, Token::Assign, &mut pos, start),
            '(' => push_single(&mut tokens, Token::LParen, &mut pos, start),
            ')' => push_single(&mut tokens, Token::RParen, &mut pos, start),
            '{' => push_single(&mut tokens, Token::LBrace, &mut pos, start),
            '}' => push_single(&mut tokens, Token::RBrace, &mut pos, start),
            ',' => push_single(&mut tokens, Token::Comma, &mut pos, start),
            ';' => push_single(&mut tokens, Token::Semicolon, &mut pos, start),
            other => return Err(format!("unexpected character '{other}' at byte {start}")),
        }
    }

    tokens.push((Token::Eof, Span::new(pos, pos)));
    Ok(tokens)
}

fn push_single(tokens: &mut Vec<(Token, Span)>, token: Token, pos: &mut usize, start: usize) {
    *pos += 1;
    tokens.push((token, Span::new(start, *pos)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_keywords_and_idents() {
        let tokens = tokenize("let x = 1;").unwrap();
        assert_eq!(tokens[0].0, Token::Let);
        assert_eq!(tokens[1].0, Token::Ident("x".into()));
    }

    #[test]
    fn lexes_modulo() {
        let tokens = tokenize("10 % 3").unwrap();
        assert_eq!(tokens[1].0, Token::Percent);
    }
}
