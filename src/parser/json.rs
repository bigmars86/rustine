//! JSON output generator - converts AST to JSON

use crate::parser::ast::{Expression, FunctionCall, GelDocument, Grammar, Statement};
use serde_json::{json, Map, Value};

/// JSON generator for Gel documents
#[derive(Debug, Default)]
pub struct JsonGenerator;

impl JsonGenerator {
    pub fn new() -> Self {
        Self
    }

    /// Main entry: generate JSON from GelDocument
    pub fn generate_from_ast(document: &GelDocument) -> String {
        let generator = Self::new();

        // For now build a simple representation of the AST structure
        let mut result = Map::new();

        // Render defines
        if !document.defines.is_empty() {
            let defines: Map<String, Value> = document
                .defines
                .iter()
                .map(|(name, expr)| (name.clone(), generator.expression_to_json(expr)))
                .collect();
            result.insert("defines".to_string(), json!(defines));
        }

        // Render grammars
        if !document.grammars.is_empty() {
            let grammars: Map<String, Value> = document
                .grammars
                .iter()
                .map(|(name, grammar)| (name.clone(), generator.grammar_to_json(grammar)))
                .collect();
            result.insert("grammars".to_string(), json!(grammars));
        }

        serde_json::to_string_pretty(&Value::Object(result)).unwrap_or_else(|_| "{}".to_string())
    }

    /// Convert a grammar node into JSON
    fn grammar_to_json(&self, grammar: &Grammar) -> Value {
        let mut result = Map::new();

        if let Some(ref inherit) = grammar.inherit {
            result.insert("inherit".to_string(), json!(inherit));
        }

        if !grammar.statements.is_empty() {
            let statements: Vec<Value> = grammar
                .statements
                .iter()
                .map(|stmt| self.statement_to_json(stmt))
                .collect();
            result.insert("statements".to_string(), json!(statements));
        }

        Value::Object(result)
    }

    /// Convert a statement into JSON
    fn statement_to_json(&self, statement: &Statement) -> Value {
        match statement {
            Statement::Match(match_stmt) => {
                json!({
                    "type": "match",
                    "case_insensitive": match_stmt.case_insensitive,
                    "patterns": self.match_list_to_json(&match_stmt.match_list),
                    "actions": match_stmt.actions.iter().map(|a| self.action_to_json(a)).collect::<Vec<_>>()
                })
            }
            Statement::When(when_stmt) => {
                json!({
                    "type": "when",
                    "patterns": self.match_list_to_json(&when_stmt.match_list),
                    "actions": when_stmt.actions.iter().map(|a| self.action_to_json(a)).collect::<Vec<_>>()
                })
            }
            Statement::Skip(skip_stmt) => {
                json!({
                    "type": "skip",
                    "pattern": self.expression_to_json(&skip_stmt.pattern)
                })
            }
            Statement::Action(func_call) => {
                json!({
                    "type": "action",
                    "name": &*func_call.name,
                    "args": func_call.args.iter().map(|e| self.expression_to_json(e)).collect::<Vec<_>>()
                })
            }
        }
    }

    /// Convert a MatchList to JSON
    fn match_list_to_json(&self, match_list: &crate::parser::ast::MatchList) -> Value {
        let alternatives: Vec<Value> = match_list
            .alternatives
            .iter()
            .map(|field_list| {
                json!({
                    "expressions": field_list.expressions.iter()
                        .map(|e| self.expression_to_json(e))
                        .collect::<Vec<_>>(),
                    "flags": field_list.flags
                })
            })
            .collect();
        json!(alternatives)
    }

    /// Convert an Expression to JSON
    fn expression_to_json(&self, expression: &Expression) -> Value {
        match expression {
            Expression::String(s) => json!({"type": "string", "value": s}),
            Expression::Regex(r) => json!({"type": "regex", "value": r}),
            Expression::Variable(v) => json!({"type": "variable", "value": v}),
            Expression::Number(n) => json!({"type": "number", "value": n}),
            Expression::Capture(i) => json!({"type": "capture", "index": i}),
            Expression::CaptureName(name) => json!({"type": "capture_name", "name": name}),
        }
    }

    fn action_to_json(&self, call: &FunctionCall) -> Value {
        json!({
            "type": "call",
            "name": &*call.name,
            "args": call.args.iter().map(|e| self.expression_to_json(e)).collect::<Vec<_>>()
        })
    }
}
