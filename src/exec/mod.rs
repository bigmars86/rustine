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

pub mod arena;
pub mod out;
pub mod streaming;

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
    pub diagnostics: Vec<Diagnostic>, // structured diagnostics (Phase 3)
    pub capture_history: Vec<Vec<String>>,
    pub capture_names_history: Vec<Vec<Option<Arc<str>>>>,
    pub output: OutputTree,
    /// Flattened contiguous tree built from the compacted node arena.
    /// All serialization uses this after `execute_precompiled`.
    pub flat: Option<FlatTree>,
    pub error: Option<String>,
}

impl ExecutionResult {
    #[allow(dead_code)]
    fn new() -> Self {
        Self::with_capacity_hint(0)
    }

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
        }
    }

    pub fn run_grammar(mut self, name: &str) -> Result<ExecutionResult> {
        if !self.ctx.doc.grammars.contains_key(name) {
            return Err(GelError::runtime(format!("Grammar not found: {}", name), None));
        }
        // Arc::clone is O(1) — just an atomic increment (no deep-cloning Statement trees).
        let statements = self.stmt_cache.get(name).map(Arc::clone).unwrap_or_default();
        let mut result = ExecutionResult::with_capacity_hint(self.ctx.input.len());
        loop {
            let start = self.ctx.pos;
            for stmt in statements.iter() {
                match stmt {
                    Statement::Match(m) => {
                        let mut scope: CaptureVec<'a> = CaptureVec::new();
                        let mut name_scope: NameVec = NameVec::new();
                        if let Some((len, acts)) = self.eval_match(m, &mut scope, &mut name_scope)? {
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
                                self.last_captures = std::mem::take(&mut scope);
                                self.last_capture_names = std::mem::take(&mut name_scope);
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
                                result.actions.extend(actions_to_run.iter().cloned());
                                if !self.last_captures.is_empty() {
                                    result
                                        .capture_history
                                        .push(self.last_captures.iter().map(|c| c.to_string()).collect());
                                    result.capture_names_history.push(self.last_capture_names.to_vec());
                                }
                                result.trace(Severity::Info, format!("match consumed {} chars", total_consumed));
                                self.fire_triggers("after", &mut result.output);
                                break; // Python: break and restart after successful match
                            }
                        }
                    }
                    Statement::When(w) => {
                        let mut scope: CaptureVec<'a> = CaptureVec::new();
                        let mut name_scope: NameVec = NameVec::new();
                        if self.eval_when(w, &mut scope, &mut name_scope)? {
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
                            result.trace(Severity::Info, "when triggered".to_string());
                            self.fire_triggers("after", &mut result.output);
                            break; // Python: break and restart after successful when
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

    fn eval_match<'m>(
        &mut self,
        m: &'m MatchStatement,
        captures_out: &mut CaptureVec<'a>,
        names_out: &mut NameVec,
    ) -> Result<Option<(usize, &'m [FunctionCall])>> {
        for alt in &m.match_list.alternatives {
            if let Some((len, groups, names)) = self.eval_field_list(alt, m.case_insensitive)? {
                captures_out.extend(groups);
                names_out.extend(names);
                return Ok(Some((len, &m.actions)));
            }
        }
        Ok(None)
    }
    fn eval_when(
        &mut self,
        w: &WhenStatement,
        captures_out: &mut CaptureVec<'a>,
        names_out: &mut NameVec,
    ) -> Result<bool> {
        // Python checks ALL alternatives in a MatchList, not just the first
        for alt in &w.match_list.alternatives {
            if let Some((_, groups, names)) = self.eval_field_list(alt, false)? {
                captures_out.extend(groups);
                names_out.extend(names);
                return Ok(true);
            }
        }
        Ok(false)
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
                // Fast-path classification exists for this pattern.
                // If it returns None, the pattern doesn't match — skip the
                // full regex fallback since the fast-path is semantically complete.
                if let Some(n) = Self::try_fast_skip_kind(kind, rem) {
                    prof_inc!(EVAL_SKIP_FAST);
                    return Ok(Some(n));
                }
                return Ok(None);
            }
        }
        self.eval_expression(&s.pattern, rem, false, &[], &[])
            .map(|o| o.map(|(n, _)| n))
    }

    /// Attempt to match common skip regex patterns using memchr/byte scanning
    /// instead of the full regex engine.  Returns `Some(consumed)` on a fast-path
    /// hit, or `None` to fall back to the regex engine.
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
                let count = bytes.iter().take_while(|b| b.is_ascii_whitespace()).count();
                if count > 0 {
                    Some(count)
                } else {
                    None
                }
            }
            FastPathKind::NonWhitespace => {
                let count = bytes.iter().take_while(|b| !b.is_ascii_whitespace()).count();
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
            FastPathKind::NonCrLf => {
                if bytes[0] != b'\r' && bytes[0] != b'\n' {
                    Some(1)
                } else {
                    None
                }
            }
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
            FastPathKind::Dot => Some(1),
            FastPathKind::None => None,
        }
    }

    /// String-matching fast skip (convenience wrapper for tests).
    ///
    /// Production code now uses [`try_fast_skip_kind`] via pre-classified enum.
    pub(crate) fn try_fast_skip(pattern: &str, rem: &str) -> Option<usize> {
        let kind = crate::parser::ast::classify_fast_path(pattern);
        Self::try_fast_skip_kind(kind, rem)
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
        // Combined regex: use find() for quick accept/reject, then captures() for groups
        if let Some(combined) = &list.compiled_regex {
            prof_inc!(EVAL_FIELD_LIST_FAST);
            let rem = self.ctx.remaining();
            let window = &rem[..rem.len().min(1000)];
            if let Some(m) = combined.find(window) {
                if m.start() != 0 {
                    prof_inc!(REGEX_FIND_MISS);
                    return Ok(None);
                }
                prof_inc!(REGEX_FIND_HIT);
                let consumed = m.end();
                if combined.captures_len() > 1 {
                    if let Some(mat) = combined.captures(window) {
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
                } else {
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
                let window = &rem[..rem.len().min(1000)];
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
                    if rem.len() >= len {
                        let slice = &rem[..len];
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
                        if rem.len() >= len {
                            let slice = &rem[..len];
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
                if let Some(m) = compiled.find(&rem[..rem.len().min(1000)]) {
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
                    if rem.len() >= len {
                        let slice = &rem[..len];
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
                        if rem.len() >= len {
                            let slice = &rem[..len];
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
    // return_levels: 0 = no return, >0 = abort N grammar levels (Python do.return(levels=1) → each Grammar consumes 1)
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
                if sub_consumed > 0 {
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
                // Python semantics: Function.parse() ignores the grammar's return value
                // and only checks whether position advanced.
                // If input was consumed (result == 1 in Python), break the action loop —
                // this skips remaining actions (e.g. do.return()) after a successful
                // sub-grammar call, matching Python's _handle_match() for-loop break.
                if sub_consumed > 0 {
                    break;
                }
                last_action_name = Some(current_name);
                continue;
            }
            if &*act.name == "out.create" {
                // Python: builder.create(path, data) → builder.enter(path) → trigger(on_add) → builder.leave()
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    let value = act.args.get(1).map(|v| self.expr_to_string(v));
                    let value_ref = value.as_deref();
                    tree.exec(RuntimeAction::OutCreate {
                        path: &path,
                        value: value_ref,
                    });
                    tree.exec(RuntimeAction::OutEnter { path: &path });
                    self.fire_triggers("on_add", tree);
                    tree.exec(RuntimeAction::OutLeave);
                }
            } else if &*act.name == "out.add" {
                // Python: builder.add(path, data) → builder.enter(path) → trigger(on_add) → builder.leave()
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    let value = act.args.get(1).map(|v| self.expr_to_string(v));
                    let value_ref = value.as_deref();
                    tree.exec(RuntimeAction::OutAdd {
                        path: &path,
                        value: value_ref,
                    });
                    tree.exec(RuntimeAction::OutEnter { path: &path });
                    self.fire_triggers("on_add", tree);
                    tree.exec(RuntimeAction::OutLeave);
                }
            } else if &*act.name == "out.replace" {
                // Python: builder.add(path, data, replace=True) → builder.enter(path) → trigger(on_add) → builder.leave()
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    let value = act.args.get(1).map(|v| self.expr_to_string(v));
                    let value_ref = value.as_deref();
                    tree.exec(RuntimeAction::OutReplace {
                        path: &path,
                        value: value_ref,
                    });
                    tree.exec(RuntimeAction::OutEnter { path: &path });
                    self.fire_triggers("on_add", tree);
                    tree.exec(RuntimeAction::OutLeave);
                }
            } else if &*act.name == "out.add_attribute" {
                // Python: builder.add_attribute(...) → builder.enter(path) → trigger(on_add) → builder.leave()
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
                // Python: builder.open(path) → trigger(on_add) → stack[-1].on_leave.append(builder.leave)
                if let Some(path_expr) = act.args.first() {
                    let path = self.expr_to_cow(path_expr);
                    tree.exec(RuntimeAction::OutOpen { path: &path });
                    self.fire_triggers("on_add", tree);
                    pending_auto_leaves += 1;
                }
            } else if &*act.name == "out.enter" {
                // Python: builder.enter(path) → trigger(on_add) → stack[-1].on_leave.append(builder.leave)
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
                // Python: do_return(context, levels=1) returns -levels
                // Each Grammar.parse() level consumes one: returns result + 1
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
                // Process enqueue immediately (Python does this inline)
                // The first arg may be a regex literal or a variable referencing a define.
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
                // Python only clears non-persistent triggers: on_match_before, on_match_after, on_add
                // It does NOT clear persistent triggers or on_leave triggers
                self.trig_before.clear();
                self.trig_after.clear();
                self.trig_on_add.clear();
            }
            last_action_name = Some(current_name);
        }
        (return_levels, do_next, fail_error, pending_auto_leaves)
    }

    // Execute another grammar at current input position without terminating outer run.
    // Returns (consumed_chars, remaining_return_levels) where remaining_return_levels is
    // the levels left after this grammar consumed one (Python: return result + 1).
    fn run_inline_grammar(&mut self, name: &str, tree: &mut OutputTree, traces: &mut Vec<String>) -> (usize, i32) {
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
                        let mut scope: CaptureVec<'a> = CaptureVec::new();
                        let mut name_scope: NameVec = NameVec::new();
                        if let Some((len, acts)) = self.eval_match(m, &mut scope, &mut name_scope).ok().flatten() {
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
                                let (ret_lvl, next, _, auto_leaves) =
                                    self.execute_runtime_actions(actions_to_run, tree, traces);
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
                        let mut scope: CaptureVec<'a> = CaptureVec::new();
                        let mut name_scope: NameVec = NameVec::new();
                        if self.eval_when(w, &mut scope, &mut name_scope).unwrap_or(false) {
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
                            let (ret_lvl, next, _, auto_leaves) =
                                self.execute_runtime_actions(actions_to_run, tree, traces);
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
                            let (ret_lvl, next, _, auto_leaves) =
                                self.execute_runtime_actions(std::slice::from_ref(&substituted), tree, traces);
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
        // Consume one level for this grammar (Python: return result + 1)
        let remaining = (return_levels - 1).max(0);
        (self.ctx.pos - start, remaining)
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
                let segs = crate::exec::out::parse_path(&path_interp);
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
                    let segs = crate::exec::out::parse_path(&path_interp);
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
                // Interpolate $N and $name using last_captures context
                let mut out = String::with_capacity(s.len());
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
                                if let Some(val) = self.last_captures.get(idx) {
                                    out.push_str(&crate::exec::out::percent_encode(val));
                                }
                            }
                        } else if let Some((i, _)) = self
                            .last_capture_names
                            .iter()
                            .enumerate()
                            .find(|(_, n)| n.as_deref() == Some(token.as_str()))
                        {
                            if let Some(val) = self.last_captures.get(i) {
                                out.push_str(&crate::exec::out::percent_encode(val));
                            }
                        }
                    } else {
                        out.push(c);
                    }
                }
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
    let mut result = Runner::new(doc, input).run_grammar(grammar)?;
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
    /// Matches Python Gelatin's `format='none'` / Dummy generator.
    None,
}

/// Serialize an [`ExecutionResult`] into the chosen output format.
///
/// This produces the **full diagnostic envelope** (`consumed`, `actions`,
/// `traces`, etc.) — useful for debugging and programmatic inspection.
/// For Python-compatible output (bare data tree only), use
/// [`serialize_tree`] instead.
pub fn serialize_execution(result: &ExecutionResult, format: RuntimeFormat) -> String {
    match format {
        RuntimeFormat::Json => serialize_json(result),
        RuntimeFormat::Xml => serialize_xml(result),
        RuntimeFormat::Yaml => serialize_yaml(result),
        RuntimeFormat::None => String::new(),
    }
}

// =========================================================================
// Python-compatible "tree-only" serialization
// =========================================================================

/// Serialize the **data tree only** from an [`ExecutionResult`], matching the
/// output format of Python Gelatin exactly (no metadata envelope).
///
/// - **JSON**: 4-space-indented `json.dumps(tree.to_dict(), indent=4)` with
///   `@attr`, `#text`, and array-collapsing conventions.
/// - **XML**: `lxml`-compatible element tree with real XML attributes and text
///   content.  Root element is `<xml>` unless `out.set_root_name` was used.
/// - **YAML**: `yaml.dump(tree.to_dict())` with PyYAML quoting conventions.
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
    tree_json_node_direct(flat, flat.root(), &mut out, 0);
    out
}

/// Write tree JSON directly to a writer (truly streaming, no intermediate buffer).
fn tree_json_to_writer<W: std::io::Write>(exec: &ExecutionResult, writer: &mut W) -> std::io::Result<()> {
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    tree_json_node_stream(flat, flat.root(), &mut WriterSink(writer), 0)
}

// ---- String fast-path (inlined, no trait dispatch) ----

/// Emit a Node as pretty-printed JSON into a `String` — fast path that avoids
/// trait-dispatch overhead by calling `String::push` / `push_str` directly.
fn tree_json_node_direct(flat: &FlatTree, node: &FlatNode, out: &mut String, indent: usize) {
    let attrs = flat.attrs_of(node);
    let has_attrs = !attrs.is_empty();
    let has_text = node.text.as_ref().is_some_and(|t| !t.is_empty());
    let has_children = node.children_len > 0;

    if !has_attrs && !has_text && !has_children {
        out.push_str("{}");
        return;
    }

    out.push('{');
    let inner = indent + 1;
    let mut first = true;

    for (k, v) in attrs {
        if !first {
            out.push(',');
        }
        out.push('\n');
        json_write_indent_direct(out, inner);
        json_write_escaped_direct(out, &format!("@{k}"));
        out.push_str(": ");
        json_write_escaped_direct(out, v);
        first = false;
    }

    if has_text {
        if !first {
            out.push(',');
        }
        out.push('\n');
        json_write_indent_direct(out, inner);
        json_write_escaped_direct(out, "#text");
        out.push_str(": ");
        json_write_escaped_direct(out, node.text.as_ref().unwrap());
        first = false;
    }

    for group in flat.iter_child_groups(node) {
        if !first {
            out.push(',');
        }
        out.push('\n');
        json_write_indent_direct(out, inner);
        json_write_escaped_direct(out, &group[0].name);
        out.push_str(": ");

        if group.len() == 1 {
            tree_json_node_direct(flat, &group[0], out, inner);
        } else {
            out.push('[');
            for (i, ch) in group.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                json_write_indent_direct(out, inner + 1);
                tree_json_node_direct(flat, ch, out, inner + 1);
            }
            out.push('\n');
            json_write_indent_direct(out, inner);
            out.push(']');
        }
        first = false;
    }

    out.push('\n');
    json_write_indent_direct(out, indent);
    out.push('}');
}

#[inline]
fn json_write_indent_direct(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

fn json_write_escaped_direct(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let n = c as u32;
                out.push_str("\\u00");
                out.push(char::from(b"0123456789abcdef"[(n >> 4) as usize]));
                out.push(char::from(b"0123456789abcdef"[(n & 0xF) as usize]));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// ---- Streaming path (generic over OutputSink) ----

fn tree_json_node_stream<S: OutputSink>(
    flat: &FlatTree,
    node: &FlatNode,
    out: &mut S,
    indent: usize,
) -> std::io::Result<()> {
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
        json_write_indent_stream(out, inner)?;
        json_write_escaped_stream(out, &format!("@{k}"))?;
        out.write_str(": ")?;
        json_write_escaped_stream(out, v)?;
        first = false;
    }

    if has_text {
        if !first {
            out.write_char(',')?;
        }
        out.write_char('\n')?;
        json_write_indent_stream(out, inner)?;
        json_write_escaped_stream(out, "#text")?;
        out.write_str(": ")?;
        json_write_escaped_stream(out, node.text.as_ref().unwrap())?;
        first = false;
    }

    for group in flat.iter_child_groups(node) {
        if !first {
            out.write_char(',')?;
        }
        out.write_char('\n')?;
        json_write_indent_stream(out, inner)?;
        json_write_escaped_stream(out, &group[0].name)?;
        out.write_str(": ")?;

        if group.len() == 1 {
            tree_json_node_stream(flat, &group[0], out, inner)?;
        } else {
            out.write_char('[')?;
            for (i, ch) in group.iter().enumerate() {
                if i > 0 {
                    out.write_char(',')?;
                }
                out.write_char('\n')?;
                json_write_indent_stream(out, inner + 1)?;
                tree_json_node_stream(flat, ch, out, inner + 1)?;
            }
            out.write_char('\n')?;
            json_write_indent_stream(out, inner)?;
            out.write_char(']')?;
        }
        first = false;
    }

    out.write_char('\n')?;
    json_write_indent_stream(out, indent)?;
    out.write_char('}')
}

#[inline]
fn json_write_indent_stream<S: OutputSink>(out: &mut S, level: usize) -> std::io::Result<()> {
    for _ in 0..level {
        out.write_str("    ")?;
    }
    Ok(())
}

fn json_write_escaped_stream<S: OutputSink>(out: &mut S, s: &str) -> std::io::Result<()> {
    out.write_char('"')?;
    for ch in s.chars() {
        match ch {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\u{0008}' => out.write_str("\\b")?,
            '\u{000C}' => out.write_str("\\f")?,
            '\n' => out.write_str("\\n")?,
            '\r' => out.write_str("\\r")?,
            '\t' => out.write_str("\\t")?,
            c if (c as u32) < 0x20 => {
                let n = c as u32;
                out.write_str("\\u00")?;
                out.write_char(char::from(b"0123456789abcdef"[(n >> 4) as usize]))?;
                out.write_char(char::from(b"0123456789abcdef"[(n & 0xF) as usize]))?;
            }
            c => out.write_char(c)?,
        }
    }
    out.write_char('"')
}

// -- Tree-only XML --------------------------------------------------------

/// A `Write` adapter that replaces `&apos;` -> `'` on-the-fly.
///
/// `quick_xml` escapes `'` in double-quoted attribute values as `&apos;`, but
/// Python's lxml does not. This wrapper transparently rewrites the output
/// without buffering the entire XML document.
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

    // Root element name: use the tree root's name if set (via out.set_root_name),
    // otherwise default to "xml" (Python Gelatin default).
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

    // Python's lxml adds a trailing newline
    writer.write_all(b"\n")
}

/// Write a tree Node as a proper XML element (matching Python lxml output).
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

    // Text content -- use lxml-compatible escaping (only <, >, &; not ' or ")
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

/// Escape XML text content matching Python lxml behaviour.
/// Only escapes `<`, `>`, `&` -- single and double quotes are valid in text content.
fn xml_escape_text_content(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            _ => out.push(ch),
        }
    }
    out
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

