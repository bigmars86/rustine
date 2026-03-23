//! AST – Abstract Syntax Tree for Gel syntax
//! Represents the parsed Gel document structure

use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;

/// Classification of regex patterns for fast-path dispatch.
///
/// Each variant corresponds to a `memchr`/byte-scanning shortcut in
/// `try_fast_skip`, eliminating runtime string matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastPathKind {
    /// `[^\n]*\n` — skip to (and including) next newline (0+ non-NL)
    SkipToNewline,
    /// `[^\n]+\n` — skip to next newline (1+ non-NL required)
    SkipToNewlinePlus,
    /// `[^\r\n]*[\r\n]` — skip to next CR or LF (0+ non-CRLF)
    SkipToCrLf,
    /// `[^\r\n]+[\r\n]` — skip to next CR or LF (1+ required)
    SkipToCrLfPlus,
    /// `[!#][^\r\n]*[\r\n]` — comment line starting with ! or #
    CommentBangHash,
    /// `#[^\r\n]*[\r\n]+` — comment line # consuming trailing newlines
    CommentHashPlus,
    /// `.*\n` or `.*?\n` — dot-star to newline
    DotStarNewline,
    /// `\s+` — one or more whitespace
    Whitespace,
    /// `\S+` — one or more non-whitespace
    NonWhitespace,
    /// `\d+` — one or more digits
    Digits,
    /// `\w+` — one or more word characters
    WordChars,
    /// `[\t ]+` — horizontal whitespace (tab/space)
    HorizWhitespace,
    /// `[\r\n]+` — one or more CR/LF
    CrLfPlus,
    /// `[\r\n]` — exactly one CR or LF
    CrLf,
    /// `[^\r\n]+` — one or more non-CRLF chars
    NonCrLfPlus,
    /// `[^\r\n]*` — zero or more non-CRLF chars
    NonCrLfStar,
    /// `[^\r\n]` — exactly one non-CRLF char
    NonCrLf,
    /// ` *[\r\n]+` — optional spaces then newlines
    SpacesNewlines,
    /// `\r?\n` — single CRLF-safe line ending (\n or \r\n)
    OptCrNl,
    /// `(?:\r?\n)+` — one or more CRLF-safe line endings
    OptCrNlPlus,
    /// `[^\r\n]*\r?\n` — skip to CRLF-safe line ending (0+ non-CRLF)
    SkipToOptCrNl,
    /// `[^\r\n]+\r?\n` — skip to CRLF-safe line ending (1+ required)
    SkipToOptCrNlPlus,
    /// `[!#][^\r\n]*\r?\n` — comment line starting with ! or # (CRLF-safe)
    CommentBangHashOpt,
    /// `#[^\r\n]*(?:\r?\n)+` — hash comment consuming trailing newlines (CRLF-safe)
    CommentHashPlusOpt,
    /// ` *(?:\r?\n)+` — optional spaces then CRLF-safe newlines
    SpacesOptCrNlPlus,
    /// `\n` — exactly one newline
    Newline,
    /// `.` — exactly one character
    Dot,
    /// Pattern not classified — fall through to regex engine
    #[default]
    None,
}

/// Classify a raw regex pattern string into a [`FastPathKind`].
///
/// Called once during `compile_regexes()` so the runtime hot path
/// dispatches on an integer enum discriminant instead of string matching.
pub fn classify_fast_path(pattern: &str) -> FastPathKind {
    match pattern {
        r"[^\n]*\n" => FastPathKind::SkipToNewline,
        r"[^\n]+\n" => FastPathKind::SkipToNewlinePlus,
        r"[^\r\n]*[\r\n]" => FastPathKind::SkipToCrLf,
        r"[^\r\n]+[\r\n]" => FastPathKind::SkipToCrLfPlus,
        r"[!#][^\r\n]*[\r\n]" => FastPathKind::CommentBangHash,
        r#"#[^\r\n]*[\r\n]+"# => FastPathKind::CommentHashPlus,
        r".*\n" | r".*?\n" => FastPathKind::DotStarNewline,
        r"\s+" => FastPathKind::Whitespace,
        r"\S+" => FastPathKind::NonWhitespace,
        r"\d+" => FastPathKind::Digits,
        r"\w+" => FastPathKind::WordChars,
        r"[\t ]+" => FastPathKind::HorizWhitespace,
        r"[\r\n]+" => FastPathKind::CrLfPlus,
        r"[\r\n]" => FastPathKind::CrLf,
        r"[^\r\n]+" => FastPathKind::NonCrLfPlus,
        r"[^\r\n]*" => FastPathKind::NonCrLfStar,
        r"[^\r\n]" => FastPathKind::NonCrLf,
        r" *[\r\n]+" => FastPathKind::SpacesNewlines,
        r"\r?\n" => FastPathKind::OptCrNl,
        r"(?:\r?\n)+" => FastPathKind::OptCrNlPlus,
        r"[^\r\n]*\r?\n" => FastPathKind::SkipToOptCrNl,
        r"[^\r\n]+\r?\n" => FastPathKind::SkipToOptCrNlPlus,
        r"[!#][^\r\n]*\r?\n" => FastPathKind::CommentBangHashOpt,
        r"#[^\r\n]*(?:\r?\n)+" => FastPathKind::CommentHashPlusOpt,
        r" *(?:\r?\n)+" => FastPathKind::SpacesOptCrNlPlus,
        r"\n" => FastPathKind::Newline,
        "." => FastPathKind::Dot,
        _ => FastPathKind::None,
    }
}

