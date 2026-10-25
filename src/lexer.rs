//! The maximal-munch tokenizer for grammar terminals.
//!
//! Given the terminal set of a grammar, the lexer scans input text and emits
//! one [`Token`] per match, skipping whitespace. The longest match wins. On
//! a tie the higher rank wins: a quoted string, then a literal, then a
//! class-run, then a positive class, then a negated class. A zero-length
//! literal is never a match. Unknown characters are still emitted (as
//! single-character tokens) so the parser can reject them precisely, unless
//! the terminal set contains the negated-class fallback, which stands for
//! every character no other terminal took.
//!
//! Recognized terminal shapes:
//!
//! | Terminal     | Emits                                                       |
//! |--------------|-------------------------------------------------------------|
//! | `"literal"`  | the exact literal                                           |
//! | `"[a-z]"`    | one character from the class (ranges like `a-z` supported)  |
//! | `"[^a-z]"`    | one character NOT in the class (negated class)              |
//! | `"[a-z]+"`   | a maximal-munch run: the longest stretch of class characters |
//! | `"any"`      | a whole double-quoted string, matched as one unit           |
//!
//! ```
//! use std::collections::HashSet;
//! use RegExt::lexer::{Lexer, Token};
//!
//! let lexer = Lexer::new(HashSet::from(["ab".to_string()]));
//! assert_eq!(
//!     lexer.tokenize("ab ab"),
//!     vec![
//!         Token::Terminal("ab".to_string()),
//!         Token::Terminal("ab".to_string()),
//!         Token::EOF,
//!     ]
//! );
//! ```

use std::collections::HashSet;

/// A lexer token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A token carrying the grammar symbol it matched.
    Terminal(String),
    /// End of input.
    EOF,
}

/// A lexer token with its byte span in the input text.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenSpan {
    pub token: Token,
    /// Byte offset of the first byte of the token.
    pub start: usize,
    /// Byte offset just past the last byte of the token.
    pub end: usize,
}

/// One terminal the scanner can emit, in propose order.
struct Term {
    name: String,
    rank: u8,
    kind: Kind,
}

enum Kind {
    /// The `"any"` quoted-string rule.
    Any,
    Run(HashSet<char>),
    Literal,
    Class(HashSet<char>),
    Negated(HashSet<char>),
}

/// A terminal match that ends at `end` (a byte offset, exclusive).
pub(crate) struct TermHit<'a> {
    pub name: &'a str,
    pub end: usize,
}

/// A symbol produced by [`Lexer::scan`], borrowed from the lexer when it is
/// a real terminal.
pub(crate) enum Lexeme<'a> {
    Term(&'a str),
    /// A character no terminal matched, and no OTHER fallback is configured.
    Raw(char),
}

/// A maximal-munch tokenizer built from a grammar's terminal set.
pub struct Lexer {
    /// Propose order: string rules, class-runs, literals, classes, negated
    /// classes. Equal rank keeps the earlier entry.
    terms: Vec<Term>,
    /// Terminal emitted for a character that no literal, class, or class-run
    /// mentions. Present only when the grammar has a negated class.
    other: Option<String>,
}

