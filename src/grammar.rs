//! Grammar description: [`Symbol`]s, [`Expr`]essions, and the
//! [`GrammarRegistry`] of productions that the LALR(1) compiler consumes.
//!
//! `GrammarRegistry::expr` parses the production-right-hand-side mini-language
//! used by the grammar-string front end: a quoted token is a terminal, an
//! unquoted one a nonterminal, whitespace separates a sequence, `|` is
//! choice, and `epsilon` is empty. `;`, `|`, whitespace, and `->` split
//! only outside quotes. Inside a terminal, `\\` and `\"` are escapes; an
//! empty terminal is rejected.
//!
//! ```
//! use RegExt::{expr, Symbol};
//!
//! let symbols = expr(r#""a" B"#).expr.to_symbols();
//! assert_eq!(
//!     symbols,
//!     vec![Symbol::Terminal("a".to_string()), Symbol::NonTerminal("B".to_string())]
//! );
//! ```
//!
//! `Expr` also carries the same quantifier combinators as [`crate::regex::Pattern`]
//! (`star`, `plus`, `opt`, `times`, `at_least`, `between`) and the same
//! class sugar (`class`, `neg_class`, `digit`, `word`). Quantified pieces
//! and choices are production-level constructs: on registration they lower
//! to fresh nonterminals (`N1`, `N2`, ...) with their own productions —
//! `a.star()` becomes `N -> a N | epsilon`, exactly how the `Pattern`
//! lowerer spells `a*` — so a grouped choice inside a sequence such as
//! `a.or(b).then(c)` registers just as well as a top-level one. What the
//! CFG dialect does *not* have are `Regex`'s `not` / `and` (context-free
//! languages are not closed under complement or intersection) and its
//! zero-width assertions (a `Regex`-engine feature); see the crate docs.

use std::collections::{HashMap, HashSet};

use crate::cfg_parser::CfgError;
use crate::regex::MAX_BOUND;

/// A grammar symbol: either a terminal (matched by the lexer) or a
/// nonterminal (derived by productions).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Symbol {
    /// A terminal symbol; the string is the token text the lexer emits.
    Terminal(String),
    /// A nonterminal symbol; the string is its name in the productions.
    NonTerminal(String),
    /// End-of-input marker used by the LALR compiler. It is not a user
    /// terminal: a production that mentions the text `EOF` is an ordinary
    /// [`Symbol::Terminal`].
    End,
}

impl From<Symbol> for String {
    fn from(sym: Symbol) -> String {
        match sym {
            Symbol::Terminal(s) | Symbol::NonTerminal(s) => s,
            // Not `"EOF"`: that string is a legal user terminal.
            Symbol::End => "\u{0}end".to_string(),
        }
    }
}

/// A (parsed) grammar expression: one symbol, a sequence, a choice, a
/// quantified piece, or the empty symbol.
///
/// The quantifier variants (`star`, `plus`, `opt`, `times`, `at_least`,
/// `between`) are the CFG spelling of `Pattern`'s: they lower to fresh
/// nonterminals when the expression is registered (see
/// [`GrammarRegistry::register`]).
#[derive(Debug, Clone)]
pub enum Expr {
    /// A single terminal.
    Terminal(String),
    /// A single nonterminal.
    NonTerminal(String),
    /// Symbols in order.
    Sequence(Vec<Expr>),
    /// Alternative derivations (registered as separate productions).
    Choice(Vec<Expr>),
    /// Zero or more repetitions (`self*`).
    Star(Box<Expr>),
    /// One or more repetitions (`self+`).
    Plus(Box<Expr>),
    /// Zero or one occurrence (`self?`).
    Opt(Box<Expr>),
    /// A bounded repetition: the base, the minimum number of copies, and the
    /// maximum (`None` = unbounded, i.e. `{n,}`).
    Bound(Box<Expr>, usize, Option<usize>),
    /// The empty (epsilon) symbol.
    Empty,
}

impl Expr {
    /// The empty (epsilon) expression.
    pub fn epsilon() -> Self {
        Expr::Empty
    }

    /// A terminal expression: a string the lexer matches (a literal, a
    /// `[class]`, a `[class]+` maximal-munch run, or `any` — see
    /// [`crate::lexer`]).
    pub fn str(symbol: impl Into<String>) -> Self {
        Expr::Terminal(symbol.into())
    }

