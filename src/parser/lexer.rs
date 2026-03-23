//! Lexer – Generic tokenization (core Gel + identifiers) using nom.

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while, take_while1},
    character::complete::{alphanumeric1, char, digit1},
    combinator::{map, recognize},
    multi::{many0, many1},
    sequence::tuple,
    IResult,
};

use crate::errors::{GelError, Span};

/// Discriminant for each kind of Gel token produced by the lexer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenKind {
    Define,
    Grammar,
    Match,
    IMatch,
    When,
    Skip,
    Identifier,
    String,
    Regex,
    Number,
    Capture,
    Colon,
    Pipe,
    Comma,
    Equals,
    LeftParen,
    RightParen,
    Newline,
    Indent,
    Comment,
    EOF,
}

/// A single lexer token carrying its kind, source slice, byte position, and span.
#[derive(Debug, Clone, Copy)]
pub struct Token<'a> {
    pub kind: TokenKind,
    pub slice: &'a str,
    pub position: usize,
    pub span: Span,
}
pub type LexResult<'a> = IResult<&'a str, Token<'a>>;

/// Compute (1-based line, 1-based column) from the full source and a byte offset.
fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Tokenize a complete Gel source string into a `Vec<Token>`.
///
/// Skips comments and pure-whitespace tokens, attaching a [`Span`] to every
/// surviving token for downstream error reporting.
pub fn lex(input: &str) -> Result<Vec<Token<'_>>, GelError> {
    let mut tokens = Vec::new();
    let mut remaining = input;
    let mut position = 0;
    let mut at_line_start = true; // Track start-of-line for indent detection
    while !remaining.is_empty() {
        // At start of line, try to consume indentation before other tokens
        if at_line_start {
            at_line_start = false;
            if let Ok((rest, tok)) = indent(remaining) {
                let consumed = remaining.len() - rest.len();
                let (line, col) = line_col(input, position);
                let span = Span::new(position, line, col, consumed);
                tokens.push(Token {
                    kind: tok.kind,
                    slice: tok.slice,
                    position,
                    span,
                });
                remaining = rest;
                position += consumed;
                continue;
            }
        }
        match next_token(remaining) {
            Ok((rest, tok)) => {
                let consumed = remaining.len() - rest.len();
                if !matches!(tok.kind, TokenKind::Comment) {
                    if tok.kind == TokenKind::Newline && !tok.slice.contains('\n') {
                        // skip intra-line whitespace
                    } else {
                        let (line, col) = line_col(input, position);
                        let span = Span::new(position, line, col, consumed);
                        tokens.push(Token {
                            kind: tok.kind,
                            slice: tok.slice,
                            position,
                            span,
                        });
                        // Mark start-of-line after actual newline tokens
                        if tok.kind == TokenKind::Newline && tok.slice.contains('\n') {
                            at_line_start = true;
                        }
                    }
                }
                remaining = rest;
                position += consumed;
            }
            Err(_) => {
                let (line, col) = line_col(input, position);
                return Err(GelError::lex(
                    format!("Unexpected character '{}'", &remaining[..remaining.len().min(1)]),
                    Span::new(position, line, col, 1),
                ));
            }
        }
    }
    let (eof_line, eof_col) = line_col(input, position);
    tokens.push(Token {
        kind: TokenKind::EOF,
        slice: "",
        position,
        span: Span::new(position, eof_line, eof_col, 0),
    });
    Ok(tokens)
}

pub(crate) fn next_token(input: &str) -> LexResult<'_> {
    alt((
        whitespace_or_newline,
        indent,
        comment,
        keyword,
        string_literal,
        regex_literal,
        number,
        capture_ref,
        identifier,
        operators,
        punctuation,
    ))(input)
}

/// Temporary token (span will be overwritten by `lex()`).
fn tmp(kind: TokenKind, slice: &str) -> Token<'_> {
    Token {
        kind,
        slice,
        position: 0,
        span: Span::unknown(),
    }
}

fn whitespace_or_newline(input: &str) -> LexResult<'_> {
    alt((
        map(recognize(many1(alt((tag("\n"), tag("\r\n"))))), |s| {
            tmp(TokenKind::Newline, s)
        }),
        map(
            take_while1(|c: char| c.is_whitespace() && c != '\n' && c != '\r'),
            |s| tmp(TokenKind::Newline, s),
        ),
    ))(input)
}

fn comment(input: &str) -> LexResult<'_> {
    map(
        recognize(tuple((
            alt((char('#'), char('!'))),
            take_while(|c: char| c != '\n' && c != '\r'),
        ))),
        |s| tmp(TokenKind::Comment, s),
    )(input)
}

