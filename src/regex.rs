//! Regex front end — patterns compile down to a CFG grammar and are matched
//! by the LALR(1) VM; a regex is thus a restricted, LALR(1) CFG. Patterns
//! can be written as strings (`Regex::new`) or built from composable
//! [`Pattern`] expressions with `or` / `then` / quantifiers / groups /
//! assertions (`Regex::from_pattern`) — same validation, same lowering.
//!
//! Supported pattern syntax:
//!   - literal characters (any run of plain characters)
//!   - `|`     alternation
//!   - `*`     zero or more repetitions (binds to the last character of a run)
//!   - `+`     one or more repetitions
//!   - `?`     zero or one occurrence
//!   - `(...)`  capture group, numbered by opening parenthesis, starting at 1
//!   - `\x`    escapes a special character so it matches literally
//!   - `(?=P)`  positive lookahead: asserts `P` follows here, consumes nothing
//!   - `(?!P)`  negative lookahead: asserts `P` does not follow here
//!   - `(?<=P)` positive lookbehind: asserts `P` precedes here, consumes nothing
//!   - `(?<!P)` negative lookbehind: asserts `P` does not precede here
//!
//! Character classes and aliases (finite sets — the regular case):
//!   - `[a-z]`  one character from the class; single members or `a-z` ranges
//!   - `[^a-z]` one character NOT in the class (negated)
//!   - `\d`     alias for `[0-9]`
//!   - `\w`     alias for `[a-zA-Z0-9_]`
//!
//! Bounded repetition (unrolled to a finite set of alternatives):
//!   - `X{n}`   exactly n copies of `X`
//!   - `X{n,}`  at least n copies of `X`
//!   - `X{n,m}` between n and m copies (requires `m >= n`)
//!     Bounds bind to the last character of a run (`ab{2}` = `a` then `bb`);
//!     the maximum bound is 100 copies.
//!
//! Non-capturing and atomic groups (neither consumes a group number):
//!   - `(?:P)`  non-capturing group — the body is inlined
//!   - `(?>P)`  atomic group — identical to `(?:P)` in this engine
//!
//! Possessive quantifiers `a*+`, `a++`, `a?+` are accepted as sugar and are
//! language-identical to the plain ones: this engine never backtracks, so
//! there is nothing for possession to control.
//!
//! Assertions are evaluated against the FULL input text at the position where
//! their placeholder sits in the winning derivation: a lookahead body must
//! match a PREFIX of the remainder there, a lookbehind body a SUFFIX of the
//! consumed text, and in both cases the body matches as far as possible.
//! Capture groups inside an assertion body are numbered in global pattern
//! order and capture when the assertion's branch is taken.
//!
//! `.` (any character), `\s`, empty alternatives, and empty or nested
//! assertion bodies are rejected: `.` is an infinite set and so not expressible
//! as CFG terminals; `\s` cannot match because whitespace is the token
//! separator (the lexer skips it); the rest are degenerate. The VM
//! deliberately stays a grammar engine. An assertion body that is not
//! conflict-free LALR(1) is refused with [`RegexError::CompileError`].
//!
//! Semantics (inherited from [`CfgParser`]):
//!   - `is_match`   the whole token stream must match. The lexer skips
//!                  whitespace, so surrounding spaces still match.
//!   - `find`       leftmost match at any position; the match extends as far
//!     as the grammar allows (leftmost start, longest match). Skipped
//!     whitespace is not part of the span.
//!   - `captures`   the leftmost match plus the span of each capture group
//!
//! ```
//! use RegExt::{Match, Regex};
//!
//! let re = Regex::new("a(b)+c").unwrap();
//! assert!(re.is_match("abbbc"));          // anchored: whole input
//! assert_eq!(re.find("xyabbcz"), Some(Match::new(2, 6)));
//! let caps = re.captures("abbc").unwrap();
//! assert_eq!(caps.matches, vec![Some(Match::new(0, 4)), Some(Match::new(2, 3))]);
//!
//! let lb = Regex::new("(?<=a)b").unwrap();
//! assert_eq!(lb.find("xab"), Some(Match::new(2, 3))); // `a` precedes, only `b` matches
//! let la = Regex::new("a(?=b)").unwrap();
//! assert_eq!(la.find("xab"), Some(Match::new(1, 2))); // `b` follows, only `a` matches
//! ```

use crate::cfg_parser::{Captures, CfgParser, Match};

/// Error returned by [`Regex::new`].
///
/// [`InvalidPattern`][Self::InvalidPattern] means the pattern syntax itself is
/// invalid in RegExt's dialect (for example a `.` or an empty alternative).
/// [`CompileError`][Self::CompileError] means the pattern is syntactically
/// fine but its CFG encoding is not conflict-free LALR(1), in which case the
/// message carries the number of table conflicts.
#[derive(Debug, Clone)]
pub enum RegexError {
    /// The pattern is not valid RegExt syntax (unbalanced groups, `.` any-char,
    /// empty alternative, quantifier without operand, dangling escape, ...).
    InvalidPattern(String),
    /// The pattern compiles to a grammar with LALR(1) conflicts, so it is
    /// refused rather than resolved in production order.
    CompileError(String),
}

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegexError::InvalidPattern(s) => write!(f, "Invalid pattern: {}", s),
            RegexError::CompileError(s) => write!(f, "Compile error: {}", s),
        }
    }
}

impl std::error::Error for RegexError {}

// =============================================================================
// Pattern AST
// =============================================================================

/// Which side of the current position a zero-width assertion inspects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertKind {
    /// The remainder of the text (lookahead).
    Lookahead,
    /// The text consumed so far (lookbehind).
    Lookbehind,
}

/// A composable regex expression — the pattern language as a value.
///
/// Build patterns with the combinators instead of one big string, then
/// compile with [`Regex::from_pattern`] — the exact same validation and
/// lowering as [`Regex::new`], so a composable pattern and its string form
/// behave identically:
///
/// | pattern string | composable                                                    |
/// |----------------|---------------------------------------------------------------|
/// | `ab`           | `Pattern::lit("a").then(Pattern::lit("b"))`                   |
/// | `a\|b`         | `Pattern::lit("a").or(Pattern::lit("b"))`                     |
/// | `a*` `a+` `a?` | `a.star()`  `a.plus()`  `a.opt()`                              |
/// | `a{2}` `a{2,}` `a{2,5}` | `a.times(2)` `a.at_least(2)` `a.between(2, 5)`      |
/// | `[a-z]` `[^a-z]` | `Pattern::class("a-z")` / `Pattern::neg_class("a-z")`      |
/// | `\d` `\w`       | `Pattern::digit()` / `Pattern::word()`                        |
/// | `(P)`          | `P.group()` — a capture group, numbered in pattern order      |
/// | `(?:P)`        | `P.non_capturing()`                                           |
/// | `(?=P)` `(?!P)` `(?<=P)` `(?<!P)` | `P.lookahead()` `P.neg_lookahead()` `P.lookbehind()` `P.neg_lookbehind()` |
///
/// The string dialect has no spelling for the empty pattern; the composable
/// API does — `Pattern::epsilon()`, so `a.or(Pattern::epsilon())` is the
/// composable `a?`. Degenerate bounds the string parser rejects (`a{3,2}`)
/// are constructible here and are refused at compile time instead, with the
/// same [`RegexError::InvalidPattern`].
#[derive(Debug, Clone)]
pub enum Pattern {
    /// A run of literal text.
    Lit(String),
    /// Patterns in order.
    Seq(Vec<Pattern>),
    /// Alternative patterns.
    Alt(Vec<Pattern>),
    /// Zero or more repetitions.
    Star(Box<Pattern>),
    /// One or more repetitions.
    Plus(Box<Pattern>),
    /// Zero or one occurrence.
    Opt(Box<Pattern>),
    /// A capture group: its 1-based id (in pattern order) and its body.
    Group(usize, Box<Pattern>),
    /// A zero-width assertion: body, which side it inspects, and whether it
    /// is negated. Asserts do not consume input and do not number groups;
    /// capture groups *inside* the body still do.
    Assert(Box<Pattern>, AssertKind, bool),
    /// A character-class atom: the member text (single characters or `a-z`
    /// ranges) and whether the class is negated.
    Class(String, bool),
    /// A bounded repetition: the base, the minimum number of copies, and the
    /// maximum (`None` = unbounded, i.e. `{n,}`).
    Bound(Box<Pattern>, usize, Option<usize>),
    /// A non-capturing group: the body is inlined without a group
    /// nonterminal, so it does not consume a group number.
    NoCap(Box<Pattern>),
    /// The empty pattern (epsilon): matches only the empty string.
    Empty,
}

