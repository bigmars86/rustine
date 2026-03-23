//! Main parser – converts token stream into AST.
//!
//! This module uses hand-written recursive-descent parsing over the token
//! stream produced by [`crate::parser::lexer`].  The lexer uses nom on `&str`;
//! the syntax parser operates on `&[Token]` without nom.

use crate::errors::{GelError, Span};
use crate::parser::ast::{
    Expression, FunctionCall, GelDocument, Grammar, MatchFieldList, MatchList, MatchStatement, SkipStatement,
    Statement, WhenStatement,
};
use crate::parser::lexer::{Token, TokenKind};

pub type TokenInput<'a> = &'a [Token<'a>];
pub type ParseResult<'a, T> = Result<(TokenInput<'a>, T), ParseError<'a>>;

#[derive(Debug)]
pub struct ParseError<'a> {
    pub input: TokenInput<'a>,
    pub kind: ParseErrorKind,
}

#[derive(Debug)]
pub enum ParseErrorKind {
    Tag,
    Eof,
    Digit,
}

impl<'a> ParseError<'a> {
    fn tag(input: TokenInput<'a>) -> Self {
        Self {
            input,
            kind: ParseErrorKind::Tag,
        }
    }
    fn eof(input: TokenInput<'a>) -> Self {
        Self {
            input,
            kind: ParseErrorKind::Eof,
        }
    }
    fn digit(input: TokenInput<'a>) -> Self {
        Self {
            input,
            kind: ParseErrorKind::Digit,
        }
    }
}

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
    match gel_document(tokens) {
        Ok((_rest, doc)) => Ok(doc),
        Err(e) => {
            let span = if !e.input.is_empty() {
                e.input[0].span
            } else {
                Span::unknown()
            };
            Err(GelError::parse(format!("unexpected token {:?}", e.kind), span))
        }
    }
}

fn gel_document<'a>(input: TokenInput<'a>) -> ParseResult<'a, GelDocument> {
    let mut rest = input;
    let mut doc = GelDocument::default();
    loop {
        if let Ok((r, _)) = token_kind(TokenKind::Newline)(rest) {
            rest = r;
            continue;
        }
        if let Ok((r, (name, expr))) = define_statement(rest) {
            doc.defines.insert(name, expr);
            rest = r;
            continue;
        }
        if let Ok((r, g)) = grammar_statement(rest) {
            doc.grammars.insert(g.name.clone(), g);
            rest = r;
            continue;
        }
        break;
    }
    let (rest, _) = token_kind(TokenKind::EOF)(rest)?;
    Ok((rest, doc))
}

fn define_statement<'a>(input: TokenInput<'a>) -> ParseResult<'a, (String, Expression)> {
    let (rest, _) = token_kind(TokenKind::Define)(input)?;
    let (rest, name) = identifier(rest)?;
    let (rest, expr) = expression(rest)?;
    Ok((rest, (name, expr)))
}

fn grammar_statement<'a>(input: TokenInput<'a>) -> ParseResult<'a, Grammar> {
    let (rest, _) = token_kind(TokenKind::Grammar)(input)?;
    let (rest, name) = identifier(rest)?;
    // Optional inheritance: '(' Identifier ')'
    let (rest, inherit) = if rest.first().is_some_and(|t| t.kind == TokenKind::LeftParen) {
        let (r1, _) = token_kind(TokenKind::LeftParen)(rest)?;
        let (r2, parent_name) = identifier(r1)?;
        let (r3, _) = token_kind(TokenKind::RightParen)(r2)?;
        (r3, Some(parent_name))
    } else {
        (rest, None)
    };
    let (rest, _) = token_kind(TokenKind::Colon)(rest)?;
    let (rest, stmts) = many0_stmt(rest);
    Ok((
        rest,
        Grammar {
            name,
            inherit,
            statements: stmts,
        },
    ))
}

fn many0_stmt<'a>(mut input: TokenInput<'a>) -> (TokenInput<'a>, Vec<Statement>) {
    let mut out = Vec::new();
    while let Ok((r, s)) = statement_line(input) {
        out.push(s);
        input = r;
    }
    (input, out)
}

fn statement_line<'a>(input: TokenInput<'a>) -> ParseResult<'a, Statement> {
    // Skip leading newlines and indents
    let mut rest = input;
    while rest.first().is_some_and(|t| t.kind == TokenKind::Newline) {
        rest = &rest[1..];
    }
    while rest.first().is_some_and(|t| t.kind == TokenKind::Indent) {
        rest = &rest[1..];
    }
    // Try skip first, then match/when/imatch, then bare function calls
    if let Ok((r, s)) = skip_statement(rest) {
        return Ok((r, Statement::Skip(s)));
    }
    if let Ok((r, m)) = match_statement(rest) {
        return Ok((r, Statement::Match(m)));
    }
    if let Ok((r, w)) = when_statement(rest) {
        return Ok((r, Statement::When(w)));
    }
    if let Ok((r, f)) = function_call(rest) {
        return Ok((r, Statement::Action(f)));
    }
    Err(ParseError::tag(rest))
}