fn tree_yaml(exec: &ExecutionResult) -> String {
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    let mut out = String::new();
    tree_yaml_node_direct(flat, flat.root(), &mut out, 0, false);
    out.push('\n');
    out
}

/// Write tree YAML directly to a writer (truly streaming, no intermediate buffer).
fn tree_yaml_to_writer<W: std::io::Write>(exec: &ExecutionResult, writer: &mut W) -> std::io::Result<()> {
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    tree_yaml_node_stream(flat, flat.root(), &mut WriterSink(&mut *writer), 0, false)?;
    writer.write_all(b"\n")
}

// ---- String fast-path (inlined, no trait dispatch) ----

fn tree_yaml_node_direct(flat: &FlatTree, node: &FlatNode, out: &mut String, indent: usize, inline: bool) {
    let attrs = flat.attrs_of(node);
    let has_attrs = !attrs.is_empty();
    let has_text = node.text.as_ref().is_some_and(|t| !t.is_empty());
    let has_children = node.children_len > 0;

    if !has_attrs && !has_text && !has_children {
        out.push_str("{}");
        return;
    }

    let mut entry_idx: usize = 0;

    for (k, v) in attrs {
        if entry_idx > 0 || !inline {
            push_yaml_indent_direct(out, indent);
        }
        out.push('\'');
        out.push('@');
        out.push_str(k);
        out.push('\'');
        out.push_str(": ");
        out.push_str(&yaml_quote_value(v));
        out.push('\n');
        entry_idx += 1;
    }

    if has_text {
        if entry_idx > 0 || !inline {
            push_yaml_indent_direct(out, indent);
        }
        out.push_str("'#text': ");
        out.push_str(&yaml_quote_value(node.text.as_ref().unwrap()));
        out.push('\n');
        entry_idx += 1;
    }

    for group in flat.iter_child_groups(node) {
        let name = &group[0].name;
        if entry_idx > 0 || !inline {
            push_yaml_indent_direct(out, indent);
        }
        out.push_str(&yaml_quote_key(name));
        out.push(':');

        if group.len() == 1 {
            out.push('\n');
            tree_yaml_node_direct(flat, &group[0], out, indent + 1, false);
        } else {
            out.push('\n');
            for ch in group {
                push_yaml_indent_direct(out, indent);
                out.push_str("- ");
                tree_yaml_node_direct(flat, ch, out, indent + 2, true);
            }
        }
        entry_idx += 1;
    }
}