impl Pattern {
    /// The empty pattern — matches only the empty string (epsilon).
    pub fn epsilon() -> Pattern {
        Pattern::Empty
    }

    /// A run of literal text (an empty run is [`Pattern::epsilon`]).
    pub fn lit(text: impl Into<String>) -> Pattern {
        let text = text.into();
        if text.is_empty() {
            Pattern::Empty
        } else {
            Pattern::Lit(text)
        }
    }

    /// One character from the class (single members or `a-z` ranges).
    pub fn class(body: impl Into<String>) -> Pattern {
        Pattern::Class(body.into(), false)
    }

    /// One character NOT in the class.
    pub fn neg_class(body: impl Into<String>) -> Pattern {
        Pattern::Class(body.into(), true)
    }

    /// The `\d` alias: one digit.
    pub fn digit() -> Pattern {
        Pattern::Class("0-9".to_string(), false)
    }

    /// The `\w` alias: one word character.
    pub fn word() -> Pattern {
        Pattern::Class("a-zA-Z0-9_".to_string(), false)
    }

    /// `self` or `other`, flattened: `a.or(b).or(c)` is one choice.
    pub fn or(self, other: Pattern) -> Pattern {
        match (self, other) {
            (Pattern::Alt(mut a), Pattern::Alt(b)) => {
                a.extend(b);
                Pattern::Alt(a)
            }
            (Pattern::Alt(mut a), b) => {
                a.push(b);
                Pattern::Alt(a)
            }
            (a, Pattern::Alt(mut b)) => {
                b.insert(0, a);
                Pattern::Alt(b)
            }
            (a, b) => Pattern::Alt(vec![a, b]),
        }
    }

    /// `self` followed by `other`, flattened: `a.then(b).then(c)` is one sequence.
    pub fn then(self, other: Pattern) -> Pattern {
        match (self, other) {
            (Pattern::Seq(mut a), Pattern::Seq(b)) => {
                a.extend(b);
                Pattern::Seq(a)
            }
            (Pattern::Seq(mut a), b) => {
                a.push(b);
                Pattern::Seq(a)
            }
            (a, Pattern::Seq(mut b)) => {
                b.insert(0, a);
                Pattern::Seq(b)
            }
            (a, b) => Pattern::Seq(vec![a, b]),
        }
    }

    /// Zero or more repetitions (`self*`).
    pub fn star(self) -> Pattern {
        Pattern::Star(Box::new(self))
    }

    /// One or more repetitions (`self+`).
    pub fn plus(self) -> Pattern {
        Pattern::Plus(Box::new(self))
    }

    /// Zero or one occurrence (`self?`).
    pub fn opt(self) -> Pattern {
        Pattern::Opt(Box::new(self))
    }

    /// Exactly `n` copies (`self{n}`); `n` must be at most 100.
    pub fn times(self, n: usize) -> Pattern {
        Pattern::Bound(Box::new(self), n, Some(n))
    }

    /// At least `n` copies (`self{n,}`); `n` must be at most 100.
    pub fn at_least(self, n: usize) -> Pattern {
        Pattern::Bound(Box::new(self), n, None)
    }

    /// Between `min` and `max` copies (`self{min,max}`); [`Regex::from_pattern`]
    /// rejects unbounded-or-inverted bounds.
    pub fn between(self, min: usize, max: usize) -> Pattern {
        Pattern::Bound(Box::new(self), min, Some(max))
    }

    /// A capture group: numbered in pattern order when compiled.
    pub fn group(self) -> Pattern {
        Pattern::Group(0, Box::new(self))
    }

    /// A non-capturing group: inlined, consumes no group number.
    pub fn non_capturing(self) -> Pattern {
        Pattern::NoCap(Box::new(self))
    }

    /// Positive lookahead: asserts `self` follows here, consumes nothing.
    pub fn lookahead(self) -> Pattern {
        Pattern::Assert(Box::new(self), AssertKind::Lookahead, false)
    }

    /// Negative lookahead: asserts `self` does NOT follow here.
    pub fn neg_lookahead(self) -> Pattern {
        Pattern::Assert(Box::new(self), AssertKind::Lookahead, true)
    }

    /// Positive lookbehind: asserts `self` precedes here, consumes nothing.
    pub fn lookbehind(self) -> Pattern {
        Pattern::Assert(Box::new(self), AssertKind::Lookbehind, false)
    }

    /// Negative lookbehind: asserts `self` does NOT precede here.
    pub fn neg_lookbehind(self) -> Pattern {
        Pattern::Assert(Box::new(self), AssertKind::Lookbehind, true)
    }
}

/// Reject patterns with an assertion nested inside another assertion (the
/// assertion machinery is evaluated one level deep, against the main text).
fn nested_assertion(node: &Pattern) -> bool {
    match node {
        Pattern::Assert(body, _, _) => assertion_nested_in(body),
        Pattern::Seq(els) => els.iter().any(nested_assertion),
        Pattern::Alt(alts) => alts.iter().any(nested_assertion),
        Pattern::Star(b) | Pattern::Plus(b) | Pattern::Opt(b) => nested_assertion(b),
        Pattern::Group(_, b) => nested_assertion(b),
        Pattern::Bound(b, _, _) => nested_assertion(b),
        Pattern::NoCap(b) => nested_assertion(b),
        Pattern::Class(..) => false,
        Pattern::Lit(_) => false,
        Pattern::Empty => false,
    }
}

fn assertion_nested_in(node: &Pattern) -> bool {
    match node {
        Pattern::Assert(_, _, _) => true,
        Pattern::Seq(els) => els.iter().any(assertion_nested_in),
        Pattern::Alt(alts) => alts.iter().any(assertion_nested_in),
        Pattern::Star(b) | Pattern::Plus(b) | Pattern::Opt(b) => assertion_nested_in(b),
        Pattern::Group(_, b) => assertion_nested_in(b),
        Pattern::Bound(b, _, _) => assertion_nested_in(b),
        Pattern::NoCap(b) => assertion_nested_in(b),
        Pattern::Class(..) => false,
        Pattern::Lit(_) => false,
        Pattern::Empty => false,
    }
}

// =============================================================================
// Pattern parser (recursive descent: alt -> seq -> rep -> atom)
// =============================================================================

struct PParser {
    chars: Vec<char>,
    pos: usize,
    next_group: usize,
}