/// Root node: complete Gel document
#[derive(Debug, Clone, Default)]
pub struct GelDocument {
    pub defines: HashMap<String, Expression>,
    pub grammars: HashMap<String, Grammar>,
    pub regex_patterns: Vec<String>,             // unique raw regex patterns
    pub pattern_indices: HashMap<String, usize>, // map raw pattern -> index
    /// Pre-compiled anchored regexes. Indexed by `idx * 2 + case_insensitive as usize`.
    /// Using a flat Vec eliminates HashMap hashing overhead in the inner matching loop.
    pub regex_cache: Vec<Option<Regex>>,
    /// Fast-path classification for each pattern, parallel to `regex_patterns`.
    pub fast_path_kinds: Vec<FastPathKind>,
}

/// Grammar block: `grammar name: statements`
#[derive(Debug, Clone)]
pub struct Grammar {
    pub name: String,
    pub inherit: Option<String>,
    pub statements: Vec<Statement>,
}

/// Different statement kinds
#[derive(Debug, Clone)]
pub enum Statement {
    Match(MatchStatement),
    When(WhenStatement),
    Skip(SkipStatement),
    Action(FunctionCall),
}

/// Match statement: `match pattern: actions`
#[derive(Debug, Clone)]
pub struct MatchStatement {
    pub match_list: MatchList,
    pub actions: Vec<FunctionCall>,
    pub case_insensitive: bool,
}

/// When statement: `when pattern: actions` (similar to match, distinct semantics)
#[derive(Debug, Clone)]
pub struct WhenStatement {
    pub match_list: MatchList,
    pub actions: Vec<FunctionCall>,
}

/// Skip statement: `skip pattern`
#[derive(Debug, Clone)]
pub struct SkipStatement {
    pub pattern: Expression,
}

/// Function call: `function_name(args)`
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub name: Arc<str>,
    pub args: Vec<Expression>,
}

/// Match list: multiple alternative patterns separated by '|'
#[derive(Debug, Clone)]
pub struct MatchList {
    pub alternatives: Vec<MatchFieldList>,
}

/// Match field list: sequence of expressions
#[derive(Debug, Clone)]
pub struct MatchFieldList {
    pub expressions: Vec<Expression>,
    pub flags: u32, // für case-insensitive etc.
    /// Pre-compiled combined regex for the entire field list.
    /// Built by `compile_field_list_regexes()` when all expressions can be
    /// resolved to regex patterns at compile time (no back-references).
    /// Each expression becomes one capture group: `(expr1)(expr2)(expr3)…`
    pub compiled_regex: Option<Regex>,
    /// Literal prefix for fast rejection: if the first expression is a
    /// String literal, store it here so we can do starts_with() before
    /// running any regex at all.
    pub literal_prefix: Option<String>,
}

/// Expression kinds (string, regex, variable, number)
#[derive(Debug, Clone)]
pub enum Expression {
    String(String),
    Regex(String),
    Variable(String),
    Number(i64),
    Capture(usize),
    CaptureName(String),
}

// Actions are represented by FunctionCall entries of Statement::Action.

