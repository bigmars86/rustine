//! Parser module: public API and re-exports.
pub mod ast;
pub mod core;
pub mod json;
pub mod lexer;
pub mod syntax;
pub mod validate;
pub mod xml;
pub mod yaml;

use crate::errors::GelError;
use crate::exec::{execute, serialize_execution, RuntimeFormat};
use crate::parser::json::JsonGenerator;
use crate::parser::syntax::parse_gel_document;
use crate::parser::xml::XmlGenerator;
use crate::parser::yaml::YamlGenerator;

/// Syntax-level output format for the static parser (AST → string).
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Json,
    Xml,
    Yaml,
}

/// High-level parser that tokenizes Gel source, builds an AST, and
/// serializes it to the selected [`OutputFormat`].
pub struct Parser {
    format: OutputFormat,
}

impl Parser {
    /// Create a new parser targeting the given output format.
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// Parse a Gel source string and return the AST serialized in the chosen format.
    pub fn parse_str(&self, input: &str) -> Result<String, GelError> {
        // Tokenize the input
        let tokens = lexer::lex(input)?;

        // Parse tokens into AST
        let ast = parse_gel_document(&tokens)?;

        // Generate output based on format and AST
        match self.format {
            OutputFormat::Json => Ok(JsonGenerator::generate_from_ast(&ast)),
            OutputFormat::Xml => Ok(XmlGenerator::generate_from_ast(&ast)),
            OutputFormat::Yaml => Ok(YamlGenerator::generate_from_ast(&ast)),
        }
    }

    /// Parse Gel source then run a named grammar against a runtime input string.
    /// Returns JSON containing actions and trace for now.
    pub fn parse_and_run(&self, gel_source: &str, grammar: &str, runtime_input: &str) -> Result<String, GelError> {
        let tokens = lexer::lex(gel_source)?;
        let mut doc = parse_gel_document(&tokens)?;

        // Semantic validation (Phase 3)
        let diags = validate::validate(&doc);
        // Hard-fail on any error-severity diagnostic.
        for d in &diags {
            if d.severity == crate::errors::Severity::Error {
                return Err(GelError::validation(d.message.clone(), d.span.unwrap_or_default()));
            }
        }

        let mut exec_result = execute(&mut doc, grammar, runtime_input)?;
        // Merge validation warnings into execution diagnostics.
        for d in diags {
            exec_result.diagnostics.push(d);
        }
        Ok(serialize_execution(&exec_result, RuntimeFormat::Json))
    }
}