impl PParser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    /// The next capture-group id; groups are numbered in pattern order.
    fn next_group_id(&mut self) -> usize {
        let id = self.next_group;
        self.next_group += 1;
        id
    }
}

fn is_special(ch: char) -> bool {
    matches!(
        ch,
        '|' | '*' | '+' | '?' | '(' | ')' | '\\' | '.' | '[' | ']' | '{' | '}'
    )
}

fn parse_pattern(pattern: &str) -> Result<Pattern, RegexError> {
    let mut p = PParser {
        chars: pattern.chars().collect(),
        pos: 0,
        next_group: 1,
    };
    let root = parse_alternation(&mut p)?;
    if p.pos != p.chars.len() {
        return Err(RegexError::InvalidPattern(format!(
            "unexpected '{}' after pattern",
            p.chars[p.pos]
        )));
    }
    Ok(root)
}

fn parse_alternation(p: &mut PParser) -> Result<Pattern, RegexError> {
    let mut alts = vec![parse_sequence(p)?];
    while p.peek() == Some('|') {
        p.bump();
        alts.push(parse_sequence(p)?);
    }
    match alts.len() {
        1 => Ok(alts.into_iter().next().unwrap()),
        _ => Ok(Pattern::Alt(alts)),
    }
}

fn parse_sequence(p: &mut PParser) -> Result<Pattern, RegexError> {
    let mut seq = Vec::new();
    while let Some(ch) = p.peek() {
        if ch == '|' || ch == ')' {
            break;
        }
        seq.push(parse_repetition(p)?);
    }
    match seq.len() {
        0 => Err(RegexError::InvalidPattern(
            "empty alternative in pattern".to_string(),
        )),
        1 => Ok(seq.into_iter().next().unwrap()),
        _ => Ok(Pattern::Seq(seq)),
    }
}

/// The maximum number of copies a bounded repetition may unroll (keeps the
/// generated grammar finite and small).
pub(crate) const MAX_BOUND: u32 = 100;

fn parse_repetition(p: &mut PParser) -> Result<Pattern, RegexError> {
    let atom = parse_atom(p)?;
    // `ab*` binds the quantifier to the last character only.
    let (prefix, atom) = split_quantified_literal(atom, p.peek());
    let node = apply_quantifiers(p, atom)?;
    if prefix.is_empty() {
        Ok(node)
    } else {
        Ok(Pattern::Seq(vec![Pattern::Lit(prefix), node]))
    }
}

/// `*`, `+`, and `?` nest (`a**` lowers and is then refused as a non-LALR
/// grammar). A `+` glued to the first quantifier is the possessive marker
/// (`a*+`). A further `+` after that marker (`a*++`), a `{` after any
/// quantifier, and any quantifier after a bound (`a{2}+`, `a{2}{3}`) are
/// syntax errors.
fn apply_quantifiers(p: &mut PParser, mut node: Pattern) -> Result<Pattern, RegexError> {
    let mut applied = false;
    let mut possessive = false;
    loop {
        match p.peek() {
            Some('{') => {
                if applied {
                    return Err(quantifier_after_quantifier());
                }
                let (min, max) = parse_bound(p)?;
                node = Pattern::Bound(Box::new(node), min, max);
                applied = true;
            }
            Some('*' | '+' | '?') => {
                if applied && matches!(node, Pattern::Bound(..)) {
                    return Err(quantifier_after_quantifier());
                }
                // `a*++`: the first `+` was the possessive marker; the
                // leftover `+` is a second possessive, not `(a*)+`.
                if applied && possessive && p.peek() == Some('+') {
                    return Err(quantifier_after_quantifier());
                }
                let op = p.peek().unwrap();
                p.bump();
                if !applied && p.peek() == Some('+') {
                    p.bump();
                    possessive = true;
                }
                node = match op {
                    '*' => Pattern::Star(Box::new(node)),
                    '+' => Pattern::Plus(Box::new(node)),
                    _ => Pattern::Opt(Box::new(node)),
                };
                applied = true;
            }
            _ => return Ok(node),
        }
    }
}

fn quantifier_after_quantifier() -> RegexError {
    RegexError::InvalidPattern("quantifier after a quantifier or repetition bound".to_string())
}

/// When a multi-character literal is followed by a quantifier, the
/// quantifier binds to its last character (`ab*` = `a` then `b*`).
fn split_quantified_literal(atom: Pattern, next: Option<char>) -> (String, Pattern) {
    let quantified = matches!(next, Some('*' | '+' | '?' | '{'));
    let Pattern::Lit(text) = atom else {
        return (String::new(), atom);
    };
    if !quantified {
        return (String::new(), Pattern::Lit(text));
    }
    let count = text.chars().count();
    if count < 2 {
        return (String::new(), Pattern::Lit(text));
    }
    let base: String = text.chars().take(count - 1).collect();
    let last: String = text.chars().skip(count - 1).collect();
    (base, Pattern::Lit(last))
}

/// Parse a repetition bound — p.pos is just past the `{` on entry. Accepts
/// `{n}`, `{n,}`, and `{n,m}` (with `m >= n` and both at most `MAX_BOUND`);
/// returns `(min, max)` where `max = None` means "at least n".
fn parse_bound(p: &mut PParser) -> Result<(usize, Option<usize>), RegexError> {
    fn digits(p: &mut PParser) -> Result<u32, RegexError> {
        let start = p.pos;
        while let Some(ch) = p.peek() {
            if ch.is_ascii_digit() {
                p.bump();
            } else {
                break;
            }
        }
        let text: String = p.chars[start..p.pos].iter().collect();
        if text.is_empty() {
            return Err(RegexError::InvalidPattern(
                "missing number in repetition bound".to_string(),
            ));
        }
        let value = text.parse::<u32>().map_err(|_| {
            RegexError::InvalidPattern("number too large in repetition bound".to_string())
        })?;
        if value > MAX_BOUND {
            return Err(RegexError::InvalidPattern(format!(
                "repetition bound {value} exceeds the maximum of {MAX_BOUND}"
            )));
        }
        Ok(value)
    }

    p.bump(); // past the '{'
    let min = digits(p)?;
    let mut max = None;
    if p.peek() == Some(',') {
        p.bump();
        if p.peek() == Some('}') {
            // `{n,}` — at least n.
            p.bump();
            return Ok((min as usize, None));
        }
        let upper = digits(p)?;
        if upper < min {
            return Err(RegexError::InvalidPattern(format!(
                "repetition bound {{{min},{upper}}} has its upper bound below its lower"
            )));
        }
        max = Some(upper);
    }
    if max.is_none() {
        // `{n}` — exactly n.
        max = Some(min);
    }
    if p.peek() != Some('}') {
        return Err(RegexError::InvalidPattern(
            "unbalanced '{' in repetition bound: expected '}'".to_string(),
        ));
    }
    p.bump();
    Ok((min as usize, max.map(|m| m as usize)))
}