fn skip_statement<'a>(input: TokenInput<'a>) -> ParseResult<'a, SkipStatement> {
    let (rest, _) = token_kind(TokenKind::Skip)(input)?;
    let (rest, expr) = expression(rest)?;
    Ok((rest, SkipStatement { pattern: expr }))
}

fn match_statement<'a>(input: TokenInput<'a>) -> ParseResult<'a, MatchStatement> {
    // match or imatch
    let (rest, case_flag) = if let Ok((r, _)) = token_kind(TokenKind::Match)(input) {
        (r, false)
    } else if let Ok((r, _)) = token_kind(TokenKind::IMatch)(input) {
        (r, true)
    } else {
        return Err(ParseError::tag(input));
    };
    // Collect first alternative
    let (mut rest_alt, first_patterns) = many1_expression(rest)?;
    let mut alts = vec![MatchFieldList {
        expressions: first_patterns,
        flags: if case_flag { 1 } else { 0 },
        compiled_regex: None,
        literal_prefix: None,
    }];
    // Additional alternatives start with Pipe then expressions
    loop {
        let peek = skip_ws_tokens(rest_alt);
        if peek.first().is_some_and(|t| t.kind == TokenKind::Pipe) {
            let (r_after_pipe, _) = token_kind(TokenKind::Pipe)(peek)?;
            let (r_after_exprs, alt_patterns) = many1_expression(r_after_pipe)?;
            alts.push(MatchFieldList {
                expressions: alt_patterns,
                flags: if case_flag { 1 } else { 0 },
                compiled_regex: None,
                literal_prefix: None,
            });
            rest_alt = r_after_exprs;
            continue;
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

fn when_statement<'a>(input: TokenInput<'a>) -> ParseResult<'a, WhenStatement> {
    let (rest, _) = token_kind(TokenKind::When)(input)?;
    let (mut rest_alt, first_patterns) = many1_expression(rest)?;
    let mut alts = vec![MatchFieldList {
        expressions: first_patterns,
        flags: 0,
        compiled_regex: None,
        literal_prefix: None,
    }];
    loop {
        let peek = skip_ws_tokens(rest_alt);
        if peek.first().is_some_and(|t| t.kind == TokenKind::Pipe) {
            let (r_after_pipe, _) = token_kind(TokenKind::Pipe)(peek)?;
            let (r_after_exprs, alt_patterns) = many1_expression(r_after_pipe)?;
            alts.push(MatchFieldList {
                expressions: alt_patterns,
                flags: 0,
                compiled_regex: None,
                literal_prefix: None,
            });
            rest_alt = r_after_exprs;
            continue;
        }
        break;
    }
    let (rest_final, _) = token_kind(TokenKind::Colon)(rest_alt)?;
    let (rest_final, actions) = action_block(rest_final)?;
    let match_list = MatchList { alternatives: alts };
    Ok((rest_final, WhenStatement { match_list, actions }))
}

fn many1_expression<'a>(input: TokenInput<'a>) -> ParseResult<'a, Vec<Expression>> {
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
        return Err(ParseError::tag(rest));
    }
    Ok((rest, out))
}

fn action_block<'a>(input: TokenInput<'a>) -> ParseResult<'a, Vec<FunctionCall>> {
    let mut rest = input;
    let mut actions = Vec::new();
    let mut baseline_indent: Option<usize> = None;
    loop {
        let saved = rest;
        // Skip newlines
        while rest.first().is_some_and(|t| t.kind == TokenKind::Newline) {
            rest = &rest[1..];
        }
        // Measure indent
        let mut indent_width: usize = 0;
        while rest.first().is_some_and(|t| t.kind == TokenKind::Indent) {
            indent_width += rest[0].slice.len();
            rest = &rest[1..];
        }
        // If indent dropped below baseline → end of block
        if let Some(base) = baseline_indent {
            if indent_width < base {
                rest = saved;
                break;
            }
        }
        match function_call(rest) {
            Ok((rfun, f)) => {
                if baseline_indent.is_none() && indent_width > 0 {
                    baseline_indent = Some(indent_width);
                }
                actions.push(f);
                rest = rfun;
            }
            Err(_) => {
                if actions.is_empty() {
                    return Err(ParseError::tag(rest));
                }
                rest = saved;
                break;
            }
        }
    }
    Ok((rest, actions))
}

