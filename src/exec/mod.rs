//! Execution engine: runs a parsed GelDocument against an input string.
//!
//! Evaluates match / when / skip statements, fires triggers, and builds
//! the output tree.  Supports regex group captures and `$N` substitution
//! in action arguments.

use crate::errors::{Diagnostic, GelError, Result, Severity};
use crate::exec::out::{ActionExecutor, FlatNode, FlatTree, OutputTree, RuntimeAction};
use crate::parser::ast::{
    Expression, FunctionCall, GelDocument, MatchFieldList, MatchStatement, SkipStatement, Statement, WhenStatement,
};
use regex::Regex;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Optional profiling counters (enabled with `--features profiling`)
// ---------------------------------------------------------------------------
#[cfg(feature = "profiling")]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "profiling")]
macro_rules! prof_inc {
    ($counter:ident) => {
        $counter.fetch_add(1, Ordering::Relaxed);
    };
}
#[cfg(not(feature = "profiling"))]
macro_rules! prof_inc {
    ($counter:ident) => {};
}

#[cfg(feature = "profiling")]
pub static EVAL_FIELD_LIST_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static EVAL_FIELD_LIST_FAST: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static EVAL_FIELD_LIST_FALLBACK: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static EVAL_FIELD_LIST_PREFIX_REJECT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static REGEX_FIND_HIT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static REGEX_FIND_MISS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static EVAL_SKIP_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static EVAL_SKIP_FAST: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static EVAL_EXPR_REGEX: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "profiling")]
pub static GRAMMAR_LOOP_ITERS: AtomicU64 = AtomicU64::new(0);

/// Stack-allocated capture vector: avoids heap allocation for ≤ 8 capture groups
/// (covers the vast majority of practical Gel patterns).
type CaptureVec<'a> = SmallVec<[Cow<'a, str>; 8]>;
/// Stack-allocated named-capture vector.
type NameVec = SmallVec<[Option<Arc<str>>; 8]>;

/// Regex matching only looks at this many bytes of the remaining input.
const SEARCH_WINDOW: usize = 1000;

/// Grammar calls nest at most this deep; a deeper call ends the run with an error.
/// Each level takes about 3 KB of stack in a release build, so 250 levels stay
/// below 1 MB, the default stack of a Windows main thread.
const MAX_GRAMMAR_DEPTH: usize = 250;

/// 1-based line number at the end of `before`.
fn line_number(before: &str) -> usize {
    memchr::memchr_iter(b'\n', before.as_bytes()).count() + 1
}

/// The first [`SEARCH_WINDOW`] bytes of `rem`, extended to the next char boundary.
#[inline]
fn search_window(rem: &str) -> &str {
    let mut end = rem.len().min(SEARCH_WINDOW);
    while !rem.is_char_boundary(end) {
        end += 1;
    }
    &rem[..end]
}

pub mod out;

/// Recursively collect statements from a grammar and its inheritance chain.
///
/// If `grammar C(B)` and `grammar B(A)`, the result is `[A_stmts..., B_stmts..., C_stmts...]`.
/// Cycle detection prevents infinite loops (e.g. `A(B)` + `B(A)`).
pub(crate) fn collect_inherited_statements(doc: &GelDocument, name: &str) -> Vec<Statement> {
    let mut visited = std::collections::HashSet::new();
    collect_inherited_inner(doc, name, &mut visited)
}

fn collect_inherited_inner(
    doc: &GelDocument,
    name: &str,
    visited: &mut std::collections::HashSet<String>,
) -> Vec<Statement> {
    let grammar = match doc.grammars.get(name) {
        Some(g) => g,
        None => return Vec::new(),
    };
    if !visited.insert(name.to_string()) {
        // Cycle detected — stop recursing
        return Vec::new();
    }
    let mut stmts = if let Some(parent) = &grammar.inherit {
        collect_inherited_inner(doc, parent, visited)
    } else {
        Vec::new()
    };
    stmts.extend(grammar.statements.clone());
    stmts
}

/// Result of executing a grammar against runtime input.
///
/// Contains consumed byte count, captured actions, diagnostic traces,
/// the built output tree, and an optional error message.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub consumed: usize,
    pub actions: Vec<FunctionCall>,
    pub traces: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub capture_history: Vec<Vec<String>>,
    pub capture_names_history: Vec<Vec<Option<Arc<str>>>>,
    pub output: OutputTree,
    /// Flattened contiguous tree built from the compacted node arena.
    /// All serialization uses this after `execute_precompiled`.
    pub flat: Option<FlatTree>,
    pub error: Option<String>,
}

impl ExecutionResult {
    /// Create with an arena pre-sized based on expected input size in bytes.
    fn with_capacity_hint(input_bytes: usize) -> Self {
        Self {
            consumed: 0,
            actions: Vec::new(),
            traces: Vec::new(),
            diagnostics: Vec::new(),
            capture_history: Vec::new(),
            capture_names_history: Vec::new(),
            output: OutputTree::with_capacity_hint(input_bytes),
            flat: None,
            error: None,
        }
    }
    /// Push a trace string AND a corresponding Diagnostic.
    fn trace(&mut self, severity: Severity, msg: String) {
        self.traces.push(msg.clone());
        self.diagnostics.push(Diagnostic {
            severity,
            message: msg,
            span: None,
        });
    }
}

/// Mutable execution context holding the document, input text, and current position.
#[derive(Debug)]
pub struct Context<'a> {
    pub doc: &'a GelDocument,
    pub input: &'a str,
    pub pos: usize,
    pub captures: Vec<String>,
}

impl<'a> Context<'a> {
    pub fn new(doc: &'a GelDocument, input: &'a str) -> Self {
        Self {
            doc,
            input,
            pos: 0,
            captures: Vec::new(),
        }
    }
    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }
    fn advance(&mut self, n: usize) {
        self.pos += n;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TriggerAction {
    pub path: String,
    pub value: Option<String>,
}

/// Statement-level execution engine.
///
/// Walks a grammar's statements, evaluates match / when / skip against
/// the remaining input, fires triggers, and builds the output tree.
pub struct Runner<'a> {
    ctx: Context<'a>,
    // single-shot triggers
    trig_before: Vec<(Regex, TriggerAction)>,
    trig_after: Vec<(Regex, TriggerAction)>,
    trig_on_add: Vec<(Regex, TriggerAction)>,
    trig_on_leave: Vec<(Regex, TriggerAction)>,
    // persistent triggers
    trig_before_persist: Vec<(Regex, TriggerAction)>,
    trig_after_persist: Vec<(Regex, TriggerAction)>,
    trig_on_add_persist: Vec<(Regex, TriggerAction)>,
    trig_on_leave_persist: Vec<(Regex, TriggerAction)>,
    last_match_text: Cow<'a, str>,
    last_captures: CaptureVec<'a>,
    last_capture_names: NameVec,
    /// Pre-computed inherited statement lists per grammar.
    /// Wrapped in Arc to allow O(1) sharing with sub-grammar calls
    /// (avoids deep-cloning Statement trees millions of times).
    stmt_cache: HashMap<String, Arc<Vec<Statement>>>,
    /// `do.fail` message from a sub-grammar; ends the run like a top-level `do.fail`.
    fail: Option<String>,
    /// Number of grammar calls in progress (see [`MAX_GRAMMAR_DEPTH`]).
    depth: usize,
    /// Record per-match actions, traces and captures (read by `serialize_execution`).
    record: bool,
}

impl<'a> Runner<'a> {
    pub fn new(doc: &'a GelDocument, input: &'a str) -> Self {
        // Pre-compute collected statements for all grammars, wrapped in Arc for O(1) sharing.
        let mut stmt_cache: HashMap<String, Arc<Vec<Statement>>> = HashMap::new();
        for name in doc.grammars.keys().cloned().collect::<Vec<_>>() {
            let stmts = collect_inherited_statements(doc, &name);
            stmt_cache.insert(name, Arc::new(stmts));
        }
        Self {
            ctx: Context::new(doc, input),
            trig_before: Vec::new(),
            trig_after: Vec::new(),
            trig_on_add: Vec::new(),
            trig_on_leave: Vec::new(),
            trig_before_persist: Vec::new(),
            trig_after_persist: Vec::new(),
            trig_on_add_persist: Vec::new(),
            trig_on_leave_persist: Vec::new(),
            last_match_text: Cow::Borrowed(""),
            last_captures: CaptureVec::new(),
            last_capture_names: NameVec::new(),
            stmt_cache,
            fail: None,
            depth: 0,
            record: true,
        }
    }

    pub fn run_grammar(mut self, name: &str) -> Result<ExecutionResult> {
        let mut result = self.run_statements(name)?;
        if result.error.is_none() {
            result.error = self.fail.take();
        }
        if result.error.is_none() && self.ctx.pos < self.ctx.input.len() {
            result.error = Some(self.unconsumed_input_error(name));
        }
        Ok(result)
    }

    /// The grammar ended (nothing matched, or `do.return`) before the end of the input.
    fn unconsumed_input_error(&self, name: &str) -> String {
        let (done, rest) = self.ctx.input.split_at(self.ctx.pos);
        let next: String = rest.chars().take(40).collect();
        format!(
            "grammar '{name}' stopped at line {} before the end of the input: {next:?}",
            line_number(done)
        )
    }

