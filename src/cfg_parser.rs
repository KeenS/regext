//! `CfgParser`: the grammar-string front end — a lexer, an LALR(1) table,
//! and the shift/reduce engine, wired together.
//!
//! A grammar is a string of `;`-separated productions:
//!
//! - the **first left-hand side is the start symbol**;
//! - a **quoted** right-hand-side symbol is a terminal, an **unquoted** one is
//!   a nonterminal;
//! - `|` is alternation (separate productions), `epsilon` is the empty symbol.
//!
//! ```
//! use RegExt::{Captures, CfgParser, Match};
//!
//! let parser = CfgParser::new(r#"S -> "x" G "y"; G -> "a" | "b""#).unwrap();
//! assert!(parser.is_match("xay"));                 // anchored
//! assert_eq!(parser.find("zzxbyq"), Some(Match::new(2, 5)));
//! assert_eq!(
//!     parser.captures("xay", &["G"]),
//!     Some(Captures { matches: vec![Some(Match::new(0, 3)), Some(Match::new(1, 2))] })
//! );
//! ```
//!
//! Terminals may be plain literals, character classes (`"[a-z]"`),
//! maximal-munch class runs (`"[a-z]+"`), or `"any"` (a whole double-quoted
//! string) — see [`crate::lexer`].

use std::collections::HashSet;

use crate::engine::{Engine, Reduction};
use crate::grammar::{GrammarRegistry, Symbol};
use crate::lalr::{LRCompiler, LalrTable};
use crate::lexer::{Lexer, Token, TokenSpan};

/// Error returned by [`CfgParser::new`]: the grammar string was malformed
/// (a production without `->`, or no productions at all).
#[derive(Debug, Clone)]
pub struct CfgError {
    message: String,
}

impl CfgError {
    /// Wrap a message (used by the composable [`crate::grammar`] front end
    /// for bound rejections the grammar-string dialect cannot produce).
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CfgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CFG Error: {}", self.message)
    }
}

impl std::error::Error for CfgError {}

/// A match span in byte offsets into the input text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    /// Byte offset of the first matched byte.
    pub start: usize,
    /// Byte offset just past the last matched byte.
    pub end: usize,
}

impl Match {
    /// Make a span from its half-open byte offsets.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// The leftmost match plus the spans of the requested capture nonterminals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captures {
    /// `matches[0]` is the span of the whole match. `matches[i]` for
    /// `i >= 1` is the span of the `i`-th requested capture nonterminal —
    /// its last occurrence in the winning derivation — or `None` when it
    /// did not participate in the match.
    pub matches: Vec<Option<Match>>,
}

/// A compiled context-free grammar: terminals (for the [`Lexer`]), a start
/// symbol, and an LALR(1) action/goto table driving the [`Engine`].
///
/// See the module docs for the grammar string format.
pub struct CfgParser {
    table: LalrTable<String>,
    start_symbol: String,
    terminals: HashSet<String>,
}

impl CfgParser {
    /// Compile a grammar string.
    ///
    /// The first left-hand side becomes the start symbol; the terminal set is
    /// derived from the productions and drives [`Lexer`] tokenization. Fails
    /// with [`CfgError`] on a production without `->` or an empty grammar.
    pub fn new(grammar: &str) -> Result<Self, CfgError> {
        let mut registry = GrammarRegistry::new();
        let mut start_symbol = None;

        for prod_str in grammar.split(';') {
            let prod_str = prod_str.trim();
            if prod_str.is_empty() {
                continue;
            }

            if let Some(idx) = prod_str.find("->") {
                let lhs = prod_str[..idx].trim();
                let rhs = prod_str[idx + 2..].trim();

                if start_symbol.is_none() {
                    start_symbol = Some(lhs.to_string());
                }

                let expr = GrammarRegistry::expr(rhs);
                registry.register(lhs, expr)?;
            } else {
                return Err(CfgError {
                    message: format!("Invalid production: {}", prod_str),
                });
            }
        }

        let start_sym = start_symbol.ok_or_else(|| CfgError {
            message: "No start symbol".to_string(),
        })?;
        // Terminals are the terminal symbols that actually occur in productions,
        // so choice/sequence syntax like `abc|def` yields `abc` and `def`.
        let terminals: HashSet<String> = registry
            .productions
            .iter()
            .flat_map(|(_, rhs)| rhs.iter())
            .filter_map(|sym| match sym {
                Symbol::Terminal(t) => Some(t.clone()),
                Symbol::NonTerminal(_) => None,
            })
            .collect();
        let table = LRCompiler::compile::<String>(&registry);

        Ok(Self {
            table,
            start_symbol: start_sym,
            terminals,
        })
    }

