//! Main parser – converts token stream into AST.
//! Uses nom for structured, modular parsing.

use nom::{branch::alt, combinator::map, multi::many0, sequence::tuple, Finish, IResult};

use crate::errors::{GelError, Span};
use crate::parser::ast::{
    Expression, FunctionCall, GelDocument, Grammar, MatchFieldList, MatchList, MatchStatement, SkipStatement,
    Statement, WhenStatement,
};
use crate::parser::lexer::{Token, TokenKind};

pub type TokenInput<'a> = &'a [Token<'a>];
pub type ParseResult<'a, T> = IResult<TokenInput<'a>, T>;

/// Skip leading `Newline` and `Indent` tokens so that continuation lines
/// (e.g. multi-line alternation `|` on the next line) are handled correctly.
fn skip_ws_tokens<'a>(input: TokenInput<'a>) -> TokenInput<'a> {
    let mut rest = input;
    while let Some(tok) = rest.first() {
        if matches!(tok.kind, TokenKind::Newline | TokenKind::Indent) {
            rest = &rest[1..];
        } else {
            break;
        }
    }
    rest
}

pub fn parse_gel_document(tokens: &[Token]) -> Result<GelDocument, GelError> {
    match gel_document(tokens).finish() {
        Ok((_rest, doc)) => Ok(doc),
        Err(e) => {
            // Extract span from the remaining token stream, if any.
            let span = if !e.input.is_empty() {
                e.input[0].span
            } else {
                Span::unknown()
            };
            Err(GelError::parse(format!("unexpected token {:?}", e.code), span))
        }
    }
}

fn gel_document(input: TokenInput) -> ParseResult<GelDocument> {
    map(
        tuple((
            many0(alt((
                map(token_kind(TokenKind::Newline), |_| None),
                map(define_statement, |(name, expr)| Some(DocumentItem::Define(name, expr))),
                map(grammar_statement, |g| Some(DocumentItem::Grammar(g))),
            ))),
            token_kind(TokenKind::EOF),
        )),
        |(items, _)| {
            let mut doc = GelDocument::default();
            for item in items.into_iter().flatten() {
                match item {
                    DocumentItem::Define(name, expr) => {
                        doc.defines.insert(name, expr);
                    }
                    DocumentItem::Grammar(g) => {
                        doc.grammars.insert(g.name.clone(), g);
                    }
                }
            }
            doc
        },
    )(input)
}

enum DocumentItem {
    Define(String, Expression),
    Grammar(Grammar),
}

fn define_statement(input: TokenInput) -> ParseResult<(String, Expression)> {
    map(
        tuple((token_kind(TokenKind::Define), identifier, expression)),
        |(_, name, expr)| (name, expr),
    )(input)
}

fn grammar_statement(input: TokenInput) -> ParseResult<Grammar> {
    // grammar name (Parent)? :
    let (rest, _) = token_kind(TokenKind::Grammar)(input)?;
    let (rest, name) = identifier(rest)?;
    // Optional inheritance: '(' Identifier ')'
    let (rest, inherit) = if let Some(tok) = rest.first() {
        if tok.kind == TokenKind::LeftParen {
            let (r1, _) = token_kind(TokenKind::LeftParen)(rest)?;
            let (r2, parent_name) = identifier(r1)?;
            let (r3, _) = token_kind(TokenKind::RightParen)(r2)?;
            (r3, Some(parent_name))
        } else {
            (rest, None)
        }
    } else {
        (rest, None)
    };
    let (rest, _) = token_kind(TokenKind::Colon)(rest)?;
    let (rest, stmts) = many0(statement_line)(rest)?;
    Ok((
        rest,
        Grammar {
            name,
            inherit,
            statements: stmts,
        },
    ))
}

fn statement_line(input: TokenInput) -> ParseResult<Statement> {
    // Optional newline(s) then optional indent(s) then a statement.
    let (rest, _) = many0(token_kind(TokenKind::Newline))(input)?;
    let (rest, _) = many0(token_kind(TokenKind::Indent))(rest)?; // ignore indent depth for now
    // Try skip first (single expression), then match/when/imatch with actions,
    // then bare function calls (e.g. out.set_root_name('tria') at grammar level).
    alt((
        map(skip_statement, Statement::Skip),
        map(match_statement, Statement::Match),
        map(when_statement, Statement::When),
        map(function_call, Statement::Action),
    ))(rest)
}

fn skip_statement(input: TokenInput) -> ParseResult<SkipStatement> {
    let (rest, (_, expr)) = tuple((token_kind(TokenKind::Skip), expression))(input)?;
    // optional trailing colon/newline ignored in skip semantics
    Ok((rest, SkipStatement { pattern: expr }))
}

