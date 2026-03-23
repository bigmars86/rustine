//! Semantic validation pass – runs after parsing, before execution.
//! Collects `Diagnostic` entries for issues like duplicate grammars,
//! undefined references, and invalid regex patterns.

use crate::errors::Diagnostic;
use crate::parser::ast::{Expression, GelDocument, Statement};
use regex::Regex;
use std::collections::HashSet;

/// Run all semantic checks and return a list of diagnostics.
/// Errors are hard (execution would fail); warnings are advisory.
pub fn validate(doc: &GelDocument) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    check_duplicate_grammars(doc, &mut diags);
    check_undefined_grammar_refs(doc, &mut diags);
    check_undefined_variables(doc, &mut diags);
    check_regex_syntax(doc, &mut diags);
    check_inheritance_targets(doc, &mut diags);
    diags
}

// ── duplicate grammar names ──────────────────────────────────────────
fn check_duplicate_grammars(_doc: &GelDocument, _diags: &mut Vec<Diagnostic>) {
    // The parser stores grammars in a HashMap which silently overwrites
    // duplicates.  Duplicate detection requires span tracking in the parser,
    // which is not implemented yet.  This hook is reserved for that upgrade.
}

// ── undefined grammar references ─────────────────────────────────────
fn check_undefined_grammar_refs(doc: &GelDocument, diags: &mut Vec<Diagnostic>) {
    let known: HashSet<&str> = doc.grammars.keys().map(|s| s.as_str()).collect();
    for grammar in doc.grammars.values() {
        for stmt in &grammar.statements {
            visit_actions(stmt, &mut |fc| {
                // Bare identifier calls without args that aren't known do.* / out.* actions
                // are treated as grammar invocations.
                if fc.args.is_empty()
                    && !fc.name.starts_with("do.")
                    && !fc.name.starts_with("out.")
                    && !known.contains(&*fc.name)
                {
                    diags.push(Diagnostic::warning(
                        format!(
                            "undefined grammar reference '{}' in grammar '{}'",
                            fc.name, grammar.name
                        ),
                        None,
                    ));
                }
            });
        }
    }
}

// ── undefined variable references ────────────────────────────────────
fn check_undefined_variables(doc: &GelDocument, diags: &mut Vec<Diagnostic>) {
    let defined: HashSet<&str> = doc.defines.keys().map(|s| s.as_str()).collect();
    for grammar in doc.grammars.values() {
        for stmt in &grammar.statements {
            visit_expressions(stmt, &mut |expr| {
                if let Expression::Variable(v) = expr {
                    if !defined.contains(v.as_str()) {
                        diags.push(Diagnostic::warning(
                            format!("undefined variable '{}' in grammar '{}'", v, grammar.name),
                            None,
                        ));
                    }
                }
            });
        }
    }
}

// ── regex syntax pre-validation ──────────────────────────────────────
fn check_regex_syntax(doc: &GelDocument, diags: &mut Vec<Diagnostic>) {
    let mut checked: HashSet<String> = HashSet::new();
    // Defines
    for expr in doc.defines.values() {
        if let Expression::Regex(r) = expr {
            check_one_regex(r, &mut checked, diags);
        }
    }
    // Grammars
    for grammar in doc.grammars.values() {
        for stmt in &grammar.statements {
            visit_expressions(stmt, &mut |expr| {
                if let Expression::Regex(r) = expr {
                    check_one_regex(r, &mut checked, diags);
                }
            });
        }
    }
}

fn check_one_regex(raw: &str, checked: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    if checked.contains(raw) {
        return;
    }
    checked.insert(raw.to_string());
    let anchored = format!("^(?:{})", raw);
    if let Err(e) = Regex::new(&anchored) {
        diags.push(Diagnostic::error(format!("invalid regex '{}': {}", raw, e), None));
    }
}

// ── inheritance target validation ────────────────────────────────────
fn check_inheritance_targets(doc: &GelDocument, diags: &mut Vec<Diagnostic>) {
    for grammar in doc.grammars.values() {
        if let Some(parent) = &grammar.inherit {
            if !doc.grammars.contains_key(parent) {
                diags.push(Diagnostic::error(
                    format!(
                        "grammar '{}' inherits from undefined grammar '{}'",
                        grammar.name, parent
                    ),
                    None,
                ));
            }
        }
    }
}

// ── AST visitorsHelper ───────────────────────────────────────────────
fn visit_actions(stmt: &Statement, f: &mut dyn FnMut(&crate::parser::ast::FunctionCall)) {
    match stmt {
        Statement::Match(m) => {
            for a in &m.actions {
                f(a);
            }
        }
        Statement::When(w) => {
            for a in &w.actions {
                f(a);
            }
        }
        Statement::Skip(_) => {}
        Statement::Action(a) => f(a),
    }
}

fn visit_expressions(stmt: &Statement, f: &mut dyn FnMut(&Expression)) {
    match stmt {
        Statement::Match(m) => {
            for alt in &m.match_list.alternatives {
                for e in &alt.expressions {
                    f(e);
                }
            }
            for a in &m.actions {
                for arg in &a.args {
                    f(arg);
                }
            }
        }
        Statement::When(w) => {
            for alt in &w.match_list.alternatives {
                for e in &alt.expressions {
                    f(e);
                }
            }
            for a in &w.actions {
                for arg in &a.args {
                    f(arg);
                }
            }
        }
        Statement::Skip(s) => f(&s.pattern),
        Statement::Action(a) => {
            for arg in &a.args {
                f(arg);
            }
        }
    }
}
