use crate::file;
use std::fs;
use std::path::{PathBuf, Path};
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum TokenKind<'a> {
    FragmentKeyword,
    QueryKeyword,
    MutationKeyword,
    OnKeyword,
    StringKeyword,
    IntKeyword,
    BoolKeyword,
    Spread,
    OpenParen,
    CloseParen,
    OpenSquare,
    CloseSquare,
    Exclamation,
    OpenBracket,
    CloseBracket,
    Colon,
    Int(i32),
    String(&'a str),
    Identifier(&'a str),
    Variable(&'a str),
}

pub struct Token<'a> {
    pub kind: TokenKind<'a>,
    pub column: u32,
    pub line: u32,
}

pub struct ErrorLocation {
    pub path: PathBuf,
    pub column: u32,
    pub line: u32,
}

pub enum ErrorKind {
    Expecting(&'static str),
    Unexpected(char),
}

pub struct Error {
    pub location: ErrorLocation,
    pub kind: ErrorKind
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error line {}, column {}", self.location.line, self.location.column);
        match self.kind {
            ErrorKind::Expecting(str) => write!(f, " : Expecting {}", str),
            ErrorKind::Unexpected(c) => write!(f, " : Unexpected token {}", c)
        }
    }
}

/*
struct LexerSrc<'a> {
    src: &'a str,
}*/

struct Tok<'a> {
    tok_slice: &'a str,
    tok_len: usize,
}

impl<'a> Tok<'a> {
    fn tok(&self) -> &'a str {
        &self.tok_slice[..self.tok_len]
    }

    fn advance(&mut self, src_it: &mut SrcIt<'a>) {
        src_it.next();
        self.tok_len += 1;
    }

    fn reset_tok(&mut self, chars: &'a str) {
        self.tok_slice = chars;
        self.tok_len = 1;
    }
}

struct SrcIt<'a> {
    path: &'a Path,
    i: std::str::Chars<'a>,
    column: u32,
    line: u32,
}

impl<'a> SrcIt<'a> {
    fn current(&self) -> Option<char> {
        self.i.clone().next()
    }

    fn next(&mut self) -> Option<char> {
        self.column += 1;
        self.i.next()
    }

    fn error(&self, kind: ErrorKind) -> Error {
        Error{
            location: ErrorLocation{
                path: self.path.to_owned(),
                column: self.column,
                line: self.line,
            },
            kind
        }
    }
}

fn add_token<'a>(tokens: &mut Vec<Token<'a>>, src_range: &SrcIt<'a>, kind: TokenKind<'a>) {
    tokens.push(Token{
        kind: kind,
        column: src_range.column,
        line: src_range.line
    });

    //self.reset_tok();
}

pub fn lex<'a>(path: &'a Path, src: &'a str) -> Result<Vec<Token<'a>>, Error> {
    let mut tok = Tok{
        tok_slice: src,
        tok_len: 1
    };

    let mut src_it = SrcIt {
        path: path,
        i: src.chars(),
        column: 0,
        line: 0
    };

    let mut tokens = vec![];

    while let Some(c) = src_it.next() {
        match c {
            //skip
            ' ' | ',' | '\r' | '\t' => {},

            '\n' => {
                src_it.line += 1
            }

            ':' => add_token(&mut tokens, &src_it, TokenKind::Colon),
            '{' => add_token(&mut tokens, &src_it, TokenKind::OpenBracket),
            '}' => add_token(&mut tokens, &src_it, TokenKind::CloseBracket),
            '(' => add_token(&mut tokens, &src_it, TokenKind::OpenParen),
            ')' => add_token(&mut tokens, &src_it, TokenKind::CloseParen),
            '[' => add_token(&mut tokens, &src_it, TokenKind::OpenSquare),
            ']' => add_token(&mut tokens, &src_it, TokenKind::CloseSquare),
            '!' => add_token(&mut tokens, &src_it, TokenKind::Exclamation),


            //spread
            '.' => {
                for _ in 0..2 {
                    let found_dot = match src_it.next() {
                        Some(c) => c == '.',
                        None => false
                    };

                    if !found_dot {
                        return Err(src_it.error(ErrorKind::Expecting("...")))
                    }
                }

                add_token(&mut tokens, &src_it, TokenKind::Spread);
            },

            //number
            '0'..='9' => {
                while let Some(c) = src_it.current() {
                    match c {
                        '0'..='9' => tok.advance(&mut src_it),
                        _ => break
                    }

                }

                println!("Got token {}", tok.tok());

                add_token(&mut tokens,&src_it, TokenKind::Int(tok.tok().parse().unwrap())); //could do the parsing ourselves
            }

            //variable
            '$' => {
                while let Some(c) = src_it.current() {
                    match c {
                        'A'..='Z' | 'a'..='z' | '0'..='9' | '_' => tok.advance(&mut src_it),
                        _ => break
                    }
                }

                add_token(&mut tokens, &src_it, TokenKind::Variable(&tok.tok()[1..]));
            }

            //identifier
            'A'..='Z' | 'a'..='z' | '_' => {
                while let Some(c) = src_it.current() {
                    match c {
                        'A'..='Z' | 'a'..='z' | '0'..='9' | '_' => tok.advance(&mut src_it),
                        _ => break,
                    }
                }

                let str = tok.tok();

                let kind = match str {
                    "fragment" => TokenKind::FragmentKeyword,
                    "query" => TokenKind::QueryKeyword,
                    "mutation" => TokenKind::MutationKeyword,
                    "on" => TokenKind::OnKeyword,
                    "String" => TokenKind::StringKeyword,
                    "Int" => TokenKind::IntKeyword,
                    "Bool" => TokenKind::BoolKeyword,
                    _ => TokenKind::Identifier(str)
                };

                add_token(&mut tokens, &src_it, kind);
            },

            _ => {
                return Err(src_it.error(ErrorKind::Unexpected(c)));
            }
        }


        tok.reset_tok(src_it.i.as_str());
    }

    Ok(tokens)
}