    fn run_statements(&mut self, name: &str) -> Result<ExecutionResult> {
        if !self.ctx.doc.grammars.contains_key(name) {
            return Err(GelError::runtime(format!("Grammar not found: {}", name), None));
        }
        // Arc::clone is O(1) — just an atomic increment (no deep-cloning Statement trees).
        let statements = self.stmt_cache.get(name).map(Arc::clone).unwrap_or_default();
        let mut result = ExecutionResult::with_capacity_hint(self.ctx.input.len());
        result.output.set_preserve_case(self.ctx.doc.preserve_case);
        loop {
            let start = self.ctx.pos;
            for stmt in statements.iter() {
                match stmt {
                    Statement::Match(m) => {
                        if let Some((len, acts, scope, name_scope)) = self.eval_match(m)? {
                            {
                                let match_text = &self.ctx.input[self.ctx.pos..self.ctx.pos + len];
                                let pos_before = self.ctx.pos;
                                if len > 0 {
                                    self.ctx.advance(len);
                                }
                                self.last_match_text = Cow::Borrowed(match_text);
                                // Skip substitution when no capture references ($N, $name) in args
                                let needs_sub = Self::actions_need_substitution(acts);
                                let substituted_storage;
                                let actions_to_run: &[FunctionCall] = if needs_sub {
                                    substituted_storage = self.substitute_actions(acts, &scope, &name_scope);
                                    &substituted_storage
                                } else {
                                    acts
                                };
                                self.last_captures = scope;
                                self.last_capture_names = name_scope;
                                self.fire_triggers("before", &mut result.output);
                                let (return_levels, did_next, fail_error, auto_leaves) = self.execute_runtime_actions(
                                    actions_to_run,
                                    &mut result.output,
                                    &mut result.traces,
                                );
                                // Track total consumption (match len + sub-grammar advances)
                                let total_consumed = self.ctx.pos - pos_before;
                                result.consumed += total_consumed;
                                // Auto-leave: fire on_leave triggers + pop tree stack for each out.open/out.enter
                                for _ in 0..auto_leaves {
                                    self.fire_triggers("on_leave", &mut result.output);
                                    result.output.leave();
                                }
                                if let Some(err) = fail_error {
                                    result.error = Some(err);
                                }
                                if return_levels > 0 {
                                    return Ok(result);
                                }
                                if did_next {
                                    break;
                                }
                                if self.record {
                                    result.actions.extend(actions_to_run.iter().cloned());
                                    if !self.last_captures.is_empty() {
                                        result
                                            .capture_history
                                            .push(self.last_captures.iter().map(|c| c.to_string()).collect());
                                        result.capture_names_history.push(self.last_capture_names.to_vec());
                                    }
                                    result.trace(Severity::Info, format!("match consumed {} chars", total_consumed));
                                }
                                self.fire_triggers("after", &mut result.output);
                                break; // restart from the first statement
                            }
                        }
                    }
                    Statement::When(w) => {
                        if let Some((scope, name_scope)) = self.eval_when(w)? {
                            let needs_sub = Self::actions_need_substitution(&w.actions);
                            let substituted_storage;
                            let actions_to_run: &[FunctionCall] = if needs_sub {
                                substituted_storage = self.substitute_actions(&w.actions, &scope, &name_scope);
                                &substituted_storage
                            } else {
                                &w.actions
                            };
                            self.last_match_text = Cow::Borrowed("");
                            self.last_captures = scope;
                            self.last_capture_names = name_scope;
                            self.fire_triggers("before", &mut result.output);
                            let (return_levels, did_next, fail_error, auto_leaves) =
                                self.execute_runtime_actions(actions_to_run, &mut result.output, &mut result.traces);
                            for _ in 0..auto_leaves {
                                self.fire_triggers("on_leave", &mut result.output);
                                result.output.leave();
                            }
                            if let Some(err) = fail_error {
                                result.error = Some(err);
                            }
                            if return_levels > 0 {
                                return Ok(result);
                            }
                            if did_next {
                                break;
                            }
                            if self.record {
                                result.trace(Severity::Info, "when triggered".to_string());
                            }
                            self.fire_triggers("after", &mut result.output);
                            break; // restart from the first statement
                        }
                    }
                    Statement::Skip(s) => {
                        if let Some(len) = self.eval_skip(s)? {
                            if len > 0 {
                                self.ctx.advance(len);
                                result.consumed += len;
                                break; /* break and restart after skip */
                            }
                        }
                    }
                    Statement::Action(a) => {
                        let pos_before_action = self.ctx.pos;
                        // Fast path: direct grammar dispatch (avoids substitute_action + execute_runtime_actions overhead).
                        if !a.name.contains('.') && self.stmt_cache.contains_key(&*a.name) {
                            let (sub_consumed, remaining_levels) =
                                self.run_inline_grammar(&a.name, &mut result.output, &mut result.traces);
                            if remaining_levels > 0 {
                                result.consumed += self.ctx.pos - pos_before_action;
                                return Ok(result);
                            }
                            if sub_consumed > 0 {
                                result.consumed += sub_consumed;
                                break; // restart statement loop
                            }
                            // Grammar didn't match — try next statement
                        } else {
                            // Non-grammar action (do.return, out.add, etc.)
                            let substituted = self.substitute_action(a, &[], &[]);
                            let (return_levels, did_next, fail_error, auto_leaves) = self.execute_runtime_actions(
                                std::slice::from_ref(&substituted),
                                &mut result.output,
                                &mut result.traces,
                            );
                            for _ in 0..auto_leaves {
                                self.fire_triggers("on_leave", &mut result.output);
                                result.output.leave();
                            }
                            if let Some(err) = fail_error {
                                result.error = Some(err);
                            }
                            if return_levels > 0 {
                                return Ok(result);
                            }
                            if did_next {
                                break;
                            }
                            let action_consumed = self.ctx.pos - pos_before_action;
                            if action_consumed > 0 {
                                result.consumed += action_consumed;
                                break; // restart statement loop
                            }
                        }
                    }
                }
            }
            if self.ctx.pos >= self.ctx.input.len() {
                break;
            }
            if self.ctx.pos == start {
                result.trace(
                    Severity::Warning,
                    format!("no match found in grammar {}, pos {}", name, self.ctx.pos),
                );
                break;
            }
        }
        Ok(result)
    }

