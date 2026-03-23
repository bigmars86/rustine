//! XML output generator - converts AST to XML

use crate::parser::ast::{Expression, FunctionCall, GelDocument, Grammar, Statement};
use std::fmt::Write;

/// XML generator for Gel documents
#[derive(Debug, Default)]
pub struct XmlGenerator {
    indent_level: usize,
}

impl XmlGenerator {
    pub fn new() -> Self {
        Self { indent_level: 0 }
    }

    /// Main entry: generate XML from GelDocument
    pub fn generate_from_ast(document: &GelDocument) -> String {
        let mut generator = Self::new();
        let mut output = String::new();

        writeln!(output, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>").unwrap();
        writeln!(output, "<gel-document>").unwrap();
        generator.indent_level += 1;

        // Defines section
        if !document.defines.is_empty() {
            generator.write_indent(&mut output);
            writeln!(output, "<defines>").unwrap();
            generator.indent_level += 1;

            for (name, expr) in &document.defines {
                generator.write_indent(&mut output);
                writeln!(output, "<define name=\"{}\">", xml_escape(name)).unwrap();
                generator.indent_level += 1;
                generator.write_expression(&mut output, expr);
                generator.indent_level -= 1;
                generator.write_indent(&mut output);
                writeln!(output, "</define>").unwrap();
            }

            generator.indent_level -= 1;
            generator.write_indent(&mut output);
            writeln!(output, "</defines>").unwrap();
        }

        // Grammars section
        if !document.grammars.is_empty() {
            generator.write_indent(&mut output);
            writeln!(output, "<grammars>").unwrap();
            generator.indent_level += 1;

            for (name, grammar) in &document.grammars {
                generator.write_grammar(&mut output, name, grammar);
            }

            generator.indent_level -= 1;
            generator.write_indent(&mut output);
            writeln!(output, "</grammars>").unwrap();
        }

        writeln!(output, "</gel-document>").unwrap();

        output
    }

    /// Write indentation based on current level
    fn write_indent(&self, output: &mut String) {
        for _ in 0..self.indent_level {
            output.push_str("  ");
        }
    }

    /// Write a grammar as XML
    fn write_grammar(&mut self, output: &mut String, name: &str, grammar: &Grammar) {
        self.write_indent(output);
        if let Some(ref inherit) = grammar.inherit {
            writeln!(
                output,
                "<grammar name=\"{}\" inherit=\"{}\">",
                xml_escape(name),
                xml_escape(inherit)
            )
            .unwrap();
        } else {
            writeln!(output, "<grammar name=\"{}\">", xml_escape(name)).unwrap();
        }

        self.indent_level += 1;

        if !grammar.statements.is_empty() {
            self.write_indent(output);
            writeln!(output, "<statements>").unwrap();
            self.indent_level += 1;

            for statement in &grammar.statements {
                self.write_statement(output, statement);
            }

            self.indent_level -= 1;
            self.write_indent(output);
            writeln!(output, "</statements>").unwrap();
        }

        self.indent_level -= 1;
        self.write_indent(output);
        writeln!(output, "</grammar>").unwrap();
    }

    /// Write a statement as XML
    fn write_statement(&mut self, output: &mut String, statement: &Statement) {
        match statement {
            Statement::Match(match_stmt) => {
                self.write_indent(output);
                writeln!(output, "<match case-insensitive=\"{}\">", match_stmt.case_insensitive).unwrap();
                self.indent_level += 1;
                self.write_match_list(output, &match_stmt.match_list);
                self.write_actions(output, &match_stmt.actions);
                self.indent_level -= 1;
                self.write_indent(output);
                writeln!(output, "</match>").unwrap();
            }
            Statement::When(when_stmt) => {
                self.write_indent(output);
                writeln!(output, "<when>").unwrap();
                self.indent_level += 1;
                self.write_match_list(output, &when_stmt.match_list);
                self.write_actions(output, &when_stmt.actions);
                self.indent_level -= 1;
                self.write_indent(output);
                writeln!(output, "</when>").unwrap();
            }
            Statement::Skip(skip_stmt) => {
                self.write_indent(output);
                writeln!(output, "<skip>").unwrap();
                self.indent_level += 1;
                self.write_expression(output, &skip_stmt.pattern);
                self.indent_level -= 1;
                self.write_indent(output);
                writeln!(output, "</skip>").unwrap();
            }
            Statement::Action(func_call) => {
                self.write_indent(output);
                writeln!(output, "<action name=\"{}\">", xml_escape(&func_call.name)).unwrap();
                self.indent_level += 1;
                if !func_call.args.is_empty() {
                    self.write_indent(output);
                    writeln!(output, "<args>").unwrap();
                    self.indent_level += 1;
                    for arg in &func_call.args {
                        self.write_expression(output, arg);
                    }
                    self.indent_level -= 1;
                    self.write_indent(output);
                    writeln!(output, "</args>").unwrap();
                }
                self.indent_level -= 1;
                self.write_indent(output);
                writeln!(output, "</action>").unwrap();
            }
        }
    }

    /// Write a MatchList as XML
    fn write_match_list(&mut self, output: &mut String, match_list: &crate::parser::ast::MatchList) {
        if !match_list.alternatives.is_empty() {
            self.write_indent(output);
            writeln!(output, "<patterns>").unwrap();
            self.indent_level += 1;

            for field_list in &match_list.alternatives {
                self.write_indent(output);
                writeln!(output, "<alternative flags=\"{}\">", field_list.flags).unwrap();
                self.indent_level += 1;
                for expr in &field_list.expressions {
                    self.write_expression(output, expr);
                }
                self.indent_level -= 1;
                self.write_indent(output);
                writeln!(output, "</alternative>").unwrap();
            }

            self.indent_level -= 1;
            self.write_indent(output);
            writeln!(output, "</patterns>").unwrap();
        }
    }

    /// Write actions as XML
    fn write_actions(&mut self, output: &mut String, actions: &[FunctionCall]) {
        if !actions.is_empty() {
            self.write_indent(output);
            writeln!(output, "<actions>").unwrap();
            self.indent_level += 1;

            for call in actions {
                self.write_action_call(output, call);
            }

            self.indent_level -= 1;
            self.write_indent(output);
            writeln!(output, "</actions>").unwrap();
        }
    }

    /// Write a single action as XML
    fn write_action_call(&mut self, output: &mut String, call: &FunctionCall) {
        self.write_indent(output);
        writeln!(output, "<call name=\"{}\">", xml_escape(&call.name)).unwrap();
        self.indent_level += 1;
        if !call.args.is_empty() {
            self.write_indent(output);
            writeln!(output, "<args>").unwrap();
            self.indent_level += 1;
            for arg in &call.args {
                self.write_expression(output, arg);
            }
            self.indent_level -= 1;
            self.write_indent(output);
            writeln!(output, "</args>").unwrap();
        }
        self.indent_level -= 1;
        self.write_indent(output);
        writeln!(output, "</call>").unwrap();
    }

    /// Write an expression as XML
    fn write_expression(&mut self, output: &mut String, expression: &Expression) {
        self.write_indent(output);
        match expression {
            Expression::String(s) => {
                writeln!(output, "<string>{}</string>", xml_escape(s)).unwrap();
            }
            Expression::Regex(r) => {
                writeln!(output, "<regex>{}</regex>", xml_escape(r)).unwrap();
            }
            Expression::Variable(v) => {
                writeln!(output, "<variable>{}</variable>", xml_escape(v)).unwrap();
            }
            Expression::Number(n) => {
                writeln!(output, "<number>{}</number>", n).unwrap();
            }
            Expression::Capture(i) => {
                writeln!(output, "<capture>{}</capture>", i).unwrap();
            }
            Expression::CaptureName(name) => {
                writeln!(output, "<capture-name>{}</capture-name>", xml_escape(name)).unwrap();
            }
        }
    }
}

/// Escapes XML special characters
fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
