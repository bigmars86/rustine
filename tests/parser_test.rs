use rustine::parser::{lexer::lex, syntax::parse_gel_document};

#[test]
fn test_parse_simple_define() {
    let input = "define nl";
    let tokens = lex(input).unwrap();

    // Debug: zeige alle Tokens
    for (i, token) in tokens.iter().enumerate() {
        println!("Token {}: {:?} = '{}'", i, token.kind, token.slice);
    }

    // Versuche zu parsen - erstmal nur schauen ob es läuft
    let result = parse_gel_document(&tokens);
    println!("Parse result: {:?}", result);

    // Für jetzt erwarten wir einen Fehler, da define unvollständig ist
    // assert!(result.is_err());
}

#[test]
fn test_parse_simple_grammar() {
    let input = "grammar test:";
    let tokens = lex(input).unwrap();

    println!("\n=== Grammar Test ===");
    for (i, token) in tokens.iter().enumerate() {
        println!("Token {}: {:?} = '{}'", i, token.kind, token.slice);
    }

    let result = parse_gel_document(&tokens);
    println!("Parse result: {:?}", result);
}

#[test]
fn test_end_to_end_parsing() {
    let input = "define nl /[\\r\\n]/\ngrammar simple:";

    // Komplett von String bis AST
    println!("\n=== End-to-End Test ===");
    println!("Input: '{}'", input);

    let tokens = lex(input).unwrap();
    println!("Tokens: {} found", tokens.len());
    for (i, token) in tokens.iter().enumerate() {
        println!("  Token {}: {:?} = '{}'", i, token.kind, token.slice);
    }

    let ast = parse_gel_document(&tokens);
    println!("AST: {:?}", ast);

    // Das sollte erfolgreich sein - ein define und eine leere grammar
    if let Ok(document) = ast {
        println!(
            "Successfully parsed {} defines and {} grammars",
            document.defines.len(),
            document.grammars.len()
        );
    }
}