impl Lexer {
    /// Build a lexer from the grammar's terminals (see the module docs for
    /// the recognized terminal shapes).
    pub fn new(terminals: HashSet<String>) -> Self {
        let mut string_rules = Vec::new();
        let mut class_runs = Vec::new();
        let mut literals = Vec::new();
        let mut char_classes = Vec::new();
        let mut negated_classes = Vec::new();
        let mut other = None;

        for terminal in terminals {
            match classify(&terminal) {
                Classified::Other => other = Some(terminal),
                Classified::Any => string_rules.push(Term {
                    name: terminal,
                    rank: 5,
                    kind: Kind::Any,
                }),
                Classified::ClassRun(members) => class_runs.push(Term {
                    name: terminal,
                    rank: 3,
                    kind: Kind::Run(members),
                }),
                Classified::Negated(members) => negated_classes.push(Term {
                    name: terminal,
                    rank: 1,
                    kind: Kind::Negated(members),
                }),
                Classified::Class(members) => char_classes.push(Term {
                    name: terminal,
                    rank: 2,
                    kind: Kind::Class(members),
                }),
                Classified::Literal => literals.push(Term {
                    name: terminal,
                    rank: 4,
                    kind: Kind::Literal,
                }),
            }
        }

        // Longest match first so overlapping literals tokenize deterministically.
        // Equal lengths keep lexicographic order; equal rank then keeps the
        // earlier candidate.
        literals.sort_by(|a, b| {
            b.name
                .len()
                .cmp(&a.name.len())
                .then_with(|| a.name.cmp(&b.name))
        });
        string_rules.sort_by(|a, b| a.name.cmp(&b.name));
        class_runs.sort_by(|a, b| a.name.cmp(&b.name));
        char_classes.sort_by(|a, b| a.name.cmp(&b.name));
        negated_classes.sort_by(|a, b| a.name.cmp(&b.name));

        let mut terms = Vec::new();
        terms.append(&mut string_rules);
        terms.append(&mut class_runs);
        terms.append(&mut literals);
        terms.append(&mut char_classes);
        terms.append(&mut negated_classes);

        Self { terms, other }
    }

    /// The negated-class fallback name, when the terminal set has one.
    pub(crate) fn other_name(&self) -> Option<&str> {
        self.other.as_deref()
    }

    /// Tokenize and record the byte span of every token.
    pub fn tokenize_with_spans(&self, input: &str) -> Vec<TokenSpan> {
        let mut spans = Vec::new();
        self.scan(input, |lexeme, start, end| {
            let symbol = match lexeme {
                Lexeme::Term(name) => name.to_string(),
                Lexeme::Raw(ch) => ch.to_string(),
            };
            spans.push(TokenSpan {
                token: Token::Terminal(symbol),
                start,
                end,
            });
        });
        spans.push(TokenSpan {
            token: Token::EOF,
            start: input.len(),
            end: input.len(),
        });
        spans
    }

    /// Tokenize `input`; the result always ends with [`Token::EOF`].
    pub fn tokenize(&self, input: &str) -> Vec<Token> {
        self.tokenize_with_spans(input)
            .into_iter()
            .map(|span| span.token)
            .collect()
    }

    /// Walk every non-whitespace token. Names of real terminals are borrowed
    /// from the lexer; a raw character is only allocated by the caller.
    pub(crate) fn scan<'a>(&'a self, input: &str, mut f: impl FnMut(Lexeme<'a>, usize, usize)) {
        let mut i = 0;
        while i < input.len() {
            let ch = char_at(input, i);
            if ch.is_whitespace() {
                i += ch.len_utf8();
                continue;
            }
            if let Some(hit) = self.best_match(input, i, input.len()) {
                let end = hit.end;
                f(Lexeme::Term(hit.name), i, end);
                i = end;
            } else {
                let end = i + ch.len_utf8();
                if let Some(name) = self.other.as_deref() {
                    f(Lexeme::Term(name), i, end);
                } else {
                    f(Lexeme::Raw(ch), i, end);
                }
                i = end;
            }
        }
    }

