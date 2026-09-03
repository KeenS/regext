use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;

pub mod lexer;

pub use lexer::{Lexer, Token};

// =============================================================================
// Core Engine
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Shift(usize),
    Reduce(usize),
    Accept,
    Error,
}

pub struct Production<S> {
    pub lhs: S,
    pub rhs_len: usize,
    pub id: usize,
}

pub trait Table<S> {
    fn action(&self, state: usize, symbol: Option<&S>) -> Action;
    fn goto(&self, state: usize, symbol: &S) -> usize;
    fn production(&self, id: usize) -> &Production<S>;
}

pub struct Engine<S> {
    stack_a: Vec<S>,
    stack_b: Vec<usize>,
}

impl<S> Engine<S>
where
    S: Clone + PartialEq + Debug,
{
    pub fn new(initial_state: usize) -> Self {
        Self {
            stack_a: Vec::new(),
            stack_b: vec![initial_state],
        }
    }

    pub fn run<T, I>(&mut self, table: &T, input: I) -> Result<S, String>
    where
        T: Table<S>,
        I: IntoIterator<Item = S>,
    {
        let mut iter = input.into_iter().peekable();
        loop {
            let current_state = *self.stack_b.last().ok_or("Stack B empty")?;
            let lookahead = iter.peek();

            match table.action(current_state, lookahead) {
                Action::Shift(next_state) => {
                    let symbol = iter.next().ok_or("Shift failed")?;
                    self.stack_a.push(symbol);
                    self.stack_b.push(next_state);
                }
                Action::Reduce(prod_id) => {
                    let prod = table.production(prod_id);
                    for _ in 0..prod.rhs_len {
                        self.stack_a.pop().ok_or("Stack A underflow")?;
                        self.stack_b.pop().ok_or("Stack B underflow")?;
                    }
                    let lhs = prod.lhs.clone();
                    self.stack_a.push(lhs.clone());
                    let state_before_goto = *self.stack_b.last().ok_or("Stack B empty")?;
                    let next_state = table.goto(state_before_goto, &lhs);
                    self.stack_b.push(next_state);
                }
                Action::Accept => return Ok(self.stack_a.pop().ok_or("Empty stack")?),
                Action::Error => {
                    return Err(format!(
                        "Syntax Error: state {} symbol {:?}",
                        current_state, lookahead
                    ));
                }
            }
        }
    }
}

// =============================================================================
// Grammar Registry
// =============================================================================

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

// =============================================================================
// LALR(1) Compiler
// =============================================================================
//
// Builds LALR(1) action/goto tables (de Remer's construction):
//   1. build the canonical LR(1) automaton — items carry explicit lookaheads
//      that are propagated through FIRST sets with nullability,
//   2. merge LR(1) states that share an LR(0) core, unioning the lookaheads
//      of corresponding items.
//
// Reduce/accept actions are therefore driven by the precise per-state
// lookaheads instead of SLR(1)'s global FOLLOW sets. Every grammar that is
// SLR(1) is LALR(1), and LALR(1) resolves conflicts that SLR(1) could not
// (its lookahead sets are a subset of SLR(1)'s). Collisions that remain
// (grammar is not LALR(1)) are counted and resolved last-write-wins in a
// deterministic (sorted item) order.

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Item {
    prod_idx: usize,
    dot: usize,
    /// Lookahead symbol; `Symbol::Terminal("EOF")` is the end-marker.
    look: Symbol,
}

fn eof() -> Symbol {
    Symbol::Terminal("EOF".to_string())
}

pub struct LalrTable<S> {
    action: HashMap<(usize, Option<S>), Action>,
    goto: HashMap<(usize, S), usize>,
    productions: Vec<Production<S>>,
    /// Number of (state, symbol) table slots where two different actions
    /// collided and were resolved last-write-wins. Zero for conflict-free
    /// LALR(1) grammars; nonzero means the grammar is not LALR(1) and the
    /// behavior on those inputs depends on production order.
    conflicts: usize,
}

