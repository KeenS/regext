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

use std::collections::HashSet;

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

/// A (parsed) grammar expression: one symbol, a sequence, a choice, or the
/// empty symbol.
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
    /// The empty (epsilon) symbol.
    Empty,
}

impl Expr {
    /// Flatten to a symbol list. Panics on [`Expr::Choice`]: choices are a
    /// production-level concept and must be registered as separate RHSes.
    pub fn to_symbols(&self) -> Vec<Symbol> {
        match self {
            Expr::Terminal(s) => vec![Symbol::Terminal(s.clone())],
            Expr::NonTerminal(s) => vec![Symbol::NonTerminal(s.clone())],
            Expr::Sequence(exprs) => exprs.iter().flat_map(|e| e.to_symbols()).collect(),
            Expr::Choice(_) => panic!("Cannot flatten Choice directly"),
            Expr::Empty => vec![],
        }
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
}

impl GrammarRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            productions: Vec::new(),
            start_symbol: None,
            terminals: HashSet::new(),
        }
    }

    /// Add productions for `name`: each alternative of a [`Expr::Choice`]
    /// becomes its own production. Returns the nonterminal symbol.
    pub fn register(&mut self, name: &str, expr: Expr) -> Symbol {
        let lhs = Symbol::NonTerminal(name.to_string());
        let rhs_list = match expr {
            Expr::Choice(choices) => choices.into_iter().map(|e| e.to_symbols()).collect(),
            e => vec![e.to_symbols()],
        };
        for rhs in rhs_list {
            self.productions.push((lhs.clone(), rhs));
        }
        if self.start_symbol.is_none() {
            self.start_symbol = Some(lhs.clone());
        }
        lhs
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
    /// Register under `name` (first registration becomes the start symbol).
    pub fn named(self, name: &str, registry: &mut GrammarRegistry) -> Symbol {
        registry.register(name, self.expr)
    }
}
/// Parse a right-hand-side string into a [`NamedExpr`] (see
/// [`GrammarRegistry::expr`]).
pub fn expr(input: &str) -> NamedExpr {
    NamedExpr {
        expr: GrammarRegistry::expr(input),
    }
}
