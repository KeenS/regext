use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A token carrying the grammar symbol it matched.
    Terminal(String),
    /// End of input.
    EOF,
}

pub struct Lexer {
    string_rules: Vec<String>,
    class_runs: Vec<(String, HashSet<char>)>,
    literals: Vec<String>,
    char_classes: Vec<(String, HashSet<char>)>,
}

impl Lexer {
    pub fn new(terminals: HashSet<String>) -> Self {
        let mut string_rules = Vec::new();
        let mut class_runs = Vec::new();
        let mut literals = Vec::new();
        let mut char_classes = Vec::new();

        for terminal in terminals {
            let chars: Vec<char> = terminal.chars().collect();
            if terminal == "any" {
                // The "any" terminal is a quoted-string token: the lexer matches
                // a whole double-quoted string as one unit (a backslash escapes
                // the following character), so structural characters inside the
                // string are never tokenized on their own.
                string_rules.push(terminal);
            } else if chars.len() >= 4
                && chars[0] == '['
                && chars[chars.len() - 1] == '+'
                && chars[chars.len() - 2] == ']'
            {
                // A [..]+ terminal is a maximal-munch run: one token of the
                // longest consecutive stretch of member characters.
                class_runs.push((terminal, Self::parse_class(&chars[1..chars.len() - 2])));
            } else if chars.len() >= 3 && chars[0] == '[' && chars[chars.len() - 1] == ']' {
                char_classes.push((terminal, Self::parse_class(&chars[1..chars.len() - 1])));
            } else {
                literals.push(terminal);
            }
        }

        // Longest match first so overlapping literals tokenize deterministically.
        literals.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        string_rules.sort();
        class_runs.sort_by(|a, b| a.0.cmp(&b.0));
        char_classes.sort_by(|a, b| a.0.cmp(&b.0));

        Self {
            string_rules,
            class_runs,
            literals,
            char_classes,
        }
    }

    fn parse_class(inner: &[char]) -> HashSet<char> {
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
        members
    }

    pub fn tokenize(&self, input: &str) -> Vec<Token> {
        let chars: Vec<char> = input.chars().collect();
        let mut tokens = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }

            match self.match_symbol(&chars, i) {
                Some((symbol, next)) => {
                    tokens.push(Token::Terminal(symbol));
                    i = next;
                }
                None => {
                    // Unknown characters are still emitted so the parser can report them.
                    tokens.push(Token::Terminal(chars[i].to_string()));
                    i += 1;
                }
            }
        }

        tokens.push(Token::EOF);
        tokens
    }

    fn match_symbol(&self, chars: &[char], i: usize) -> Option<(String, usize)> {
        if chars[i] == '"' {
            for rule in &self.string_rules {
                if let Some(end) = Self::quoted_end(chars, i) {
                    return Some((rule.clone(), end));
                }
            }
        }

        for (run, members) in &self.class_runs {
            if members.contains(&chars[i]) {
                let mut end = i + 1;
                while end < chars.len() && members.contains(&chars[end]) {
                    end += 1;
                }
                return Some((run.clone(), end));
            }
        }

        for literal in &self.literals {
            let len = literal.chars().count();
            if i + len <= chars.len() {
                let slice: String = chars[i..i + len].iter().collect();
                if slice == *literal {
                    return Some((literal.clone(), i + len));
                }
            }
        }

        for (class, members) in &self.char_classes {
            if members.contains(&chars[i]) {
                return Some((class.clone(), i + 1));
            }
        }

        None
    }

    fn quoted_end(chars: &[char], i: usize) -> Option<usize> {
        // chars[i] == '"': find the index just past the closing quote.
        let mut j = i + 1;
        while j < chars.len() {
            match chars[j] {
                '\\' => j += 2,
                '"' => return Some(j + 1),
                _ => j += 1,
            }
        }
        None
    }
}
