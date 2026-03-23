use rustine::parser::{lexer::lex, syntax::parse_gel_document};

#[test]
fn debug_parser_step_by_step() {
    let input = "define nl /[\\r\\n]/"; // Nur define, ohne grammar

    println!("\n=== Debug Parser ===");
    println!("Input: '{}'", input);

    let tokens = lex(input).unwrap();
    println!("Tokens:");
    for (i, token) in tokens.iter().enumerate() {
        println!("  {}: {:?} = '{}'", i, token.kind, token.slice);
    }

    let result = parse_gel_document(&tokens);
    println!("Parse result: {:?}", result);

    if let Ok(ast) = result {
        println!(
            "✓ Success: {} defines, {} grammars",
            ast.defines.len(),
            ast.grammars.len()
        );
        for (name, expr) in &ast.defines {
            println!("  Define: {} = {:?}", name, expr);
        }
    }
}

#[test]
fn debug_grammar_only() {
    let input = "grammar simple:";

    println!("\n=== Grammar Only ===");
    println!("Input: '{}'", input);

    let tokens = lex(input).unwrap();
    println!("Tokens:");
    for (i, token) in tokens.iter().enumerate() {
        println!("  {}: {:?} = '{}'", i, token.kind, token.slice);
    }

    let result = parse_gel_document(&tokens);
    println!("Parse result: {:?}", result);
}
