use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;

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
// API Layer: Grammar IR & Registry
// =============================================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Symbol {
    Terminal(String),
    NonTerminal(String),
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
}

impl GrammarRegistry {
    pub fn new() -> Self {
        Self { productions: Vec::new(), start_symbol: None }
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
        if input.contains('|') {
            let choices = input.split('|').map(|s| Self::expr(s.trim())).collect();
            Expr::Choice(choices)
        } else if input.contains(' ') {
            let parts = input.split_whitespace().map(|s| Self::expr(s)).collect();
            Expr::Sequence(parts)
        } else if input == "epsilon" {
            Expr::Empty
        } else {
            let first_char = input.chars().next().unwrap();
            if first_char.is_uppercase() {
                Expr::NonTerminal(input.to_string())
            } else {
                Expr::Terminal(input.to_string())
            }
        }
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

impl<S: Clone + PartialEq + Eq + Hash + Debug> SLRTable<S> {
    pub fn action_table(&self) -> &HashMap<(usize, Option<S>), Action> {
        &self.action
    }
    pub fn goto_table(&self) -> &HashMap<(usize, S), usize> {
        &self.goto
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