fn keyword(input: &str) -> LexResult<'_> {
    // Try each keyword; after matching the tag, verify the next character is
    // not an identifier-continuation char so that e.g. "skipline" is lexed as
    // an Identifier, not as Skip + Identifier("line").
    fn kw<'a>(word: &'static str, kind: TokenKind) -> impl Fn(&'a str) -> LexResult<'a> {
        move |input: &'a str| {
            let (rest, matched) = tag(word)(input)?;
            // If the next char is alphanumeric or '_', this is actually an identifier, not a keyword.
            if rest.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                return Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Tag,
                )));
            }
            Ok((rest, tmp(kind, matched)))
        }
    }
    alt((
        kw("define", TokenKind::Define),
        kw("grammar", TokenKind::Grammar),
        kw("imatch", TokenKind::IMatch), // imatch before match
        kw("match", TokenKind::Match),
        kw("when", TokenKind::When),
        kw("skip", TokenKind::Skip),
    ))(input)
}

// Indentation: only recognized if at start of line (preceded by newline in higher-level scan). Here we approximate by matching leading spaces.
fn indent(input: &str) -> LexResult<'_> {
    if input.starts_with("    ") {
        // 4-space blocks; simple heuristic
        let count = input.chars().take_while(|c| *c == ' ').count();
        let slice = &input[..count];
        Ok((&input[count..], tmp(TokenKind::Indent, slice)))
    } else if input.starts_with("\t") {
        let count = input.chars().take_while(|c| *c == '\t').count();
        let slice = &input[..count];
        Ok((&input[count..], tmp(TokenKind::Indent, slice)))
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )))
    }
}

fn identifier(input: &str) -> LexResult<'_> {
    // Allow identifiers starting with letter, digit, or underscore; then include '-', '.', '/', '@'
    map(
        recognize(tuple((
            alt((alphanumeric1, tag("_"))),
            many0(alt((alphanumeric1, tag("_"), tag("-"), tag("."), tag("/"), tag("@")))),
        ))),
        |s| tmp(TokenKind::Identifier, s),
    )(input)
}

fn string_literal(input: &str) -> LexResult<'_> {
    // Support escapes (\\', \\"), allow newlines. Strategy: iterate manually.
    fn scan(s: &str, delim: char) -> Option<(&str, &str)> {
        if !s.starts_with(delim) {
            return None;
        }
        let mut idx = 1; // skip opening
        let bytes = s.as_bytes();
        while idx < bytes.len() {
            let c = bytes[idx] as char;
            if c == '\\' {
                // escape – skip next char if any
                idx += 2;
                continue;
            }
            if c == delim {
                // closing
                let end = idx + 1;
                return Some((&s[end..], &s[..end]));
            }
            idx += 1;
        }
        None // unterminated
    }
    if let Some((rest, slice)) = scan(input, '\'') {
        return Ok((rest, tmp(TokenKind::String, slice)));
    }
    if let Some((rest, slice)) = scan(input, '"') {
        return Ok((rest, tmp(TokenKind::String, slice)));
    }
    Err(nom::Err::Error(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Tag,
    )))
}

fn regex_literal(input: &str) -> LexResult<'_> {
    // Regex with escapes: /.../ allowing \/ to escape slash.
    if !input.starts_with('/') {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Char,
        )));
    }
    let bytes = input.as_bytes();
    let mut idx = 1; // after opening
    while idx < bytes.len() {
        let c = bytes[idx] as char;
        if c == '\\' {
            idx += 2;
            continue;
        }
        if c == '/' {
            // closing
            let end = idx + 1;
            let slice = &input[..end];
            let rest = &input[end..];
            return Ok((rest, tmp(TokenKind::Regex, slice)));
        }
        idx += 1;
    }
    Err(nom::Err::Error(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Tag,
    )))
}

fn number(input: &str) -> LexResult<'_> {
    map(digit1, |s| tmp(TokenKind::Number, s))(input)
}

fn capture_ref(input: &str) -> LexResult<'_> {
    if let Some(rest) = input.strip_prefix('$') {
        // Accept either digits (numeric capture) or identifier-like name until non-alnum/underscore
        let name_len = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        if name_len > 0 {
            let slice = &input[..1 + name_len];
            let remaining = &input[1 + name_len..];
            return Ok((remaining, tmp(TokenKind::Capture, slice)));
        }
    }
    Err(nom::Err::Error(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Tag,
    )))
}

fn operators(input: &str) -> LexResult<'_> {
    alt((
        map(char(':'), |_| tmp(TokenKind::Colon, ":")),
        map(char('|'), |_| tmp(TokenKind::Pipe, "|")),
        map(char(','), |_| tmp(TokenKind::Comma, ",")),
        map(char('='), |_| tmp(TokenKind::Equals, "=")),
    ))(input)
}

fn punctuation(input: &str) -> LexResult<'_> {
    alt((
        map(char('('), |_| tmp(TokenKind::LeftParen, "(")),
        map(char(')'), |_| tmp(TokenKind::RightParen, ")")),
    ))(input)
}
