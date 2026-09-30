//! Grammar description: [`Symbol`]s, [`Expr`]essions, and the
//! [`GrammarRegistry`] of productions that the LALR(1) compiler consumes.
//!
//! `GrammarRegistry::expr` parses the production-right-hand-side mini-language
//! used by the grammar-string front end: a quoted token is a terminal, an
//! unquoted one a nonterminal, whitespace separates a sequence, `|` is
//! choice, and `epsilon` is empty.
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

use std::collections::HashSet;

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
}

impl From<Symbol> for String {
    fn from(sym: Symbol) -> String {
        match sym {
            Symbol::Terminal(s) => s,
            Symbol::NonTerminal(s) => s,
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
    pub fn named(
        self,
        name: &str,
        registry: &mut GrammarRegistry,
    ) -> Result<Symbol, CfgError> {
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
}

impl GrammarRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            productions: Vec::new(),
            start_symbol: None,
            terminals: HashSet::new(),
            next_name: 0,
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
            let taken = self
                .productions
                .iter()
                .any(|(lhs, _)| *lhs == Symbol::NonTerminal(name.clone()))
                || self.start_symbol.as_ref() == Some(&Symbol::NonTerminal(name.clone()));
            if !taken {
                return name;
            }
        }
    }

    fn push_production(&mut self, lhs: &str, rhs: Vec<Symbol>) {
        self.productions.push((Symbol::NonTerminal(lhs.to_string()), rhs));
    }

    /// Lower `expr` to a symbol list, registering the fresh nonterminals
    /// that production-level nodes require. Production order mirrors the
    /// `Pattern` lowerer's (`Star`: `N -> body N` then `N -> epsilon`;
    /// `Plus`: `N -> body N` then `N -> body`; `Opt`: `N -> epsilon` then
    /// `N -> body`), so equivalent composable patterns and expressions
    /// compile to conflict-equivalent grammars.
    fn lower(&mut self, expr: Expr) -> Result<Vec<Symbol>, CfgError> {
        match expr {
            Expr::Terminal(s) => Ok(vec![Symbol::Terminal(s)]),
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
    pub fn expr(input: &str) -> Expr {
        let t = input.trim();
        // A quoted token ("...") is a terminal. Its content is the pattern:
        //   "any"      -> any double-quoted string (matched as one unit)
        //   [..]/[..]+ -> character class (optionally a maximal-munch run)
        //   anything   -> an as-is literal
        // An unquoted token is a non-terminal (the old case convention is gone).
        if t.len() >= 2
            && t.starts_with('"')
            && t.ends_with('"')
            && !t[1..t.len() - 1].contains('"')
        {
            let content: String = t.chars().skip(1).take(t.chars().count() - 2).collect();
            return Expr::Terminal(content);
        }
        if t.contains('|') {
            let choices = t.split('|').map(|s| Self::expr(s.trim())).collect();
            return Expr::Choice(choices);
        }
        if t.contains(' ') {
            let parts = t.split_whitespace().map(Self::expr).collect();
            return Expr::Sequence(parts);
        }
        if t == "epsilon" {
            return Expr::Empty;
        }
        Expr::NonTerminal(t.to_string())
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
    pub fn named(
        self,
        name: &str,
        registry: &mut GrammarRegistry,
    ) -> Result<Symbol, CfgError> {
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