    /// A character-class terminal: one character from the class (single
    /// members or `a-z` ranges) — the same terminal shape the lexer
    /// understands, spelled without the brackets.
    pub fn class(body: impl Into<String>) -> Self {
        Expr::str(format!("[{}]", body.into()))
    }

    /// A negated character-class terminal: one character NOT in the class.
    pub fn neg_class(body: impl Into<String>) -> Self {
        Expr::str(format!("[^{}]", body.into()))
    }

    /// The `\d` analog: one digit.
    pub fn digit() -> Self {
        Expr::class("0-9")
    }

    /// The `\w` analog: one word character.
    pub fn word() -> Self {
        Expr::class("a-zA-Z0-9_")
    }

    /// A reference to a named piece: the nonterminal `call`ed inside a
    /// sequence or choice (defined by a production registered under that
    /// name).
    pub fn call(name: impl Into<String>) -> Self {
        Expr::NonTerminal(name.into())
    }

    /// `self | other` — choice, flattened so `a.or(b).or(c)` is one choice.
    pub fn or(self, other: Expr) -> Expr {
        match (self, other) {
            (Expr::Choice(mut a), Expr::Choice(b)) => {
                a.extend(b);
                Expr::Choice(a)
            }
            (Expr::Choice(mut a), b) => {
                a.push(b);
                Expr::Choice(a)
            }
            (a, Expr::Choice(mut b)) => {
                b.insert(0, a);
                Expr::Choice(b)
            }
            (a, b) => Expr::Choice(vec![a, b]),
        }
    }

    /// `self other` — sequence, flattened so `a.then(b).then(c)` is one sequence.
    pub fn then(self, other: Expr) -> Expr {
        match (self, other) {
            (Expr::Sequence(mut a), Expr::Sequence(b)) => {
                a.extend(b);
                Expr::Sequence(a)
            }
            (Expr::Sequence(mut a), b) => {
                a.push(b);
                Expr::Sequence(a)
            }
            (a, Expr::Sequence(mut b)) => {
                b.insert(0, a);
                Expr::Sequence(b)
            }
            (a, b) => Expr::Sequence(vec![a, b]),
        }
    }

    /// Zero or more repetitions (`self*`): registered as a fresh nonterminal
    /// `N` with `N -> self N | epsilon`. A quantifier over the empty
    /// expression is just the empty expression (`ε* = ε`).
    pub fn star(self) -> Self {
        if matches!(self, Expr::Empty) {
            Expr::Empty
        } else {
            Expr::Star(Box::new(self))
        }
    }

    /// One or more repetitions (`self+`): registered as `N -> self N | self`.
    pub fn plus(self) -> Self {
        if matches!(self, Expr::Empty) {
            Expr::Empty
        } else {
            Expr::Plus(Box::new(self))
        }
    }

    /// Zero or one occurrence (`self?`): registered as `N -> epsilon | self`.
    pub fn opt(self) -> Self {
        if matches!(self, Expr::Empty) {
            Expr::Empty
        } else {
            Expr::Opt(Box::new(self))
        }
    }

    /// Exactly `n` copies (`self{n}`); `n` must be at most 100 — the cap is
    /// checked at registration (see [`GrammarRegistry::register`]).
    pub fn times(self, n: usize) -> Self {
        Expr::Bound(Box::new(self), n, Some(n))
    }

    /// At least `n` copies (`self{n,}`); `n` must be at most 100.
    pub fn at_least(self, n: usize) -> Self {
        Expr::Bound(Box::new(self), n, None)
    }

    /// Between `min` and `max` copies (`self{min,max}`); registration
    /// refuses unbounded-or-inverted bounds.
    pub fn between(self, min: usize, max: usize) -> Self {
        Expr::Bound(Box::new(self), min, Some(max))
    }

    /// Flatten to a symbol list. Panics on production-level nodes
    /// ([`Expr::Choice`] and the quantifiers): they are production-level
    /// constructs and must be registered, which is where their fresh
    /// nonterminals come from.
    pub fn to_symbols(&self) -> Vec<Symbol> {
        match self {
            Expr::Terminal(s) => vec![Symbol::Terminal(s.clone())],
            Expr::NonTerminal(s) => vec![Symbol::NonTerminal(s.clone())],
            Expr::Sequence(exprs) => exprs.iter().flat_map(|e| e.to_symbols()).collect(),
            Expr::Choice(_) => panic!("Cannot flatten Choice directly"),
            Expr::Star(_) | Expr::Plus(_) | Expr::Opt(_) | Expr::Bound(..) => {
                panic!("Cannot flatten a quantified Expr directly")
            }
            Expr::Empty => vec![],
        }
    }