#[inline]
fn push_yaml_indent_direct(out: &mut String, indent: usize) {
    const SPACES: &str = "                                                                ";
    let n = indent * 2;
    if n <= SPACES.len() {
        out.push_str(&SPACES[..n]);
    } else {
        for _ in 0..n {
            out.push(' ');
        }
    }
}

// ---- Streaming path (generic over OutputSink) ----

fn tree_yaml_node_stream<S: OutputSink>(
    flat: &FlatTree,
    node: &FlatNode,
    out: &mut S,
    indent: usize,
    inline: bool,
) -> std::io::Result<()> {
    let attrs = flat.attrs_of(node);
    let has_attrs = !attrs.is_empty();
    let has_text = node.text.as_ref().is_some_and(|t| !t.is_empty());
    let has_children = node.children_len > 0;

    if !has_attrs && !has_text && !has_children {
        return out.write_str("{}");
    }

    let mut entry_idx: usize = 0;

    for (k, v) in attrs {
        if entry_idx > 0 || !inline {
            push_yaml_indent_stream(out, indent)?;
        }
        out.write_char('\'')?;
        out.write_char('@')?;
        out.write_str(k)?;
        out.write_char('\'')?;
        out.write_str(": ")?;
        out.write_str(&yaml_quote_value(v))?;
        out.write_char('\n')?;
        entry_idx += 1;
    }

    if has_text {
        if entry_idx > 0 || !inline {
            push_yaml_indent_stream(out, indent)?;
        }
        out.write_str("'#text': ")?;
        out.write_str(&yaml_quote_value(node.text.as_ref().unwrap()))?;
        out.write_char('\n')?;
        entry_idx += 1;
    }

    for group in flat.iter_child_groups(node) {
        let name = &group[0].name;
        if entry_idx > 0 || !inline {
            push_yaml_indent_stream(out, indent)?;
        }
        out.write_str(&yaml_quote_key(name))?;
        out.write_char(':')?;

        if group.len() == 1 {
            out.write_char('\n')?;
            tree_yaml_node_stream(flat, &group[0], out, indent + 1, false)?;
        } else {
            out.write_char('\n')?;
            for ch in group {
                push_yaml_indent_stream(out, indent)?;
                out.write_str("- ")?;
                tree_yaml_node_stream(flat, ch, out, indent + 2, true)?;
            }
        }
        entry_idx += 1;
    }

    Ok(())
}