    /// Compile a [`GrammarRegistry`] (built composable-style with
    /// [`Expr::or`][crate::grammar::Expr::or] / [`Expr::then`][crate::grammar::Expr::then],
    /// `expr(...).named(...)`, or `GrammarRegistry::register`): the same
    /// start symbol, terminal set, and LALR(1) table as [`CfgParser::new`].
    pub fn from_registry(registry: GrammarRegistry) -> Result<Self, CfgError> {
        let start_symbol = registry.start_symbol.clone().ok_or_else(|| CfgError {
            message: "No start symbol".to_string(),
        })?;
        let terminals: HashSet<String> = registry
            .productions
            .iter()
            .flat_map(|(_, rhs)| rhs.iter())
            .filter_map(|sym| match sym {
                Symbol::Terminal(t) => Some(t.clone()),
                Symbol::NonTerminal(_) => None,
            })
            .collect();
        let table = LRCompiler::compile::<String>(&registry);
        Ok(Self {
            table,
            start_symbol: start_symbol.into(),
            terminals,
        })
    }

    /// Anchored match: `true` when the ENTIRE input (all of its tokens)
    /// derives the start symbol.
    pub fn is_match(&self, text: &str) -> bool {
        let spans = self.token_spans(text);
        let n = spans.len().saturating_sub(1);
        self.derives(&spans, 0, n)
    }

    /// `true` when the token slice `spans[start..end]` derives the start
    /// symbol (an empty slice when `start == end`).
    fn derives(&self, spans: &[TokenSpan], start: usize, end: usize) -> bool {
        let symbols = Self::slice_symbols(spans, start, end);
        let mut engine = Engine::new(0);
        matches!(engine.run(&self.table, symbols), Ok(sym) if sym == self.start_symbol)
    }

    /// Number of (state, symbol) action slots where two different LALR(1)
    /// actions collided and were resolved in production order. Zero means
    /// the grammar is conflict-free LALR(1); nonzero means the grammar is
    /// not LALR(1) and behavior on the colliding inputs depends on the
    /// order of the productions.
    pub fn conflict_count(&self) -> usize {
        self.table.conflict_count()
    }

    /// Tokenize `text` with the grammar's terminal set, recording the byte
    /// span of every token (a trailing EOF span is included).
    pub fn token_spans(&self, text: &str) -> Vec<TokenSpan> {
        Lexer::new(self.terminals.clone()).tokenize_with_spans(text)
    }

    /// Symbols of the token slice `spans[start..end]` (EOF excluded by
    /// the caller passing `end <= spans.len() - 1`).
    fn slice_symbols(spans: &[TokenSpan], start: usize, end: usize) -> Vec<String> {
        spans[start..end]
            .iter()
            .map(|t| match &t.token {
                Token::Terminal(s) => s.clone(),
                Token::EOF => unreachable!("EOF token is excluded from slices"),
            })
            .collect()
    }

    /// Byte span of the token range `spans[start..end]`; an empty range is
    /// zero-width at the start position.
    fn slice_bytes(spans: &[TokenSpan], start: usize, end: usize) -> (usize, usize) {
        if start == end {
            (spans[start].start, spans[start].start)
        } else {
            (spans[start].start, spans[end - 1].end)
        }
    }

    /// Like [`CfgParser::find`], but a candidate `(start, end)` (token
    /// indices) is accepted only when `ok(start, end)` also holds. Among the
    /// accepted ends at the leftmost start, the LONGEST is preferred, so a
    /// failing long candidate falls back to a passing shorter one.
    pub fn find_tokens_with<F>(&self, text: &str, mut ok: F) -> Option<(usize, usize)>
    where
        F: FnMut(usize, usize) -> bool,
    {
        let spans = self.token_spans(text);
        let n = spans.len().saturating_sub(1);
        for start in 0..=n {
            let mut ends = Vec::new();
            for end in start..=n {
                if self.derives(&spans, start, end) {
                    ends.push(end);
                }
            }
            for end in ends.into_iter().rev() {
                if ok(start, end) {
                    return Some((start, end));
                }
            }
        }
        None
    }

