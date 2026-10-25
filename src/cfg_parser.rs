//! `CfgParser`: the grammar-string front end — a lexer, an LALR(1) table,
//! and the shift/reduce engine, wired together.
//!
//! A grammar is a string of `;`-separated productions:
//!
//! - the **first left-hand side is the start symbol**;
//! - a **quoted** right-hand-side symbol is a terminal, an **unquoted** one is
//!   a nonterminal;
//! - `|` is alternation (separate productions), `epsilon` is the empty symbol;
//! - `;`, `|`, whitespace, and `->` are recognized only outside quotes.
//!   Inside a terminal, `\\` and `\"` are escapes. An empty terminal and an
//!   unclosed quote are errors.
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
//! string). Positive and negated classes are expanded into character
//! terminals before the LALR table is built; class-runs stay lexer
//! terminals. The lexer takes the longest match across every terminal kind.
//! See [`crate::lexer`].

use crate::engine::Reduction;
use crate::exec::Exec;
use crate::grammar::{self, GrammarRegistry, Symbol};
use crate::lalr::LRCompiler;
use crate::lexer::{self, Lexer, Token, TokenSpan};

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

/// A compiled context-free grammar: a cached [`Lexer`] and an LALR(1)
/// interpreter over interned symbol ids.
///
/// See the module docs for the grammar string format.
pub struct CfgParser {
    lexer: Lexer,
    exec: Exec,
    /// Distinct `(state, symbol)` slots whose actions collided at compile time.
    conflicts: usize,
}

impl CfgParser {
    /// Compile a grammar string.
    ///
    /// The first left-hand side becomes the start symbol; the terminal set is
    /// derived from the productions and drives [`Lexer`] tokenization. Fails
    /// with [`CfgError`] on a production without `->` or an empty grammar.
    pub fn new(grammar: &str) -> Result<Self, CfgError> {
        let mut registry = GrammarRegistry::new();
        let productions = grammar::split_grammar(grammar)?;
        if productions.is_empty() {
            return Err(CfgError {
                message: "No start symbol".to_string(),
            });
        }
        let start_sym = productions[0].0.clone();
        for (lhs, rhs) in productions {
            let expr = grammar::parse_rhs(&rhs)?;
            registry.register(&lhs, expr)?;
        }
        Self::finish(registry, start_sym)
    }

    /// Compile a [`GrammarRegistry`] (built composable-style with
    /// [`Expr::or`][crate::grammar::Expr::or] / [`Expr::then`][crate::grammar::Expr::then],
    /// `expr(...).named(...)`, or `GrammarRegistry::register`): the same
    /// start symbol, terminal set, and LALR(1) table as [`CfgParser::new`].
    pub fn from_registry(registry: GrammarRegistry) -> Result<Self, CfgError> {
        let start_symbol = registry.start_symbol.clone().ok_or_else(|| CfgError {
            message: "No start symbol".to_string(),
        })?;
        let start_sym = match start_symbol {
            Symbol::NonTerminal(name) | Symbol::Terminal(name) => name,
            Symbol::End => {
                return Err(CfgError {
                    message: "No start symbol".to_string(),
                });
            }
        };
        Self::finish(registry, start_sym)
    }

    fn finish(mut registry: GrammarRegistry, start_symbol: String) -> Result<Self, CfgError> {
        let (productions, terminals) =
            grammar::expand_classes(std::mem::take(&mut registry.productions))?;
        registry.productions = productions;
        let first_chars = crate::exec::first_chars(&registry.productions, &start_symbol);
        let table = LRCompiler::compile::<String>(&registry);
        let conflicts = table.conflict_count();
        let exec = Exec::from_table(&table, &start_symbol, first_chars);
        let lexer = Lexer::new(terminals);
        Ok(Self {
            lexer,
            exec,
            conflicts,
        })
    }

    /// Anchored match: `true` when every token of `text` derives the start
    /// symbol. The lexer skips whitespace, so surrounding spaces still match.
    pub fn is_match(&self, text: &str) -> bool {
        self.exec.accepts_text(&self.lexer, text)
    }