    /// Register this expression under `name` in `registry` (the first
    /// registration becomes the start symbol) and return the nonterminal.
    /// Fails with [`CfgError`] when a repetition bound is inverted or
    /// exceeds the 100-copy cap.
    pub fn named(self, name: &str, registry: &mut GrammarRegistry) -> Result<Symbol, CfgError> {
        registry.register(name, self)
    }
}

/// The set of productions a [`crate::lalr::LRCompiler`] consumes.
///
/// The first registered nonterminal is the start symbol.
pub struct GrammarRegistry {
    /// Productions as (LHS, RHS) pairs, in registration order. Order matters:
    /// remaining table conflicts are resolved in this order.
    pub productions: Vec<(Symbol, Vec<Symbol>)>,
    /// The first registered nonterminal, if any.
    pub start_symbol: Option<Symbol>,
    /// Terminal names that occur in the productions.
    pub terminals: HashSet<String>,
    /// The next fresh nonterminal name to hand out for production-level
    /// nodes (choices nested in a sequence, quantified pieces).
    next_name: usize,
    /// Nonterminal names a fresh name must not take: the LHS being
    /// registered, nonterminals the expression refers to, and names already
    /// handed out. Checked before those productions exist.
    reserved: HashSet<String>,
}

impl GrammarRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            productions: Vec::new(),
            start_symbol: None,
            terminals: HashSet::new(),
            next_name: 0,
            reserved: HashSet::new(),
        }
    }

    /// Add productions for `name`: each alternative of a [`Expr::Choice`]
    /// becomes its own production; production-level nodes (choices nested
    /// in a sequence, quantified pieces) get fresh nonterminals `N1`,
    /// `N2`, ... whose productions are registered alongside. Returns the
    /// nonterminal symbol.
    ///
    /// Fails with [`CfgError`] on a repetition bound that exceeds the
    /// 100-copy cap or an inverted `between` bound — the same checks the
    /// `Pattern` front end applies at compile time.
    pub fn register(&mut self, name: &str, expr: Expr) -> Result<Symbol, CfgError> {
        self.reserved.insert(name.to_string());
        reserve_refs(&expr, &mut self.reserved);
        let lhs = Symbol::NonTerminal(name.to_string());
        let rhs_list = match expr {
            Expr::Choice(choices) => choices
                .into_iter()
                .map(|e| self.lower(e))
                .collect::<Result<Vec<_>, CfgError>>()?,
            e => vec![self.lower(e)?],
        };
        for rhs in rhs_list {
            self.productions.push((lhs.clone(), rhs));
        }
        if self.start_symbol.is_none() {
            self.start_symbol = Some(lhs.clone());
        }
        Ok(lhs)
    }

    /// A fresh nonterminal name (`N1`, `N2`, ...), never colliding with a
    /// nonterminal already registered in this registry.
    fn fresh_name(&mut self) -> String {
        loop {
            self.next_name += 1;
            let name = format!("N{}", self.next_name);
            let symbol = Symbol::NonTerminal(name.clone());
            let taken = self.reserved.contains(&name)
                || self.productions.iter().any(|(lhs, _)| lhs == &symbol)
                || self.start_symbol.as_ref() == Some(&symbol);
            if !taken {
                self.reserved.insert(name.clone());
                return name;
            }
        }
    }

    fn push_production(&mut self, lhs: &str, rhs: Vec<Symbol>) {
        self.productions
            .push((Symbol::NonTerminal(lhs.to_string()), rhs));
    }

    /// Lower `expr` to a symbol list, registering the fresh nonterminals
    /// that production-level nodes require. Production order mirrors the
    /// `Pattern` lowerer's (`Star`: `N -> body N` then `N -> epsilon`;
    /// `Plus`: `N -> body N` then `N -> body`; `Opt`: `N -> epsilon` then
    /// `N -> body`), so equivalent composable patterns and expressions
    /// compile to conflict-equivalent grammars.
    fn lower(&mut self, expr: Expr) -> Result<Vec<Symbol>, CfgError> {
        match expr {
            Expr::Terminal(s) => {
                if s.is_empty() {
                    return Err(CfgError::new("empty terminal"));
                }
                self.terminals.insert(s.clone());
                Ok(vec![Symbol::Terminal(s)])
            }
            Expr::NonTerminal(s) => Ok(vec![Symbol::NonTerminal(s)]),
            Expr::Empty => Ok(vec![]),
            Expr::Sequence(exprs) => {
                let mut out = Vec::new();
                for e in exprs {
                    out.extend(self.lower(e)?);
                }
                Ok(out)
            }
            Expr::Choice(choices) => {
                let name = self.fresh_name();
                for choice in choices {
                    let rhs = self.lower(choice)?;
                    self.push_production(&name, rhs);
                }
                Ok(vec![Symbol::NonTerminal(name)])
            }
            Expr::Star(base) => {
                let body = self.lower(*base)?;
                if body.is_empty() {
                    return Ok(vec![]); // (ε)* = ε
                }
                let name = self.fresh_name();
                let mut rec = body.clone();
                rec.push(Symbol::NonTerminal(name.clone()));
                self.push_production(&name, rec);
                self.push_production(&name, vec![]);
                Ok(vec![Symbol::NonTerminal(name)])
            }
            Expr::Plus(base) => {
                let body = self.lower(*base)?;
                if body.is_empty() {
                    return Ok(vec![]); // (ε)+ = ε
                }
                let name = self.fresh_name();
                let mut rec = body.clone();
                rec.push(Symbol::NonTerminal(name.clone()));
                self.push_production(&name, rec);
                self.push_production(&name, body);
                Ok(vec![Symbol::NonTerminal(name)])
            }
            Expr::Opt(base) => {
                let body = self.lower(*base)?;
                if body.is_empty() {
                    return Ok(vec![]); // (ε)? = ε
                }
                let name = self.fresh_name();
                self.push_production(&name, vec![]);
                self.push_production(&name, body);
                Ok(vec![Symbol::NonTerminal(name)])
            }
            Expr::Bound(base, min, max) => {
                let body = self.lower(*base)?;
                self.lower_bound(body, min, max)
            }
        }
    }

    /// Lower a bounded repetition over the already-lowered `body`, in the
    /// same production order as the `Pattern` lowerer's `{n}`, `{n,}`, and
    /// `{n,m}` forms.
    fn lower_bound(
        &mut self,
        body: Vec<Symbol>,
        min: usize,
        max: Option<usize>,
    ) -> Result<Vec<Symbol>, CfgError> {
        if min > MAX_BOUND as usize {
            return Err(CfgError::new(format!(
                "repetition bound {min} exceeds the maximum of {MAX_BOUND}"
            )));
        }
        if let Some(max) = max {
            if max > MAX_BOUND as usize {
                return Err(CfgError::new(format!(
                    "repetition bound {max} exceeds the maximum of {MAX_BOUND}"
                )));
            }
            if max < min {
                return Err(CfgError::new(format!(
                    "repetition bound {min}..{max} has its upper bound below its lower"
                )));
            }
        }
        match max {
            Some(max) if min == max => {
                // Exactly n copies (inlined; n == 0 is the empty symbol).
                Ok(Self::copies(&body, min))
            }
            Some(max) => {
                // n..m: one nonterminal with m-n+1 alternatives of
                // n..m copies (separate productions, LALR-safe).
                let name = self.fresh_name();
                for k in min..=max {
                    self.push_production(&name, Self::copies(&body, k));
                }
                Ok(vec![Symbol::NonTerminal(name)])
            }
            None => {
                // At least n: n copies plus any number of extras (`base*`),
                // so `{n,}` matches exactly n as well.
                if body.is_empty() {
                    return Ok(vec![]);
                }
                let extra = self.fresh_name();
                let mut rec = body.clone();
                rec.push(Symbol::NonTerminal(extra.clone()));
                self.push_production(&extra, rec);
                self.push_production(&extra, vec![]);
                let mut out = Self::copies(&body, min);
                out.push(Symbol::NonTerminal(extra));
                Ok(out)
            }
        }
    }

    /// `body` repeated `n` times (clone-based: symbols are not `Copy`).
    fn copies(body: &[Symbol], n: usize) -> Vec<Symbol> {
        let mut out = Vec::with_capacity(body.len() * n);
        for _ in 0..n {
            out.extend(body.iter().cloned());
        }
        out
    }

    /// Parse a right-hand-side string: quoted = terminal, `|` = choice,
    /// whitespace = sequence, `epsilon` = empty, anything else = nonterminal.
    ///
    /// `;`, `|`, whitespace, and `->` split tokens only outside quotes.
    /// Inside a quote, `\\` and `\"` are escapes. Panics on an unclosed
    /// quote or an empty terminal; [`parse_rhs`] returns those as errors.
    pub fn expr(input: &str) -> Expr {
        parse_rhs(input).expect("grammar expression")
    }
}

