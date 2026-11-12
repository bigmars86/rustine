use rustine::parser::lexer::{lex, TokenKind};

#[test]
fn escaped_quotes_in_string() {
    let input = "define msg \"He said: \\\"Hello\\\"\""; // string with escaped quotes
    let toks = lex(input).unwrap();
    assert!(toks
        .iter()
        .any(|t| t.kind == TokenKind::String && t.slice.contains("\\\"Hello\\\"")));
}

#[test]
fn escaped_slash_in_regex() {
    let input = r"define r /foo\/bar/"; // regex containing escaped slash
    let toks = lex(input).unwrap();
    assert!(toks
        .iter()
        .any(|t| t.kind == TokenKind::Regex && t.slice == "/foo\\/bar/"));
}