impl<S: Clone + PartialEq + Eq + Hash + Debug> LalrTable<S> {
    pub fn conflict_count(&self) -> usize {
        self.conflicts
    }
}

impl<S: Clone + PartialEq + Eq + Hash + Debug> Table<S> for LalrTable<S> {
    fn action(&self, state: usize, symbol: Option<&S>) -> Action {
        self.action
            .get(&(state, symbol.cloned()))
            .cloned()
            .unwrap_or(Action::Error)
    }
    fn goto(&self, state: usize, symbol: &S) -> usize {
        *self.goto.get(&(state, symbol.clone())).unwrap_or(&0)
    }
    fn production(&self, id: usize) -> &Production<S> {
        &self.productions[id]
    }
}

/// FIRST of the sequence `seq` followed by lookahead `look`:
/// FIRST(seq) plus the lookahead itself when the whole sequence is nullable.
fn first_of_seq(
    seq: &[Symbol],
    look: &Symbol,
    first: &HashMap<Symbol, HashSet<Symbol>>,
    nullable: &HashSet<Symbol>,
) -> HashSet<Symbol> {
    let mut out = HashSet::new();
    let mut all_nullable = true;
    for sym in seq {
        if let Symbol::Terminal(_) = sym {
            out.insert(sym.clone());
            all_nullable = false;
            break;
        }
        if let Some(f) = first.get(sym) {
            out.extend(f.iter().cloned());
        }
        if !nullable.contains(sym) {
            all_nullable = false;
            break;
        }
    }
    if all_nullable {
        out.insert(look.clone());
    }
    out
}

/// LR(1) closure: add productions for non-terminals at the dot, with the
/// lookaheads FIRST(rest-of-RHS followed by the item's lookahead).
fn lalr_closure(
    items: Vec<Item>,
    prods: &[(Symbol, Vec<Symbol>)],
    first: &HashMap<Symbol, HashSet<Symbol>>,
    nullable: &HashSet<Symbol>,
) -> Vec<Item> {
    let mut cl = items;
    let mut seen: HashSet<Item> = cl.iter().cloned().collect();
    let mut i = 0;
    while i < cl.len() {
        let Item {
            prod_idx,
            dot,
            look,
        } = cl[i].clone();
        let rhs = &prods[prod_idx].1;
        if dot < rhs.len()
            && let Symbol::NonTerminal(_) = &rhs[dot]
        {
            let looks = first_of_seq(&rhs[dot + 1..], &look, first, nullable);
            for (idx, (lhs, _)) in prods.iter().enumerate() {
                if lhs == &rhs[dot] {
                    for l in &looks {
                        let ni = Item {
                            prod_idx: idx,
                            dot: 0,
                            look: l.clone(),
                        };
                        if seen.insert(ni.clone()) {
                            cl.push(ni);
                        }
                    }
                }
            }
        }
        i += 1;
    }
    cl.sort();
    cl.dedup();
    cl
}

pub struct LRCompiler;

