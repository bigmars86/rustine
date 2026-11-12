use rustine::parser::lexer::{lex, TokenKind};

#[test]
fn string_literal_may_span_lines() {
    let source = "define ws /\\s+/\nmessage \"Hello\nWorld from Gelatin\"\n";
    let tokens = lex(source).unwrap();
    assert!(
        tokens
            .iter()
            .any(|t| t.kind == TokenKind::String && t.slice == "\"Hello\nWorld from Gelatin\""),
        "multiline string token missing"
    );
}