#[inline]
fn push_yaml_indent_stream<S: OutputSink>(out: &mut S, indent: usize) -> std::io::Result<()> {
    const SPACES: &str = "                                                                ";
    let n = indent * 2;
    if n <= SPACES.len() {
        out.write_str(&SPACES[..n])
    } else {
        for _ in 0..n {
            out.write_char(' ')?;
        }
        Ok(())
    }
}

/// Quote a YAML mapping key using PyYAML conventions.
/// Keys starting with `@` or `#` are single-quoted.
fn yaml_quote_key(k: &str) -> Cow<'_, str> {
    if k.starts_with('@') || k.starts_with('#') {
        Cow::Owned(format!("'{}'", k))
    } else {
        Cow::Borrowed(k)
    }
}

/// Quote a YAML scalar value using PyYAML conventions.
/// - Strings that look like dates (YYYY-MM-DD), numbers, booleans, or
///   contain special chars are single-quoted.
/// - Strings with single quotes inside get double-quoted.
fn yaml_quote_value(s: &str) -> Cow<'_, str> {
    if s.is_empty() {
        return Cow::Borrowed("''");
    }
    // Check if the value needs quoting
    let needs_quoting = s.contains(':')
        || s.contains('#')
        || s.contains('\'')
        || s.contains('"')
        || s.contains('\n')
        || s.contains('\r')
        || s.starts_with(' ')
        || s.ends_with(' ')
        || s.starts_with('{')
        || s.starts_with('[')
        || s.starts_with('*')
        || s.starts_with('&')
        || s.starts_with('!')
        || s.starts_with('%')
        || s.starts_with('|')
        || s.starts_with('>')
        || s.starts_with('@')
        || looks_like_number(s)
        || looks_like_date(s)
        || matches!(
            s,
            "true"
                | "false"
                | "yes"
                | "no"
                | "null"
                | "True"
                | "False"
                | "Yes"
                | "No"
                | "Null"
                | "TRUE"
                | "FALSE"
                | "YES"
                | "NO"
                | "NULL"
                | "on"
                | "off"
                | "On"
                | "Off"
                | "ON"
                | "OFF"
        );
    if !needs_quoting {
        return Cow::Borrowed(s);
    }
    // Use single quotes unless the string itself contains single quotes
    if !s.contains('\'') {
        Cow::Owned(format!("'{}'", s))
    } else if !s.contains('"') {
        Cow::Owned(format!("\"{}\"", s))
    } else {
        // Both quote types present — use single quotes with doubled escaping
        Cow::Owned(format!("'{}'", s.replace('\'', "''")))
    }
}

