//! YAML output generator - converts AST to YAML
//!
//! NOTE: This is a minimal pretty-ish YAML renderer without external dependencies.
//! It mirrors the structure produced by `JsonGenerator` but emits YAML mapping/sequence syntax.
//! We avoid introducing serde_yaml yet to keep the dependency surface small.

use crate::parser::ast::{Expression, FunctionCall, GelDocument, Grammar, Statement};

#[derive(Debug, Default)]
pub struct YamlGenerator;

impl YamlGenerator {
    pub fn new() -> Self {
        Self
    }

    pub fn generate_from_ast(document: &GelDocument) -> String {
        let gen = Self::new();
        let mut out = String::new();
        // Root document
        out.push_str("gel_document:\n");
        if !document.defines.is_empty() {
            out.push_str("  defines:\n");
            for (name, expr) in &document.defines {
                out.push_str(&format!(
                    "    {}: {{ {} }}\n",
                    escape_key(name),
                    gen.expression_inline(expr)
                ));
            }
        }
        if !document.grammars.is_empty() {
            out.push_str("  grammars:\n");
            for (name, grammar) in &document.grammars {
                out.push_str(&format!("    {}:\n", escape_key(name)));
                gen.grammar_to_yaml(&mut out, grammar, 3);
            }
        }
        out
    }

    fn grammar_to_yaml(&self, out: &mut String, grammar: &Grammar, indent: usize) {
        if let Some(ref inherit) = grammar.inherit {
            indent_line(out, indent, &format!("inherit: {}", escape_scalar(inherit)));
        }
        if !grammar.statements.is_empty() {
            indent_line(out, indent, "statements:");
            for stmt in &grammar.statements {
                self.statement_to_yaml(out, stmt, indent + 1);
            }
        }
    }

    fn statement_to_yaml(&self, out: &mut String, statement: &Statement, indent: usize) {
        match statement {
            Statement::Match(m) => {
                indent_line(out, indent, "- type: match");
                indent_line(out, indent + 1, &format!("case_insensitive: {}", m.case_insensitive));
                self.match_list_to_yaml(out, &m.match_list, indent + 1);
                if !m.actions.is_empty() {
                    indent_line(out, indent + 1, "actions:");
                    for a in &m.actions {
                        self.action_to_yaml(out, a, indent + 2);
                    }
                }
            }
            Statement::When(w) => {
                indent_line(out, indent, "- type: when");
                self.match_list_to_yaml(out, &w.match_list, indent + 1);
                if !w.actions.is_empty() {
                    indent_line(out, indent + 1, "actions:");
                    for a in &w.actions {
                        self.action_to_yaml(out, a, indent + 2);
                    }
                }
            }
            Statement::Skip(s) => {
                indent_line(out, indent, "- type: skip");
                indent_line(
                    out,
                    indent + 1,
                    &format!("pattern: {{ {} }}", self.expression_inline(&s.pattern)),
                );
            }
            Statement::Action(call) => {
                indent_line(out, indent, "- type: action");
                indent_line(out, indent + 1, &format!("name: {}", escape_scalar(&call.name)));
                if !call.args.is_empty() {
                    indent_line(out, indent + 1, "args:");
                    for arg in &call.args {
                        indent_line(out, indent + 2, &format!("- {{ {} }}", self.expression_inline(arg)));
                    }
                }
            }
        }
    }

    fn match_list_to_yaml(&self, out: &mut String, ml: &crate::parser::ast::MatchList, indent: usize) {
        if ml.alternatives.is_empty() {
            return;
        }
        indent_line(out, indent, "patterns:");
        for alt in &ml.alternatives {
            indent_line(out, indent + 1, &format!("- flags: {}", alt.flags));
            if !alt.expressions.is_empty() {
                indent_line(out, indent + 2, "expressions:");
                for expr in &alt.expressions {
                    indent_line(out, indent + 3, &format!("- {{ {} }}", self.expression_inline(expr)));
                }
            }
        }
    }

    fn action_to_yaml(&self, out: &mut String, call: &FunctionCall, indent: usize) {
        indent_line(out, indent, "- call:");
        indent_line(out, indent + 1, &format!("name: {}", escape_scalar(&call.name)));
        if !call.args.is_empty() {
            indent_line(out, indent + 1, "args:");
            for arg in &call.args {
                indent_line(out, indent + 2, &format!("- {{ {} }}", self.expression_inline(arg)));
            }
        }
    }

    fn expression_inline(&self, e: &Expression) -> String {
        match e {
            Expression::String(s) => format!("type: string, value: {}", escape_scalar(s)),
            Expression::Regex(r) => format!("type: regex, value: {}", escape_scalar(r)),
            Expression::Variable(v) => format!("type: variable, value: {}", escape_scalar(v)),
            Expression::Number(n) => format!("type: number, value: {}", n),
            Expression::Capture(i) => format!("type: capture, index: {}", i),
            Expression::CaptureName(name) => format!("type: capture_name, name: {}", escape_scalar(name)),
        }
    }
}

fn indent_line(out: &mut String, indent: usize, line: &str) {
    for _ in 0..indent {
        out.push_str("  ");
    }
    out.push_str(line);
    out.push('\n');
}

fn escape_scalar(s: &str) -> String {
    // If scalar contains special chars or starts/ends with space, quote it.
    let needs_quotes = s
        .chars()
        .any(|c| matches!(c, ':' | '-' | '#' | '{' | '}' | '[' | ']' | ','))
        || s.contains('\n')
        || s.trim() != s;
    if needs_quotes {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn escape_key(s: &str) -> String {
    escape_scalar(s)
}