impl LRCompiler {
    pub fn compile<S>(registry: &GrammarRegistry) -> LalrTable<S>
    where
        S: Clone + PartialEq + Eq + Hash + Debug + From<Symbol>,
    {
        let start_sym = registry.start_symbol.as_ref().expect("No start symbol");
        let mut prods = registry.productions.clone();
        let start_sym_augmented = Symbol::NonTerminal(format!(
            "{}_prime",
            match start_sym {
                Symbol::NonTerminal(n) => n,
                Symbol::Terminal(n) => n,
            }
        ));
        prods.insert(0, (start_sym_augmented.clone(), vec![start_sym.clone()]));

        let productions_vec: Vec<Production<S>> = prods
            .iter()
            .enumerate()
            .map(|(i, (lhs, rhs))| Production {
                lhs: S::from(lhs.clone()),
                rhs_len: rhs.len(),
                id: i,
            })
            .collect();

        // -- nullable + FIRST, fixpoint with nullability-aware sequences --
        let mut nullable: HashSet<Symbol> = HashSet::new();
        let mut first: HashMap<Symbol, HashSet<Symbol>> = HashMap::new();
        loop {
            let (n0, f0) = (nullable.clone(), first.clone());
            for (lhs, rhs) in prods.iter() {
                let mut all_nullable = true;
                for sym in rhs.iter() {
                    if let Symbol::Terminal(_) = sym {
                        first.entry(lhs.clone()).or_default().insert(sym.clone());
                        all_nullable = false;
                        break;
                    }
                    if let Some(f) = first.get(sym).cloned() {
                        first.entry(lhs.clone()).or_default().extend(f);
                    }
                    if !nullable.contains(sym) {
                        all_nullable = false;
                        break;
                    }
                }
                if all_nullable {
                    nullable.insert(lhs.clone());
                }
            }
            if nullable == n0 && first == f0 {
                break;
            }
        }

        // -- canonical LR(1) automaton --
        let mut states: Vec<Vec<Item>> = Vec::new();
        let mut trans: HashMap<(usize, Symbol), usize> = HashMap::new();
        let start = lalr_closure(
            vec![Item {
                prod_idx: 0,
                dot: 0,
                look: eof(),
            }],
            &prods,
            &first,
            &nullable,
        );
        states.push(start);
        let mut queue = VecDeque::new();
        queue.push_back(0);
        while let Some(id) = queue.pop_front() {
            let mut syms = HashSet::new();
            for it in &states[id] {
                let rhs = &prods[it.prod_idx].1;
                if it.dot < rhs.len() {
                    syms.insert(rhs[it.dot].clone());
                }
            }
            for sym in syms {
                let advanced: Vec<Item> = states[id]
                    .iter()
                    .filter(|it| {
                        let rhs = &prods[it.prod_idx].1;
                        it.dot < rhs.len() && rhs[it.dot] == sym
                    })
                    .map(|it| Item {
                        prod_idx: it.prod_idx,
                        dot: it.dot + 1,
                        look: it.look.clone(),
                    })
                    .collect();
                let next = lalr_closure(advanced, &prods, &first, &nullable);
                if let Some(t) = states.iter().position(|s| *s == next) {
                    trans.insert((id, sym.clone()), t);
                } else {
                    states.push(next);
                    trans.insert((id, sym.clone()), states.len() - 1);
                    queue.push_back(states.len() - 1);
                }
            }
        }

        // -- merge LR(1) states that share an LR(0) core (LALR(1)) --
        let mut core_map: HashMap<Vec<(usize, usize)>, usize> = HashMap::new();
        let mut lalr_states: Vec<Vec<Item>> = Vec::new();
        let mut state_of: Vec<usize> = Vec::new();
        for items in states.iter() {
            let mut core: Vec<(usize, usize)> =
                items.iter().map(|it| (it.prod_idx, it.dot)).collect();
            core.sort();
            core.dedup();
            let id = match core_map.get(&core).cloned() {
                Some(id) => id,
                None => {
                    core_map.insert(core, lalr_states.len());
                    lalr_states.push(items.clone());
                    lalr_states.len() - 1
                }
            };
            for it in items {
                if !lalr_states[id].contains(it) {
                    lalr_states[id].push(it.clone());
                }
            }
            state_of.push(id);
        }

        // -- transitions over the merged states --
        let mut mtrans: HashMap<(usize, Symbol), usize> = HashMap::new();
        let mut keys: Vec<&(usize, Symbol)> = trans.keys().collect();
        keys.sort();
        for k in keys {
            mtrans
                .entry((state_of[k.0], k.1.clone()))
                .or_insert_with(|| state_of[trans[k]]);
        }

        // -- actions and goto --
        let mut action_table: HashMap<(usize, Option<S>), Action> = HashMap::new();
        let mut goto_table: HashMap<(usize, S), usize> = HashMap::new();
        let mut conflicts: usize = 0;
        for (st, items) in lalr_states.iter().enumerate() {
            let mut sorted: Vec<&Item> = items.iter().collect();
            sorted.sort();
            for it in sorted {
                let rhs = &prods[it.prod_idx].1;
                if it.dot < rhs.len() {
                    let sym = &rhs[it.dot];
                    let target = mtrans.get(&(st, sym.clone())).copied().unwrap_or(0);
                    match sym {
                        Symbol::Terminal(_) => {
                            let k = (st, Some(S::from(sym.clone())));
                            let a = Action::Shift(target);
                            if action_table.get(&k) != Some(&a) {
                                if action_table.contains_key(&k) {
                                    conflicts += 1;
                                }
                                action_table.insert(k, a);
                            }
                        }
                        Symbol::NonTerminal(_) => {
                            goto_table.insert((st, S::from(sym.clone())), target);
                        }
                    }
                } else {
                    let a = if it.prod_idx == 0 && it.look == eof() {
                        Action::Accept
                    } else {
                        Action::Reduce(it.prod_idx)
                    };
                    let k = if it.look == eof() {
                        (st, None)
                    } else {
                        (st, Some(S::from(it.look.clone())))
                    };
                    if action_table.get(&k) != Some(&a) {
                        if action_table.contains_key(&k) {
                            conflicts += 1;
                        }
                        action_table.insert(k, a);
                    }
                }
            }
        }

        LalrTable {
            action: action_table,
            goto: goto_table,
            productions: productions_vec,
            conflicts,
        }
    }
}

