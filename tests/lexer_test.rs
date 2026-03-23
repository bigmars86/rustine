use rustine::parser::lexer::{lex, TokenKind};

#[test]
fn test_lexer_simple() {
    let input = "define nl /[\\r\\n]/";
    let tokens = lex(input).unwrap();

    // Überprüfe die Token-Typen
    assert_eq!(tokens[0].kind, TokenKind::Define);
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
    assert_eq!(tokens[1].slice, "nl");
    assert_eq!(tokens[2].kind, TokenKind::Regex);
    assert_eq!(tokens[2].slice, "/[\\r\\n]/");
    assert_eq!(tokens[3].kind, TokenKind::EOF);
}

#[test]
fn test_lexer_grammar_block() {
    let input = "grammar user:\n    match 'Name:' ws";
    let tokens = lex(input).unwrap();

    // grammar, identifier, colon, newline, indent, match, string, identifier
    assert_eq!(tokens[0].kind, TokenKind::Grammar);
    assert_eq!(tokens[1].kind, TokenKind::Identifier);
    assert_eq!(tokens[1].slice, "user");
    assert_eq!(tokens[2].kind, TokenKind::Colon);
    assert_eq!(tokens[3].kind, TokenKind::Newline);
    assert_eq!(tokens[4].kind, TokenKind::Indent);
    assert_eq!(tokens[5].kind, TokenKind::Match);
    assert_eq!(tokens[6].kind, TokenKind::String);
    // New lexer returns the full slice including quotes for literals.
    assert_eq!(tokens[6].slice, "'Name:'");
}

#[test]
fn test_lexer_comments() {
    let input = "# This is a comment\ndefine test /abc/";
    let tokens = lex(input).unwrap();

    // Kommentare sollten übersprungen werden
    assert_eq!(tokens[0].kind, TokenKind::Newline);
    assert_eq!(tokens[1].kind, TokenKind::Define);
    assert_eq!(tokens[2].kind, TokenKind::Identifier);
}
