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
    S: Clone + PartialEq + Debug 
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
        I: IntoIterator<Item = S> 
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
                Action::Error => return Err(format!("Syntax Error: state {} symbol {:?}", current_state, lookahead)),
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
        Self { productions: Vec::new(), start_symbol: None, terminals: HashSet::new() }
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
        if self.start_symbol.is_none() { self.start_symbol = Some(lhs.clone()); }
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
            let parts = t.split_whitespace().map(|s| Self::expr(s)).collect();
            return Expr::Sequence(parts);
        }
        if t == "epsilon" {
            return Expr::Empty;
        }
        Expr::NonTerminal(t.to_string())
    }
}

pub struct NamedExpr { pub expr: Expr }
impl NamedExpr {
    pub fn named(self, name: &str, registry: &mut GrammarRegistry) -> Symbol {
        registry.register(name, self.expr)
    }
}
pub fn expr(input: &str) -> NamedExpr { NamedExpr { expr: GrammarRegistry::expr(input) } }

// =============================================================================
// SLR Compiler
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Item {
    prod_idx: usize,
    dot: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct State {
    items: Vec<Item>,
}

pub struct SLRTable<S> {
    action: HashMap<(usize, Option<S>), Action>,
    goto: HashMap<(usize, S), usize>,
    productions: Vec<Production<S>>,
}

impl<S: Clone + PartialEq + Eq + Hash + Debug> Table<S> for SLRTable<S> {
    fn action(&self, state: usize, symbol: Option<&S>) -> Action {
        self.action.get(&(state, symbol.cloned())).cloned().unwrap_or(Action::Error)
    }
    fn goto(&self, state: usize, symbol: &S) -> usize {
        *self.goto.get(&(state, symbol.clone())).unwrap_or(&0)
    }
    fn production(&self, id: usize) -> &Production<S> {
        &self.productions[id]
    }
}

pub struct LRCompiler;

impl LRCompiler {
    pub fn compile<S: Clone + PartialEq + Eq + Hash + Debug>(registry: &GrammarRegistry) -> SLRTable<S> 
    where 
        S: From<Symbol> 
    {
        let start_sym = registry.start_symbol.as_ref().expect("No start symbol");
        let mut prods = registry.productions.clone();
        let start_sym_augmented = Symbol::NonTerminal(format!("{}_prime", 
            match start_sym { Symbol::NonTerminal(n) => n, Symbol::Terminal(n) => n }
        ));
        prods.insert(0, (start_sym_augmented.clone(), vec![start_sym.clone()]));

        let mut productions_vec = Vec::new();
        for (i, (lhs, rhs)) in prods.iter().enumerate() {
            productions_vec.push(Production {
                lhs: S::from(lhs.clone()),
                rhs_len: rhs.len(),
                id: i,
            });
        }

        fn closure(items: Vec<Item>, prods: &[(Symbol, Vec<Symbol>)]) -> Vec<Item> {
            let mut closure = items;
            let mut changed = true;
            while changed {
                changed = false;
                let mut new_items = Vec::new();
                for item in &closure {
                    let rhs = &prods[item.prod_idx].1;
                    if item.dot < rhs.len() {
                        let symbol = &rhs[item.dot];
                        if let Symbol::NonTerminal(_) = symbol {
                            for (idx, (lhs, _)) in prods.iter().enumerate() {
                                if lhs == symbol {
                                    new_items.push(Item { prod_idx: idx, dot: 0 });
                                }
                            }
                        }
                    }
                }
                for ni in new_items {
                    if !closure.contains(&ni) {
                        closure.push(ni);
                        changed = true;
                    }
                }
            }
            closure.sort();
            closure
        }

        let mut states = Vec::new();
        let mut queue = VecDeque::new();
        let start_items = closure(vec![Item { prod_idx: 0, dot: 0 }], &prods);
        let start_state = State { items: start_items };
        states.push(start_state.clone());
        queue.push_back(start_state);

        let mut transitions = HashMap::new();
        while let Some(state) = queue.pop_front() {
            let state_id = states.iter().position(|s| s == &state).unwrap();
            let mut symbols = HashSet::new();
            for item in &state.items {
                let rhs = &prods[item.prod_idx].1;
                if item.dot < rhs.len() {
                    symbols.insert(rhs[item.dot].clone());
                }
            }
            for sym in symbols {
                let next_items = closure(
                    state.items.iter()
                        .filter(|item| {
                            let rhs = &prods[item.prod_idx].1;
                            item.dot < rhs.len() && rhs[item.dot] == sym
                        })
                        .map(|item| Item { prod_idx: item.prod_idx, dot: item.dot + 1 })
                        .collect(),
                    &prods
                );
                let next_state = State { items: next_items };
                if let Some(id) = states.iter().position(|s| s == &next_state) {
                    transitions.insert((state_id, sym), id);
                } else {
                    states.push(next_state.clone());
                    queue.push_back(next_state);
                    transitions.insert((state_id, sym), states.len() - 1);
                }
            }
        }

        let mut first_sets: HashMap<Symbol, HashSet<Symbol>> = HashMap::new();
        let mut changed = true;
        while changed {
            changed = false;
            for (lhs, rhs) in &prods {
                let mut current_first = HashSet::new();
                if !rhs.is_empty() {
                    let first_sym = &rhs[0];
                    if let Symbol::Terminal(_) = first_sym {
                        current_first.insert(first_sym.clone());
                    } else if let Some(f) = first_sets.get(first_sym) {
                        for s in f { current_first.insert(s.clone()); }
                    }
                }
                let entry = first_sets.entry(lhs.clone()).or_default();
                let old_len = entry.len();
                entry.extend(current_first);
                if entry.len() != old_len { changed = true; }
            }
        }

        let mut follow_sets: HashMap<Symbol, HashSet<Symbol>> = HashMap::new();
        let start_sym_augmented = Symbol::NonTerminal(format!("{}_prime", 
            match start_sym { Symbol::NonTerminal(n) => n, Symbol::Terminal(n) => n }
        ));
        follow_sets.entry(start_sym_augmented).or_default().insert(Symbol::Terminal("EOF".to_string()));

        changed = true;
        while changed {
            changed = false;
            for (lhs, rhs) in &prods {
                for i in 0..rhs.len() {
                    let sym = &rhs[i];
                    if let Symbol::NonTerminal(_) = sym {
                        let mut next_follow = HashSet::new();
                        if i + 1 < rhs.len() {
                            let next_sym = &rhs[i+1];
                            if let Symbol::Terminal(_) = next_sym {
                                next_follow.insert(next_sym.clone());
                            } else if let Some(f) = first_sets.get(next_sym) {
                                for s in f { next_follow.insert(s.clone()); }
                            }
                        } else {
                            if let Some(f) = follow_sets.get(lhs) {
                                for s in f { next_follow.insert(s.clone()); }
                            }
                        }
                        let entry = follow_sets.entry(sym.clone()).or_default();
                        let old_len = entry.len();
                        entry.extend(next_follow);
                        if entry.len() != old_len { changed = true; }
                    }
                }
            }
        }

        let mut action_table = HashMap::new();
        let mut goto_table = HashMap::new();
        for (i, state) in states.iter().enumerate() {
            for item in &state.items {
                let rhs = &prods[item.prod_idx].1;
                if item.dot < rhs.len() {
                    let sym = &rhs[item.dot];
                    if let Some(&next_state) = transitions.get(&(i, sym.clone())) {
                        match sym {
                            Symbol::Terminal(_) => {
                                action_table.insert((i, Some(S::from(sym.clone()))), Action::Shift(next_state));
                            }
                            Symbol::NonTerminal(_) => {
                                goto_table.insert((i, S::from(sym.clone())), next_state);
                            }
                        }
                    }
                } else {
                    if item.prod_idx == 0 {
                        action_table.insert((i, None), Action::Accept);
                    } else {
                        let lhs = &prods[item.prod_idx].0;
                        if let Some(follow) = follow_sets.get(lhs) {
                            for sym in follow {
                                if let Symbol::Terminal(t) = sym {
                                    if t == "EOF" {
                                        action_table.insert((i, None), Action::Reduce(item.prod_idx));
                                    } else {
                                        action_table.insert((i, Some(S::from(sym.clone()))), Action::Reduce(item.prod_idx));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        SLRTable { action: action_table, goto: goto_table, productions: productions_vec }
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
            Some(Captures { matches: vec![Match::new(0, text.len())] })
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
    table: SLRTable<String>,
    start_symbol: String,
    terminals: HashSet<String>,
}

impl CfgParser {
    pub fn new(grammar: &str) -> Result<Self, CfgError> {
        let mut registry = GrammarRegistry::new();
        let mut start_symbol = None;
        
        for prod_str in grammar.split(';') {
            let prod_str = prod_str.trim();
            if prod_str.is_empty() { continue; }
            
            if let Some(idx) = prod_str.find("->") {
                let lhs = prod_str[..idx].trim();
                let rhs = prod_str[idx + 2..].trim();
                
                if start_symbol.is_none() {
                    start_symbol = Some(lhs.to_string());
                }
                
                let expr = GrammarRegistry::expr(rhs);
                registry.register(lhs, expr);
            } else {
                return Err(CfgError { message: format!("Invalid production: {}", prod_str) });
            }
        }
        
        let start_sym = start_symbol.ok_or_else(|| CfgError { message: "No start symbol".to_string() })?;
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
        
        Ok(Self { table, start_symbol: start_sym, terminals })
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
        match engine.run(&self.table, symbols.into_iter()) {
            Ok(result) => result == self.start_symbol,
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_encoded_as_cfg() {
        let re = Regex::new("a").unwrap();
        assert!(re.is_match("a"));
    }

#[test]
    fn zz_debug_terminals() {
        let p = CfgParser::new(r#"S -> "[1-5]""#).unwrap();
        eprintln!("terminals: {:?}", p.terminals);
        let lexer = Lexer::new(p.terminals.clone());
        eprintln!("tokens: {:?}", lexer.tokenize("3"));
        let symbols: Vec<String> = lexer
            .tokenize("3")
            .into_iter()
            .filter_map(|t| match t { Token::Terminal(s) => Some(s), Token::EOF => None })
            .collect();
        for s in &symbols {
            eprintln!("action(0, {:?}) = {:?}", s, p.table.action(0, Some(s)));
        }
        let mut engine = Engine::new(0);
        eprintln!("run: {:?}", engine.run(&p.table, symbols));
        eprintln!("start: {:?}", p.start_symbol);
    }

    #[test]
    fn test_cfg_basic() {
        let parser = CfgParser::new(r#"S -> "a""#).unwrap();
        assert!(parser.is_match("a"));
    }
}
