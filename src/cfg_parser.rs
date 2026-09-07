//! `CfgParser`: grammar-string front end (lexer + LALR(1) table + engine).

use std::collections::HashSet;

use crate::engine::{Engine, Reduction};
use crate::grammar::{GrammarRegistry, Symbol};
use crate::lalr::{LRCompiler, LalrTable};
use crate::lexer::{Lexer, Token, TokenSpan};

#[derive(Debug, Clone)]
pub struct CfgError {
    message: String,
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

pub struct CfgParser {
    table: LalrTable<String>,
    start_symbol: String,
    terminals: HashSet<String>,
}

impl CfgParser {
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
                registry.register(lhs, expr);
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

    pub fn is_match(&self, text: &str) -> bool {
        let lexer = Lexer::new(self.terminals.clone());
        let symbols: Vec<String> = lexer
            .tokenize(text)
            .into_iter()
            .filter_map(|token| match token {
                Token::Terminal(symbol) => Some(symbol),
                Token::EOF => None,
            })
            .collect();
        let mut engine = Engine::new(0);
        match engine.run(&self.table, symbols) {
            Ok(result) => result == self.start_symbol,
            Err(_) => false,
        }
    }

    /// Number of (state, symbol) action slots where two different LALR(1)
    /// actions collided and were resolved in production order. Zero means
    /// the grammar is conflict-free LALR(1); nonzero means the grammar is
    /// not LALR(1) and behavior on the colliding inputs depends on the
    /// order of the productions.
    pub fn conflict_count(&self) -> usize {
        self.table.conflict_count()
    }

    fn tokenize(&self, text: &str) -> Vec<TokenSpan> {
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

    /// Leftmost derivable prefix at token position `start`, as the largest
    /// end index `e` (with `start <= e <= n`) such that `tokens[start..e]`
    /// is derived by the grammar; `None` when no prefix at all derives.
    fn leftmost_end(&self, spans: &[TokenSpan], start: usize, n: usize) -> Option<usize> {
        let mut best = None;
        for end in start..=n {
            let symbols = Self::slice_symbols(spans, start, end);
            let mut engine = Engine::new(0);
            if engine.run(&self.table, symbols).is_ok() {
                best = Some(end);
            }
        }
        best
    }

    /// Find the leftmost match of this grammar anywhere in `text`.
    ///
    /// The match begins at the earliest position from which the grammar
    /// can derive a prefix of the remaining input, and extends as far as
    /// the grammar allows at that position (leftmost start, longest
    /// match). Returns the byte span of the match.
    pub fn find(&self, text: &str) -> Option<Match> {
        let spans = self.tokenize(text);
        let n = spans.len().saturating_sub(1); // exclude the trailing EOF token
        for start in 0..=n {
            if let Some(end) = self.leftmost_end(&spans, start, n) {
                let (b0, b1) = Self::slice_bytes(&spans, start, end);
                return Some(Match::new(b0, b1));
            }
        }
        None
    }

    /// Find the leftmost match (see [`CfgParser::find`]) and report the
    /// spans of the requested capture nonterminals, in the order given.
    ///
    /// A capture is the range of input covered by the last reduction of
    /// that nonterminal in the winning derivation; groups that did not
    /// participate are reported as `None`.
    pub fn captures(&self, text: &str, groups: &[&str]) -> Option<Captures> {
        let spans = self.tokenize(text);
        let n = spans.len().saturating_sub(1);
        let mut winner = None;
        for start in 0..=n {
            if let Some(end) = self.leftmost_end(&spans, start, n) {
                winner = Some((start, end));
                break;
            }
        }
        let (start, end) = winner?;

        let symbols = Self::slice_symbols(&spans, start, end);
        let mut engine = Engine::new(0);
        let (_start_symbol, reductions): (String, Vec<Reduction<String>>) =
            engine.run_traced(&self.table, symbols).ok()?;

        let mut matches = vec![Some(Self::match_bytes(&spans, start, end))];
        for name in groups {
            let span = reductions
                .iter()
                .rfind(|r| &r.lhs == name)
                .map(|r| Self::reduction_bytes(&spans, start, r.start, r.end));
            matches.push(span);
        }
        Some(Captures { matches })
    }

    fn match_bytes(spans: &[TokenSpan], start: usize, end: usize) -> Match {
        let (b0, b1) = Self::slice_bytes(spans, start, end);
        Match::new(b0, b1)
    }

    /// Map a token range (relative to the matched slice starting at token
    /// `slice_start`) to a byte span in the original text.
    fn reduction_bytes(spans: &[TokenSpan], slice_start: usize, t0: usize, t1: usize) -> Match {
        if t0 == t1 {
            // Zero-width capture: at the boundary before token slice_start + t0.
            let at = spans[slice_start + t0].start;
            Match::new(at, at)
        } else {
            let (b0, b1) = Self::slice_bytes(spans, slice_start + t0, slice_start + t1);
            Match::new(b0, b1)
        }
    }
}
