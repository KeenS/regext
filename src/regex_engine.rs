use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub enum RegexNode {
    Literal(char),
    CharClass(Vec<char>),
    Concat(Box<RegexNode>, Box<RegexNode>),
    Alternation(Box<RegexNode>, Box<RegexNode>),
    Star(Box<RegexNode>),
    Plus(Box<RegexNode>),
    Optional(Box<RegexNode>),
}

#[derive(Debug, Clone)]
pub struct NFAState {
    id: usize,
    transitions: Vec<(Option<char>, usize)>,
    epsilon_transitions: Vec<usize>,
    is_accept: bool,
}

pub struct RegexEngine {
    pattern: String,
    nfa: Vec<NFAState>,
    start_state: usize,
    accept_state: usize,
}

impl RegexEngine {
    pub fn new(pattern: &str) -> Self {
        let mut engine = Self {
            pattern: pattern.to_string(),
            nfa: Vec::new(),
            start_state: 0,
            accept_state: 0,
        };
        engine.build_nfa(pattern);
        engine
    }
    
    fn build_nfa(&mut self, pattern: &str) {
        // Simplified NFA construction
        self.nfa.push(NFAState {
            id: 0,
            transitions: vec![],
            epsilon_transitions: vec![],
            is_accept: false,
        });
        
        let mut current_state = 0;
        let mut chars = pattern.chars().peekable();
        
        while let Some(c) = chars.next() {
            match c {
                '*' => {
                    // Kleene star - add epsilon transitions
                    self.nfa[current_state].epsilon_transitions.push(current_state);
                }
                '+' => {
                    // One or more - similar to star but requires at least one
                }
                '|' => {
                    // Alternation
                }
                '[' => {
                    // Character class
                    let mut class_chars = Vec::new();
                    while let Some(&next_c) = chars.peek() {
                        if next_c == ']' {
                            chars.next();
                            break;
                        }
                        class_chars.push(next_c);
                        chars.next();
                    }
                    // Add transition for character class
                }
                _ => {
                    // Literal character
                    let next_state = self.nfa.len();
                    self.nfa.push(NFAState {
                        id: next_state,
                        transitions: vec![],
                        epsilon_transitions: vec![],
                        is_accept: false,
                    });
                    self.nfa[current_state].transitions.push((Some(c), next_state));
                    current_state = next_state;
                }
            }
        }
        
        self.nfa[current_state].is_accept = true;
        self.accept_state = current_state;
    }
    
    pub fn is_match(&self, text: &str) -> bool {
        // NFA simulation with epsilon closure
        let mut current_states = self.epsilon_closure(vec![self.start_state]);
        
        for c in text.chars() {
            let mut next_states = Vec::new();
            
            for state_id in &current_states {
                if let Some(state) = self.nfa.get(state_id) {
                    for (char_opt, next_id) in &state.transitions {
                        if let Some(expected_char) = char_opt {
                            if *expected_char == c {
                                next_states.push(*next_id);
                            }
                        }
                    }
                }
            }
            
            current_states = self.epsilon_closure(next_states);
            if current_states.is_empty() {
                return false;
            }
        }
        
        current_states.contains(&self.accept_state)
    }
    
    fn epsilon_closure(&self, states: Vec<usize>) -> Vec<usize> {
        let mut closure = states.clone();
        let mut queue = VecDeque::from(closure.clone());
        
        while let Some(state_id) = queue.pop_front() {
            if let Some(state) = self.nfa.get(state_id) {
                for &next_id in &state.epsilon_transitions {
                    if !closure.contains(&next_id) {
                        closure.push(next_id);
                        queue.push_back(next_id);
                    }
                }
            }
        }
        
        closure
    }
}
