use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

#[test]
fn parse_grammar_inheritance() {
    let src = r#"
grammar base:
    skip /\n/
grammar child(base):
    match 'x':
        do.return()
"#;
    let tokens = lex(src).unwrap();
    let doc = parse_gel_document(&tokens).unwrap();
    let base = doc.grammars.get("base").expect("base grammar");
    assert!(base.inherit.is_none(), "base should have no parent");
    let child = doc.grammars.get("child").expect("child grammar");
    assert_eq!(child.inherit.as_deref(), Some("base"));
    assert_eq!(child.statements.len(), 1);
}