fn looks_like_number(s: &str) -> bool {
    s.parse::<f64>().is_ok()
}

fn looks_like_date(s: &str) -> bool {
    // Matches YYYY-MM-DD pattern
    s.len() == 10
        && s.as_bytes().get(4) == Some(&b'-')
        && s.as_bytes().get(7) == Some(&b'-')
        && s[..4].chars().all(|c| c.is_ascii_digit())
        && s[5..7].chars().all(|c| c.is_ascii_digit())
        && s[8..10].chars().all(|c| c.is_ascii_digit())
}

/// Serialize an [`ExecutionResult`] to JSON using `serde_json`.
///
/// The output tree follows Gelatin conventions: attributes are prefixed with
/// `@`, text content uses the `#text` key, and children with the same tag
/// name are collected into JSON arrays.
fn serialize_json(exec: &ExecutionResult) -> String {
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

    let result = json!({
        "consumed": exec.consumed,
        "actions": actions,
        "traces": exec.traces,
        "capture_history": exec.capture_history,
        "capture_names_history": exec.capture_names_history.iter().map(|scope| scope.iter().map(|n| n.as_deref().unwrap_or("").to_string()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "error": exec.error,
        "diagnostics": diagnostics,
        "output": output,
    });

    serde_json::to_string_pretty(&result).unwrap_or_default()
}

/// Convert an output-tree [`Node`](crate::exec::out::Node) into a
/// `serde_json::Value`, preserving the `@attr` / `#text` conventions.
///
/// Children are grouped by name in **insertion order** (matching the Python
/// Gelatin `OrderedDefaultDict` behaviour).  Same-named siblings that appear
/// more than once are collected into a JSON array.
fn output_node_to_value(flat: &FlatTree, node: &FlatNode) -> serde_json::Value {
    use serde_json::{Map, Value};

    let mut obj = Map::new();

    // Attributes with @ prefix
    for (k, v) in flat.attrs_of(node) {
        obj.insert(format!("@{}", k), Value::String(v.clone()));
    }

    // Text content (skip empty strings — Python Gelatin treats them as absent)
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

fn serialize_yaml(exec: &ExecutionResult) -> String {
    let mut out = String::new();
    out.push_str("execution:\n  consumed: ");
    out.push_str(&exec.consumed.to_string());
    out.push_str("\n  actions:\n");
    if exec.actions.is_empty() {
        out.push_str("    []\n");
    } else {
        for a in &exec.actions {
            out.push_str("    - name: ");
            out.push_str(&yaml_scalar(&a.name));
            if a.args.is_empty() {
                out.push('\n');
            } else {
                out.push_str("\n      args:\n");
                for arg in &a.args {
                    out.push_str("        - ");
                    match arg {
                        Expression::String(s) => {
                            out.push_str(&yaml_scalar(s));
                        }
                        Expression::Regex(r) => {
                            out.push_str(&yaml_scalar(r));
                        }
                        Expression::Number(n) => {
                            out.push_str(&n.to_string());
                        }
                        Expression::Variable(v) => {
                            out.push_str(&yaml_scalar(v));
                        }
                        Expression::Capture(i) => {
                            out.push_str(&format!("\"${}\"", i));
                        }
                        Expression::CaptureName(name) => {
                            out.push_str(&format!("\"${}\"", name));
                        }
                    }
                    out.push('\n');
                }
            }
        }
    }
    out.push_str("  traces:\n");
    if exec.traces.is_empty() {
        out.push_str("    []\n");
    } else {
        for t in &exec.traces {
            out.push_str("    - ");
            out.push_str(&yaml_scalar(t));
            out.push('\n');
        }
    }
    out.push_str("  capture_history:\n");
    if exec.capture_history.is_empty() {
        out.push_str("    []\n");
    } else {
        for scope in &exec.capture_history {
            out.push_str("    - [");
            for (i, val) in scope.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&yaml_scalar(val));
            }
            out.push_str("]\n");
        }
    }
    out.push_str("  capture_names_history:\n");
    if exec.capture_names_history.is_empty() {
        out.push_str("    []\n");
    } else {
        for scope in &exec.capture_names_history {
            out.push_str("    - [");
            for (i, val) in scope.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&yaml_scalar(val.as_deref().unwrap_or("")));
            }
            out.push_str("]\n");
        }
    }
    out.push_str("  error: ");
    if let Some(e) = &exec.error {
        out.push_str(&yaml_scalar(e));
    } else {
        out.push_str("null");
    }
    out.push_str("\n  output:\n");
    let flat = exec.flat.as_ref().expect("FlatTree not built");
    serialize_yaml_node(flat, flat.root(), &mut out, 2);
    out
}

fn yaml_scalar(s: &str) -> String {
    if s.chars()
        .any(|c| c.is_whitespace() || matches!(c, ':' | '-' | '#' | ',' | '[' | ']' | '{' | '}'))
    {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn serialize_yaml_node(flat: &FlatTree, n: &FlatNode, out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
    out.push_str("- name: ");
    out.push_str(&yaml_scalar(&n.name));
    out.push('\n');
    let n_attrs = flat.attrs_of(n);
    if !n_attrs.is_empty() {
        for (k, v) in n_attrs {
            for _ in 0..indent {
                out.push_str("  ");
            }
            out.push_str("  attribute: ");
            out.push_str(&yaml_scalar(k));
            out.push_str(": ");
            out.push_str(&yaml_scalar(v));
            out.push('\n');
        }
    }
    if let Some(txt) = &n.text {
        for _ in 0..indent {
            out.push_str("  ");
        }
        out.push_str("  text: ");
        out.push_str(&yaml_scalar(txt));
        out.push('\n');
    }
    if n.children_len > 0 {
        for _ in 0..indent {
            out.push_str("  ");
        }
        out.push_str("  children:\n");
        for ch in flat.children_of(n) {
            serialize_yaml_node(flat, ch, out, indent + 2);
        }
    }
}
