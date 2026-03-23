use rustine::parser::lexer::lex;

#[test]
fn debug_lexer_tokens() {
    let input = "define nl";
    let tokens = lex(input).unwrap();

    for (i, token) in tokens.iter().enumerate() {
        println!("Token {}: {:?} = '{}'", i, token.kind, token.slice);
    }

    // This test just prints debug info
    assert!(!tokens.is_empty());
}
