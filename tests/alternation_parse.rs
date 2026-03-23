use rustine::parser::ast::Statement;
use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

#[test]
fn parse_match_alternation() {
    let src = r#"
grammar demo:
    match 'foo' /bar/ | 'baz' 'qux':
        out.open('demo')
"#;
    let tokens = lex(src).unwrap();
    let doc = parse_gel_document(&tokens).unwrap();
    let g = doc.grammars.get("demo").expect("demo grammar");
    assert_eq!(g.statements.len(), 1);
    match &g.statements[0] {
        Statement::Match(m) => {
            assert_eq!(m.match_list.alternatives.len(), 2, "expected two alternatives");
            assert!(m.match_list.alternatives[0].expressions.len() >= 2);
            assert!(m.match_list.alternatives[1].expressions.len() >= 2);
        }
        _ => panic!("not a match statement"),
    }
}
