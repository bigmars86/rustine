use rustine::parser::json::JsonGenerator;

#[test]
fn test_json_generator() {
    // Test die Generatoren direkt mit manuell erstelltem AST
    use rustine::parser::ast::{Expression, GelDocument, Grammar};

    println!("\n=== Generator Test ===");

    // Erstelle einen Test-AST
    let mut document = GelDocument::default();

    // Füge ein define hinzu
    document
        .defines
        .insert("nl".to_string(), Expression::Regex("[\\r\\n]".to_string()));
    document
        .defines
        .insert("ws".to_string(), Expression::Regex("\\s+".to_string()));

    // Füge eine Grammar hinzu
    let grammar = Grammar {
        name: "simple".to_string(),
        inherit: None,
        statements: Vec::new(),
    };
    document.grammars.insert("simple".to_string(), grammar);

    println!("✓ Created test AST");
    println!("  - Defines: {}", document.defines.len());
    println!("  - Grammars: {}", document.grammars.len());

    // Test JSON generation
    let json_output = JsonGenerator::generate_from_ast(&document);
    println!("\n=== JSON Output ===");
    println!("{}", json_output);

    // Validate outputs
    assert!(!json_output.is_empty());
    assert!(json_output.contains("defines"));
    assert!(json_output.contains("grammars"));
}

#[test]
fn test_end_to_end_with_generators() {
    // Test mit den echten Parser-Funktionen
    use rustine::parser::{OutputFormat, Parser};

    let input = "define ws /\\s+/\ngrammar test:";

    // Test JSON
    let json_parser = Parser::new(OutputFormat::Json);
    let json_result = json_parser.parse_str(input);
    println!("\n=== End-to-End JSON ===");
    match &json_result {
        Ok(output) => println!("{}", output),
        Err(e) => println!("Error: {:?}", e),
    }
    assert!(json_result.is_ok());
}