fn match_statement(input: TokenInput) -> ParseResult<MatchStatement> {
    // match or imatch
    let (rest, (case_flag, _kw_slice)) = alt((
        map(token_kind(TokenKind::Match), |_| (false, "match")),
        map(token_kind(TokenKind::IMatch), |_| (true, "imatch")),
    ))(input)?;
    // Collect first alternative
    let (mut rest_alt, first_patterns) = many1_expression(rest)?;
    let mut alts = vec![MatchFieldList {
        expressions: first_patterns,
        flags: if case_flag { 1 } else { 0 },
        compiled_regex: None, literal_prefix: None,
    }];
    // Additional alternatives start with Pipe then expressions.
    // The Pipe may appear on a continuation line after Newline+Indent tokens,
    // e.g.  match expr1 nl\n        | expr2 nl:
    loop {
        let peek = skip_ws_tokens(rest_alt);
        if let Some(tok) = peek.first() {
            if tok.kind == TokenKind::Pipe {
                let (r_after_pipe, _) = token_kind(TokenKind::Pipe)(peek)?;
                let (r_after_exprs, alt_patterns) = many1_expression(r_after_pipe)?;
                alts.push(MatchFieldList {
                    expressions: alt_patterns,
                    flags: if case_flag { 1 } else { 0 },
                    compiled_regex: None, literal_prefix: None,
                });
                rest_alt = r_after_exprs;
                continue;
            }
        }
        break;
    }
    let (rest_final, _) = token_kind(TokenKind::Colon)(rest_alt)?;
    let (rest_final, actions) = action_block(rest_final)?;
    let match_list = MatchList { alternatives: alts };
    Ok((
        rest_final,
        MatchStatement {
            match_list,
            actions,
            case_insensitive: case_flag,
        },
    ))
}

fn when_statement(input: TokenInput) -> ParseResult<WhenStatement> {
    let (rest, _) = token_kind(TokenKind::When)(input)?;
    // Collect first alternative
    let (mut rest_alt, first_patterns) = many1_expression(rest)?;
    let mut alts = vec![MatchFieldList {
        expressions: first_patterns,
        flags: 0,
        compiled_regex: None, literal_prefix: None,
    }];
    // Additional alternatives start with Pipe then expressions (same as match_statement)
    loop {
        let peek = skip_ws_tokens(rest_alt);
        if let Some(tok) = peek.first() {
            if tok.kind == TokenKind::Pipe {
                let (r_after_pipe, _) = token_kind(TokenKind::Pipe)(peek)?;
                let (r_after_exprs, alt_patterns) = many1_expression(r_after_pipe)?;
                alts.push(MatchFieldList {
                    expressions: alt_patterns,
                    flags: 0,
                    compiled_regex: None, literal_prefix: None,
                });
                rest_alt = r_after_exprs;
                continue;
            }
        }
        break;
    }
    let (rest_final, _) = token_kind(TokenKind::Colon)(rest_alt)?;
    let (rest_final, actions) = action_block(rest_final)?;
    let match_list = MatchList { alternatives: alts };
    Ok((rest_final, WhenStatement { match_list, actions }))
}

fn many1_expression(input: TokenInput) -> ParseResult<Vec<Expression>> {
    // At least one expression, collecting across continuation lines.
    // Newline/Indent tokens are treated as whitespace (matching Python's
    // <ws> := [ \t\n]+ between expressions in a match_field_list).
    let mut out = Vec::new();
    let mut rest = input;
    loop {
        // Skip Newline/Indent — supports multi-line match/when field lists.
        while rest
            .first()
            .is_some_and(|t| matches!(t.kind, TokenKind::Newline | TokenKind::Indent))
        {
            rest = &rest[1..];
        }
        if rest.is_empty() {
            break;
        }
        if let Some(tok) = rest.first() {
            match tok.kind {
                TokenKind::Colon | TokenKind::Pipe => break,
                TokenKind::Match
                | TokenKind::IMatch
                | TokenKind::When
                | TokenKind::Skip
                | TokenKind::Define
                | TokenKind::Grammar => break,
                _ => {}
            }
        }
        match expression(rest) {
            Ok((r, e)) => {
                out.push(e);
                rest = r;
            }
            Err(_) => break,
        }
    }
    if out.is_empty() {
        return Err(nom::Err::Error(nom::error::Error::new(
            rest,
            nom::error::ErrorKind::Tag,
        )));
    }
    Ok((rest, out))
}

fn action_block(input: TokenInput) -> ParseResult<Vec<FunctionCall>> {
    // Actions: lines beginning with indentation followed by function_call.
    // We detect the indent depth of the first action to establish a baseline;
    // subsequent lines at a lesser indent level belong to the parent grammar.
    let mut rest = input;
    let mut actions = Vec::new();
    let mut baseline_indent: Option<usize> = None;
    loop {
        // Save position before consuming newlines (for rewinding on de-indent)
        let saved = rest;
        let (r1, _) = many0(token_kind(TokenKind::Newline))(rest)?;
        // Measure total indent width on this line
        let mut indent_width: usize = 0;
        let mut r2 = r1;
        loop {
            if let Some((first, after)) = r2.split_first() {
                if first.kind == TokenKind::Indent {
                    indent_width += first.slice.len();
                    r2 = after;
                    continue;
                }
            }
            break;
        }
        // If we already have actions and indent dropped below baseline → end of block
        if let Some(base) = baseline_indent {
            if indent_width < base {
                rest = saved; // rewind to before the newlines
                break;
            }
        }
        match function_call(r2) {
            Ok((rfun, f)) => {
                if baseline_indent.is_none() && indent_width > 0 {
                    baseline_indent = Some(indent_width);
                }
                actions.push(f);
                rest = rfun;
            }
            Err(_) => {
                if actions.is_empty() {
                    return Err(nom::Err::Error(nom::error::Error::new(
                        r2,
                        nom::error::ErrorKind::Tag,
                    )));
                }
                rest = saved; // rewind
                break;
            }
        }
    }
    Ok((rest, actions))
}

