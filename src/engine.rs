//! The shift/reduce core: the [`Table`] interface that a parsed grammar
//! implements, and the two-stack [`Engine`] that evaluates it.
//!
//! A table maps `(state, lookahead)` to an [`Action`] and `(state,
//! nonterminal)` to a next state; the engine keeps a symbol stack and a state
//! stack, shifting input symbols and reducing productions until it accepts or
//! errors. [`run_traced`][Engine::run_traced] additionally records, for every
//! reduction, the input range it covered — which is how capture-group spans
//! are computed.
//!
//! ```
//! use RegExt::{Engine, GrammarRegistry, LRCompiler, expr};
//!
//! let mut registry = GrammarRegistry::new();
//! expr(r#""a" "b""#).named("S", &mut registry).unwrap();
//! let table = LRCompiler::compile::<String>(&registry);
//! let mut engine = Engine::new(0);
//! let result = engine.run(&table, vec!["a".to_string(), "b".to_string()]);
//! assert_eq!(result.as_deref(), Ok("S"));
//! ```

use std::fmt::Debug;

/// The action a table prescribes for a `(state, lookahead)` pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Push the input symbol and move to the given state.
    Shift(usize),
    /// Pop `rhs_len` symbols, push the production's LHS, then follow its
    /// goto; `usize` is the production id.
    Reduce(usize),
    /// The input is fully consumed and the start symbol derived.
    Accept,
    /// No valid action: the input is not in the language.
    Error,
}

/// One production of a table: its left-hand side, the length of its right-hand
/// side, and its id (the index into [`Table::production`]).
pub struct Production<S> {
    /// The nonterminal the production derives.
    pub lhs: S,
    /// Number of RHS symbols to pop on a reduce.
    pub rhs_len: usize,
    /// Opaque production id.
    pub id: usize,
}

/// A compiled parse table over symbol type `S`.
///
/// `LalrTable<S>` (see [`crate::lalr`]) is the built-in implementation.
/// `action` takes the lookahead as `Option<&S>` where `None` is end-of-input.
pub trait Table<S> {
    /// The action for `state` with lookahead `symbol` (`None` = EOF).
    fn action(&self, state: usize, symbol: Option<&S>) -> Action;
    /// The state reached after reducing a nonterminal `symbol` in `state`.
    fn goto(&self, state: usize, symbol: &S) -> usize;
    /// The production with the given id.
    fn production(&self, id: usize) -> &Production<S>;
}

/// One production reduction and the input range it covered, as a
/// half-open range of token indices.
#[derive(Debug, Clone)]
pub struct Reduction<S> {
    pub lhs: S,
    /// Index of the first token covered (inclusive).
    pub start: usize,
    /// Index just past the last token covered.
    pub end: usize,
}

/// A two-stack shift/reduce evaluator for any [`Table`].
///
/// `stack_a` holds symbols, `stack_b` the corresponding states.
pub struct Engine<S> {
    stack_a: Vec<S>,
    stack_b: Vec<usize>,
}

impl<S> Engine<S>
where
    S: Clone + PartialEq + Debug,
{
    /// Start in `initial_state` with empty stacks. For a `LalrTable` this is
    /// state `0`.
    pub fn new(initial_state: usize) -> Self {
        Self {
            stack_a: Vec::new(),
            stack_b: vec![initial_state],
        }
    }

    /// Run the engine over `input` (an iterator of symbols, EOF excluded).
    /// Returns the derived start symbol on success.
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

    /// Run the engine while tracking the input range each reduction covers.
    /// Returns the derived start symbol and the reductions in the order
    /// they occur. Epsilon reductions get a zero-width span at the
    /// current frontier of the parse.
    pub fn run_traced<T, I>(
        &mut self,
        table: &T,
        input: I,
    ) -> Result<(S, Vec<Reduction<S>>), String>
    where
        T: Table<S>,
        I: IntoIterator<Item = S>,
    {
        let mut iter = input.into_iter().peekable();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut reductions: Vec<Reduction<S>> = Vec::new();
        let mut input_idx = 0;

        loop {
            let current_state = *self.stack_b.last().ok_or("Stack B empty")?;
            let lookahead = iter.peek();

            match table.action(current_state, lookahead) {
                Action::Shift(next_state) => {
                    let symbol = iter.next().ok_or("Shift failed")?;
                    self.stack_a.push(symbol);
                    self.stack_b.push(next_state);
                    spans.push((input_idx, input_idx + 1));
                    input_idx += 1;
                }
                Action::Reduce(prod_id) => {
                    let prod = table.production(prod_id);
                    let (start, end) = if prod.rhs_len == 0 {
                        match spans.last() {
                            Some((_, frontier)) => (*frontier, *frontier),
                            None => (0, 0),
                        }
                    } else {
                        let right = spans.pop().ok_or("Span stack underflow")?;
                        let mut left = right.0;
                        for _ in 1..prod.rhs_len {
                            let s = spans.pop().ok_or("Span stack underflow")?;
                            left = s.0;
                        }
                        (left, right.1)
                    };
                    for _ in 0..prod.rhs_len {
                        self.stack_a.pop().ok_or("Stack A underflow")?;
                        self.stack_b.pop().ok_or("Stack B underflow")?;
                    }
                    let lhs = prod.lhs.clone();
                    reductions.push(Reduction {
                        lhs: lhs.clone(),
                        start,
                        end,
                    });
                    self.stack_a.push(lhs.clone());
                    spans.push((start, end));
                    let state_before_goto = *self.stack_b.last().ok_or("Stack B empty")?;
                    let next_state = table.goto(state_before_goto, &lhs);
                    self.stack_b.push(next_state);
                }
                Action::Accept => {
                    return Ok((self.stack_a.pop().ok_or("Empty stack")?, reductions));
                }
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