// =============================================================================
// Regex Engine - Encoded as CFG
// =============================================================================

#[derive(Debug, Clone)]
pub enum RegexError {
    InvalidPattern(String),
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

#[derive(Debug, Clone)]
pub struct Match {
    pub start: usize,
    pub end: usize,
}

impl Match {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone)]
pub struct Captures {
    pub matches: Vec<Match>,
}

pub struct Regex {
    pattern: String,
    cfg_parser: Option<CfgParser>,
}

impl Regex {
    /// The original pattern string this regex was built from.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn new(pattern: &str) -> Result<Self, RegexError> {
        // Encode regex pattern as CFG and use CFG engine
        let cfg_parser = Self::regex_to_cfg(pattern)?;
        Ok(Self {
            pattern: pattern.to_string(),
            cfg_parser: Some(cfg_parser),
        })
    }

    fn quote(t: &str) -> String {
        format!("\"{}\"", t)
    }

    fn regex_to_cfg(pattern: &str) -> Result<CfgParser, RegexError> {
        // Convert regex to CFG grammar. Terminals are quoted, e.g.:
        //   a*   -> S -> "a" S | epsilon
        //   a+   -> S -> "a" S | "a"
        //   a|b  -> S -> "a" | "b"
        //   ab   -> S -> "ab"
        let mut grammar_parts = Vec::new();

        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                let char_part = Self::quote(parts[0].trim());
                grammar_parts.push(format!("S -> {} S | epsilon", char_part));
            }
        } else if pattern.contains('+') {
            let parts: Vec<&str> = pattern.split('+').collect();
            if parts.len() == 2 {
                let char_part = Self::quote(parts[0].trim());
                grammar_parts.push(format!("S -> {} S | {}", char_part, char_part));
            }
        } else if pattern.contains('|') {
            let alts = pattern
                .split('|')
                .map(|p| Self::quote(p.trim()))
                .collect::<Vec<_>>()
                .join(" | ");
            grammar_parts.push(format!("S -> {}", alts));
        } else {
            grammar_parts.push(format!("S -> {}", Self::quote(pattern.trim())));
        }

        let grammar = grammar_parts.join("; ");
        CfgParser::new(&grammar).map_err(|e| RegexError::CompileError(e.to_string()))
    }

    pub fn is_match(&self, text: &str) -> bool {
        if let Some(parser) = &self.cfg_parser {
            parser.is_match(text)
        } else {
            false
        }
    }

    pub fn find(&self, text: &str) -> Option<Match> {
        if self.is_match(text) {
            Some(Match::new(0, text.len()))
        } else {
            None
        }
    }

    pub fn captures(&self, text: &str) -> Option<Captures> {
        if self.is_match(text) {
            Some(Captures {
                matches: vec![Match::new(0, text.len())],
            })
        } else {
            None
        }
    }
}

// =============================================================================
// CFG Parser
// =============================================================================

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
}