fn parse_atom(p: &mut PParser) -> Result<Pattern, RegexError> {
    match p.peek() {
        None => Err(RegexError::InvalidPattern(
            "unexpected end of pattern".to_string(),
        )),
        Some('(') => {
            p.bump();
            if p.peek() == Some('?') {
                p.bump();
                match p.peek() {
                    // `(?P)` = non-capturing, `(?>P)` = atomic (identical
                    // here: without backtracking there is nothing to make
                    // atomic). Neither consumes a group number.
                    Some(':') | Some('>') => {
                        p.bump();
                        let inner = parse_alternation(p)?;
                        if p.peek() != Some(')') {
                            return Err(RegexError::InvalidPattern(
                                "unbalanced '(' in pattern".to_string(),
                            ));
                        }
                        p.bump();
                        Ok(Pattern::NoCap(Box::new(inner)))
                    }
                    _ => parse_assertion(p),
                }
            } else {
                let id = p.next_group_id();
                let inner = parse_alternation(p)?;
                if p.peek() != Some(')') {
                    return Err(RegexError::InvalidPattern(
                        "unbalanced '(' in pattern".to_string(),
                    ));
                }
                p.bump();
                Ok(Pattern::Group(id, Box::new(inner)))
            }
        }
        Some('[') => parse_class(p),
        Some('.') => Err(RegexError::InvalidPattern(
            "'.': any-character is not expressible as a CFG".to_string(),
        )),
        Some('*') | Some('+') | Some('?') => Err(RegexError::InvalidPattern(
            "quantifier with no operand".to_string(),
        )),
        Some('{') => Err(RegexError::InvalidPattern(
            "repetition bound without an operand".to_string(),
        )),
        Some('}') => Err(RegexError::InvalidPattern(
            "unexpected '}' in pattern".to_string(),
        )),
        Some(']') => Err(RegexError::InvalidPattern(
            "unexpected ']' in pattern".to_string(),
        )),
        Some('\\') => {
            p.bump();
            match p.peek() {
                Some(ch) => {
                    p.bump();
                    match ch {
                        // Predefined aliases: the same lowering as the
                        // matching class atom.
                        'd' => Ok(Pattern::Class("0-9".to_string(), false)),
                        'w' => Ok(Pattern::Class("a-zA-Z0-9_".to_string(), false)),
                        // Whitespace is the token separator: the lexer
                        // skips it, so no terminal can match it.
                        's' => Err(RegexError::InvalidPattern(
                            "'\\s' is rejected: whitespace is the token separator \
                             (the lexer skips it), so no terminal can match it"
                                .to_string(),
                        )),
                        _ => Ok(Pattern::Lit(ch.to_string())),
                    }
                }
                None => Err(RegexError::InvalidPattern(
                    "dangling escape at end of pattern".to_string(),
                )),
            }
        }
        Some(_) => parse_literal_run(p),
    }
}

/// Parse a character class introduced by `[`: an optional leading `^`
/// negates the class, members are single characters or `a-z` ranges, the
/// class closes at the first `]`, a nested `[` is rejected, and an empty
/// body is rejected.
fn parse_class(p: &mut PParser) -> Result<Pattern, RegexError> {
    // p.pos is just past the '['.
    p.bump();
    let mut negated = false;
    if p.peek() == Some('^') {
        negated = true;
        p.bump();
    }
    let start = p.pos;
    while let Some(ch) = p.peek() {
        if ch == ']' {
            break;
        }
        if ch == '[' {
            return Err(RegexError::InvalidPattern(
                "a '[' inside a class is not supported".to_string(),
            ));
        }
        p.bump();
    }
    let body: String = p.chars[start..p.pos].iter().collect();
    if body.is_empty() {
        return Err(RegexError::InvalidPattern(
            "empty class in pattern".to_string(),
        ));
    }
    if let Err(msg) = crate::lexer::parse_class_members(&body) {
        return Err(RegexError::InvalidPattern(msg));
    }
    if p.peek() != Some(']') {
        return Err(RegexError::InvalidPattern(
            "unbalanced '[' in class: expected ']'".to_string(),
        ));
    }
    p.bump();
    Ok(Pattern::Class(body, negated))
}

/// Parse a zero-width assertion introduced by `(?`:
///   `(?=P)`  positive lookahead     `(?!P)`  negative lookahead
///   `(?<=P)` positive lookbehind    `(?<!P)` negative lookbehind
fn parse_assertion(p: &mut PParser) -> Result<Pattern, RegexError> {
    // p.pos is just past the '?'.
    let (kind, negated) = match p.peek() {
        Some('=') => {
            p.bump();
            (AssertKind::Lookahead, false)
        }
        Some('!') => {
            p.bump();
            (AssertKind::Lookahead, true)
        }
        Some('<') => {
            p.bump();
            match p.peek() {
                Some('=') => {
                    p.bump();
                    (AssertKind::Lookbehind, false)
                }
                Some('!') => {
                    p.bump();
                    (AssertKind::Lookbehind, true)
                }
                _ => {
                    return Err(RegexError::InvalidPattern(
                        "malformed lookbehind: expected '(?<=' or '(?<!'".to_string(),
                    ));
                }
            }
        }
        _ => {
            return Err(RegexError::InvalidPattern(
                "unsupported construct after '(?'".to_string(),
            ));
        }
    };
    let body = parse_alternation(p)?;
    if p.peek() != Some(')') {
        return Err(RegexError::InvalidPattern(
            "unbalanced '(' in assertion".to_string(),
        ));
    }
    p.bump();
    Ok(Pattern::Assert(Box::new(body), kind, negated))
}

/// Consume a run of plain characters. A following quantifier is applied by
/// [`parse_repetition`], which binds it to the run's last character.
fn parse_literal_run(p: &mut PParser) -> Result<Pattern, RegexError> {
    let start = p.pos;
    while let Some(ch) = p.peek() {
        if is_special(ch) {
            break;
        }
        p.bump();
    }
    if start == p.pos {
        return Err(RegexError::InvalidPattern(
            "unexpected character in pattern".to_string(),
        ));
    }
    let run: String = p.chars[start..p.pos].iter().collect();
    Ok(Pattern::Lit(run))
}

/// Collapse the composable API's identity nodes so the lowering sees the
/// same well-formed trees the parser produces: `Empty` drops out of
/// sequences and of non-capturing groups (it is an identity), and a
/// quantifier over `Empty` is just `Empty` (the language of `ε*`, `ε+`,
/// `ε?`, and `ε{m,n}` is `{ε}`). `Empty` inside an alternative, a capture
/// group, or an assertion body is meaningful and is kept: it lowers to the
/// grammar's `epsilon` symbol.
fn normalize(node: &Pattern) -> Pattern {
    match node {
        Pattern::Empty | Pattern::Lit(_) | Pattern::Class(..) => node.clone(),
        Pattern::Seq(els) => {
            let kept: Vec<Pattern> = els
                .iter()
                .map(normalize)
                .filter(|e| !matches!(e, Pattern::Empty))
                .collect();
            // Adjacent literals are one terminal, matching a string run
            // (`lit("a").then(lit("b"))` = `ab`). A quantifier stays a
            // separate node, so `a` then `b*` does not collapse.
            let merged = merge_adjacent_lits(kept);
            match merged.len() {
                0 => Pattern::Empty,
                1 => merged.into_iter().next().unwrap(),
                _ => Pattern::Seq(merged),
            }
        }
        Pattern::Alt(alts) => Pattern::Alt(alts.iter().map(normalize).collect()),
        Pattern::Star(base) => {
            let base = normalize(base);
            if matches!(base, Pattern::Empty) {
                Pattern::Empty
            } else {
                Pattern::Star(Box::new(base))
            }
        }
        Pattern::Plus(base) => {
            let base = normalize(base);
            if matches!(base, Pattern::Empty) {
                Pattern::Empty
            } else {
                Pattern::Plus(Box::new(base))
            }
        }
        Pattern::Opt(base) => {
            let base = normalize(base);
            if matches!(base, Pattern::Empty) {
                Pattern::Empty
            } else {
                Pattern::Opt(Box::new(base))
            }
        }
        Pattern::Group(id, base) => Pattern::Group(*id, Box::new(normalize(base))),
        Pattern::NoCap(base) => {
            let base = normalize(base);
            if matches!(base, Pattern::Empty) {
                Pattern::Empty
            } else {
                Pattern::NoCap(Box::new(base))
            }
        }
        Pattern::Bound(base, min, max) => {
            let base = normalize(base);
            if matches!(base, Pattern::Empty) {
                // Any number of copies of ε is still just ε.
                Pattern::Empty
            } else {
                Pattern::Bound(Box::new(base), *min, *max)
            }
        }
        Pattern::Assert(base, kind, negated) => {
            Pattern::Assert(Box::new(normalize(base)), *kind, *negated)
        }
    }
}