    /// Longest match at byte `i` that ends at or before `limit`. `limit` and
    /// `i` are char boundaries. On a tie the higher rank wins; equal rank
    /// keeps the earlier candidate.
    pub(crate) fn best_match(&self, input: &str, i: usize, limit: usize) -> Option<TermHit<'_>> {
        if i >= limit || i >= input.len() {
            return None;
        }
        let ch = char_at(input, i);
        let mut best_index: Option<usize> = None;
        let mut best_end = i;
        let mut best_rank: u8 = 0;
        for (index, term) in self.terms.iter().enumerate() {
            let Some(end) = match_term(term, input, i, limit, ch) else {
                continue;
            };
            if end <= i {
                continue;
            }
            let better = best_index.is_none()
                || end > best_end
                || (end == best_end && term.rank > best_rank);
            if better {
                best_index = Some(index);
                best_end = end;
                best_rank = term.rank;
            }
        }
        best_index.map(|index| TermHit {
            name: &self.terms[index].name,
            end: best_end,
        })
    }

    /// Byte ends `e` such that `text[start..e]` is covered by its own tokens,
    /// plus `start` itself (the empty slice). Leading whitespace yields only
    /// the empty end: the first token would not sit at byte 0 of the slice.
    ///
    /// Along a covered slice every token but the last is the greedy token of
    /// the suffix (a longer match also fits). The last token is the longest
    /// match that ends at the slice. Ends strictly inside a greedy token are
    /// the covered ends of that shorter window, where the greedy match no
    /// longer fits and a shorter terminal or raw characters do.
    pub(crate) fn covered_ends(&self, text: &str, start: usize) -> Vec<usize> {
        let mut ends = vec![start];
        if start >= text.len() || char_at(text, start).is_whitespace() {
            return ends;
        }
        // (position, exclusive limit). Both are char boundaries.
        let mut stack = vec![(start, text.len())];
        while let Some((mut pos, limit)) = stack.pop() {
            if pos >= limit || pos >= text.len() {
                continue;
            }
            if pos != start && char_at(text, pos).is_whitespace() {
                pos = skip_ws(text, pos, limit);
                if pos >= limit || pos >= text.len() {
                    continue;
                }
            }
            if let Some(hit) = self.best_match(text, pos, limit) {
                ends.push(hit.end);
                let prev = prev_char_boundary(text, hit.end);
                if hit.end < limit {
                    stack.push((hit.end, limit));
                }
                if prev > pos {
                    stack.push((pos, prev));
                }
            } else {
                let ch = char_at(text, pos);
                let end = pos + ch.len_utf8();
                if end <= limit {
                    ends.push(end);
                    stack.push((end, limit));
                }
            }
        }
        ends.sort_unstable();
        ends.dedup();
        ends
    }
}

fn match_term(term: &Term, input: &str, i: usize, limit: usize, ch: char) -> Option<usize> {
    match &term.kind {
        Kind::Any => {
            if ch != '"' {
                return None;
            }
            quoted_end(input, i, limit)
        }
        Kind::Run(members) => run_end(input, i, limit, members),
        Kind::Literal => {
            let len = term.name.len();
            if len == 0 || i + len > limit {
                return None;
            }
            if input[i..].starts_with(term.name.as_str()) {
                Some(i + len)
            } else {
                None
            }
        }
        Kind::Class(members) => {
            if !members.contains(&ch) {
                return None;
            }
            let end = i + ch.len_utf8();
            (end <= limit).then_some(end)
        }
        Kind::Negated(members) => {
            if members.contains(&ch) {
                return None;
            }
            let end = i + ch.len_utf8();
            (end <= limit).then_some(end)
        }
    }
}

fn run_end(input: &str, i: usize, limit: usize, members: &HashSet<char>) -> Option<usize> {
    if i >= limit {
        return None;
    }
    let ch = char_at(input, i);
    if !members.contains(&ch) {
        return None;
    }
    let mut end = i + ch.len_utf8();
    while end < limit {
        let c = char_at(input, end);
        if !members.contains(&c) {
            break;
        }
        end += c.len_utf8();
    }
    Some(end)
}

fn quoted_end(input: &str, i: usize, limit: usize) -> Option<usize> {
    // `input[i]` is the opening quote. A backslash escapes one character.
    // The match fails when the closing quote lies past `limit`.
    let mut j = i + 1;
    while j < limit {
        let ch = char_at(input, j);
        match ch {
            '\\' => {
                let next = j + ch.len_utf8();
                if next >= limit {
                    return None;
                }
                let escaped = char_at(input, next);
                j = next + escaped.len_utf8();
            }
            '"' => return Some(j + ch.len_utf8()),
            _ => j += ch.len_utf8(),
        }
    }
    None
}