fn reserve_refs(expr: &Expr, reserved: &mut HashSet<String>) {
    match expr {
        Expr::NonTerminal(name) => {
            reserved.insert(name.clone());
        }
        Expr::Sequence(xs) | Expr::Choice(xs) => {
            for item in xs {
                reserve_refs(item, reserved);
            }
        }
        Expr::Star(inner) | Expr::Plus(inner) | Expr::Opt(inner) | Expr::Bound(inner, _, _) => {
            reserve_refs(inner, reserved);
        }
        Expr::Terminal(_) | Expr::Empty => {}
    }
}

impl Default for GrammarRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A parsed expression plus a name to register it under.
pub struct NamedExpr {
    /// The parsed expression.
    pub expr: Expr,
}
impl NamedExpr {
    /// `self | other`; `other` may be an [`Expr`] or another `NamedExpr`.
    pub fn or(self, other: impl Into<Expr>) -> Self {
        NamedExpr {
            expr: self.expr.or(other.into()),
        }
    }

    /// `self other`; `other` may be an [`Expr`] or another `NamedExpr`.
    pub fn then(self, other: impl Into<Expr>) -> Self {
        NamedExpr {
            expr: self.expr.then(other.into()),
        }
    }

    /// `self*` — zero or more repetitions.
    pub fn star(self) -> Self {
        NamedExpr {
            expr: self.expr.star(),
        }
    }

