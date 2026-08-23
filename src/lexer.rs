use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Terminal(String),
    EOF,
}

pub struct Lexer {
    terminals: HashSet<String>,
    char_classes: HashMap<String, HashSet<char>>,
}

use std::collections::HashMap;

impl Lexer {
    pub fn new(terminals: HashSet<String>) -> Self {
        let mut lexer = Self {
            terminals,
            char_classes: HashMap::new(),
        };
        lexer.init_char_classes();
        lexer
    }
    
    fn init_char_classes(&mut self) {
        for terminal in &self.terminals {
            if terminal.starts_with('[') && terminal.ends_with(']') {
                let inner = &terminal[1..terminal.len()-1];
                let mut chars = HashSet::new();
                for c in inner.chars() {
                    chars.insert(c);
                }
                self.char_classes.insert(terminal.clone(), chars);
            }
        }
    }
    
    pub fn tokenize(&self, input: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;
        
        while i < chars.len() {
            let c = chars[i];
            
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            
            // Try to match multi-char terminals
            let mut matched = false;
            for terminal in &self.terminals {
                let term_chars: Vec<char> = terminal.chars().collect();
                if i + term_chars.len() <= chars.len() {
                    let slice: String = chars[i..i+term_chars.len()].iter().collect();
                    if slice == *terminal {
                        tokens.push(Token::Terminal(slice));
                        i += term_chars.len();
                        matched = true;
                        break;
                    }
                }
            }
            
            if !matched {
                // Try char classes
                let mut char_class_matched = false;
                for (class, chars_set) in &self.char_classes {
                    if chars_set.contains(&c) {
                        tokens.push(Token::Terminal(c.to_string()));
                        i += 1;
                        char_class_matched = true;
                        break;
                    }
                }
                
                if !char_class_matched {
                    // Single char token
                    tokens.push(Token::Terminal(c.to_string()));
                    i += 1;
                }
            }
        }
        
        tokens.push(Token::EOF);
        tokens
    }
}