enum Classified {
    Other,
    Any,
    ClassRun(HashSet<char>),
    Negated(HashSet<char>),
    Class(HashSet<char>),
    Literal,
}

/// Same shapes [`Lexer::new`] recognizes. A leading NUL is the synthetic
/// OTHER terminal, never a user literal. Class-runs are detected before
/// negated classes, so `[^a]+` stays a run of `^` and `a`.
fn classify(terminal: &str) -> Classified {
    if terminal.starts_with('\u{0}') {
        return Classified::Other;
    }
    if terminal == "any" {
        return Classified::Any;
    }
    let chars: Vec<char> = terminal.chars().collect();
    if chars.len() >= 4
        && chars[0] == '['
        && chars[chars.len() - 1] == '+'
        && chars[chars.len() - 2] == ']'
    {
        let inner: String = chars[1..chars.len() - 2].iter().collect();
        return Classified::ClassRun(parse_class_members(&inner).unwrap_or_default());
    }
    if chars.len() >= 4 && chars[0] == '[' && chars[1] == '^' && chars[chars.len() - 1] == ']' {
        let inner: String = chars[2..chars.len() - 1].iter().collect();
        return Classified::Negated(parse_class_members(&inner).unwrap_or_default());
    }
    if chars.len() >= 3 && chars[0] == '[' && chars[chars.len() - 1] == ']' {
        let inner: String = chars[1..chars.len() - 1].iter().collect();
        return Classified::Class(parse_class_members(&inner).unwrap_or_default());
    }
    Classified::Literal
}

/// Characters that can begin a match of this terminal, or `None` when that
/// set is open-ended (a negated class, or the OTHER fallback).
pub(crate) fn terminal_start_chars(terminal: &str) -> Option<HashSet<char>> {
    match classify(terminal) {
        Classified::Other | Classified::Negated(_) => None,
        Classified::Any => Some(HashSet::from(['"'])),
        Classified::ClassRun(members) | Classified::Class(members) => Some(members),
        Classified::Literal => match terminal.chars().next() {
            Some(ch) => Some(HashSet::from([ch])),
            None => Some(HashSet::new()),
        },
    }
}

pub(crate) fn char_at(text: &str, index: usize) -> char {
    text[index..]
        .chars()
        .next()
        .expect("index is a char boundary")
}

pub(crate) fn next_char_end(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return index;
    }
    index + char_at(text, index).len_utf8()
}

pub(crate) fn prev_char_boundary(text: &str, index: usize) -> usize {
    let mut prev = index - 1;
    while !text.is_char_boundary(prev) {
        prev -= 1;
    }
    prev
}

pub(crate) fn skip_ws(text: &str, mut index: usize, limit: usize) -> usize {
    while index < limit {
        let ch = char_at(text, index);
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

/// Members of a class body (`a`, `a-z`, a hyphen at either edge). A range
/// whose start sorts after its end is an error; equal endpoints are one
/// character. `inner` is the text between the brackets, without a leading
/// `^`.
pub(crate) fn parse_class_members(inner: &str) -> Result<HashSet<char>, String> {
    let chars: Vec<char> = inner.chars().collect();
    let mut members = HashSet::new();
    let mut i = 0;
    while i < chars.len() {
        if i + 2 < chars.len() && chars[i + 1] == '-' {
            let start = chars[i];
            let end = chars[i + 2];
            if start > end {
                return Err(format!("inverted character range '{start}-{end}' in class"));
            }
            for c in start..=end {
                members.insert(c);
            }
            i += 3;
        } else {
            members.insert(chars[i]);
            i += 1;
        }
    }
    Ok(members)
}