    /// `self+` — one or more repetitions.
    pub fn plus(self) -> Self {
        NamedExpr {
            expr: self.expr.plus(),
        }
    }

    /// `self?` — zero or one occurrence.
    pub fn opt(self) -> Self {
        NamedExpr {
            expr: self.expr.opt(),
        }
    }

    /// `self{n}` — exactly `n` copies.
    pub fn times(self, n: usize) -> Self {
        NamedExpr {
            expr: self.expr.times(n),
        }
    }

    /// `self{n,}` — at least `n` copies.
    pub fn at_least(self, n: usize) -> Self {
        NamedExpr {
            expr: self.expr.at_least(n),
        }
    }

    /// `self{min,max}` — between `min` and `max` copies.
    pub fn between(self, min: usize, max: usize) -> Self {
        NamedExpr {
            expr: self.expr.between(min, max),
        }
    }

    /// Register under `name` (first registration becomes the start symbol).
    pub fn named(self, name: &str, registry: &mut GrammarRegistry) -> Result<Symbol, CfgError> {
        self.expr.named(name, registry)
    }
}
impl From<Expr> for NamedExpr {
    fn from(expr: Expr) -> Self {
        Self { expr }
    }
}
impl From<NamedExpr> for Expr {
    fn from(named: NamedExpr) -> Self {
        named.expr
    }
}
/// Parse a right-hand-side string into a [`NamedExpr`] (see
/// [`GrammarRegistry::expr`]).
pub fn expr(input: &str) -> NamedExpr {
    NamedExpr {
        expr: GrammarRegistry::expr(input),
    }
}

/// Parse one right-hand side. Fails on an unclosed quote, a dangling
/// escape, or an empty quoted terminal.
pub(crate) fn parse_rhs(input: &str) -> Result<Expr, CfgError> {
    let choices = split_outside_quotes(input, '|')?;
    if choices.len() > 1 {
        let mut alts = Vec::new();
        for choice in choices {
            alts.push(parse_sequence_rhs(&choice)?);
        }
        return Ok(Expr::Choice(alts));
    }
    parse_sequence_rhs(input)
}

fn parse_sequence_rhs(input: &str) -> Result<Expr, CfgError> {
    let parts = split_whitespace_outside_quotes(input)?;
    if parts.is_empty() {
        return Ok(Expr::NonTerminal(String::new()));
    }
    if parts.len() == 1 {
        return parse_symbol_token(&parts[0]);
    }
    let mut seq = Vec::new();
    for part in parts {
        seq.push(parse_symbol_token(&part)?);
    }
    Ok(Expr::Sequence(seq))
}