    /// The reductions of the winning derivation of the token slice
    /// `spans[start..end]`, when it derives the start symbol.
    pub fn slice_reductions(
        &self,
        text: &str,
        start: usize,
        end: usize,
    ) -> Option<Vec<Reduction<String>>> {
        let spans = self.token_spans(text);
        let symbols = Self::slice_symbols(&spans, start, end);
        let mut engine = Engine::new(0);
        match engine.run_traced(&self.table, symbols) {
            Ok((sym, reductions)) if sym == self.start_symbol => Some(reductions),
            _ => None,
        }
    }

    /// The longest prefix of `text` that derives the start symbol; `None`
    /// when no prefix (not even the empty one) matches. (A left-anchored
    /// `find`; useful for lookahead-style checks.)
    pub fn prefix_match(&self, text: &str) -> Option<Match> {
        let spans = self.token_spans(text);
        let n = spans.len().saturating_sub(1);
        let mut best = None;
        for end in 0..=n {
            if self.derives(&spans, 0, end) {
                best = Some(end);
            }
        }
        let end = best?;
        Some(self.match_bytes(text, 0, end))
    }

    /// The longest suffix of `text` that derives the start symbol — the match
    /// ending exactly at the end of `text`, starting as far left as possible;
    /// `None` when no suffix matches. (A right-anchored `find`; useful for
    /// lookbehind-style checks.)
    pub fn suffix_match(&self, text: &str) -> Option<Match> {
        let spans = self.token_spans(text);
        let n = spans.len().saturating_sub(1);
        for start in 0..=n {
            if self.derives(&spans, start, n) {
                return Some(self.match_bytes(text, start, n));
            }
        }
        None
    }

    /// Find the leftmost match of this grammar anywhere in `text`.
    ///
    /// The match begins at the earliest position from which the grammar
    /// can derive a prefix of the remaining input, and extends as far as
    /// the grammar allows at that position (leftmost start, longest
    /// match). Returns the byte span of the match.
    pub fn find(&self, text: &str) -> Option<Match> {
        let (start, end) = self.find_tokens_with(text, |_, _| true)?;
        Some(self.match_bytes(text, start, end))
    }

    /// Find the leftmost match (see [`CfgParser::find`]) and report the
    /// spans of the requested capture nonterminals, in the order given.
    ///
    /// A capture is the range of input covered by the last reduction of
    /// that nonterminal in the winning derivation; groups that did not
    /// participate are reported as `None`.
    pub fn captures(&self, text: &str, groups: &[&str]) -> Option<Captures> {
        let (start, end) = self.find_tokens_with(text, |_, _| true)?;
        let reductions = self.slice_reductions(text, start, end)?;
        let whole = self.match_bytes(text, start, end);
        let mut matches = vec![Some(whole)];
        for name in groups {
            let span = reductions
                .iter()
                .rfind(|r| &r.lhs == name)
                .map(|r| self.reduction_bytes(text, start, r));
            matches.push(span);
        }
        Some(Captures { matches })
    }

    /// The byte span of the token slice `[start, end)`.
    pub fn match_bytes(&self, text: &str, start: usize, end: usize) -> Match {
        let spans = self.token_spans(text);
        let (b0, b1) = Self::slice_bytes(&spans, start, end);
        Match::new(b0, b1)
    }

    /// Map a reduction of the slice starting at token `slice_start` to a byte
    /// span in the original text. A zero-width reduction maps to the boundary
    /// before token `slice_start + r.start`.
    pub fn reduction_bytes(&self, text: &str, slice_start: usize, r: &Reduction<String>) -> Match {
        let spans = self.token_spans(text);
        if r.start == r.end {
            let at = spans[slice_start + r.start].start;
            Match::new(at, at)
        } else {
            let (b0, b1) = Self::slice_bytes(&spans, slice_start + r.start, slice_start + r.end);
            Match::new(b0, b1)
        }
    }
}
