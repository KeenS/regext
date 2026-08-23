use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A token carrying the grammar symbol it matched.
    Terminal(String),
    /// End of input.
    EOF,
}

pub struct Lexer {
    literal_terminals: Vec<String>,
    char_classes: Vec<(String, HashSet<char>)>,
}

impl Lexer {
    pub fn new(terminals: HashSet<String>) -> Self {
        let mut literal_terminals: Vec<String> = terminals.into_iter().collect();
        // Longest match first so overlapping terminals tokenize deterministically.
        literal_terminals.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));

        let mut char_classes = Vec::new();
        for terminal in &literal_terminals {
            let chars: Vec<char> = terminal.chars().collect();
            if chars.len() >= 3 && chars.first() == Some(&'[') && chars.last() == Some(&']') {
                let inner = &chars[1..chars.len() - 1];
                let mut members = HashSet::new();
                let mut i = 0;
                while i < inner.len() {
                    if i + 2 < inner.len() && inner[i + 1] == '-' {
                        for c in inner[i]..=inner[i + 2] {
                            members.insert(c);
                        }
                        i += 3;
                    } else {
                        members.insert(inner[i]);
                        i += 1;
                    }
                }
                char_classes.push((terminal.clone(), members));
            }
        }
        char_classes.sort_by(|a, b| a.0.cmp(&b.0));

        Self {
            literal_terminals,
            char_classes,
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

            // Try to match literal terminals (longest first).
            let mut matched = false;
            for terminal in &self.literal_terminals {
                let term_chars: Vec<char> = terminal.chars().collect();
                if i + term_chars.len() <= chars.len() {
                    let slice: String = chars[i..i + term_chars.len()].iter().collect();
                    if slice == *terminal {
                        tokens.push(Token::Terminal(terminal.clone()));
                        i += term_chars.len();
                        matched = true;
                        break;
                    }
                }
            }

            if !matched {
                // Try char classes; the token carries the class symbol so the
                // parse table can match it against the grammar production.
                let mut class_matched = false;
                for (class, members) in &self.char_classes {
                    if members.contains(&c) {
                        tokens.push(Token::Terminal(class.clone()));
                        i += 1;
                        class_matched = true;
                        break;
                    }
                }

                if !class_matched {
                    // Unknown characters are still emitted so the parser can report them.
                    tokens.push(Token::Terminal(c.to_string()));
                    i += 1;
                }
            }
        }

        tokens.push(Token::EOF);
        tokens
    }
}