fn parse_symbol_token(token: &str) -> Result<Expr, CfgError> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(Expr::NonTerminal(String::new()));
    }
    if token == "epsilon" {
        return Ok(Expr::Empty);
    }
    if token.starts_with('"') {
        let chars: Vec<char> = token.chars().collect();
        let (content, next) = scan_quoted(&chars, 0)?;
        if next != chars.len() {
            return Err(CfgError::new(format!(
                "trailing input after terminal: {}",
                chars[next..].iter().collect::<String>()
            )));
        }
        if content.is_empty() {
            return Err(CfgError::new("empty terminal"));
        }
        return Ok(Expr::Terminal(content));
    }
    Ok(Expr::NonTerminal(token.to_string()))
}

/// Split a grammar into `(lhs, rhs)` productions. Separators and the `->`
/// arrow are recognized only outside quotes.
pub(crate) fn split_grammar(grammar: &str) -> Result<Vec<(String, String)>, CfgError> {
    let mut productions = Vec::new();
    for piece in split_outside_quotes(grammar, ';')? {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let (lhs, rhs) = split_arrow(piece)?;
        if lhs.is_empty() {
            return Err(CfgError::new(format!("Invalid production: {piece}")));
        }
        productions.push((lhs, rhs));
    }
    Ok(productions)
}

fn split_arrow(production: &str) -> Result<(String, String), CfgError> {
    let chars: Vec<char> = production.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            let (_, next) = scan_quoted(&chars, i)?;
            i = next;
            continue;
        }
        if chars[i] == '-' && i + 1 < chars.len() && chars[i + 1] == '>' {
            let lhs: String = chars[..i].iter().collect();
            let rhs: String = chars[i + 2..].iter().collect();
            return Ok((lhs.trim().to_string(), rhs.trim().to_string()));
        }
        i += 1;
    }
    Err(CfgError::new(format!("Invalid production: {production}")))
}

fn split_outside_quotes(input: &str, sep: char) -> Result<Vec<String>, CfgError> {
    let chars: Vec<char> = input.chars().collect();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            let (_, next) = scan_quoted(&chars, i)?;
            i = next;
            continue;
        }
        if chars[i] == sep {
            parts.push(chars[start..i].iter().collect());
            i += 1;
            start = i;
            continue;
        }
        i += 1;
    }
    parts.push(chars[start..].iter().collect());
    Ok(parts)
}

fn split_whitespace_outside_quotes(input: &str) -> Result<Vec<String>, CfgError> {
    let chars: Vec<char> = input.chars().collect();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] == '"' {
            let (_, next) = scan_quoted(&chars, i)?;
            parts.push(chars[i..next].iter().collect());
            i = next;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            if chars[i] == '"' {
                break;
            }
            i += 1;
        }
        parts.push(chars[start..i].iter().collect());
    }
    Ok(parts)
}

/// `i` points at the opening quote. Returns the unescaped content and the
/// index just past the closing quote.
fn scan_quoted(chars: &[char], i: usize) -> Result<(String, usize), CfgError> {
    let mut j = i + 1;
    let mut content = String::new();
    while j < chars.len() {
        match chars[j] {
            '\\' => {
                if j + 1 >= chars.len() {
                    return Err(CfgError::new("dangling escape in terminal"));
                }
                content.push(chars[j + 1]);
                j += 2;
            }
            '"' => return Ok((content, j + 1)),
            ch => {
                content.push(ch);
                j += 1;
            }
        }
    }
    Err(CfgError::new("unclosed quote in grammar"))
}

#[derive(Clone)]
enum TermKind {
    Any,
    ClassRun(HashSet<char>),
    Positive(HashSet<char>),
    Negated(HashSet<char>),
    Literal,
}

