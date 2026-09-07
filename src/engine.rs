//! Core VM: the shift/reduce table interface and two-stack engine.

use std::fmt::Debug;

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
