//! The LALR(1) compiler: de Remer's construction over a
//! [`GrammarRegistry`].
//!
//! The construction has two phases:
//!
//! 1. Build the canonical **LR(1)** automaton — items carry explicit
//!    lookaheads propagated through FIRST sets with nullability.
//! 2. Merge LR(1) states that share an **LR(0) core** (LALR(1)), unioning the
//!    lookaheads of corresponding items.
//!
//! Reduce/accept actions are therefore driven by precise per-state lookaheads
//! instead of SLR(1)'s global FOLLOW sets: every SLR(1) grammar is LALR(1),
//! and LALR(1) resolves conflicts SLR(1) cannot. Collisions that remain mean
//! the grammar is **not** LALR(1); they are counted (see
//! [`LalrTable::conflict_count`]) and resolved last-write-wins in a
//! deterministic (sorted-item) order so behavior stays reproducible.
//!
//! ```
//! use RegExt::{GrammarRegistry, LRCompiler, expr};
//!
//! // Conflict-free grammar:
//! let mut ok = GrammarRegistry::new();
//! expr(r#""a" | "b""#).named("S", &mut ok).unwrap();
//! assert_eq!(LRCompiler::compile::<String>(&ok).conflict_count(), 0);
//!
//! // The `(a)+a` shape: after the repetition, a further `a` could extend it
//! // or be the trailing literal — a genuine shift/reduce conflict, which is
//! // counted rather than hidden.
//! let mut bad = GrammarRegistry::new();
//! expr(r#"N1 "a""#).named("S", &mut bad).unwrap();
//! expr(r#"G1 N1 | G1"#).named("N1", &mut bad).unwrap();
//! expr(r#""a""#).named("G1", &mut bad).unwrap();
//! assert_eq!(LRCompiler::compile::<String>(&bad).conflict_count(), 1);
//! ```

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::Hash;

use crate::engine::{Action, Production, Table};
use crate::grammar::{GrammarRegistry, Symbol};

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

/// A compiled LALR(1) table over symbol type `S`; implements
/// [`crate::engine::Table`].
pub struct LalrTable<S> {
    /// `(state, lookahead)` actions; `None` lookahead is end-of-input.
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
    /// Number of `(state, symbol)` slots whose actions collided and were
    /// resolved in production order. Zero for conflict-free LALR(1) grammars;
    /// non-zero means the grammar is not LALR(1).
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

/// The LALR(1) compiler (unit struct; see the module docs).
pub struct LRCompiler;

impl LRCompiler {
    /// Compile a registry's productions into a table. The first registered
    /// nonterminal is the start symbol (an augmented production is prepended).
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