fn classify_terminal(term: &str) -> Result<TermKind, CfgError> {
    if term.is_empty() {
        return Err(CfgError::new("empty terminal"));
    }
    if term == "any" {
        return Ok(TermKind::Any);
    }
    let chars: Vec<char> = term.chars().collect();
    if chars.len() >= 4
        && chars[0] == '['
        && chars[chars.len() - 1] == '+'
        && chars[chars.len() - 2] == ']'
    {
        let inner: String = chars[1..chars.len() - 2].iter().collect();
        let members =
            crate::lexer::parse_class_members(&inner).map_err(|message| CfgError::new(message))?;
        return Ok(TermKind::ClassRun(members));
    }
    if chars.len() >= 4 && chars[0] == '[' && chars[1] == '^' && chars[chars.len() - 1] == ']' {
        let inner: String = chars[2..chars.len() - 1].iter().collect();
        let members =
            crate::lexer::parse_class_members(&inner).map_err(|message| CfgError::new(message))?;
        return Ok(TermKind::Negated(members));
    }
    if chars.len() >= 3 && chars[0] == '[' && chars[chars.len() - 1] == ']' {
        let inner: String = chars[1..chars.len() - 1].iter().collect();
        let members =
            crate::lexer::parse_class_members(&inner).map_err(|message| CfgError::new(message))?;
        if members.is_empty() {
            return Err(CfgError::new(format!(
                "empty character class in terminal \"{term}\""
            )));
        }
        return Ok(TermKind::Positive(members));
    }
    Ok(TermKind::Literal)
}

fn sorted_chars(members: &HashSet<char>) -> Vec<char> {
    let mut chars: Vec<char> = members.iter().copied().collect();
    chars.sort();
    chars
}

fn occurrences_are_sole(prods: &[(Symbol, Vec<Symbol>)], term: &str) -> bool {
    let mut saw = false;
    for (_, rhs) in prods {
        let hits = rhs
            .iter()
            .filter(|sym| matches!(sym, Symbol::Terminal(name) if name == term))
            .count();
        if hits == 0 {
            continue;
        }
        saw = true;
        if hits != 1 || rhs.len() != 1 {
            return false;
        }
    }
    saw
}

fn push_unique(out: &mut Vec<(Symbol, Vec<Symbol>)>, prod: (Symbol, Vec<Symbol>)) {
    if !out.iter().any(|existing| existing == &prod) {
        out.push(prod);
    }
}

fn emit_negated(
    lhs: &Symbol,
    excluded: &HashSet<char>,
    chars: &HashSet<char>,
    other: &str,
    out: &mut Vec<(Symbol, Vec<Symbol>)>,
) {
    push_unique(
        out,
        (lhs.clone(), vec![Symbol::Terminal(other.to_string())]),
    );
    let mut included: Vec<char> = chars
        .iter()
        .copied()
        .filter(|ch| !excluded.contains(ch))
        .collect();
    included.sort();
    for ch in included {
        push_unique(out, (lhs.clone(), vec![Symbol::Terminal(ch.to_string())]));
    }
}