    fn ids_of(&self, spans: &[TokenSpan]) -> Vec<u32> {
        spans
            .iter()
            .map(|span| match &span.token {
                Token::Terminal(name) => self.exec.id_of_name(name),
                Token::EOF => unreachable!("EOF token is excluded from slices"),
            })
            .collect()
    }

    /// Number of (state, symbol) action slots where two different LALR(1)
    /// actions collided and were resolved in production order. Zero means
    /// the grammar is conflict-free LALR(1); nonzero means the grammar is
    /// not LALR(1) and behavior on the colliding inputs depends on the
    /// order of the productions.
    pub fn conflict_count(&self) -> usize {
        self.conflicts
    }

    /// Tokenize `text` with the grammar's terminal set, recording the byte
    /// span of every token (a trailing EOF span is included).
    pub fn token_spans(&self, text: &str) -> Vec<TokenSpan> {
        self.lexer.tokenize_with_spans(text)
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
        let ids = self.ids_of(&spans[..n]);
        for start in 0..=n {
            let mut ends = Vec::new();
            for end in start..=n {
                if self.exec.accepts_ids(&ids[start..end]) {
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
        let ids = self.ids_of(&spans[start..end]);
        self.exec.trace_ids(&ids)
    }

    /// The longest prefix of `text` that derives the start symbol; `None`
    /// when no prefix (not even the empty one) matches. (A left-anchored
    /// `find`; useful for lookahead-style checks.)
    pub fn prefix_match(&self, text: &str) -> Option<Match> {
        let spans = self.token_spans(text);
        let n = spans.len().saturating_sub(1);
        let ids = self.ids_of(&spans[..n]);
        let end = self.exec.longest_prefix(&ids)?;
        let (start, stop) = Self::slice_bytes(&spans, 0, end);
        Some(Match::new(start, stop))
    }

    /// The longest suffix of `text` that derives the start symbol — the match
    /// ending exactly at the end of `text`, starting as far left as possible;
    /// `None` when no suffix matches. (A right-anchored `find`; useful for
    /// lookbehind-style checks.)
    pub fn suffix_match(&self, text: &str) -> Option<Match> {
        let spans = self.token_spans(text);
        let n = spans.len().saturating_sub(1);
        let ids = self.ids_of(&spans[..n]);
        for start in 0..=n {
            if self.exec.accepts_ids(&ids[start..]) {
                let (byte_start, byte_end) = Self::slice_bytes(&spans, start, n);
                return Some(Match::new(byte_start, byte_end));
            }
        }
        None
    }

    /// Find the leftmost match of this grammar anywhere in `text`.
    ///
    /// The match begins at the earliest char boundary from which the
    /// grammar can derive a slice, and extends as far as the grammar allows
    /// at that position (leftmost start, longest match). A non-empty slice
    /// is a candidate only when its tokens cover it exactly, so whitespace
    /// the lexer skipped is not part of the span. Returns byte offsets.
    pub fn find(&self, text: &str) -> Option<Match> {
        let (start, end) = self.find_bytes(text)?;
        Some(Match::new(start, end))
    }

    /// Leftmost char-boundary slice whose tokens cover it and that derives
    /// the start symbol. Longest end at that start wins.
    ///
    /// A nullable grammar matches the empty slice at byte 0, so no later
    /// start is examined. Every other start whose first character cannot
    /// begin a FIRST terminal is skipped without tokenizing.
    pub(crate) fn find_bytes(&self, text: &str) -> Option<(usize, usize)> {
        if self.exec.nullable() {
            return self
                .exec
                .longest_match(&self.lexer, text, 0)
                .map(|end| (0, end));
        }
        let mut start = 0;
        while start < text.len() {
            if self.exec.may_start(text, start) {
                if let Some(end) = self.exec.longest_match(&self.lexer, text, start) {
                    if end > start {
                        return Some((start, end));
                    }
                }
            }
            start = lexer::next_char_end(text, start);
        }
        None
    }

    /// Covered slice ends of `text` beginning at `start`, including `start`.
    pub(crate) fn covered_ends(&self, text: &str, start: usize) -> Vec<usize> {
        self.lexer.covered_ends(text, start)
    }

    /// `true` when the non-empty `text` tokenizes with the first token at
    /// byte 0 and the last token ending at `text.len()`.
    #[cfg(test)]
    pub(crate) fn covers_text(&self, text: &str) -> bool {
        let spans = self.token_spans(text);
        let count = spans.len().saturating_sub(1);
        count > 0 && spans[0].start == 0 && spans[count - 1].end == text.len()
    }

    /// Find the leftmost match (see [`CfgParser::find`]) and report the
    /// spans of the requested capture nonterminals, in the order given.
    ///
    /// A capture is the range of input covered by the last reduction of
    /// that nonterminal in the winning derivation; groups that did not
    /// participate are reported as `None`.
    pub fn captures(&self, text: &str, groups: &[&str]) -> Option<Captures> {
        let (start, end) = self.find_bytes(text)?;
        let slice = &text[start..end];
        let reductions = self.slice_reductions(slice, 0, self.token_count(slice))?;
        let whole = Match::new(start, end);
        let mut matches = vec![Some(whole)];
        for name in groups {
            let span = reductions.iter().rfind(|r| &r.lhs == name).map(|r| {
                let inner = self.reduction_bytes(slice, 0, r);
                Match::new(start + inner.start, start + inner.end)
            });
            matches.push(span);
        }
        Some(Captures { matches })
    }

    fn token_count(&self, text: &str) -> usize {
        self.token_spans(text).len().saturating_sub(1)
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

#[cfg(test)]
mod find_oracle {
    use super::*;

    fn bounds(text: &str) -> Vec<usize> {
        let mut bounds: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
        bounds.push(text.len());
        bounds
    }

    fn brute_find(parser: &CfgParser, text: &str) -> Option<(usize, usize)> {
        let bounds = bounds(text);
        for (start_i, &start) in bounds.iter().enumerate() {
            for &end in bounds[start_i..].iter().rev() {
                let slice = &text[start..end];
                if start != end && !parser.covers_text(slice) {
                    continue;
                }
                if parser.is_match(slice) {
                    return Some((start, end));
                }
            }
        }
        None
    }

    fn brute_ends(parser: &CfgParser, text: &str, start: usize) -> Vec<usize> {
        bounds(text)
            .into_iter()
            .filter(|&end| end >= start && (end == start || parser.covers_text(&text[start..end])))
            .collect()
    }

    #[test]
    fn find_matches_covered_slices() {
        let grammars = [
            r#"S -> "a""#,
            r#"S -> "ab""#,
            r#"S -> "a" "b""#,
            r#"S -> "ab" "c" | "b""#,
            r#"S -> "ab" "c" | "a""#,
            r#"S -> "a" | epsilon"#,
            r#"S -> epsilon"#,
            r#"S -> "a" S | epsilon"#,
            r#"S -> A "b"; A -> "a" A | epsilon"#,
            r#"S -> "[a-z]+""#,
            r#"S -> "[a-z]+" | "[a-c]+""#,
            r#"S -> "ab" | "[a-z]+""#,
            r#"S -> "[^abc]""#,
            r#"S -> "é""#,
            r#"S -> "any""#,
        ];
        let texts = [
            "", "a", "ab", "abc", "a b", "  a", "xa", "xaa", "baaa", "aaa", "é", "aéé", "ab ",
            "xab", "abc!", "1abcd", "   ", "b a", "abcd", r#""hi""#, "x\"ab",
        ];
        for grammar in grammars {
            let parser = CfgParser::new(grammar).unwrap();
            for text in texts {
                assert_eq!(
                    parser.find_bytes(text),
                    brute_find(&parser, text),
                    "{grammar} on {text:?}"
                );
                let mut start = 0;
                while start <= text.len() {
                    assert_eq!(
                        parser.covered_ends(text, start),
                        brute_ends(&parser, text, start),
                        "ends {grammar} @ {start} in {text:?}"
                    );
                    if start == text.len() {
                        break;
                    }
                    start = lexer::next_char_end(text, start);
                }
            }
        }
    }
}