fn function_call(input: TokenInput) -> ParseResult<FunctionCall> {
    let (rest, name) = identifier(input)?;
    // Allow bare identifier (no parens) as a grammar invocation — Python: Function.py
    if rest.first().map(|t| t.kind != TokenKind::LeftParen).unwrap_or(true) {
        return Ok((rest, FunctionCall { name: name.into(), args: Vec::new() }));
    }
    let (rest, _) = token_kind(TokenKind::LeftParen)(rest)?;
    // Args: zero or more expressions separated by commas
    let mut args = Vec::new();
    let mut r = rest;
    loop {
        if let Some(tok) = r.first() {
            if tok.kind == TokenKind::RightParen {
                break;
            }
        }
        match expression(r) {
            Ok((r2, arg)) => {
                args.push(arg);
                r = r2;
            }
            Err(_) => break,
        }
        if let Some(tok) = r.first() {
            if tok.kind == TokenKind::Comma {
                let (r3, _) = token_kind(TokenKind::Comma)(r)?;
                r = r3;
            }
        }
    }
    let (rfinal, _) = token_kind(TokenKind::RightParen)(r)?;
    Ok((rfinal, FunctionCall { name: name.into(), args }))
}

fn expression(input: TokenInput) -> ParseResult<Expression> {
    alt((
        map(string_literal, Expression::String),
        map(regex_literal, Expression::Regex),
        map(number_literal, Expression::Number),
        map(capture_literal, Expression::Capture),
        map(capture_name_literal, Expression::CaptureName),
        map(identifier, Expression::Variable),
    ))(input)
}

// === Token parsers ===

// === Token parser helper functions ===

/// Parse a token of a specific kind
fn token_kind(expected: TokenKind) -> impl Fn(TokenInput) -> ParseResult<&str> {
    move |input: TokenInput| {
        if let Some((first, rest)) = input.split_first() {
            if first.kind == expected {
                Ok((rest, first.slice))
            } else {
                Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Tag,
                )))
            }
        } else {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Eof,
            )))
        }
    }
}

/// Parse an identifier token
fn identifier(input: TokenInput) -> ParseResult<String> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Identifier {
            Ok((rest, first.slice.to_string()))
        } else {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Eof,
        )))
    }
}

/// Parse a string literal token
fn string_literal(input: TokenInput) -> ParseResult<String> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::String {
            let raw = first.slice;
            // Trim surrounding quotes if present
            let unquoted = if raw.len() >= 2 { &raw[1..raw.len() - 1] } else { raw };
            Ok((rest, unquoted.to_string()))
        } else {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Eof,
        )))
    }
}

/// Parse a regex literal token
fn regex_literal(input: TokenInput) -> ParseResult<String> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Regex {
            let raw = first.slice;
            let inner = if raw.len() >= 2 { &raw[1..raw.len() - 1] } else { raw };
            Ok((rest, inner.to_string()))
        } else {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Eof,
        )))
    }
}

/// Parse a number token
fn number_literal(input: TokenInput) -> ParseResult<i64> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Number {
            match first.slice.parse::<i64>() {
                Ok(num) => Ok((rest, num)),
                Err(_) => Err(nom::Err::Error(nom::error::Error::new(
                    input,
                    nom::error::ErrorKind::Digit,
                ))),
            }
        } else {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Eof,
        )))
    }
}

fn capture_literal(input: TokenInput) -> ParseResult<usize> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Capture {
            // slice like $1 $23 -> parse digits after '$'
            let digits = &first.slice[1..];
            if let Ok(idx) = digits.parse::<usize>() {
                return Ok((rest, idx));
            }
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Digit,
            )))
        } else {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Eof,
        )))
    }
}

fn capture_name_literal(input: TokenInput) -> ParseResult<String> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Capture {
            let body = &first.slice[1..];
            if body.chars().all(|c| c.is_ascii_alphabetic() || c == '_') && !body.is_empty() {
                return Ok((rest, body.to_string()));
            }
            // ensure not purely digits (that would be handled by capture_literal) by requiring at least one non-digit
            if body.chars().any(|c| !c.is_ascii_digit()) && !body.is_empty() {
                // treat mixed alnum with first non-digit as name
                return Ok((rest, body.to_string()));
            }
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        } else {
            Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Tag,
            )))
        }
    } else {
        Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Eof,
        )))
    }
}