fn merge_adjacent_lits(nodes: Vec<Pattern>) -> Vec<Pattern> {
    let mut out = Vec::new();
    for node in nodes {
        if let Pattern::Lit(text) = &node {
            if let Some(Pattern::Lit(prev)) = out.last_mut() {
                prev.push_str(text);
                continue;
            }
        }
        out.push(node);
    }
    out
}

/// Quote `text` as a grammar terminal, escaping `\` and `"` so the grammar
/// scanner reads the same characters back.
fn quote_terminal(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        if ch == '\\' || ch == '"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

// =============================================================================
// CFG code generation
// =============================================================================

/// The generated grammar string plus the capture-group nonterminals in
/// pattern order.
struct Cfg {
    grammar: String,
    groups: Vec<(usize, String)>,
    /// Zero-width assertions in pattern order.
    assertions: Vec<AssertionSpec>,
}

/// One zero-width assertion: the placeholder nonterminal in the main
/// grammar, which side of the position it inspects, whether it is negated,
/// and its (not-yet-compiled) body.
struct AssertionSpec {
    placeholder: String,
    kind: AssertKind,
    negated: bool,
    body: Pattern,
}

struct Lowerer {
    rules: Vec<(String, String)>,
    n_counter: usize,
    a_counter: usize,
    groups: Vec<(usize, String)>,
    assertions: Vec<AssertionSpec>,
}

impl Lowerer {
    fn fresh_n(&mut self) -> String {
        self.n_counter += 1;
        format!("N{}", self.n_counter)
    }

    /// `syms` repeated `n` times (clone-based: symbols are not `Copy`).
    fn copies(syms: &[String], n: usize) -> Vec<String> {
        let mut out = Vec::with_capacity(syms.len() * n);
        for _ in 0..n {
            out.extend(syms.iter().cloned());
        }
        out
    }

    /// Emit the grammar symbols for `node`, registering any named
    /// nonterminals (alternations, repetitions, groups) it introduces.
    fn emit(&mut self, node: &Pattern) -> Vec<String> {
        match node {
            Pattern::Lit(s) => vec![quote_terminal(s)],
            Pattern::Seq(els) => els.iter().flat_map(|e| self.emit(e)).collect(),
            Pattern::Empty => vec![],
            Pattern::Alt(alts) => {
                let name = self.fresh_n();
                // An empty alternative is the epsilon symbol of the grammar
                // language (the composable API can build `a | <empty>`).
                let rhs = alts
                    .iter()
                    .map(|a| {
                        let e = self.emit(a);
                        if e.is_empty() {
                            String::from("epsilon")
                        } else {
                            e.join(" ")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                self.rules.push((name.clone(), rhs));
                vec![name]
            }
            Pattern::Star(base) => {
                let body = self.emit(base).join(" ");
                let name = self.fresh_n();
                self.rules
                    .push((name.clone(), format!("{} {}", body, name)));
                self.rules.push((name.clone(), "epsilon".to_string()));
                vec![name]
            }
            Pattern::Plus(base) => {
                let body = self.emit(base).join(" ");
                let name = self.fresh_n();
                self.rules
                    .push((name.clone(), format!("{} {}", body, name)));
                self.rules.push((name.clone(), body));
                vec![name]
            }
            Pattern::Opt(base) => {
                let body = self.emit(base).join(" ");
                let name = self.fresh_n();
                self.rules.push((name.clone(), "epsilon".to_string()));
                self.rules.push((name.clone(), body));
                vec![name]
            }
            Pattern::Group(id, inner) => {
                let name = format!("G{}", id);
                let rhs = self.emit(inner);
                let rhs = if rhs.is_empty() {
                    "epsilon".to_string()
                } else {
                    rhs.join(" ")
                };
                self.rules.push((name.clone(), rhs));
                self.groups.push((*id, name.clone()));
                vec![name]
            }
            Pattern::Class(body, negated) => {
                // One quoted terminal, in exactly the lexer's class form.
                let prefix = if *negated { "^" } else { "" };
                vec![quote_terminal(&format!("[{prefix}{body}]"))]
            }
            Pattern::NoCap(inner) => {
                // Syntactic sugar over sequence/alternation: inline the body.
                self.emit(inner)
            }
            Pattern::Bound(base, min, max) => {
                let body_syms = self.emit(base);
                match max {
                    Some(max) if *min == *max => {
                        // Exactly n copies.
                        if *min == 0 {
                            let name = self.fresh_n();
                            self.rules.push((name.clone(), "epsilon".to_string()));
                            vec![name]
                        } else {
                            Lowerer::copies(&body_syms, *min)
                        }
                    }
                    Some(max) => {
                        // n..m: one nonterminal with m-n+1 alternatives of
                        // n..m copies (separate productions, LALR-safe).
                        let name = self.fresh_n();
                        for k in *min..=*max {
                            let mut alt = Lowerer::copies(&body_syms, k);
                            // k = 0 is the empty derivation: the grammar
                            // language spells it as `epsilon`.
                            if alt.is_empty() {
                                alt = vec!["epsilon".to_string()];
                            }
                            self.rules.push((name.clone(), alt.join(" ")));
                        }
                        vec![name]
                    }
                    None => {
                        // At least n: n copies plus any number of extras
                        // (`base*`), so `{n,}` matches exactly n as well.
                        let extra = self.fresh_n();
                        self.rules
                            .push((extra.clone(), format!("{} {}", body_syms.join(" "), extra)));
                        self.rules.push((extra.clone(), "epsilon".to_string()));
                        let mut out = Lowerer::copies(&body_syms, *min);
                        out.push(extra);
                        out
                    }
                }
            }
            Pattern::Assert(body, kind, negated) => {
                // The main grammar only sees a zero-width placeholder; the
                // assertion body is compiled separately and verified at
                // parse time by the `Regex` front end.
                self.a_counter += 1;
                let name = format!("A{}", self.a_counter);
                self.rules.push((name.clone(), "epsilon".to_string()));
                self.assertions.push(AssertionSpec {
                    placeholder: name.clone(),
                    kind: *kind,
                    negated: *negated,
                    body: (**body).clone(),
                });
                vec![name]
            }
        }
    }
}

/// Lower the pattern AST into a CFG grammar string (start symbol `S`
/// first) and the ordered capture-group names.
fn lower(node: &Pattern) -> Cfg {
    let mut lowerer = Lowerer {
        rules: Vec::new(),
        n_counter: 0,
        a_counter: 0,
        groups: Vec::new(),
        assertions: Vec::new(),
    };
    let root = lowerer.emit(node);
    let root = if root.is_empty() {
        "epsilon".to_string()
    } else {
        root.join(" ")
    };
    let mut grammar = format!("S -> {root}");
    let mut seen = std::collections::HashSet::new();
    for (lhs, rhs) in &lowerer.rules {
        // A group inside a bound is emitted once per copy; identical
        // productions are kept only once (order preserved).
        if seen.insert((lhs.clone(), rhs.clone())) {
            grammar.push_str(&format!("; {lhs} -> {rhs}"));
        }
    }
    // Order capture groups by their pattern-order id (the emit traversal
    // visits nested groups depth-first, which is not pattern order).
    let mut groups = lowerer.groups;
    groups.sort_by_key(|(id, _)| *id);
    // A group inside a bound appears once per copy; one entry per id.
    groups.dedup_by_key(|(id, _)| *id);
    Cfg {
        grammar,
        groups,
        assertions: lowerer.assertions,
    }
}

// =============================================================================
// Regex
// =============================================================================

use std::collections::HashMap;

use crate::engine::Reduction;

/// A compiled zero-width assertion: a placeholder in the main grammar, plus
/// its compiled body, which side of the position it inspects, and its
/// capture groups.
struct Assertion {
    placeholder: String,
    kind: AssertKind,
    negated: bool,
    body: CfgParser,
    /// Capture groups inside the body: (global id, name), pattern order.
    groups: Vec<(usize, String)>,
}

impl Assertion {
    /// Evaluate at byte position `pos` of `text`.
    ///
    /// Returns `(holds, groups)`: whether the (possibly negated) condition
    /// is satisfied, and the byte spans of the body's groups (in the body's
    /// group order) when it is. A lookahead needs the body to match a PREFIX
    /// of the remainder; a lookbehind needs it to match a SUFFIX of the
    /// consumed text (ending exactly at `pos`), reaching as far back as
    /// possible in both cases.
    fn evaluate(&self, text: &str, pos: usize) -> (bool, Vec<(String, Option<Match>)>) {
        let present = if self.kind == AssertKind::Lookbehind {
            let prefix = &text[..pos];
            match self.body.suffix_match(prefix) {
                Some(m) => self.capture_in(&prefix[m.start..], m.start),
                None => None,
            }
        } else {
            let suffix = &text[pos..];
            match self.body.prefix_match(suffix) {
                Some(m) => self.capture_in(&suffix[..m.end], pos),
                None => None,
            }
        };
        let holds = present.is_some() != self.negated;
        (holds, present.unwrap_or_default())
    }

    /// The body's groups, mapped to byte offsets in `text`
    /// (`slice_base` is the absolute start of the winning body slice).
    fn capture_in(&self, slice: &str, slice_base: usize) -> Option<Vec<(String, Option<Match>)>> {
        let names: Vec<&str> = self.groups.iter().map(|(_, n)| n.as_str()).collect();
        let caps = self.body.captures(slice, &names)?;
        let out = self
            .groups
            .iter()
            .enumerate()
            .map(|(i, (_, name))| {
                (
                    name.clone(),
                    caps.matches[i + 1]
                        .map(|m| Match::new(m.start + slice_base, m.end + slice_base)),
                )
            })
            .collect();
        Some(out)
    }
}

/// Checks a composable tree the string dialect's parser would have
/// rejected up front: empty classes, and repetition bounds that break the
/// shared 100-copy cap or are inverted (`{3,2}`).
fn validate(node: &Pattern) -> Result<(), RegexError> {
    match node {
        Pattern::Class(body, _) => {
            if body.is_empty() {
                return Err(RegexError::InvalidPattern(
                    "empty class in pattern".to_string(),
                ));
            }
            if let Err(msg) = crate::lexer::parse_class_members(body) {
                return Err(RegexError::InvalidPattern(msg));
            }
        }
        Pattern::Bound(_, min, max) => {
            if *min > MAX_BOUND as usize {
                return Err(RegexError::InvalidPattern(format!(
                    "repetition bound {min} exceeds the maximum of {MAX_BOUND}"
                )));
            }
            if let Some(max) = max {
                if *max > MAX_BOUND as usize {
                    return Err(RegexError::InvalidPattern(format!(
                        "repetition bound {max} exceeds the maximum of {MAX_BOUND}"
                    )));
                }
                if *max < *min {
                    return Err(RegexError::InvalidPattern(format!(
                        "repetition bound {{{min},{max}}} has its upper bound below its lower"
                    )));
                }
            }
        }
        _ => {}
    }
    match node {
        Pattern::Seq(els) => els.iter().try_for_each(validate),
        Pattern::Alt(alts) => alts.iter().try_for_each(validate),
        Pattern::Star(b) => validate(b),
        Pattern::Plus(b) => validate(b),
        Pattern::Opt(b) => validate(b),
        Pattern::Group(_, b) => validate(b),
        Pattern::NoCap(b) => validate(b),
        Pattern::Bound(b, _, _) => validate(b),
        Pattern::Assert(b, _, _) => validate(b),
        Pattern::Lit(_) | Pattern::Class(..) | Pattern::Empty => Ok(()),
    }
}

/// Number capture groups in pattern order — the order the parser assigns
/// ids to — so a built tree gets the same `G1`, `G2`, ... names.
fn assign_group_ids(node: &mut Pattern, next: &mut usize) {
    match node {
        Pattern::Seq(els) => els.iter_mut().for_each(|e| assign_group_ids(e, next)),
        Pattern::Alt(alts) => alts.iter_mut().for_each(|e| assign_group_ids(e, next)),
        Pattern::Star(b) => assign_group_ids(b, next),
        Pattern::Plus(b) => assign_group_ids(b, next),
        Pattern::Opt(b) => assign_group_ids(b, next),
        Pattern::Group(id, b) => {
            *id = *next;
            *next += 1;
            assign_group_ids(b, next);
        }
        Pattern::NoCap(b) => assign_group_ids(b, next),
        Pattern::Bound(b, _, _) => assign_group_ids(b, next),
        Pattern::Assert(b, _, _) => assign_group_ids(b, next),
        Pattern::Lit(_) | Pattern::Class(_, _) | Pattern::Empty => {}
    }
}

/// A compiled regular expression: a pattern lowered to a CFG grammar and
/// matched by the LALR(1) engine.
///
/// `Regex` is a restricted, honest dialect of regular expressions — see the
/// crate-level documentation for the pattern language (including lookahead
/// and lookbehind), the matching semantics (anchored
/// [`is_match`][Self::is_match], leftmost [`find`][Self::find]), and the
/// capture-group semantics.
pub struct Regex {
    pattern: String,
    body: RegexBody,
}

/// What a [`Regex`] computes: the language of a lowered grammar, the
/// complement of another `Regex` ([`Regex::not`]), or the intersection of
/// two `Regex`es ([`Regex::and`]).
enum RegexBody {
    /// The lowered grammar's language (plus its assertion machinery).
    Grammar {
        cfg: Box<CfgParser>,
        assertions: Vec<Assertion>,
        groups: Vec<String>,
    },
    /// The set of inputs the wrapped regex rejects.
    Complement(Box<Regex>),
    /// The inputs accepted by BOTH sides.
    Intersection(Box<Regex>, Box<Regex>),
}

impl Regex {
    /// Compile a pattern: parse, lower to a CFG, build the LALR(1) table.
    /// Fails with `InvalidPattern` on bad syntax, and with `CompileError`
    /// when the encoding (or an assertion body) is not LALR(1).
    pub fn new(pattern: &str) -> Result<Self, RegexError> {
        let ast = parse_pattern(pattern)?;
        // An escape ends a literal run, so `a\|b` is three atoms. Merge them
        // the same way the composable front end merges `lit("a").then(lit("|"))`.
        let ast = normalize(&ast);
        Self::compile(pattern.to_string(), &ast)
    }

    /// Compile a composable [`Pattern`] (built with the `Pattern`
    /// combinators — `lit`, `or`, `then`, quantifiers, groups, assertions —
    /// rather than parsed from a pattern string).
    ///
    /// Runs the exact same validation and lowering as [`Regex::new`], so a
    /// composable pattern and its string form match identically. Two extra
    /// checks apply to a built tree: repetition bounds must respect the
    /// same 100-copy cap (and be ordered) as the string dialect, and a
    /// memberless class such as `Pattern::class("")` is refused.
    ///
    /// ```
    /// use RegExt::{Pattern, Regex};
    ///
    /// // `[+-]?\d{1,3}`
    /// let re = Regex::from_pattern(
    ///     Pattern::lit("+")
    ///         .or(Pattern::lit("-"))
    ///         .opt()
    ///         .then(Pattern::digit().between(1, 3)),
    /// )
    /// .unwrap();
    /// assert!(re.is_match("+17"));
    /// assert!(re.is_match("-123"));
    /// assert!(!re.is_match("1234"));
    /// ```
    pub fn from_pattern(pattern: Pattern) -> Result<Self, RegexError> {
        validate(&pattern)?;
        let mut ast = normalize(&pattern);
        // Number capture groups in pattern order, the order the parser
        // assigns ids to.
        let mut next = 1;
        assign_group_ids(&mut ast, &mut next);
        Self::compile("Pattern (composable)".to_string(), &ast)
    }

    /// The shared compile pipeline behind both front ends: reject nested
    /// assertions, lower to a CFG, build the LALR(1) table (refusing
    /// conflicts), compile each assertion body, and order the capture
    /// groups.
    fn compile(pattern: String, ast: &Pattern) -> Result<Self, RegexError> {
        if nested_assertion(ast) {
            return Err(RegexError::InvalidPattern(
                "an assertion inside an assertion is not supported".to_string(),
            ));
        }
        let cfg = lower(ast);
        let main =
            CfgParser::new(&cfg.grammar).map_err(|e| RegexError::CompileError(e.to_string()))?;
        if main.conflict_count() > 0 {
            return Err(RegexError::CompileError(format!(
                "pattern compiles to a grammar with {} LALR(1) conflict(s)",
                main.conflict_count()
            )));
        }

        let mut assertions = Vec::new();
        for spec in &cfg.assertions {
            let body = lower(&spec.body);
            let body_cfg = CfgParser::new(&body.grammar)
                .map_err(|e| RegexError::CompileError(e.to_string()))?;
            if body_cfg.conflict_count() > 0 {
                return Err(RegexError::CompileError(format!(
                    "assertion subpattern compiles to a grammar with {} LALR(1) conflict(s)",
                    body_cfg.conflict_count()
                )));
            }
            assertions.push(Assertion {
                placeholder: spec.placeholder.clone(),
                kind: spec.kind,
                negated: spec.negated,
                body: body_cfg,
                groups: body.groups,
            });
        }

        // Capture groups: the main grammar's plus each assertion body's, in
        // global pattern order.
        let mut groups = cfg.groups;
        for a in &assertions {
            groups.extend(a.groups.iter().cloned());
        }
        groups.sort_by_key(|(id, _)| *id);
        let groups = groups.into_iter().map(|(_, name)| name).collect();

        Ok(Self {
            pattern,
            body: RegexBody::Grammar {
                cfg: Box::new(main),
                assertions,
                groups,
            },
        })
    }

    /// The original pattern string, unchanged.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Number of capture groups in the pattern (groups inside assertions
    /// are included).
    pub fn group_count(&self) -> usize {
        match &self.body {
            RegexBody::Grammar { groups, .. } => groups.len(),
            RegexBody::Complement(inner) => inner.group_count(),
            RegexBody::Intersection(left, right) => left.group_count() + right.group_count(),
        }
    }

    /// The complement language: `re.not().is_match(t) == !re.is_match(t)`.
    ///
    /// The complement of a regular language is regular (invert a DFA's
    /// accepting states), but it is generally **not expressible as a RegExt
    /// pattern**: the stream's alphabet is the whole Unicode space (the
    /// lexer emits any unknown character as a single-character token), so
    /// "everything except `L`" needs a universal character set that the
    /// finite-terminal dialect cannot write — the same reason `.` is
    /// rejected. `not()` is therefore realized at the engine level instead
    /// of by re-lowering:
    ///
    /// - `is_match` is the boolean complement of the wrapped pattern;
    /// - `find` returns the leftmost slice (longest at that start) the
    ///   wrapped pattern **rejects**;
    /// - `captures` returns that slice with every group `None` — a rejected
    ///   slice has no winning derivation, so no group participates.
    ///
    /// Complement pairs cancel: `re.not().not()` behaves exactly like `re`.
    ///
    /// ```
    /// use RegExt::{Match, Regex};
    ///
    /// let not_a = Regex::new("a").unwrap().not();
    /// assert!(not_a.is_match("b"));
    /// assert!(!not_a.is_match("a"));
    /// assert_eq!(not_a.find("xa"), Some(Match::new(0, 2)));
    /// assert_eq!(not_a.find("a"), Some(Match::new(0, 0)));
    /// ```
    // Inherent (rather than `std::ops::Not`) so `re.not()` resolves without
    // importing the trait, consistent with the other `Regex` methods.
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Regex {
        Regex {
            pattern: format!("not({})", self.pattern),
            body: RegexBody::Complement(Box::new(self)),
        }
    }

    /// The intersection language:
    /// `e1.and(e2).is_match(t) == e1.is_match(t) && e2.is_match(t)`.
    ///
    /// The intersection of two regular languages is regular (build the
    /// product of their DFAs), and RegExt realizes it at the engine level,
    /// like [`Regex::not`] — there is no RegExt pattern that spells
    /// "pattern `A` and pattern `B`" together:
    ///
    /// - `is_match` is the conjunction of both sides on the whole input;
    /// - `find` returns the leftmost slice (longest at that start) accepted
    ///   by **both** sides of the **same** text — a language-level
    ///   conjunction, so the sides may tokenize the text differently
    ///   (e.g. `ab` is one terminal of `"ab"` but two of `"a"`);
    /// - `captures` reports the left side's group spans first (in its
    ///   pattern order), then the right side's — each side's winning
    ///   derivation supplies its own groups, and a side with no winning
    ///   derivation (e.g. a complement) reports its groups as `None`.
    ///
    /// `and` composes with `not`: `e1.and(e2).not()` is everything except
    /// the intersection, and `e1.and(e2).not().not()` cancels back to
    /// `e1.and(e2)`.
    ///
    /// ```
    /// use RegExt::{Match, Regex};
    ///
    /// let re = Regex::new("a|b").unwrap().and(Regex::new("a|c").unwrap());
    /// assert!(re.is_match("a"));      // the only common string
    /// assert!(!re.is_match("b"));
    /// assert_eq!(re.find("xabz"), Some(Match::new(1, 2)));
    /// ```
    pub fn and(self, other: Regex) -> Regex {
        Regex {
            pattern: format!("and({}, {})", self.pattern, other.pattern),
            body: RegexBody::Intersection(Box::new(self), Box::new(other)),
        }
    }

    /// A non-empty slice is a `find` candidate only when every innermost
    /// grammar's tokens cover it exactly (first token at byte 0, last token
    /// ending at the slice length). Skipped whitespace is therefore not part
    /// of a match span. An empty slice is always a candidate.
    fn covers_span(&self, text: &str, b0: usize, b1: usize) -> bool {
        match &self.body {
            RegexBody::Grammar { cfg, .. } => cfg.covers_text(&text[b0..b1]),
            RegexBody::Complement(inner) => inner.covers_span(text, b0, b1),
            RegexBody::Intersection(left, right) => {
                left.covers_span(text, b0, b1) && right.covers_span(text, b0, b1)
            }
        }
    }

    /// Whether the byte slice `text[b0..b1]` is in `self`'s language: its
    /// own tokens derive the innermost grammar's start symbol and every
    /// assertion in the winning derivation holds (evaluated against the
    /// FULL text at the slice's absolute position), with each complement
    /// layer negating the wrapped regex's decision and each intersection
    /// requiring BOTH sides to accept the same slice.
    fn accepts_span(&self, text: &str, b0: usize, b1: usize) -> bool {
        match &self.body {
            RegexBody::Grammar {
                cfg, assertions, ..
            } => {
                let sub = &text[b0..b1];
                let n = cfg.token_spans(sub).len().saturating_sub(1);
                let Some(reductions) = cfg.slice_reductions(sub, 0, n) else {
                    return false;
                };
                Self::assertions_hold(assertions, cfg, &reductions, sub, text, b0)
            }
            RegexBody::Complement(inner) => !inner.accepts_span(text, b0, b1),
            RegexBody::Intersection(left, right) => {
                left.accepts_span(text, b0, b1) && right.accepts_span(text, b0, b1)
            }
        }
    }

    /// The leftmost accepted slice (leftmost start, longest at that start),
    /// as a byte span; `None` when no slice of the input is accepted.
    /// Candidates are char boundaries. A non-empty slice must be covered by
    /// its tokens; the empty slice is tried last at that start.
    fn find_slice(&self, text: &str) -> Option<(usize, usize)> {
        let mut bounds: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
        bounds.push(text.len());
        for (start_i, &start) in bounds.iter().enumerate() {
            for &end in bounds[start_i..].iter().rev() {
                if start != end && !self.covers_span(text, start, end) {
                    continue;
                }
                if self.accepts_span(text, start, end) {
                    return Some((start, end));
                }
            }
        }
        None
    }

    /// The ENTIRE input must match (anchored at both ends), and every
    /// assertion in the winning derivation must hold. For an intersection,
    /// both sides must match the entire input.
    pub fn is_match(&self, text: &str) -> bool {
        self.accepts_span(text, 0, text.len())
    }

    /// Leftmost match at any position (leftmost start, longest at that
    /// start among candidates whose assertions hold). For an intersection,
    /// the slice must be accepted by both sides.
    pub fn find(&self, text: &str) -> Option<Match> {
        let (start, end) = self.find_slice(text)?;
        Some(Match::new(start, end))
    }

    /// Leftmost match plus the span of each capture group, in pattern order;
    /// a group that did not participate is `None`. Groups inside assertions
    /// capture when their assertion is in the winning branch.
    ///
    /// A complement has no winning derivation for its slices (the wrapped
    /// regex rejected them), so every group is reported `None`; complement
    /// pairs cancel, so `re.not().not()` captures exactly like `re`.
    ///
    /// An intersection reports the left side's groups first (in its pattern
    /// order), then the right side's: each side accepted the slice, so each
    /// side's winning derivation supplies its own groups.
    pub fn captures(&self, text: &str) -> Option<Captures> {
        let (b0, b1) = self.find_slice(text)?;
        let whole = Match::new(b0, b1);
        let groups = self.group_spans(text, b0, b1);
        let mut matches = vec![Some(whole)];
        matches.extend(groups);
        Some(Captures { matches })
    }

    /// The span of each capture group for a slice that `self` ACCEPTED, in
    /// group order. A complemented side rejected the slice, so it (and its
    /// inner side's groups, parity permitting) reports `None` — see
    /// [`Self::rejected_group_spans`]; an intersection concatenates the
    /// accepted slices' groups, left side first.
    fn group_spans(&self, text: &str, b0: usize, b1: usize) -> Vec<Option<Match>> {
        match &self.body {
            RegexBody::Grammar {
                cfg,
                assertions,
                groups,
            } => {
                let sub = &text[b0..b1];
                let n = cfg.token_spans(sub).len().saturating_sub(1);
                let reductions = cfg
                    .slice_reductions(sub, 0, n)
                    .expect("an accepted slice derives the start symbol");

                let mut by_name: HashMap<String, Option<Match>> = HashMap::new();
                for name in groups {
                    if let Some(r) = reductions.iter().rfind(|r| &r.lhs == name) {
                        let m = cfg.reduction_bytes(sub, 0, r);
                        by_name.insert(name.clone(), Some(Match::new(b0 + m.start, b0 + m.end)));
                    }
                }
                for a in assertions {
                    if let Some(r) = reductions
                        .iter()
                        .rfind(|r| r.lhs.as_str() == a.placeholder.as_str())
                    {
                        let pos = cfg.reduction_bytes(sub, 0, r);
                        let (_, spans) = a.evaluate(text, b0 + pos.start);
                        for (name, span) in spans {
                            by_name.insert(name, span);
                        }
                    }
                }

                groups
                    .iter()
                    .map(|name| by_name.get(name).copied().flatten())
                    .collect()
            }
            RegexBody::Complement(inner) => inner.rejected_group_spans(text, b0, b1),
            RegexBody::Intersection(left, right) => {
                let mut spans = left.group_spans(text, b0, b1);
                spans.extend(right.group_spans(text, b0, b1));
                spans
            }
        }
    }

    /// `self` REJECTED the slice, so no group in it participates: every
    /// group is `None` — except that a complement of a rejected slice is a
    /// complement whose inner side ACCEPTED it, so the layers cancel
    /// (repeating, which is how `re.not().not()` captures like `re`).
    fn rejected_group_spans(&self, text: &str, b0: usize, b1: usize) -> Vec<Option<Match>> {
        match &self.body {
            RegexBody::Grammar { groups, .. } => vec![None; groups.len()],
            RegexBody::Complement(inner) => inner.group_spans(text, b0, b1),
            RegexBody::Intersection(left, right) => {
                vec![None; left.group_count() + right.group_count()]
            }
        }
    }

    /// Every assertion whose placeholder occurs in `reductions` must hold.
    /// `reductions` parse the slice `sub` (offsets relative to it);
    /// assertions evaluate against the FULL `text` at the slice's absolute
    /// position (`base` + offset).
    fn assertions_hold(
        assertions: &[Assertion],
        cfg: &CfgParser,
        reductions: &[Reduction<String>],
        sub: &str,
        text: &str,
        base: usize,
    ) -> bool {
        for a in assertions {
            // A placeholder absent from the derivation sat on an alternative
            // branch that was not taken: the assertion is vacuously satisfied
            // (and its groups simply do not participate).
            let Some(r) = reductions
                .iter()
                .rfind(|r| r.lhs.as_str() == a.placeholder.as_str())
            else {
                continue;
            };
            let pos = cfg.reduction_bytes(sub, 0, r);
            let (holds, _) = a.evaluate(text, base + pos.start);
            if !holds {
                return false;
            }
        }
        true
    }
}