fn function_call<'a>(input: TokenInput<'a>) -> ParseResult<'a, FunctionCall> {
    let (rest, name) = identifier(input)?;
    // Allow bare identifier (no parens) as a grammar invocation
    if rest.first().map(|t| t.kind != TokenKind::LeftParen).unwrap_or(true) {
        return Ok((
            rest,
            FunctionCall {
                name: name.into(),
                args: Vec::new(),
            },
        ));
    }
    let (rest, _) = token_kind(TokenKind::LeftParen)(rest)?;
    let mut args = Vec::new();
    let mut r = rest;
    loop {
        if r.first().is_some_and(|t| t.kind == TokenKind::RightParen) {
            break;
        }
        match expression(r) {
            Ok((r2, arg)) => {
                args.push(arg);
                r = r2;
            }
            Err(_) => break,
        }
        if r.first().is_some_and(|t| t.kind == TokenKind::Comma) {
            r = &r[1..]; // consume comma
        }
    }
    let (rfinal, _) = token_kind(TokenKind::RightParen)(r)?;
    Ok((
        rfinal,
        FunctionCall {
            name: name.into(),
            args,
        },
    ))
}

fn expression<'a>(input: TokenInput<'a>) -> ParseResult<'a, Expression> {
    if let Ok((r, s)) = string_literal(input) {
        return Ok((r, Expression::String(s)));
    }
    if let Ok((r, s)) = regex_literal(input) {
        return Ok((r, Expression::Regex(s)));
    }
    if let Ok((r, n)) = number_literal(input) {
        return Ok((r, Expression::Number(n)));
    }
    if let Ok((r, c)) = capture_literal(input) {
        return Ok((r, Expression::Capture(c)));
    }
    if let Ok((r, n)) = capture_name_literal(input) {
        return Ok((r, Expression::CaptureName(n)));
    }
    if let Ok((r, v)) = identifier(input) {
        return Ok((r, Expression::Variable(v)));
    }
    Err(ParseError::tag(input))
}

// === Token parser helper functions ===

fn token_kind<'a>(expected: TokenKind) -> impl Fn(TokenInput<'a>) -> ParseResult<'a, &'a str> {
    move |input: TokenInput<'a>| {
        if let Some((first, rest)) = input.split_first() {
            if first.kind == expected {
                Ok((rest, first.slice))
            } else {
                Err(ParseError::tag(input))
            }
        } else {
            Err(ParseError::eof(input))
        }
    }
}

fn identifier<'a>(input: TokenInput<'a>) -> ParseResult<'a, String> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Identifier {
            Ok((rest, first.slice.to_string()))
        } else {
            Err(ParseError::tag(input))
        }
    } else {
        Err(ParseError::eof(input))
    }
}

fn string_literal<'a>(input: TokenInput<'a>) -> ParseResult<'a, String> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::String {
            let raw = first.slice;
            let unquoted = if raw.len() >= 2 { &raw[1..raw.len() - 1] } else { raw };
            Ok((rest, unquoted.to_string()))
        } else {
            Err(ParseError::tag(input))
        }
    } else {
        Err(ParseError::eof(input))
    }
}

fn regex_literal<'a>(input: TokenInput<'a>) -> ParseResult<'a, String> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Regex {
            let raw = first.slice;
            let inner = if raw.len() >= 2 { &raw[1..raw.len() - 1] } else { raw };
            Ok((rest, inner.to_string()))
        } else {
            Err(ParseError::tag(input))
        }
    } else {
        Err(ParseError::eof(input))
    }
}

fn number_literal<'a>(input: TokenInput<'a>) -> ParseResult<'a, i64> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Number {
            match first.slice.parse::<i64>() {
                Ok(num) => Ok((rest, num)),
                Err(_) => Err(ParseError::digit(input)),
            }
        } else {
            Err(ParseError::tag(input))
        }
    } else {
        Err(ParseError::eof(input))
    }
}

fn capture_literal<'a>(input: TokenInput<'a>) -> ParseResult<'a, usize> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Capture {
            let digits = &first.slice[1..];
            if let Ok(idx) = digits.parse::<usize>() {
                return Ok((rest, idx));
            }
            Err(ParseError::digit(input))
        } else {
            Err(ParseError::tag(input))
        }
    } else {
        Err(ParseError::eof(input))
    }
}

fn capture_name_literal<'a>(input: TokenInput<'a>) -> ParseResult<'a, String> {
    if let Some((first, rest)) = input.split_first() {
        if first.kind == TokenKind::Capture {
            let body = &first.slice[1..];
            if body.chars().all(|c| c.is_ascii_alphabetic() || c == '_') && !body.is_empty() {
                return Ok((rest, body.to_string()));
            }
            if body.chars().any(|c| !c.is_ascii_digit()) && !body.is_empty() {
                return Ok((rest, body.to_string()));
            }
            Err(ParseError::tag(input))
        } else {
            Err(ParseError::tag(input))
        }
    } else {
        Err(ParseError::eof(input))
    }
}