/// Rewrite positive classes into shared single-character terminals and
/// negated classes into those characters plus one OTHER token. Class-runs
/// stay lexer terminals. Also returns every terminal the lexer must know,
/// including characters that are excluded by a negated class and therefore
/// have no production.
pub(crate) fn expand_classes(
    prods: Vec<(Symbol, Vec<Symbol>)>,
) -> Result<(Vec<(Symbol, Vec<Symbol>)>, HashSet<String>), CfgError> {
    let mut kinds: HashMap<String, TermKind> = HashMap::new();
    for (_, rhs) in &prods {
        for sym in rhs {
            if let Symbol::Terminal(term) = sym {
                if !kinds.contains_key(term) {
                    kinds.insert(term.clone(), classify_terminal(term)?);
                }
            }
        }
    }

    let mut class_run_chars: HashSet<char> = HashSet::new();
    let mut positive: HashMap<String, HashSet<char>> = HashMap::new();
    let mut negated: HashMap<String, HashSet<char>> = HashMap::new();
    for (name, kind) in &kinds {
        match kind {
            TermKind::ClassRun(members) => class_run_chars.extend(members.iter().copied()),
            TermKind::Positive(members) => {
                positive.insert(name.clone(), members.clone());
            }
            TermKind::Negated(members) => {
                negated.insert(name.clone(), members.clone());
            }
            TermKind::Any | TermKind::Literal => {}
        }
    }

    let mut nt_id = 0;
    let mut fresh_nt = || {
        nt_id += 1;
        format!("\u{0}C{nt_id}")
    };

    let mut inline_pos: HashSet<String> = HashSet::new();
    let mut pos_nt: HashMap<String, String> = HashMap::new();
    for term in positive.keys() {
        if occurrences_are_sole(&prods, term) {
            inline_pos.insert(term.clone());
        } else {
            pos_nt.insert(term.clone(), fresh_nt());
        }
    }

    let mut expanded: Vec<(Symbol, Vec<Symbol>)> = Vec::new();
    for (lhs, rhs) in &prods {
        if let [Symbol::Terminal(term)] = rhs.as_slice() {
            if inline_pos.contains(term) {
                for ch in sorted_chars(&positive[term]) {
                    push_unique(
                        &mut expanded,
                        (lhs.clone(), vec![Symbol::Terminal(ch.to_string())]),
                    );
                }
                continue;
            }
        }
        let new_rhs = rhs
            .iter()
            .map(|sym| match sym {
                Symbol::Terminal(term) => {
                    if let Some(name) = pos_nt.get(term) {
                        Symbol::NonTerminal(name.clone())
                    } else {
                        sym.clone()
                    }
                }
                other => other.clone(),
            })
            .collect();
        expanded.push((lhs.clone(), new_rhs));
    }
    for (term, name) in &pos_nt {
        let lhs = Symbol::NonTerminal(name.clone());
        for ch in sorted_chars(&positive[term]) {
            push_unique(
                &mut expanded,
                (lhs.clone(), vec![Symbol::Terminal(ch.to_string())]),
            );
        }
    }

    let mut inline_neg: HashSet<String> = HashSet::new();
    let mut neg_nt: HashMap<String, String> = HashMap::new();
    for term in negated.keys() {
        if occurrences_are_sole(&expanded, term) {
            inline_neg.insert(term.clone());
        } else {
            neg_nt.insert(term.clone(), fresh_nt());
        }
    }

    let mut char_terms: HashSet<char> = HashSet::new();
    for (_, rhs) in &expanded {
        for sym in rhs {
            if let Symbol::Terminal(term) = sym {
                if term.chars().count() == 1 {
                    char_terms.extend(term.chars());
                } else if term != "any" && !term.starts_with('[') {
                    for ch in term.chars() {
                        if !class_run_chars.contains(&ch) {
                            char_terms.insert(ch);
                        }
                    }
                }
            }
        }
    }
    if !negated.is_empty() {
        for excluded in negated.values() {
            for ch in excluded {
                if !class_run_chars.contains(ch) {
                    char_terms.insert(*ch);
                }
            }
        }
    }

    let other = if negated.is_empty() {
        None
    } else {
        let mut name = "\u{0}other".to_string();
        let mut taken = HashSet::new();
        for (_, rhs) in &expanded {
            for sym in rhs {
                if let Symbol::Terminal(term) = sym {
                    taken.insert(term.clone());
                }
            }
        }
        while taken.contains(&name) || char_terms.iter().any(|ch| ch.to_string() == name) {
            name.push('\u{0}');
        }
        Some(name)
    };

    let mut rewritten = Vec::new();
    for (lhs, rhs) in expanded {
        if let [Symbol::Terminal(term)] = rhs.as_slice() {
            if inline_neg.contains(term) {
                emit_negated(
                    &lhs,
                    &negated[term],
                    &char_terms,
                    other.as_deref().expect("negated class has an OTHER token"),
                    &mut rewritten,
                );
                continue;
            }
        }
        let new_rhs = rhs
            .into_iter()
            .map(|sym| match &sym {
                Symbol::Terminal(term) => {
                    if let Some(name) = neg_nt.get(term) {
                        Symbol::NonTerminal(name.clone())
                    } else {
                        sym
                    }
                }
                _ => sym,
            })
            .collect();
        rewritten.push((lhs, new_rhs));
    }
    if let Some(other_name) = &other {
        for (term, name) in &neg_nt {
            let lhs = Symbol::NonTerminal(name.clone());
            emit_negated(
                &lhs,
                &negated[term],
                &char_terms,
                other_name,
                &mut rewritten,
            );
        }
    }

    let mut terminals = HashSet::new();
    for (_, rhs) in &rewritten {
        for sym in rhs {
            if let Symbol::Terminal(term) = sym {
                terminals.insert(term.clone());
            }
        }
    }
    for ch in &char_terms {
        terminals.insert(ch.to_string());
    }
    if let Some(other_name) = &other {
        terminals.insert(other_name.clone());
    }
    Ok((rewritten, terminals))
}