impl GelDocument {
    /// Assign indices to all regex literal patterns found and precompile both case variants.
    ///
    /// When the `parallel` feature is enabled, regex compilation is parallelized
    /// with rayon (each pattern is independent).
    pub fn compile_regexes(&mut self) {
        use std::collections::HashSet;
        let mut set: HashSet<String> = HashSet::new();
        fn collect(expr: &Expression, out: &mut std::collections::HashSet<String>) {
            if let Expression::Regex(r) = expr {
                out.insert(r.clone());
            }
        }
        for expr in self.defines.values() {
            collect(expr, &mut set);
        }
        for grammar in self.grammars.values() {
            for stmt in &grammar.statements {
                match stmt {
                    Statement::Match(m) => {
                        for alt in &m.match_list.alternatives {
                            for e in &alt.expressions {
                                collect(e, &mut set);
                            }
                        }
                        for act in &m.actions {
                            for arg in &act.args {
                                collect(arg, &mut set);
                            }
                        }
                    }
                    Statement::When(w) => {
                        for alt in &w.match_list.alternatives {
                            for e in &alt.expressions {
                                collect(e, &mut set);
                            }
                        }
                        for act in &w.actions {
                            for arg in &act.args {
                                collect(arg, &mut set);
                            }
                        }
                    }
                    Statement::Skip(s) => collect(&s.pattern, &mut set),
                    Statement::Action(a) => {
                        for arg in &a.args {
                            collect(arg, &mut set);
                        }
                    }
                }
            }
        }
        let mut patterns: Vec<String> = set.into_iter().collect();
        patterns.sort();

        // Filter to only new patterns
        let new_patterns: Vec<(usize, String)> = {
            let mut out = Vec::new();
            for pat in patterns {
                if self.pattern_indices.contains_key(&pat) {
                    continue;
                }
                let idx = self.regex_patterns.len() + out.len();
                out.push((idx, pat));
            }
            out
        };

        if !new_patterns.is_empty() {
            // Register patterns and indices first (sequential, fast)
            for (idx, pat) in &new_patterns {
                self.regex_patterns.push(pat.clone());
                self.pattern_indices.insert(pat.clone(), *idx);
            }

            // Classify patterns for fast-path dispatch
            self.fast_path_kinds
                .resize(self.regex_patterns.len(), FastPathKind::None);
            for (idx, pat) in &new_patterns {
                self.fast_path_kinds[*idx] = classify_fast_path(pat);
            }

            // Compile regexes: each pattern → 2 compiled regexes (case-sensitive + insensitive)
            let compiled = Self::compile_patterns_batch(&new_patterns);

            // Store results
            let max_slot = new_patterns.last().map(|(idx, _)| idx * 2 + 1).unwrap_or(0);
            if max_slot >= self.regex_cache.len() {
                self.regex_cache.resize_with(max_slot + 1, || None);
            }
            for (slot, rx) in compiled {
                self.regex_cache[slot] = rx;
            }
        }

        // Also pre-compile combined field-list regexes for fast matching.
        self.compile_field_list_regexes();

        // Pre-resolve Expression::Variable references so that runtime
        // eval_expression / eval_expression_with_groups never needs to
        // look up the defines HashMap or clone Expressions.
        self.resolve_defines_inline();
    }