    #[allow(clippy::type_complexity)]
    fn eval_match<'m>(
        &mut self,
        m: &'m MatchStatement,
    ) -> Result<Option<(usize, &'m [FunctionCall], CaptureVec<'a>, NameVec)>> {
        for alt in &m.match_list.alternatives {
            if let Some((len, groups, names)) = self.eval_field_list(alt, m.case_insensitive)? {
                return Ok(Some((len, &m.actions, groups, names)));
            }
        }
        Ok(None)
    }
    fn eval_when(&mut self, w: &WhenStatement) -> Result<Option<(CaptureVec<'a>, NameVec)>> {
        for alt in &w.match_list.alternatives {
            if let Some((_, groups, names)) = self.eval_field_list(alt, false)? {
                return Ok(Some((groups, names)));
            }
        }
        Ok(None)
    }
    fn eval_skip(&mut self, s: &SkipStatement) -> Result<Option<usize>> {
        prof_inc!(EVAL_SKIP_CALLS);
        let rem = self.ctx.remaining();
        // Fast path: use memchr for common skip patterns to avoid regex overhead.
        // Resolve variables to get the underlying regex pattern.
        let resolved = match &s.pattern {
            Expression::Regex(r) => Some(r.as_str()),
            Expression::Variable(v) => match self.resolve_variable(v) {
                Some(Expression::Regex(r)) => Some(r.as_str()),
                _ => None,
            },
            _ => None,
        };
        if let Some(pattern) = resolved {
            // Look up pre-classified FastPathKind (O(1) enum dispatch)
            let kind = self
                .ctx
                .doc
                .pattern_indices
                .get(pattern)
                .and_then(|&idx| self.ctx.doc.fast_path_kinds.get(idx).copied())
                .unwrap_or(crate::parser::ast::FastPathKind::None);
            if kind != crate::parser::ast::FastPathKind::None {
                let n = Self::try_fast_skip_kind(kind, rem);
                if fast_skip_decides(kind, rem, n) {
                    if n.is_some() {
                        prof_inc!(EVAL_SKIP_FAST);
                    }
                    return Ok(n);
                }
            }
        }
        self.eval_expression(&s.pattern, rem, false, &[], &[])
            .map(|o| o.map(|(n, _)| n))
    }

    /// Attempt to match common skip regex patterns using memchr/byte scanning
    /// instead of the full regex engine.  Returns `Some(consumed)` on a fast-path
    /// hit and `None` if the pattern does not match; [`fast_skip_decides`] tells
    /// when non-ASCII text needs the regex instead.
    ///
    /// Dispatches on a pre-classified [`FastPathKind`] enum (integer match)
    /// instead of comparing pattern strings at runtime.
    pub(crate) fn try_fast_skip_kind(kind: crate::parser::ast::FastPathKind, rem: &str) -> Option<usize> {
        use crate::parser::ast::FastPathKind;
        let bytes = rem.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        match kind {
            FastPathKind::SkipToNewline => memchr::memchr(b'\n', bytes).map(|pos| pos + 1),
            FastPathKind::SkipToNewlinePlus => {
                if let Some(pos) = memchr::memchr(b'\n', bytes) {
                    if pos >= 1 {
                        Some(pos + 1)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            FastPathKind::SkipToCrLf => memchr::memchr2(b'\r', b'\n', bytes).map(|pos| pos + 1),
            FastPathKind::SkipToCrLfPlus => {
                if let Some(pos) = memchr::memchr2(b'\r', b'\n', bytes) {
                    if pos >= 1 {
                        Some(pos + 1)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            FastPathKind::CommentBangHash => {
                if bytes[0] == b'!' || bytes[0] == b'#' {
                    if let Some(pos) = memchr::memchr2(b'\r', b'\n', &bytes[1..]) {
                        return Some(1 + pos + 1);
                    }
                }
                None
            }
            FastPathKind::CommentHashPlus => {
                if bytes[0] == b'#' {
                    if let Some(pos) = memchr::memchr2(b'\r', b'\n', &bytes[1..]) {
                        let line_end = 1 + pos;
                        let trailing = bytes[line_end..]
                            .iter()
                            .take_while(|&&b| b == b'\r' || b == b'\n')
                            .count();
                        if trailing > 0 {
                            return Some(line_end + trailing);
                        }
                    }
                }
                None
            }
            FastPathKind::DotStarNewline => memchr::memchr(b'\n', bytes).map(|pos| pos + 1),
            FastPathKind::Whitespace => {
                let count = bytes.iter().take_while(|&&b| is_regex_space(b)).count();
                if count > 0 {
                    Some(count)
                } else {
                    None
                }
            }
            FastPathKind::NonWhitespace => {
                let count = bytes.iter().take_while(|&&b| !is_regex_space(b)).count();
                if count > 0 {
                    Some(count)
                } else {
                    None
                }
            }
            FastPathKind::Digits => {
                let count = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
                if count > 0 {
                    Some(count)
                } else {
                    None
                }
            }
            FastPathKind::WordChars => {
                let count = bytes
                    .iter()
                    .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
                    .count();
                if count > 0 {
                    Some(count)
                } else {
                    None
                }
            }
            FastPathKind::HorizWhitespace => {
                let count = bytes.iter().take_while(|&&b| b == b'\t' || b == b' ').count();
                if count > 0 {
                    Some(count)
                } else {
                    None
                }
            }
            FastPathKind::CrLfPlus => {
                let count = bytes.iter().take_while(|&&b| b == b'\r' || b == b'\n').count();
                if count > 0 {
                    Some(count)
                } else {
                    None
                }
            }
            FastPathKind::CrLf => {
                if bytes[0] == b'\r' || bytes[0] == b'\n' {
                    Some(1)
                } else {
                    None
                }
            }
            FastPathKind::NonCrLfPlus => {
                if let Some(pos) = memchr::memchr2(b'\r', b'\n', bytes) {
                    if pos > 0 {
                        Some(pos)
                    } else {
                        None
                    }
                } else if !bytes.is_empty() {
                    Some(bytes.len())
                } else {
                    None
                }
            }
            FastPathKind::NonCrLfStar => {
                if let Some(pos) = memchr::memchr2(b'\r', b'\n', bytes) {
                    Some(pos)
                } else {
                    Some(bytes.len())
                }
            }
            FastPathKind::NonCrLf => rem
                .chars()
                .next()
                .filter(|&c| c != '\r' && c != '\n')
                .map(char::len_utf8),
            FastPathKind::SpacesNewlines => {
                let spaces = bytes.iter().take_while(|&&b| b == b' ').count();
                let newlines = bytes[spaces..]
                    .iter()
                    .take_while(|&&b| b == b'\r' || b == b'\n')
                    .count();
                if newlines > 0 {
                    Some(spaces + newlines)
                } else {
                    None
                }
            }
            // ── CRLF-safe variants (\r?\n treated as single line ending) ──
            FastPathKind::OptCrNl => {
                // \r?\n — match \n (1 byte) or \r\n (2 bytes)
                if bytes[0] == b'\n' {
                    Some(1)
                } else if bytes[0] == b'\r' && bytes.len() > 1 && bytes[1] == b'\n' {
                    Some(2)
                } else {
                    None
                }
            }
            FastPathKind::OptCrNlPlus => {
                // (?:\r?\n)+ — one or more \n or \r\n sequences
                let mut pos = 0;
                let mut count = 0usize;
                while pos < bytes.len() {
                    if bytes[pos] == b'\n' {
                        pos += 1;
                        count += 1;
                    } else if bytes[pos] == b'\r' && pos + 1 < bytes.len() && bytes[pos + 1] == b'\n' {
                        pos += 2;
                        count += 1;
                    } else {
                        break;
                    }
                }
                if count > 0 {
                    Some(pos)
                } else {
                    None
                }
            }
            FastPathKind::SkipToOptCrNl => {
                // [^\r\n]*\r?\n — skip non-CRLF chars then consume one \n or \r\n
                memchr::memchr(b'\n', bytes).map(|nl_pos| nl_pos + 1)
            }
            FastPathKind::SkipToOptCrNlPlus => {
                // [^\r\n]+\r?\n — skip 1+ non-CRLF chars then consume one \n or \r\n
                if let Some(nl_pos) = memchr::memchr(b'\n', bytes) {
                    // Need at least 1 non-CRLF char before the line ending
                    let content_end = if nl_pos > 0 && bytes[nl_pos - 1] == b'\r' {
                        nl_pos - 1
                    } else {
                        nl_pos
                    };
                    if content_end >= 1 {
                        Some(nl_pos + 1)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            FastPathKind::CommentBangHashOpt => {
                // [!#][^\r\n]*\r?\n — comment starting with ! or #, to line ending
                if bytes[0] == b'!' || bytes[0] == b'#' {
                    if let Some(nl_pos) = memchr::memchr(b'\n', &bytes[1..]) {
                        return Some(1 + nl_pos + 1);
                    }
                }
                None
            }
            FastPathKind::CommentHashPlusOpt => {
                // #[^\r\n]*(?:\r?\n)+ — hash comment consuming trailing line endings
                if bytes[0] == b'#' {
                    if let Some(nl_pos) = memchr::memchr(b'\n', &bytes[1..]) {
                        let mut pos = 1 + nl_pos + 1; // past the first \n
                                                      // consume additional \r?\n sequences
                        while pos < bytes.len() {
                            if bytes[pos] == b'\n' {
                                pos += 1;
                            } else if bytes[pos] == b'\r' && pos + 1 < bytes.len() && bytes[pos + 1] == b'\n' {
                                pos += 2;
                            } else {
                                break;
                            }
                        }
                        return Some(pos);
                    }
                }
                None
            }
            FastPathKind::SpacesOptCrNlPlus => {
                // *(?:\r?\n)+ — optional spaces then one-or-more CRLF-safe newlines
                let spaces = bytes.iter().take_while(|&&b| b == b' ').count();
                let rest = &bytes[spaces..];
                let mut pos = 0;
                let mut count = 0usize;
                while pos < rest.len() {
                    if rest[pos] == b'\n' {
                        pos += 1;
                        count += 1;
                    } else if rest[pos] == b'\r' && pos + 1 < rest.len() && rest[pos + 1] == b'\n' {
                        pos += 2;
                        count += 1;
                    } else {
                        break;
                    }
                }
                if count > 0 {
                    Some(spaces + pos)
                } else {
                    None
                }
            }
            FastPathKind::Newline => {
                if bytes[0] == b'\n' {
                    Some(1)
                } else {
                    None
                }
            }
            FastPathKind::Dot => rem.chars().next().filter(|&c| c != '\n').map(char::len_utf8),
            FastPathKind::None => None,
        }
    }

    #[allow(clippy::type_complexity)]
    fn eval_field_list(
        &mut self,
        list: &MatchFieldList,
        case_insensitive: bool,
    ) -> Result<Option<(usize, CaptureVec<'a>, NameVec)>> {
        prof_inc!(EVAL_FIELD_LIST_CALLS);
        // Ultra-fast literal prefix rejection: if the first expression is a
        // string literal, check starts_with() before any regex work.
        if let Some(prefix) = &list.literal_prefix {
            let rem = self.ctx.remaining();
            let ci = case_insensitive;
            let reject = if ci {
                !rem.get(..prefix.len())
                    .map(|s| s.eq_ignore_ascii_case(prefix))
                    .unwrap_or(false)
            } else {
                !rem.starts_with(prefix.as_str())
            };
            if reject {
                prof_inc!(EVAL_FIELD_LIST_PREFIX_REJECT);
                return Ok(None);
            }
        }
        // Combined regex: one captures() search when groups are needed, else find()
        if let Some(combined) = &list.compiled_regex {
            prof_inc!(EVAL_FIELD_LIST_FAST);
            let rem = self.ctx.remaining();
            let window = search_window(rem);
            if combined.captures_len() > 1 {
                if let Some(mat) = combined.captures(window) {
                    let consumed = mat.get(0).map_or(0, |m| m.end());
                    if mat.get(0).is_some_and(|m| m.start() == 0) {
                        prof_inc!(REGEX_FIND_HIT);
                        let mut all_captures: CaptureVec<'a> = CaptureVec::new();
                        let mut all_names: NameVec = NameVec::new();
                        for gi in 1..mat.len() {
                            let text: Cow<'a, str> = mat
                                .get(gi)
                                .map(|g| Cow::Borrowed(&rem[g.start()..g.end()]))
                                .unwrap_or(Cow::Borrowed(""));
                            all_captures.push(text);
                            all_names.push(None);
                        }
                        return Ok(Some((consumed, all_captures, all_names)));
                    }
                }
            } else if let Some(m) = combined.find(window) {
                if m.start() == 0 {
                    prof_inc!(REGEX_FIND_HIT);
                    let consumed = m.end();
                    let all_captures: CaptureVec<'a> = smallvec::smallvec![Cow::Borrowed(&rem[..consumed])];
                    let all_names: NameVec = smallvec::smallvec![None];
                    return Ok(Some((consumed, all_captures, all_names)));
                }
            }
            prof_inc!(REGEX_FIND_MISS);
            return Ok(None);
        }

        prof_inc!(EVAL_FIELD_LIST_FALLBACK);
        // Fallback: per-expression evaluation (handles back-references, Variables not in defines, etc.)
        let mut offset = 0usize;
        let mut rem: &'a str = self.ctx.remaining();
        let mut all_captures: CaptureVec<'a> = CaptureVec::new();
        let mut all_names: NameVec = NameVec::new();
        for expr in &list.expressions {
            match self.eval_expression_with_groups(expr, rem, case_insensitive, &all_captures, &all_names)? {
                Some((consumed, groups)) => {
                    if consumed > rem.len() {
                        return Ok(None);
                    }
                    offset += consumed;
                    rem = &rem[consumed..];
                    for (text, name) in groups {
                        all_captures.push(text);
                        all_names.push(name);
                    }
                }
                None => return Ok(None),
            }
        }
        Ok(Some((offset, all_captures, all_names)))
    }

    /// Evaluate an expression and return all capture groups (including inner regex groups).
    /// Returns (bytes_consumed, Vec<(captured_text, group_name)>) or None.
    /// Capture text uses `Cow::Borrowed` for regex matches (zero-copy from input).
    #[allow(clippy::type_complexity)]
    fn eval_expression_with_groups(
        &mut self,
        expr: &Expression,
        rem: &'a str,
        case_insensitive: bool,
        captures_so_far: &[Cow<'a, str>],
        names_so_far: &[Option<Arc<str>>],
    ) -> Result<Option<(usize, Vec<(Cow<'a, str>, Option<Arc<str>>)>)>> {
        match expr {
            Expression::Regex(r) => {
                // All regexes are pre-compiled in compile_regexes(); skip ensure_compiled overhead.
                let idx = match self.ctx.doc.pattern_indices.get(r.as_str()) {
                    Some(&i) => i,
                    None => return Ok(None), // unknown pattern → no match
                };
                let slot = idx * 2 + case_insensitive as usize;
                let compiled = match self.ctx.doc.regex_cache.get(slot).and_then(|o| o.as_ref()) {
                    Some(rx) => rx,
                    None => return Ok(None),
                };
                // Two-phase: fast find() first (DFA), only compute captures on hit.
                // Window the input to avoid scanning the full remaining buffer.
                let window = search_window(rem);
                prof_inc!(EVAL_EXPR_REGEX);
                if let Some(m) = compiled.find(window) {
                    if m.start() == 0 {
                        let consumed = m.end();
                        let num_groups = compiled.captures_len();
                        let mut groups = Vec::with_capacity(num_groups);
                        // Check if the pattern has inner capture groups
                        if num_groups > 1 {
                            // Need full captures for inner groups
                            if let Some(mat) = compiled.captures(window) {
                                groups.push((Cow::Borrowed(&rem[..mat.get(0).unwrap().end()]), None));
                                let group_names: Vec<Option<&str>> = compiled.capture_names().collect();
                                for gi in 1..mat.len() {
                                    let text: Cow<'a, str> = mat
                                        .get(gi)
                                        .map(|g| Cow::Borrowed(&rem[g.start()..g.end()]))
                                        .unwrap_or(Cow::Borrowed(""));
                                    let name = group_names.get(gi).and_then(|n| *n).map(Arc::from);
                                    groups.push((text, name));
                                }
                            }
                        } else {
                            // No inner groups — just group 0 (the full match)
                            groups.push((Cow::Borrowed(&rem[..consumed]), None));
                        }
                        return Ok(Some((consumed, groups)));
                    }
                }
                Ok(None)
            }
            Expression::String(s) => {
                let matched = if case_insensitive {
                    let end = rem.len().min(s.len());
                    if rem.get(..end).map(|sl| sl.eq_ignore_ascii_case(s)).unwrap_or(false) {
                        Some(s.len())
                    } else {
                        None
                    }
                } else if rem.starts_with(s.as_str()) {
                    Some(s.len())
                } else {
                    None
                };
                Ok(matched.map(|len| (len, vec![(Cow::Owned(s.clone()), None)])))
            }
            Expression::Number(n) => {
                let s = n.to_string();
                if rem.starts_with(&s) {
                    Ok(Some((s.len(), vec![(Cow::Owned(s), None)])))
                } else {
                    Ok(None)
                }
            }
            Expression::Variable(v) => {
                if let Some(resolved) = self.resolve_variable(v) {
                    let cloned = resolved.clone();
                    self.eval_expression_with_groups(&cloned, rem, case_insensitive, captures_so_far, names_so_far)
                } else {
                    Ok(None)
                }
            }
            Expression::Capture(idx) => {
                if let Some(val) = captures_so_far.get(*idx) {
                    let len = val.len();
                    if let Some(slice) = rem.get(..len) {
                        if (case_insensitive && slice.eq_ignore_ascii_case(val)) || slice == val.as_ref() {
                            return Ok(Some((len, vec![(val.clone(), None)])));
                        }
                    }
                }
                Ok(None)
            }
            Expression::CaptureName(name) => {
                // Look up the named capture in captures accumulated so far
                if let Some((i, _)) = names_so_far
                    .iter()
                    .enumerate()
                    .find(|(_, n)| n.as_deref() == Some(name.as_str()))
                {
                    if let Some(val) = captures_so_far.get(i) {
                        let len = val.len();
                        if let Some(slice) = rem.get(..len) {
                            if (case_insensitive && slice.eq_ignore_ascii_case(val)) || slice == val.as_ref() {
                                return Ok(Some((len, vec![(val.clone(), None)])));
                            }
                        }
                    }
                }
                Ok(None)
            }
        }
    }
    fn resolve_variable(&self, name: &str) -> Option<&Expression> {
        self.ctx.doc.defines.get(name)
    }
    /// Simple expression evaluator (used by eval_skip). Returns (consumed, matched_text).
    fn eval_expression(
        &mut self,
        expr: &Expression,
        rem: &str,
        case_insensitive: bool,
        groups_so_far: &[String],
        names_so_far: &[Option<Arc<str>>],
    ) -> Result<Option<(usize, String)>> {
        match expr {
            Expression::String(s) => {
                let matched = if case_insensitive {
                    let end = rem.len().min(s.len());
                    if rem.get(..end).map(|sl| sl.eq_ignore_ascii_case(s)).unwrap_or(false) {
                        Some(s.len())
                    } else {
                        None
                    }
                } else if rem.starts_with(s.as_str()) {
                    Some(s.len())
                } else {
                    None
                };
                Ok(matched.map(|len| (len, s.clone())))
            }
            Expression::Regex(r) => {
                // All regexes pre-compiled; skip ensure_compiled overhead.
                let idx = match self.ctx.doc.pattern_indices.get(r.as_str()) {
                    Some(&i) => i,
                    None => return Ok(None),
                };
                let slot = idx * 2 + case_insensitive as usize;
                let compiled = match self.ctx.doc.regex_cache.get(slot).and_then(|o| o.as_ref()) {
                    Some(rx) => rx,
                    None => return Ok(None),
                };
                if let Some(m) = compiled.find(search_window(rem)) {
                    if m.start() == 0 {
                        return Ok(Some((m.end(), rem[..m.end()].to_string())));
                    }
                }
                Ok(None)
            }
            Expression::Number(n) => {
                let s = n.to_string();
                if rem.starts_with(&s) {
                    Ok(Some((s.len(), s)))
                } else {
                    Ok(None)
                }
            }
            Expression::Variable(v) => {
                if let Some(resolved) = self.resolve_variable(v) {
                    let cloned = resolved.clone();
                    self.eval_expression(&cloned, rem, case_insensitive, groups_so_far, &[])
                } else {
                    Ok(None)
                }
            }
            Expression::Capture(idx) => {
                if let Some(val) = groups_so_far.get(*idx) {
                    let len = val.len();
                    if let Some(slice) = rem.get(..len) {
                        if (case_insensitive && slice.eq_ignore_ascii_case(val)) || slice == val {
                            return Ok(Some((len, val.clone())));
                        }
                    }
                }
                Ok(None)
            }
            Expression::CaptureName(name) => {
                // Look up the named capture by name
                if let Some((i, _)) = names_so_far
                    .iter()
                    .enumerate()
                    .find(|(_, n)| n.as_deref() == Some(name.as_str()))
                {
                    if let Some(val) = groups_so_far.get(i) {
                        let len = val.len();
                        if let Some(slice) = rem.get(..len) {
                            if (case_insensitive && slice.eq_ignore_ascii_case(val)) || slice == val {
                                return Ok(Some((len, val.clone())));
                            }
                        }
                    }
                }
                Ok(None)
            }
        }
    }

    fn substitute_actions(
        &self,
        actions: &[FunctionCall],
        scope: &[Cow<'a, str>],
        name_scope: &[Option<Arc<str>>],
    ) -> Vec<FunctionCall> {
        actions
            .iter()
            .map(|a| self.substitute_action(a, scope, name_scope))
            .collect()
    }
    /// Check whether any action in the slice references capture groups ($N, $name).
    /// When false, `substitute_actions` can be skipped entirely.
    #[inline]
    fn actions_need_substitution(actions: &[FunctionCall]) -> bool {
        actions.iter().any(|a| {
            a.args
                .iter()
                .any(|arg| matches!(arg, Expression::Capture(_) | Expression::CaptureName(_)))
        })
    }
    fn substitute_action(
        &self,
        action: &FunctionCall,
        scope: &[Cow<'a, str>],
        name_scope: &[Option<Arc<str>>],
    ) -> FunctionCall {
        let mut new_args = Vec::with_capacity(action.args.len());
        for arg in &action.args {
            let replaced = match arg {
                Expression::Capture(i) => scope
                    .get(*i)
                    .map(|v| Expression::String(crate::exec::out::percent_encode(v)))
                    .unwrap_or(Expression::String(String::new())),
                Expression::CaptureName(n) => {
                    if let Some((i, _)) = name_scope.iter().enumerate().find(|(_, s)| s.as_deref() == Some(&**n)) {
                        scope
                            .get(i)
                            .map(|v| Expression::String(crate::exec::out::percent_encode(v)))
                            .unwrap_or(Expression::String(String::new()))
                    } else {
                        Expression::String(String::new())
                    }
                }
                _ => arg.clone(),
            };
            new_args.push(replaced);
        }
        FunctionCall {
            name: action.name.clone(),
            args: new_args,
        }
    }

    // Returns (return_levels, did_next, fail_error, pending_auto_leaves)
    // return_levels: 0 = no return, >0 = leave N grammar levels
    fn execute_runtime_actions(
        &mut self,
        actions: &[FunctionCall],
        tree: &mut OutputTree,
        traces: &mut Vec<String>,
    ) -> (i32, bool, Option<String>, usize) {
        let mut skip_rest = false; // do.skip
        let mut return_levels: i32 = 0; // do.return(levels)
        let mut do_next = false; // do.next
        let mut fail_error: Option<String> = None;
        let mut last_action_name: Option<&str> = None;
        let mut pending_auto_leaves: usize = 0; // count of out.open/out.enter calls needing auto-leave on statement exit
        for act in actions {
            if skip_rest || return_levels > 0 || do_next {
                break;
            }
            // Track previous action name early so inline grammar branch can see it
            let current_name: &str = &act.name;
            // Inline grammar invocation: name matches a grammar defined in document (and not a built-in action prefix)
            if !act.name.contains('.') && self.ctx.doc.grammars.contains_key(&*act.name) {
                let (sub_consumed, _remaining_levels) = self.run_inline_grammar(&act.name, tree, traces);
                if sub_consumed > 0 && self.record {
                    traces.push(format!("subgrammar {} consumed {} chars", act.name, sub_consumed));
                }
                if let Some(prev) = &last_action_name {
                    if *prev == "out.open" {
                        tree.leave();
                        // The out.open incremented pending_auto_leaves; consuming
                        // the leave here means the caller must not fire it again.
                        pending_auto_leaves = pending_auto_leaves.saturating_sub(1);
                    }
                }
                // The sub-grammar's return level is ignored; if it consumed input,
                // the remaining actions (e.g. do.return()) are skipped.
                if sub_consumed > 0 {
                    break;
                }
                last_action_name = Some(current_name);
                continue;
            }
            if &*act.name == "out.create" {
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    let value = act.args.get(1).map(|v| self.expr_to_string(v));
                    let value_ref = value.as_deref();
                    tree.exec(RuntimeAction::OutCreate {
                        path: &path,
                        value: value_ref,
                    });
                    if self.has_on_add_triggers() && names_node(&path) {
                        tree.exec(RuntimeAction::OutEnter { path: &path });
                        self.fire_triggers("on_add", tree);
                        tree.exec(RuntimeAction::OutLeave);
                    }
                }
            } else if &*act.name == "out.add" {
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    let value = act.args.get(1).map(|v| self.expr_to_string(v));
                    let value_ref = value.as_deref();
                    tree.exec(RuntimeAction::OutAdd {
                        path: &path,
                        value: value_ref,
                    });
                    if self.has_on_add_triggers() && names_node(&path) {
                        tree.exec(RuntimeAction::OutEnter { path: &path });
                        self.fire_triggers("on_add", tree);
                        tree.exec(RuntimeAction::OutLeave);
                    }
                }
            } else if &*act.name == "out.replace" {
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    let value = act.args.get(1).map(|v| self.expr_to_string(v));
                    let value_ref = value.as_deref();
                    tree.exec(RuntimeAction::OutReplace {
                        path: &path,
                        value: value_ref,
                    });
                    if self.has_on_add_triggers() && names_node(&path) {
                        tree.exec(RuntimeAction::OutEnter { path: &path });
                        self.fire_triggers("on_add", tree);
                        tree.exec(RuntimeAction::OutLeave);
                    }
                }
            } else if &*act.name == "out.add_attribute" {
                if let (Some(path_expr), Some(name_expr), Some(val_expr)) =
                    (act.args.first(), act.args.get(1), act.args.get(2))
                {
                    let path = self.expr_to_cow(path_expr);
                    let name = self.expr_to_cow(name_expr);
                    let value = self.expr_to_cow(val_expr);
                    tree.exec(RuntimeAction::OutAddAttribute {
                        path: &path,
                        name: &name,
                        value: &value,
                    });
                    tree.exec(RuntimeAction::OutEnter { path: &path });
                    self.fire_triggers("on_add", tree);
                    tree.exec(RuntimeAction::OutLeave);
                }
            } else if &*act.name == "out.set_root_name" {
                if let Some(name_expr) = act.args.first() {
                    let name = self.expr_to_cow(name_expr);
                    tree.exec(RuntimeAction::OutSetRootName { name: &name });
                }
            } else if &*act.name == "out.open" {
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    tree.exec(RuntimeAction::OutOpen { path: &path });
                    self.fire_triggers("on_add", tree);
                    pending_auto_leaves += 1;
                }
            } else if &*act.name == "out.enter" {
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    tree.exec(RuntimeAction::OutEnter { path: &path });
                    self.fire_triggers("on_add", tree);
                    pending_auto_leaves += 1;
                }
            } else if &*act.name == "out.leave" {
                self.fire_triggers("on_leave", tree);
                tree.exec(RuntimeAction::OutLeave);
                // If user explicitly called out.leave(), it cancels one pending auto-leave
                pending_auto_leaves = pending_auto_leaves.saturating_sub(1);
            } else if &*act.name == "do.skip" {
                // stop executing further actions in this block
                skip_rest = true;
                continue;
            } else if &*act.name == "do.next" {
                // signal to outer loop: stop current action list, continue grammar loop without consuming more input
                do_next = true;
                continue;
            } else if &*act.name == "do.return" {
                let levels = act
                    .args
                    .first()
                    .map(|e| match e {
                        Expression::Number(n) => *n as i32,
                        Expression::String(s) => s.parse::<i32>().unwrap_or(1),
                        _ => 1,
                    })
                    .unwrap_or(1);
                return_levels = levels.max(1);
                continue;
            } else if &*act.name == "do.say" {
                if let Some(msg_expr) = act.args.first() {
                    traces.push(format!("say: {}", self.expr_to_string(msg_expr)));
                }
            } else if &*act.name == "do.warn" {
                if let Some(msg_expr) = act.args.first() {
                    traces.push(format!("warn: {}", self.expr_to_string(msg_expr)));
                }
            } else if &*act.name == "do.fail" {
                self.ctx.pos = self.ctx.input.len();
                let err_msg = act
                    .args
                    .first()
                    .map(|e| self.expr_to_string(e))
                    .unwrap_or_else(|| "fail".to_string());
                traces.push("fail invoked".to_string());
                traces.push(format!("fail: {}", err_msg));
                fail_error = Some(err_msg);
                return_levels = 1;
                break;
            } else if act.name.starts_with("out.enqueue_") {
                // The first arg is a regex literal or a variable referencing one.
                let raw_opt = match act.args.first() {
                    Some(Expression::Regex(r)) => Some(r.clone()),
                    Some(Expression::Variable(v)) => {
                        if let Some(Expression::Regex(r)) = self.resolve_variable(v) {
                            Some(r.clone())
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(raw) = raw_opt {
                    if let Ok(rx) = Regex::new(&format!("(?s){}", raw)) {
                        let path = act.args.get(1).map(|p| self.expr_to_string(p)).unwrap_or_default();
                        let value = act.args.get(2).map(|v| self.expr_to_string(v));
                        let trig = TriggerAction { path, value };
                        match &*act.name {
                            "out.enqueue_before" => self.trig_before.push((rx, trig)),
                            "out.enqueue_after" => self.trig_after.push((rx, trig)),
                            "out.enqueue_on_add" => self.trig_on_add.push((rx, trig)),
                            "out.enqueue_on_leave" => self.trig_on_leave.push((rx, trig)),
                            "out.enqueue_before_persist" => self.trig_before_persist.push((rx, trig)),
                            "out.enqueue_after_persist" => self.trig_after_persist.push((rx, trig)),
                            "out.enqueue_on_add_persist" => self.trig_on_add_persist.push((rx, trig)),
                            "out.enqueue_on_leave_persist" => self.trig_on_leave_persist.push((rx, trig)),
                            _ => {}
                        }
                    }
                }
            } else if &*act.name == "out.clear_queue" {
                // Persistent and on_leave triggers are kept.
                self.trig_before.clear();
                self.trig_after.clear();
                self.trig_on_add.clear();
            }
            last_action_name = Some(current_name);
        }
        (return_levels, do_next, fail_error, pending_auto_leaves)
    }

    // Run another grammar at the current position.
    // Returns (consumed bytes, return levels left after this grammar consumed one).
    fn run_inline_grammar(&mut self, name: &str, tree: &mut OutputTree, traces: &mut Vec<String>) -> (usize, i32) {
        if self.depth == MAX_GRAMMAR_DEPTH {
            // Ends the run like `do.fail`.
            let start = self.ctx.pos;
            let line = line_number(&self.ctx.input[..start]);
            self.fail.get_or_insert_with(|| {
                format!(
                    "grammar calls nested more than {MAX_GRAMMAR_DEPTH} levels deep (calling '{name}' at line {line})"
                )
            });
            self.ctx.pos = self.ctx.input.len();
            return (self.ctx.pos - start, 0);
        }
        self.depth += 1;
        let result = self.run_inline_grammar_statements(name, tree, traces);
        self.depth -= 1;
        result
    }

    fn run_inline_grammar_statements(
        &mut self,
        name: &str,
        tree: &mut OutputTree,
        traces: &mut Vec<String>,
    ) -> (usize, i32) {
        // Arc::clone is O(1) — just an atomic increment.
        let statements = match self.stmt_cache.get(name).map(Arc::clone) {
            Some(s) => s,
            None => return (0, 0),
        };
        let start = self.ctx.pos;
        let mut return_levels: i32 = 0;
        loop {
            let pos_before = self.ctx.pos;
            prof_inc!(GRAMMAR_LOOP_ITERS);
            for stmt in statements.iter() {
                match stmt {
                    Statement::Match(m) => {
                        if let Some((len, acts, scope, name_scope)) = self.eval_match(m).ok().flatten() {
                            {
                                let match_text = &self.ctx.input[self.ctx.pos..self.ctx.pos + len];
                                if len > 0 {
                                    self.ctx.advance(len);
                                }
                                self.last_match_text = Cow::Borrowed(match_text);
                                let needs_sub = Self::actions_need_substitution(acts);
                                let substituted_storage;
                                let actions_to_run: &[FunctionCall] = if needs_sub {
                                    substituted_storage = self.substitute_actions(acts, &scope, &name_scope);
                                    &substituted_storage
                                } else {
                                    acts
                                };
                                self.last_captures = scope;
                                self.last_capture_names = name_scope;
                                self.fire_triggers("before", tree);
                                let (ret_lvl, next, fail, auto_leaves) =
                                    self.execute_runtime_actions(actions_to_run, tree, traces);
                                self.fail = self.fail.take().or(fail);
                                for _ in 0..auto_leaves {
                                    self.fire_triggers("on_leave", tree);
                                    tree.leave();
                                }
                                if ret_lvl > 0 {
                                    return_levels = ret_lvl;
                                    break;
                                }
                                if next {
                                    break;
                                }
                                self.fire_triggers("after", tree);
                                break; // break and restart after successful match
                            }
                        }
                    }
                    Statement::When(w) => {
                        if let Some((scope, name_scope)) = self.eval_when(w).ok().flatten() {
                            let needs_sub = Self::actions_need_substitution(&w.actions);
                            let substituted_storage;
                            let actions_to_run: &[FunctionCall] = if needs_sub {
                                substituted_storage = self.substitute_actions(&w.actions, &scope, &name_scope);
                                &substituted_storage
                            } else {
                                &w.actions
                            };
                            self.last_match_text = Cow::Borrowed("");
                            self.last_captures = scope;
                            self.last_capture_names = name_scope;
                            self.fire_triggers("before", tree);
                            let (ret_lvl, next, fail, auto_leaves) =
                                self.execute_runtime_actions(actions_to_run, tree, traces);
                            self.fail = self.fail.take().or(fail);
                            for _ in 0..auto_leaves {
                                self.fire_triggers("on_leave", tree);
                                tree.leave();
                            }
                            if ret_lvl > 0 {
                                return_levels = ret_lvl;
                                break;
                            }
                            if next {
                                break;
                            }
                            self.fire_triggers("after", tree);
                            break; // break and restart after successful when
                        }
                    }
                    Statement::Skip(s) => {
                        if let Some(len) = self.eval_skip(s).ok().flatten() {
                            if len > 0 {
                                self.ctx.advance(len);
                                break; /* break and restart */
                            }
                        }
                    }
                    Statement::Action(a) => {
                        let pos_before_action = self.ctx.pos;
                        // Fast path: direct grammar dispatch (same as run_grammar optimization).
                        if !a.name.contains('.') && self.stmt_cache.contains_key(&*a.name) {
                            let (sub_consumed, remaining) = self.run_inline_grammar(&a.name, tree, traces);
                            if remaining > 0 {
                                return_levels = remaining;
                                break;
                            }
                            if sub_consumed > 0 {
                                break;
                            }
                        } else {
                            let substituted = self.substitute_action(a, &[], &[]);
                            let (ret_lvl, next, fail, auto_leaves) =
                                self.execute_runtime_actions(std::slice::from_ref(&substituted), tree, traces);
                            self.fail = self.fail.take().or(fail);
                            for _ in 0..auto_leaves {
                                self.fire_triggers("on_leave", tree);
                                tree.leave();
                            }
                            if ret_lvl > 0 {
                                return_levels = ret_lvl;
                                break;
                            }
                            if next {
                                break;
                            }
                            if self.ctx.pos > pos_before_action {
                                break;
                            }
                        }
                    }
                }
                if return_levels > 0 {
                    break;
                }
            }
            if return_levels > 0 {
                break;
            }
            if self.ctx.pos == pos_before {
                break;
            }
        }
        // Consume one level for this grammar
        let remaining = (return_levels - 1).max(0);
        (self.ctx.pos - start, remaining)
    }

    /// Without `on_add` triggers, entering the node that create/add/replace just
    /// made and leaving it again changes nothing.
    fn has_on_add_triggers(&self) -> bool {
        !self.trig_on_add.is_empty() || !self.trig_on_add_persist.is_empty()
    }

    fn fire_triggers(&mut self, kind: &str, tree: &mut OutputTree) {
        // Fast bail: skip capture allocation if no triggers registered for this kind.
        let (has_single, has_persist) = match kind {
            "before" => (!self.trig_before.is_empty(), !self.trig_before_persist.is_empty()),
            "after" => (!self.trig_after.is_empty(), !self.trig_after_persist.is_empty()),
            "on_add" => (!self.trig_on_add.is_empty(), !self.trig_on_add_persist.is_empty()),
            "on_leave" => (!self.trig_on_leave.is_empty(), !self.trig_on_leave_persist.is_empty()),
            _ => return,
        };
        if !has_single && !has_persist {
            return;
        }
        let text: &str = &self.last_match_text;
        // Resolve Cow captures to &str slices for inner helper functions.
        let caps: Vec<&str> = self.last_captures.iter().map(|c| c.as_ref()).collect();
        let _names = &self.last_capture_names;
        // Helper closure to fire list (single-shot)
        fn fire_list(
            captures: &[&str],
            names: &[Option<Arc<str>>],
            list: &mut Vec<(Regex, TriggerAction)>,
            text: &str,
            tree: &mut OutputTree,
        ) {
            let mut indices = Vec::new();
            for (i, (rx, _)) in list.iter().enumerate() {
                if rx.is_match(text) {
                    indices.push(i);
                }
            }
            for idx in indices.into_iter().rev() {
                let (_, trig) = list.swap_remove(idx);
                let path_interp = interpolate_local(&trig.path, captures, names);
                let value_interp = trig.value.as_ref().map(|v| interpolate_local(v, captures, names));
                let segs = tree.path_segments(&path_interp);
                tree.add_path(&segs, value_interp);
            }
        }
        // Persistent list: retain entries
        fn fire_persist(
            captures: &[&str],
            names: &[Option<Arc<str>>],
            list: &[(Regex, TriggerAction)],
            text: &str,
            tree: &mut OutputTree,
        ) {
            for (rx, trig) in list.iter() {
                if rx.is_match(text) {
                    let path_interp = interpolate_local(&trig.path, captures, names);
                    let value_interp = trig.value.as_ref().map(|v| interpolate_local(v, captures, names));
                    let segs = tree.path_segments(&path_interp);
                    tree.add_path(&segs, value_interp);
                }
            }
        }
        let caps = &caps;
        let names = &self.last_capture_names;
        if kind == "before" {
            fire_list(caps, names, &mut self.trig_before, text, tree);
            let persist = self.trig_before_persist.clone();
            fire_persist(caps, names, &persist, text, tree);
        } else if kind == "after" {
            fire_list(caps, names, &mut self.trig_after, text, tree);
            let persist = self.trig_after_persist.clone();
            fire_persist(caps, names, &persist, text, tree);
        } else if kind == "on_add" {
            fire_list(caps, names, &mut self.trig_on_add, text, tree);
            let persist = self.trig_on_add_persist.clone();
            fire_persist(caps, names, &persist, text, tree);
        } else if kind == "on_leave" {
            fire_list(caps, names, &mut self.trig_on_leave, text, tree);
            let persist = self.trig_on_leave_persist.clone();
            fire_persist(caps, names, &persist, text, tree);
        }
    }
}

/// Whether an `out.*` path names a node; `""` (e.g. an empty capture) or only slashes name none.
fn names_node(path: &str) -> bool {
    path.split('/').any(|part| !part.is_empty())
}

/// ASCII bytes in the regex class `\s` (which also has vertical tab, unlike `u8::is_ascii_whitespace`).
fn is_regex_space(b: u8) -> bool {
    b.is_ascii_whitespace() || b == 0x0B
}

/// The skip fast paths compare ASCII bytes, while `\s`, `\S`, `\d` and `\w` are Unicode
/// classes: when a non-ASCII character decides the outcome, only the regex can tell.
fn fast_skip_decides(kind: crate::parser::ast::FastPathKind, rem: &str, n: Option<usize>) -> bool {
    use crate::parser::ast::FastPathKind;
    let len = n.unwrap_or(0);
    let bytes = rem.as_bytes();
    match kind {
        FastPathKind::Whitespace | FastPathKind::Digits | FastPathKind::WordChars => {
            bytes.get(len).is_none_or(u8::is_ascii)
        }
        FastPathKind::NonWhitespace => bytes[..len].is_ascii(),
        _ => true,
    }
}

// Helper for trigger interpolation ($N and $name)
fn interpolate_local(s: &str, captures: &[&str], names: &[Option<Arc<str>>]) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'$') {
            // \$ → literal '$'
            out.push('$');
            chars.next();
        } else if c == '$' {
            let mut token = String::new();
            while let Some(&p) = chars.peek() {
                if p.is_ascii_alphanumeric() || p == '_' {
                    token.push(p);
                    chars.next();
                } else {
                    break;
                }
            }
            if token.is_empty() {
                out.push('$');
            } else if token.chars().all(|d| d.is_ascii_digit()) {
                if let Ok(idx) = token.parse::<usize>() {
                    if let Some(val) = captures.get(idx) {
                        out.push_str(val);
                    }
                }
            } else if let Some((i, _)) = names
                .iter()
                .enumerate()
                .find(|(_, n)| n.as_deref() == Some(token.as_str()))
            {
                if let Some(val) = captures.get(i) {
                    out.push_str(val);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
impl<'a> Runner<'a> {
    /// Evaluate an expression to a string, returning a `Cow` to avoid allocation
    /// when no interpolation is needed (the common case for path arguments).
    fn expr_to_cow<'b>(&'b self, expr: &'b Expression) -> Cow<'b, str> {
        match expr {
            Expression::String(s) => {
                if !s.contains('$') {
                    Cow::Borrowed(s.as_str())
                } else {
                    Cow::Owned(self.expr_to_string(expr))
                }
            }
            Expression::Number(n) => Cow::Owned(n.to_string()),
            Expression::Regex(r) => Cow::Owned(format!("/{}/", r)),
            Expression::Variable(v) => {
                if let Some(inner) = self.resolve_variable(v) {
                    self.expr_to_cow(inner)
                } else {
                    Cow::Borrowed("")
                }
            }
            Expression::Capture(_) | Expression::CaptureName(_) => Cow::Borrowed(""),
        }
    }
    fn expr_to_string(&self, expr: &Expression) -> String {
        match expr {
            Expression::String(s) => {
                // Fast path: no interpolation needed if no '$' present.
                if !s.contains('$') {
                    return s.clone();
                }
                // Interpolate $N and $name using last_captures context; `\$` is a literal `$`.
                let bytes = s.as_bytes();
                let mut out = String::with_capacity(s.len() + 16);
                let mut lit = 0;
                let mut i = 0;
                while let Some(off) = memchr::memchr2(b'\\', b'$', &bytes[i..]) {
                    let j = i + off;
                    if bytes[j] == b'\\' {
                        if bytes.get(j + 1) == Some(&b'$') {
                            out.push_str(&s[lit..j]);
                            out.push('$');
                            lit = j + 2;
                            i = j + 2;
                        } else {
                            i = j + 1;
                        }
                        continue;
                    }
                    out.push_str(&s[lit..j]);
                    let start = j + 1;
                    let len = bytes[start..]
                        .iter()
                        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
                        .count();
                    let token = &s[start..start + len];
                    if token.is_empty() {
                        out.push('$');
                    } else if let Some(val) = self.capture_by_token(token) {
                        crate::exec::out::percent_encode_into(&mut out, val);
                    }
                    lit = start + len;
                    i = lit;
                }
                out.push_str(&s[lit..]);
                out
            }
            Expression::Number(n) => n.to_string(),
            Expression::Regex(r) => format!("/{}/", r),
            Expression::Variable(v) => {
                if let Some(inner) = self.resolve_variable(v) {
                    self.expr_to_string(inner)
                } else {
                    String::new()
                }
            }
            Expression::Capture(_) | Expression::CaptureName(_) => String::new(), // should have been substituted already
        }
    }

    /// Value of `$N` (all digits) or `$name` in the last match.
    fn capture_by_token(&self, token: &str) -> Option<&str> {
        let idx = if token.bytes().all(|d| d.is_ascii_digit()) {
            token.parse::<usize>().ok()?
        } else {
            self.last_capture_names
                .iter()
                .position(|n| n.as_deref() == Some(token))?
        };
        self.last_captures.get(idx).map(|c| c.as_ref())
    }
}

/// Execute the named grammar from `doc` against `input`, returning an [`ExecutionResult`].
///
/// Calls `compile_regexes()` to ensure all regexes are pre-compiled.
/// If you have already pre-compiled (e.g. via `GelContext`), use
/// [`execute_precompiled`] to avoid the `&mut` requirement.
pub fn execute(doc: &mut GelDocument, grammar: &str, input: &str) -> Result<ExecutionResult> {
    // Ensure all regexes (individual + combined field-list) are pre-compiled.
    // This is idempotent and fast if already done.
    doc.compile_regexes();
    execute_precompiled(doc, grammar, input)
}

/// Execute the named grammar against an already-compiled [`GelDocument`].
///
/// Unlike [`execute`], this takes `&GelDocument` (no mutation required).
/// The caller must ensure `doc.compile_regexes()` has already been called.
pub fn execute_precompiled(doc: &GelDocument, grammar: &str, input: &str) -> Result<ExecutionResult> {
    flatten(Runner::new(doc, input).run_grammar(grammar)?)
}

/// Like [`execute_precompiled`], but records only the output tree and the error,
/// not the per-match actions, traces and captures: less memory and time when
/// [`serialize_execution`] is not needed.
pub fn execute_tree(doc: &GelDocument, grammar: &str, input: &str) -> Result<ExecutionResult> {
    let mut runner = Runner::new(doc, input);
    runner.record = false;
    flatten(runner.run_grammar(grammar)?)
}

fn flatten(mut result: ExecutionResult) -> Result<ExecutionResult> {
    // Compact the arena tree (reorder children by name-group, drop
    // build-time indices) and flatten into BFS-ordered Vec<FlatNode>.
    // The arena Vec<Node> is consumed and freed in one shot —
    // dramatically reduces allocator fragmentation / RSS.
    let tree = std::mem::take(&mut result.output);
    result.flat = Some(tree.compact_and_flatten());
    Ok(result)
}

// === Runtime serialization (ExecutionResult) ===

/// Serialization format for [`ExecutionResult`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFormat {
    Json,
    Xml,
    Yaml,
    /// Discard output (runs grammar, produces empty string).
    None,
}

/// Serialize the full diagnostic envelope (`consumed`, `actions`, `traces`,
/// `output`, …) for debugging. For the bare data tree use [`serialize_tree`].
pub fn serialize_execution(result: &ExecutionResult, format: RuntimeFormat) -> String {
    match format {
        RuntimeFormat::Json => serialize_json(result),
        RuntimeFormat::Xml => serialize_xml(result),
        RuntimeFormat::Yaml => serialize_yaml(result),
        RuntimeFormat::None => String::new(),
    }
}

// =========================================================================
// Tree-only serialization
// =========================================================================

/// Serialize the data tree only (no metadata envelope).
///
/// - **JSON**: 4-space indent; attributes as `@name`, text as `#text`,
///   repeated siblings as arrays.
/// - **XML**: attributes as XML attributes; root `<xml>` unless
///   `out.set_root_name` was used.
/// - **YAML**: same mapping as JSON.
pub fn serialize_tree(result: &ExecutionResult, format: RuntimeFormat) -> String {
    match format {
        RuntimeFormat::Json => tree_json(result),
        RuntimeFormat::Xml => tree_xml(result),
        RuntimeFormat::Yaml => tree_yaml(result),
        RuntimeFormat::None => String::new(),
    }
}

/// Write the **data tree only** directly to a [`Write`](std::io::Write) impl,
/// avoiding the intermediate `String` allocation.
///
/// For large outputs (100 MB+), this significantly reduces peak memory
/// compared to [`serialize_tree`] which builds the full output in RAM.
pub fn serialize_tree_to_writer<W: std::io::Write>(
    result: &ExecutionResult,
    format: RuntimeFormat,
    writer: &mut W,
) -> std::io::Result<()> {
    match format {
        RuntimeFormat::Json => tree_json_to_writer(result, writer),
        RuntimeFormat::Xml => tree_xml_to_writer(result, writer),
        RuntimeFormat::Yaml => tree_yaml_to_writer(result, writer),
        RuntimeFormat::None => Ok(()),
    }
}

// -- OutputSink trait for dual String / streaming serialization ------------

/// Abstraction over `String` (in-memory) and `io::Write` (streaming) sinks.
/// `String` operations never fail; `Write` operations propagate I/O errors.
trait OutputSink {
    fn write_str(&mut self, s: &str) -> std::io::Result<()>;
    fn write_char(&mut self, c: char) -> std::io::Result<()>;
}

impl OutputSink for String {
    #[inline(always)]
    fn write_str(&mut self, s: &str) -> std::io::Result<()> {
        self.push_str(s);
        Ok(())
    }
    #[inline(always)]
    fn write_char(&mut self, c: char) -> std::io::Result<()> {
        self.push(c);
        Ok(())
    }
}

/// Adapter: any `std::io::Write` becomes an `OutputSink`.
struct WriterSink<W>(W);

impl<W: std::io::Write> OutputSink for WriterSink<W> {
    #[inline]
    fn write_str(&mut self, s: &str) -> std::io::Result<()> {
        self.0.write_all(s.as_bytes())
    }
    #[inline]
    fn write_char(&mut self, c: char) -> std::io::Result<()> {
        let mut buf = [0u8; 4];
        self.0.write_all(c.encode_utf8(&mut buf).as_bytes())
    }
}

// -- Tree-only JSON -------------------------------------------------------

fn tree_json(exec: &ExecutionResult) -> String {
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    let mut out = String::new();
    tree_json_node(flat, flat.root(), &mut out, 0).expect("writing to a String cannot fail");
    out
}

/// Write tree JSON directly to a writer (truly streaming, no intermediate buffer).
fn tree_json_to_writer<W: std::io::Write>(exec: &ExecutionResult, writer: &mut W) -> std::io::Result<()> {
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    tree_json_node(flat, flat.root(), &mut WriterSink(writer), 0)
}

fn tree_json_node<S: OutputSink>(flat: &FlatTree, node: &FlatNode, out: &mut S, indent: usize) -> std::io::Result<()> {
    let attrs = flat.attrs_of(node);
    let has_attrs = !attrs.is_empty();
    let has_text = node.text.as_ref().is_some_and(|t| !t.is_empty());
    let has_children = node.children_len > 0;

    if !has_attrs && !has_text && !has_children {
        return out.write_str("{}");
    }

    out.write_char('{')?;
    let inner = indent + 1;
    let mut first = true;

    for (k, v) in attrs {
        if !first {
            out.write_char(',')?;
        }
        out.write_char('\n')?;
        json_write_indent(out, inner)?;
        out.write_str("\"@")?;
        json_write_escaped_body(out, k)?;
        out.write_char('"')?;
        out.write_str(": ")?;
        json_write_escaped(out, v)?;
        first = false;
    }

    if has_text {
        if !first {
            out.write_char(',')?;
        }
        out.write_char('\n')?;
        json_write_indent(out, inner)?;
        json_write_escaped(out, "#text")?;
        out.write_str(": ")?;
        json_write_escaped(out, node.text.as_ref().unwrap())?;
        first = false;
    }

    for group in flat.iter_child_groups(node) {
        if !first {
            out.write_char(',')?;
        }
        out.write_char('\n')?;
        json_write_indent(out, inner)?;
        json_write_escaped(out, &group[0].name)?;
        out.write_str(": ")?;

        if group.len() == 1 {
            tree_json_node(flat, &group[0], out, inner)?;
        } else {
            out.write_char('[')?;
            for (i, ch) in group.iter().enumerate() {
                if i > 0 {
                    out.write_char(',')?;
                }
                out.write_char('\n')?;
                json_write_indent(out, inner + 1)?;
                tree_json_node(flat, ch, out, inner + 1)?;
            }
            out.write_char('\n')?;
            json_write_indent(out, inner)?;
            out.write_char(']')?;
        }
        first = false;
    }

    out.write_char('\n')?;
    json_write_indent(out, indent)?;
    out.write_char('}')
}

#[inline]
fn json_write_indent<S: OutputSink>(out: &mut S, level: usize) -> std::io::Result<()> {
    for _ in 0..level {
        out.write_str("    ")?;
    }
    Ok(())
}

fn json_write_escaped<S: OutputSink>(out: &mut S, s: &str) -> std::io::Result<()> {
    out.write_char('"')?;
    json_write_escaped_body(out, s)?;
    out.write_char('"')
}

/// JSON-escape `s` without the quotes; runs that need no escaping are written in one piece.
fn json_write_escaped_body<S: OutputSink>(out: &mut S, s: &str) -> std::io::Result<()> {
    let mut run = 0;
    for (i, b) in s.bytes().enumerate() {
        let esc = match b {
            b'"' => Some("\\\""),
            b'\\' => Some("\\\\"),
            0x08 => Some("\\b"),
            0x0C => Some("\\f"),
            b'\n' => Some("\\n"),
            b'\r' => Some("\\r"),
            b'\t' => Some("\\t"),
            0..=0x1F => None,
            _ => continue,
        };
        out.write_str(&s[run..i])?;
        match esc {
            Some(e) => out.write_str(e)?,
            None => {
                out.write_str("\\u00")?;
                out.write_char(char::from(b"0123456789abcdef"[(b >> 4) as usize]))?;
                out.write_char(char::from(b"0123456789abcdef"[(b & 0xF) as usize]))?;
            }
        }
        run = i + 1;
    }
    out.write_str(&s[run..])
}

// -- Tree-only XML --------------------------------------------------------

/// A `Write` adapter that turns `&apos;` back into `'` (valid inside
/// double-quoted attributes) without buffering the whole document.
///
/// Safety assumption: `quick_xml` always writes complete attribute values in a
/// single `write_all` call, so `&apos;` is never split across calls.
struct AposWriter<W: std::io::Write> {
    inner: W,
}

impl<W: std::io::Write> std::io::Write for AposWriter<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        write_bytes_replacing_apos(&mut self.inner, data)?;
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn tree_xml(exec: &ExecutionResult) -> String {
    let mut buf = Vec::new();
    tree_xml_to_writer(exec, &mut buf).expect("XML write to Vec never fails");
    // SAFETY: quick_xml produces valid UTF-8; AposWriter only replaces valid
    // UTF-8 subsequences.
    unsafe { String::from_utf8_unchecked(buf) }
}

/// Write tree XML directly to a writer (truly streaming via `AposWriter`).
fn tree_xml_to_writer<W: std::io::Write>(exec: &ExecutionResult, writer: &mut W) -> std::io::Result<()> {
    use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
    use quick_xml::Writer;

    let flat = exec.flat.as_ref().expect("FlatTree not built");
    let root = flat.root();

    // Root element: name from out.set_root_name, otherwise "xml".
    let root_name = if root.name.is_empty() || &*root.name == "." {
        "xml"
    } else {
        &root.name
    };

    {
        let mut w = Writer::new_with_indent(AposWriter { inner: &mut *writer }, b' ', 2);

        // Open root element (attributes of root node become XML attributes)
        let mut start = BytesStart::new(root_name);
        for (k, v) in flat.attrs_of(root) {
            start.push_attribute((&**k, v.as_str()));
        }
        w.write_event(Event::Start(start)).expect("root start");

        // Text content of root (rare but possible)
        if let Some(txt) = &root.text {
            if !txt.is_empty() {
                let escaped = xml_escape_text_content(txt);
                w.write_event(Event::Text(BytesText::from_escaped(escaped)))
                    .expect("root text");
            }
        }

        // Recursively write children
        for group in flat.iter_child_groups(root) {
            for ch in group {
                tree_xml_node(flat, ch, &mut w);
            }
        }

        w.write_event(Event::End(BytesEnd::new(root_name))).expect("root end");
    } // AposWriter dropped here, releases borrow on writer

    writer.write_all(b"\n")
}

/// Write a node as an XML element (self-closing when empty).
fn tree_xml_node<W: std::io::Write>(flat: &FlatTree, node: &FlatNode, writer: &mut quick_xml::Writer<W>) {
    use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};

    let tag: &str = &node.name;
    let has_text = node.text.as_ref().is_some_and(|t| !t.is_empty());

    // Check if node is completely empty (no non-empty text, no children)
    let is_empty = !has_text && node.children_len == 0;

    if is_empty {
        // Self-closing: <tag/> or <tag attr="val"/>
        let mut elem = BytesStart::new(tag);
        for (k, v) in flat.attrs_of(node) {
            elem.push_attribute((&**k, v.as_str()));
        }
        writer.write_event(Event::Empty(elem)).expect("empty elem");
        return;
    }

    // Open element with XML attributes
    let mut start = BytesStart::new(tag);
    for (k, v) in flat.attrs_of(node) {
        start.push_attribute((&**k, v.as_str()));
    }
    writer.write_event(Event::Start(start)).expect("elem start");

    // Text content
    if has_text {
        let txt = node.text.as_ref().unwrap();
        let escaped = xml_escape_text_content(txt);
        writer
            .write_event(Event::Text(BytesText::from_escaped(escaped)))
            .expect("text");
    }

    // Children, grouped by name in first-occurrence order
    for group in flat.iter_child_groups(node) {
        for ch in group {
            tree_xml_node(flat, ch, writer);
        }
    }

    writer.write_event(Event::End(BytesEnd::new(tag))).expect("elem end");
}

/// Escape `<`, `>` and `&` in XML text (quotes are valid in text content).
fn xml_escape_text_content(s: &str) -> Cow<'_, str> {
    if !s.bytes().any(|b| matches!(b, b'<' | b'>' | b'&')) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut run = 0;
    for (i, b) in s.bytes().enumerate() {
        let esc = match b {
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'&' => "&amp;",
            _ => continue,
        };
        out.push_str(&s[run..i]);
        out.push_str(esc);
        run = i + 1;
    }
    out.push_str(&s[run..]);
    Cow::Owned(out)
}

/// Write raw bytes to a writer, replacing all `&apos;` with `'`.
fn write_bytes_replacing_apos<W: std::io::Write>(writer: &mut W, bytes: &[u8]) -> std::io::Result<()> {
    let needle = b"&apos;";
    let mut cursor = 0usize;
    for pos in memchr::memmem::find_iter(bytes, needle) {
        writer.write_all(&bytes[cursor..pos])?;
        writer.write_all(b"'")?;
        cursor = pos + needle.len();
    }
    writer.write_all(&bytes[cursor..])
}

// -- Tree-only YAML -------------------------------------------------------
//
// The same mapping as JSON: each node is a mapping of `'@attr'`, `'#text'` and
// child names, same-named children form a sequence and an empty node is `{}`.

fn tree_yaml(exec: &ExecutionResult) -> String {
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    let mut out = String::new();
    tree_yaml_root(flat, &mut out).expect("writing to a String cannot fail");
    out
}

/// Write tree YAML directly to a writer (truly streaming, no intermediate buffer).
fn tree_yaml_to_writer<W: std::io::Write>(exec: &ExecutionResult, writer: &mut W) -> std::io::Result<()> {
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    tree_yaml_root(flat, &mut WriterSink(writer))
}

fn tree_yaml_root<S: OutputSink>(flat: &FlatTree, out: &mut S) -> std::io::Result<()> {
    let root = flat.root();
    if is_empty_node(root) {
        return out.write_str("{}\n");
    }
    tree_yaml_entries(flat, root, out, 0, false)
}

fn is_empty_node(node: &FlatNode) -> bool {
    node.attr_len == 0 && node.children_len == 0 && node.text.as_ref().is_none_or(|t| t.is_empty())
}

/// Write the entries of a non-empty node, one per line at `level`; with `inline`
/// the first entry continues the current line (after `- `).
fn tree_yaml_entries<S: OutputSink>(
    flat: &FlatTree,
    node: &FlatNode,
    out: &mut S,
    level: usize,
    inline: bool,
) -> std::io::Result<()> {
    let mut indent = !inline;
    for (k, v) in flat.attrs_of(node) {
        if indent {
            yaml_indent(out, level)?;
        }
        indent = true;
        yaml_key(out, "@", k, level)?;
        out.write_str(": ")?;
        yaml_scalar(out, "", v)?;
        out.write_char('\n')?;
    }
    if let Some(text) = node.text.as_deref().filter(|t| !t.is_empty()) {
        if indent {
            yaml_indent(out, level)?;
        }
        indent = true;
        out.write_str("'#text': ")?;
        yaml_scalar(out, "", text)?;
        out.write_char('\n')?;
    }
    for group in flat.iter_child_groups(node) {
        if indent {
            yaml_indent(out, level)?;
        }
        indent = true;
        yaml_key(out, "", &group[0].name, level)?;
        match group {
            [child] if is_empty_node(child) => out.write_str(": {}\n")?,
            [child] => {
                out.write_str(":\n")?;
                tree_yaml_entries(flat, child, out, level + 1, false)?;
            }
            _ => {
                out.write_str(":\n")?;
                for child in group {
                    yaml_indent(out, level)?;
                    if is_empty_node(child) {
                        out.write_str("- {}\n")?;
                    } else {
                        out.write_str("- ")?;
                        tree_yaml_entries(flat, child, out, level + 1, true)?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[inline]
fn yaml_indent<S: OutputSink>(out: &mut S, level: usize) -> std::io::Result<()> {
    const SPACES: &str = "                                                                ";
    let n = level * 2;
    if n <= SPACES.len() {
        out.write_str(&SPACES[..n])
    } else {
        for _ in 0..n {
            out.write_char(' ')?;
        }
        Ok(())
    }
}

/// Keys longer than this are written as explicit `? key` entries: YAML limits
/// implicit keys to 1024 characters, and an escaped byte takes up to 4.
const YAML_MAX_IMPLICIT_KEY: usize = 250;

/// Write a mapping key; the caller continues with `:` on the same line.
fn yaml_key<S: OutputSink>(out: &mut S, prefix: &str, key: &str, level: usize) -> std::io::Result<()> {
    if key.len() <= YAML_MAX_IMPLICIT_KEY {
        return yaml_scalar(out, prefix, key);
    }
    out.write_str("? ")?;
    yaml_scalar(out, prefix, key)?;
    out.write_char('\n')?;
    yaml_indent(out, level)
}

/// Write `prefix` + `s` as a scalar that YAML 1.1 and 1.2 parsers read back as
/// this string: plain where nothing can change its meaning, otherwise
/// single-quoted, or double-quoted with escapes for line breaks, tabs and
/// characters YAML does not allow unescaped.
fn yaml_scalar<S: OutputSink>(out: &mut S, prefix: &str, s: &str) -> std::io::Result<()> {
    if yaml_needs_escapes(s) {
        out.write_char('"')?;
        out.write_str(prefix)?;
        yaml_escape_double(out, s)?;
        out.write_char('"')
    } else if prefix.is_empty() && yaml_is_plain(s) {
        out.write_str(s)
    } else {
        out.write_char('\'')?;
        out.write_str(prefix)?;
        yaml_escape_single(out, s)?;
        out.write_char('\'')
    }
}

/// Control characters (including tab and line breaks), DEL, C1 controls,
/// U+2028/U+2029 (line breaks in YAML 1.1), U+FEFF, U+FFFE and U+FFFF.
fn yaml_needs_escapes(s: &str) -> bool {
    let b = s.as_bytes();
    b.iter().enumerate().any(|(i, &c)| match c {
        0x00..=0x1F | 0x7F => true,
        0xC2 => matches!(b.get(i + 1), Some(0x80..=0x9F)),
        0xE2 => b.get(i + 1) == Some(&0x80) && matches!(b.get(i + 2), Some(0xA8 | 0xA9)),
        0xEF => matches!(
            (b.get(i + 1), b.get(i + 2)),
            (Some(0xBB), Some(0xBF)) | (Some(0xBF), Some(0xBE | 0xBF))
        ),
        _ => false,
    })
}

/// Plain only where no YAML 1.1 or 1.2 resolver reads anything but a string
/// (numbers, dates, booleans, null, `<<`, `=`) and no indicator applies.
fn yaml_is_plain(s: &str) -> bool {
    let b = s.as_bytes();
    let (Some(&first), Some(&last)) = (b.first(), b.last()) else {
        return false;
    };
    !matches!(
        first,
        b'-' | b'?'
            | b':'
            | b','
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'#'
            | b'&'
            | b'*'
            | b'!'
            | b'|'
            | b'>'
            | b'\''
            | b'"'
            | b'%'
            | b'@'
            | b'`'
            | b'0'..=b'9' | b'+' | b'.' | b'~' | b'<' | b'=' | b' '
    ) && last != b' '
        && memchr::memchr2(b':', b'#', b).is_none()
        && !["y", "n", "yes", "no", "true", "false", "on", "off", "null"]
            .iter()
            .any(|w| w.eq_ignore_ascii_case(s))
}

fn yaml_escape_single<S: OutputSink>(out: &mut S, s: &str) -> std::io::Result<()> {
    let mut run = 0;
    for (i, _) in s.match_indices('\'') {
        out.write_str(&s[run..=i])?;
        out.write_char('\'')?;
        run = i + 1;
    }
    out.write_str(&s[run..])
}

fn yaml_escape_double<S: OutputSink>(out: &mut S, s: &str) -> std::io::Result<()> {
    let mut run = 0;
    for (i, c) in s.char_indices() {
        let (esc, hex_digits) = match c {
            '"' => ("\\\"", 0),
            '\\' => ("\\\\", 0),
            '\n' => ("\\n", 0),
            '\r' => ("\\r", 0),
            '\t' => ("\\t", 0),
            '\0'..='\x1F' | '\x7F'..='\u{9F}' => ("\\x", 2),
            '\u{2028}' | '\u{2029}' | '\u{FEFF}' | '\u{FFFE}' | '\u{FFFF}' => ("\\u", 4),
            _ => continue,
        };
        out.write_str(&s[run..i])?;
        out.write_str(esc)?;
        for d in (0..hex_digits).rev() {
            out.write_char(char::from(b"0123456789ABCDEF"[(c as usize >> (4 * d)) & 0xF]))?;
        }
        run = i + c.len_utf8();
    }
    out.write_str(&s[run..])
}

/// Serialize an [`ExecutionResult`] to JSON using `serde_json`.
///
/// In the output tree, attributes are prefixed with `@`, text uses the
/// `#text` key, and same-named children become arrays.
fn serialize_json(exec: &ExecutionResult) -> String {
    serde_json::to_string_pretty(&execution_value(exec)).unwrap_or_default()
}

/// The [`serialize_execution`] envelope; JSON and YAML are written from it.
fn execution_value(exec: &ExecutionResult) -> serde_json::Value {
    use serde_json::{json, Map, Value};

    let actions: Vec<Value> = exec
        .actions
        .iter()
        .map(|a| {
            let args: Vec<Value> = a
                .args
                .iter()
                .map(|arg| match arg {
                    Expression::String(s) => Value::String(s.clone()),
                    Expression::Regex(r) => Value::String(format!("/{}/", r)),
                    Expression::Number(n) => json!(*n),
                    Expression::Variable(v) => Value::String(format!("${}", v)),
                    Expression::Capture(i) => Value::String(format!("${}", i)),
                    Expression::CaptureName(name) => Value::String(format!("${}", name)),
                })
                .collect();
            json!({"name": &*a.name, "args": args})
        })
        .collect();

    let diagnostics: Vec<Value> = exec
        .diagnostics
        .iter()
        .map(|d| {
            let mut obj = Map::new();
            obj.insert("severity".into(), Value::String(d.severity.to_string()));
            obj.insert("message".into(), Value::String(d.message.clone()));
            if let Some(s) = &d.span {
                obj.insert("line".into(), json!(s.line));
                obj.insert("col".into(), json!(s.col));
                obj.insert("offset".into(), json!(s.offset));
            }
            Value::Object(obj)
        })
        .collect();

    let flat = exec.flat.as_ref().expect("FlatTree not built");
    let output = output_node_to_value(flat, flat.root());

    json!({
        "consumed": exec.consumed,
        "actions": actions,
        "traces": exec.traces,
        "capture_history": exec.capture_history,
        "capture_names_history": exec.capture_names_history.iter().map(|scope| scope.iter().map(|n| n.as_deref().unwrap_or("").to_string()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "error": exec.error,
        "diagnostics": diagnostics,
        "output": output,
    })
}

/// Convert an output-tree [`Node`](crate::exec::out::Node) into a
/// `serde_json::Value` (`@attr` / `#text` conventions).
///
/// Children are grouped by name in insertion order; repeated names become
/// a JSON array.
fn output_node_to_value(flat: &FlatTree, node: &FlatNode) -> serde_json::Value {
    use serde_json::{Map, Value};

    let mut obj = Map::new();

    // Attributes with @ prefix
    for (k, v) in flat.attrs_of(node) {
        obj.insert(format!("@{}", k), Value::String(v.clone()));
    }

    // Empty text counts as no text
    if let Some(txt) = &node.text {
        if !txt.is_empty() {
            obj.insert("#text".into(), Value::String((**txt).clone()));
        }
    }

    // Children grouped by name, preserving first-occurrence insertion order.
    // After compact(), children are physically grouped — iter_child_groups()
    // detects boundaries via Rc pointer equality.  Zero allocation.
    if node.children_len > 0 {
        for group in flat.iter_child_groups(node) {
            let name = &group[0].name;
            if group.len() == 1 {
                obj.insert(name.to_string(), output_node_to_value(flat, &group[0]));
            } else {
                let arr: Vec<Value> = group.iter().map(|ch| output_node_to_value(flat, ch)).collect();
                obj.insert(name.to_string(), Value::Array(arr));
            }
        }
    }

    Value::Object(obj)
}

// ---------------------------------------------------------------------------
// XML serialization via quick-xml
// ---------------------------------------------------------------------------

/// Serialize an [`ExecutionResult`] to XML using `quick_xml::Writer`.
///
/// Attribute values and text content are escaped through quick-xml's
/// built-in routines, eliminating hand-rolled escape helpers.
fn serialize_xml(exec: &ExecutionResult) -> String {
    use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, Event};
    use quick_xml::Writer;
    use std::io::Cursor;

    let mut w = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);

    // <?xml version="1.0" encoding="UTF-8"?>
    w.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .expect("xml decl");

    // <execution consumed="N">
    let mut exec_start = BytesStart::new("execution");
    exec_start.push_attribute(("consumed", exec.consumed.to_string().as_str()));
    w.write_event(Event::Start(exec_start)).expect("execution start");

    // --- actions ---
    if !exec.actions.is_empty() {
        w.write_event(Event::Start(BytesStart::new("actions")))
            .expect("actions start");
        for a in &exec.actions {
            let mut action = BytesStart::new("action");
            action.push_attribute(("name", &*a.name));
            w.write_event(Event::Start(action)).expect("action start");
            if !a.args.is_empty() {
                w.write_event(Event::Start(BytesStart::new("args")))
                    .expect("args start");
                for arg in &a.args {
                    w.write_event(Event::Start(BytesStart::new("arg"))).expect("arg start");
                    let text = match arg {
                        Expression::String(s) => s.clone(),
                        Expression::Regex(r) => r.clone(),
                        Expression::Number(n) => n.to_string(),
                        Expression::Variable(v) => v.clone(),
                        Expression::Capture(i) => format!("${}", i),
                        Expression::CaptureName(name) => format!("${}", name),
                    };
                    xml_write_escaped_text(&mut w, &text);
                    w.write_event(Event::End(BytesEnd::new("arg"))).expect("arg end");
                }
                w.write_event(Event::End(BytesEnd::new("args"))).expect("args end");
            }
            w.write_event(Event::End(BytesEnd::new("action"))).expect("action end");
        }
        w.write_event(Event::End(BytesEnd::new("actions")))
            .expect("actions end");
    }

    // --- traces ---
    if !exec.traces.is_empty() {
        w.write_event(Event::Start(BytesStart::new("traces")))
            .expect("traces start");
        for t in &exec.traces {
            w.write_event(Event::Start(BytesStart::new("trace")))
                .expect("trace start");
            xml_write_escaped_text(&mut w, t);
            w.write_event(Event::End(BytesEnd::new("trace"))).expect("trace end");
        }
        w.write_event(Event::End(BytesEnd::new("traces"))).expect("traces end");
    }

    // --- captures ---
    if !exec.capture_history.is_empty() {
        w.write_event(Event::Start(BytesStart::new("captures")))
            .expect("captures start");
        for (i, scope) in exec.capture_history.iter().enumerate() {
            let mut scope_start = BytesStart::new("scope");
            scope_start.push_attribute(("index", i.to_string().as_str()));
            w.write_event(Event::Start(scope_start)).expect("scope start");
            for (j, val) in scope.iter().enumerate() {
                let name = exec
                    .capture_names_history
                    .get(i)
                    .and_then(|nms| nms.get(j))
                    .map(|n| n.as_deref().unwrap_or(""))
                    .unwrap_or("");
                let mut value_start = BytesStart::new("value");
                value_start.push_attribute(("name", name));
                w.write_event(Event::Start(value_start)).expect("value start");
                xml_write_escaped_text(&mut w, val);
                w.write_event(Event::End(BytesEnd::new("value"))).expect("value end");
            }
            w.write_event(Event::End(BytesEnd::new("scope"))).expect("scope end");
        }
        w.write_event(Event::End(BytesEnd::new("captures")))
            .expect("captures end");
    }

    // --- output ---
    w.write_event(Event::Start(BytesStart::new("output")))
        .expect("output start");
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    xml_write_node(flat, flat.root(), &mut w);
    w.write_event(Event::End(BytesEnd::new("output"))).expect("output end");

    // --- error ---
    w.write_event(Event::Start(BytesStart::new("error")))
        .expect("error start");
    if let Some(e) = &exec.error {
        xml_write_escaped_text(&mut w, e);
    }
    w.write_event(Event::End(BytesEnd::new("error"))).expect("error end");

    // </execution>
    w.write_event(Event::End(BytesEnd::new("execution")))
        .expect("execution end");

    String::from_utf8(w.into_inner().into_inner()).unwrap_or_default()
}

/// Write properly-escaped text content via quick-xml.
fn xml_write_escaped_text<W: std::io::Write>(writer: &mut quick_xml::Writer<W>, text: &str) {
    use quick_xml::events::{BytesText, Event};
    let escaped = quick_xml::escape::escape(text);
    writer
        .write_event(Event::Text(BytesText::from_escaped(escaped)))
        .expect("text write");
}

/// Recursively write an output-tree node as XML via quick-xml.
fn xml_write_node<W: std::io::Write>(flat: &FlatTree, node: &FlatNode, writer: &mut quick_xml::Writer<W>) {
    use quick_xml::events::{BytesEnd, BytesStart, Event};

    let mut start = BytesStart::new("node");
    start.push_attribute(("name", &*node.name));
    writer.write_event(Event::Start(start)).expect("node start");

    // Attributes as self-closing child elements
    for (k, v) in flat.attrs_of(node) {
        let mut attr_elem = BytesStart::new("attr");
        attr_elem.push_attribute(("key", &**k));
        attr_elem.push_attribute(("value", v.as_str()));
        writer.write_event(Event::Empty(attr_elem)).expect("attr");
    }

    // Text content
    if let Some(txt) = &node.text {
        xml_write_escaped_text(writer, txt);
    }

    // Recurse into children
    for ch in flat.children_of(node) {
        xml_write_node(flat, ch, writer);
    }

    writer.write_event(Event::End(BytesEnd::new("node"))).expect("node end");
}

/// The envelope of [`serialize_json`] as YAML.
fn serialize_yaml(exec: &ExecutionResult) -> String {
    let envelope = execution_value(exec);
    let map = envelope.as_object().expect("the envelope is a JSON object");
    let mut out = String::new();
    yaml_json_mapping(&mut out, map, 0, false).expect("writing to a String cannot fail");
    out
}

/// Entries of a non-empty JSON object at `level` (`inline` as in [`tree_yaml_entries`]).
fn yaml_json_mapping<S: OutputSink>(
    out: &mut S,
    map: &serde_json::Map<String, serde_json::Value>,
    level: usize,
    inline: bool,
) -> std::io::Result<()> {
    let mut indent = !inline;
    for (k, v) in map {
        if indent {
            yaml_indent(out, level)?;
        }
        indent = true;
        yaml_key(out, "", k, level)?;
        out.write_char(':')?;
        yaml_json_value(out, v, level + 1)?;
    }
    Ok(())
}

/// Items of a non-empty JSON array at `level` (`inline` as in [`tree_yaml_entries`]).
fn yaml_json_sequence<S: OutputSink>(
    out: &mut S,
    items: &[serde_json::Value],
    level: usize,
    inline: bool,
) -> std::io::Result<()> {
    use serde_json::Value;
    let mut indent = !inline;
    for item in items {
        if indent {
            yaml_indent(out, level)?;
        }
        indent = true;
        out.write_char('-')?;
        match item {
            Value::Object(map) if !map.is_empty() => {
                out.write_char(' ')?;
                yaml_json_mapping(out, map, level + 1, true)?;
            }
            Value::Array(inner) if !inner.is_empty() => {
                out.write_char(' ')?;
                yaml_json_sequence(out, inner, level + 1, true)?;
            }
            _ => yaml_json_value(out, item, level + 1)?,
        }
    }
    Ok(())
}

/// Write `value` after a key's `:` or an item's `-`; nested entries go to `level`.
fn yaml_json_value<S: OutputSink>(out: &mut S, value: &serde_json::Value, level: usize) -> std::io::Result<()> {
    use serde_json::Value;
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.write_char('\n')?;
            yaml_json_mapping(out, map, level, false)
        }
        Value::Array(items) if !items.is_empty() => {
            out.write_char('\n')?;
            yaml_json_sequence(out, items, level, false)
        }
        Value::Object(_) => out.write_str(" {}\n"),
        Value::Array(_) => out.write_str(" []\n"),
        Value::String(s) => {
            out.write_char(' ')?;
            yaml_scalar(out, "", s)?;
            out.write_char('\n')
        }
        scalar => {
            out.write_char(' ')?;
            out.write_str(&scalar.to_string())?;
            out.write_char('\n')
        }
    }
}

#[cfg(test)]
mod fast_skip_tests {
    use super::{fast_skip_decides, Runner};
    use crate::parser::ast::{classify_fast_path, FastPathKind};
    use regex::Regex;

    /// Where a fast path decides, it consumes exactly what the regex consumes.
    #[test]
    fn fast_paths_agree_with_the_regex() {
        let patterns = [
            r"[^\n]*\n",
            r"[^\n]+\n",
            r"[^\r\n]*[\r\n]",
            r"[^\r\n]+[\r\n]",
            r"[!#][^\r\n]*[\r\n]",
            r"#[^\r\n]*[\r\n]+",
            r".*\n",
            r".*?\n",
            r"\s+",
            r"\S+",
            r"\d+",
            r"\w+",
            r"[\t ]+",
            r"[\r\n]+",
            r"[\r\n]",
            r"[^\r\n]+",
            r"[^\r\n]*",
            r"[^\r\n]",
            r" *[\r\n]+",
            r"\r?\n",
            r"(?:\r?\n)+",
            r"[^\r\n]*\r?\n",
            r"[^\r\n]+\r?\n",
            r"[!#][^\r\n]*\r?\n",
            r"#[^\r\n]*(?:\r?\n)+",
            r" *(?:\r?\n)+",
            r"\n",
            ".",
        ];
        let inputs = [
            "",
            "a",
            "abc\n",
            "\n",
            "\r\n",
            "  x",
            "\t\x0B\x0C\r\n x",
            "\u{a0}x",
            "x\u{a0}y",
            "café\n",
            "é",
            "\u{661}\u{662}\n",
            "123abc",
            "_a1 b",
            "#c\r\n\r\nx",
            "!c\n",
            "  \n\n",
            "\u{2028}\n",
            "\u{1F642}\n",
        ];
        for p in patterns {
            let kind = classify_fast_path(p);
            assert_ne!(kind, FastPathKind::None, "{p} has no fast path");
            let re = Regex::new(&format!("^(?:{p})")).unwrap();
            for input in inputs {
                let n = Runner::try_fast_skip_kind(kind, input);
                if fast_skip_decides(kind, input, n) {
                    let expected = re.find(input).map(|m| m.end()).filter(|&end| end > 0);
                    assert_eq!(n.filter(|&len| len > 0), expected, "{p:?} on {input:?}");
                }
            }
        }
    }
}
