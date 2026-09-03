//! Grammar description: symbols, expressions, and the registry of
//! productions that the LALR(1) compiler consumes.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Symbol {
    Terminal(String),
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

#[derive(Debug, Clone)]
pub enum Expr {
    Terminal(String),
    NonTerminal(String),
    Sequence(Vec<Expr>),
    Choice(Vec<Expr>),
    Empty,
}

impl Expr {
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

pub struct GrammarRegistry {
    pub productions: Vec<(Symbol, Vec<Symbol>)>,
    pub start_symbol: Option<Symbol>,
    pub terminals: HashSet<String>,
}

impl GrammarRegistry {
    pub fn new() -> Self {
        Self {
            productions: Vec::new(),
            start_symbol: None,
            terminals: HashSet::new(),
        }
    }

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

pub struct NamedExpr {
    pub expr: Expr,
}
impl NamedExpr {
    pub fn named(self, name: &str, registry: &mut GrammarRegistry) -> Symbol {
        registry.register(name, self.expr)
    }
}
pub fn expr(input: &str) -> NamedExpr {
    NamedExpr {
        expr: GrammarRegistry::expr(input),
    }
}
