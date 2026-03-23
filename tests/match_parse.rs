use rustine::parser::lexer::lex;
use rustine::parser::syntax::parse_gel_document;

#[test]
fn parse_basic_match_when_skip() {
    let src = r#"
define ws /\s+/
grammar input:
    match 'User:' ws:
        out.open('user')
    when 'User:' ws:
        do.return()
    skip /\n/
"#;
    let tokens = lex(src).unwrap();
    let doc = parse_gel_document(&tokens).unwrap();
    let g = doc.grammars.get("input").expect("grammar input missing");
    assert_eq!(g.statements.len(), 3, "expected 3 statements (match, when, skip)");
    // Check kinds
    match &g.statements[0] {
        rustine::parser::ast::Statement::Match(m) => assert!(m.match_list.alternatives[0].expressions.len() >= 2),
        _ => panic!("first not match"),
    }
    match &g.statements[1] {
        rustine::parser::ast::Statement::When(w) => assert!(w.match_list.alternatives[0].expressions.len() >= 2),
        _ => panic!("second not when"),
    }
    match &g.statements[2] {
        rustine::parser::ast::Statement::Skip(s) => {
            assert!(matches!(s.pattern, rustine::parser::ast::Expression::Regex(_)))
        }
        _ => panic!("third not skip"),
    }
}