    /// Compile a batch of regex patterns, returning (slot, Option<Regex>) pairs.
    ///
    /// When `parallel` feature is enabled, uses rayon for parallel compilation.
    fn compile_patterns_batch(patterns: &[(usize, String)]) -> Vec<(usize, Option<Regex>)> {
        // Build work items: (slot, pattern_string)
        let work: Vec<(usize, String)> = patterns
            .iter()
            .flat_map(|(idx, pat)| {
                [
                    (idx * 2, format!("^(?:{})", pat)),
                    (idx * 2 + 1, format!("(?i)^(?:{})", pat)),
                ]
            })
            .collect();

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            work.into_par_iter()
                .map(|(slot, pattern)| (slot, Regex::new(&pattern).ok()))
                .collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            work.into_iter()
                .map(|(slot, pattern)| (slot, Regex::new(&pattern).ok()))
                .collect()
        }
    }
    /// Ensure a single pattern (raw, without anchors) is indexed and compiled for a case variant.
    pub fn ensure_compiled(&mut self, raw: &str, case_insensitive: bool) -> Result<(), String> {
        let idx = if let Some(i) = self.pattern_indices.get(raw) {
            *i
        } else {
            let i = self.regex_patterns.len();
            self.regex_patterns.push(raw.to_string());
            self.pattern_indices.insert(raw.to_string(), i);
            i
        };
        let slot = idx * 2 + case_insensitive as usize;
        // Grow the Vec to accommodate the new slot
        if slot >= self.regex_cache.len() {
            self.regex_cache.resize_with(slot + 1, || None);
        }
        if self.regex_cache[slot].is_none() {
            let pattern = if case_insensitive {
                format!("(?i)^(?:{})", raw)
            } else {
                format!("^(?:{})", raw)
            };
            match Regex::new(&pattern) {
                Ok(rx) => {
                    self.regex_cache[slot] = Some(rx);
                    Ok(())
                }
                Err(e) => Err(format!("Invalid regex '{}': {}", raw, e)),
            }?
        }
        Ok(())
    }

    /// Pre-resolve all `Expression::Variable(v)` references to their
    /// actual define values (Regex/String/Number) at compile time.
    ///
    /// This eliminates HashMap lookups and Expression clones at runtime.
    /// Only references whose chain resolves to a concrete expression are
    /// replaced; unresolved or cyclic references are left as-is for the
    /// runtime fallback to handle.
    fn resolve_defines_inline(&mut self) {
        let defines = self.defines.clone();
        fn resolve_expr(expr: &mut Expression, defines: &HashMap<String, Expression>) {
            if let Expression::Variable(v) = expr {
                let mut name = v.as_str();
                let mut visited = std::collections::HashSet::new();
                loop {
                    if !visited.insert(name.to_string()) {
                        return; // cycle — leave as Variable
                    }
                    match defines.get(name) {
                        Some(Expression::Variable(next)) => {
                            name = next;
                        }
                        Some(resolved) => {
                            *expr = resolved.clone();
                            return;
                        }
                        None => return, // unresolved — leave as Variable
                    }
                }
            }
        }
        for grammar in self.grammars.values_mut() {
            for stmt in &mut grammar.statements {
                match stmt {
                    Statement::Match(m) => {
                        for alt in &mut m.match_list.alternatives {
                            for e in &mut alt.expressions {
                                resolve_expr(e, &defines);
                            }
                        }
                    }
                    Statement::When(w) => {
                        for alt in &mut w.match_list.alternatives {
                            for e in &mut alt.expressions {
                                resolve_expr(e, &defines);
                            }
                        }
                    }
                    Statement::Skip(s) => {
                        resolve_expr(&mut s.pattern, &defines);
                    }
                    Statement::Action(a) => {
                        for arg in &mut a.args {
                            resolve_expr(arg, &defines);
                        }
                    }
                }
            }
        }
    }

    /// Pre-compile combined regexes for all `MatchFieldList` entries where
    /// every expression resolves to a known regex pattern (no back-references).
    ///
    /// This mirrors Python Gelatin's approach: each field list becomes ONE
    /// regex `(expr1)(expr2)(expr3)…` instead of N separate evaluations.
    ///
    /// When `parallel` feature is enabled, regex compilation is parallelized.
    pub fn compile_field_list_regexes(&mut self) {
        // Snapshot defines for variable resolution (avoids borrow conflicts).
        let defines: HashMap<String, Expression> = self.defines.clone();

        // Phase 1: Extract literal prefixes and collect compilation work items.
        // Each work item is (grammar_index, stmt_index, alt_index, pattern_string).
        let grammar_names: Vec<String> = self.grammars.keys().cloned().collect();
        let mut work: Vec<(String, usize, usize, String)> = Vec::new();

        for gname in &grammar_names {
            let grammar = self.grammars.get_mut(gname).unwrap();
            for (si, stmt) in grammar.statements.iter_mut().enumerate() {
                let field_lists: Vec<&mut MatchFieldList> = match stmt {
                    Statement::Match(m) => m.match_list.alternatives.iter_mut().collect(),
                    Statement::When(w) => w.match_list.alternatives.iter_mut().collect(),
                    _ => continue,
                };
                for (ai, fl) in field_lists.into_iter().enumerate() {
                    // Extract literal prefix (always sequential — cheap)
                    if fl.literal_prefix.is_none() {
                        fl.literal_prefix = Self::extract_literal_prefix(&fl.expressions, &defines);
                    }
                    if fl.compiled_regex.is_some() {
                        continue;
                    }
                    let case_insensitive = fl.flags & 1 != 0;
                    if let Some(combined) = Self::build_combined_pattern(&fl.expressions, &defines) {
                        let anchored = if case_insensitive {
                            format!("(?i)^{}", combined)
                        } else {
                            format!("^{}", combined)
                        };
                        work.push((gname.clone(), si, ai, anchored));
                    }
                }
            }
        }

        if work.is_empty() {
            return;
        }

        // Phase 2: Compile all patterns (parallel when feature enabled).
        let compiled: Vec<(String, usize, usize, Option<Regex>)>;

        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            compiled = work
                .into_par_iter()
                .map(|(gn, si, ai, pat)| {
                    let rx = Regex::new(&pat).ok();
                    (gn, si, ai, rx)
                })
                .collect();
        }
        #[cfg(not(feature = "parallel"))]
        {
            compiled = work
                .into_iter()
                .map(|(gn, si, ai, pat)| {
                    let rx = Regex::new(&pat).ok();
                    (gn, si, ai, rx)
                })
                .collect();
        }

        // Phase 3: Assign compiled regexes back to their field lists.
        for (gname, si, ai, rx) in compiled {
            if let Some(rx) = rx {
                let grammar = self.grammars.get_mut(&gname).unwrap();
                let fl = match &mut grammar.statements[si] {
                    Statement::Match(m) => &mut m.match_list.alternatives[ai],
                    Statement::When(w) => &mut w.match_list.alternatives[ai],
                    _ => continue,
                };
                fl.compiled_regex = Some(rx);
            }
        }
    }

    /// Try to build a combined regex pattern string from a list of expressions.
    /// Returns `None` if any expression cannot be resolved at compile time
    /// (e.g. back-references like `$1`, `$name`).
    fn build_combined_pattern(expressions: &[Expression], defines: &HashMap<String, Expression>) -> Option<String> {
        let mut parts: Vec<String> = Vec::with_capacity(expressions.len());
        for expr in expressions {
            match Self::expr_to_regex_pattern(expr, defines) {
                Some(p) => parts.push(p),
                None => return None, // can't compile this field list
            }
        }
        // Join as separate capture groups: (part1)(part2)(part3)
        Some(parts.into_iter().map(|p| format!("({})", p)).collect::<String>())
    }

    /// Convert a single expression to its regex pattern string (like Python's `re_value()`).
    /// Returns `None` for expressions that depend on runtime state.
    fn expr_to_regex_pattern(expr: &Expression, defines: &HashMap<String, Expression>) -> Option<String> {
        match expr {
            Expression::String(s) => Some(regex::escape(s)),
            Expression::Regex(r) => Some(r.clone()),
            Expression::Number(n) => Some(n.to_string()),
            Expression::Variable(v) => {
                // Resolve the variable through defines (potentially chained).
                let mut name = v.as_str();
                let mut visited = std::collections::HashSet::new();
                loop {
                    if !visited.insert(name.to_string()) {
                        return None; // cycle
                    }
                    match defines.get(name) {
                        Some(resolved) => match resolved {
                            Expression::Variable(next) => {
                                name = next;
                            }
                            other => return Self::expr_to_regex_pattern(other, defines),
                        },
                        None => return None, // undefined variable
                    }
                }
            }
            // Back-references depend on runtime captures — can't pre-compile.
            Expression::Capture(_) | Expression::CaptureName(_) => None,
        }
    }

    /// Extract a literal string prefix from the first expression of a field list.
    /// Used for fast rejection: if input doesn't start with this prefix, skip the regex.
    fn extract_literal_prefix(expressions: &[Expression], defines: &HashMap<String, Expression>) -> Option<String> {
        let first = expressions.first()?;
        match first {
            Expression::String(s) => {
                if !s.is_empty() {
                    Some(s.clone())
                } else {
                    None
                }
            }
            Expression::Variable(v) => {
                // Resolve through defines to see if it's a string
                let mut name = v.as_str();
                let mut visited = std::collections::HashSet::new();
                loop {
                    if !visited.insert(name.to_string()) {
                        return None;
                    }
                    match defines.get(name) {
                        Some(Expression::String(s)) => {
                            return if !s.is_empty() { Some(s.clone()) } else { None };
                        }
                        Some(Expression::Variable(next)) => {
                            name = next;
                        }
                        _ => return None,
                    }
                }
            }
            _ => None,
        }
    }
}
