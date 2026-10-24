//! The maximal-munch tokenizer for grammar terminals.
//!
//! Given the terminal set of a grammar, the lexer scans input text and emits
//! one [`Token`] per match, skipping whitespace. The longest match wins. On
//! a tie the higher rank wins: a quoted string, then a literal, then a
//! class-run, then a positive class, then a negated class. A zero-length
//! literal is never a match. Unknown characters are still emitted (as
//! single-character tokens) so the parser can reject them precisely, unless
//! the terminal set contains the negated-class fallback, which stands for
//! every character no other terminal took.
//!
//! Recognized terminal shapes:
//!
//! | Terminal     | Emits                                                       |
//! |--------------|-------------------------------------------------------------|
//! | `"literal"`  | the exact literal                                           |
//! | `"[a-z]"`    | one character from the class (ranges like `a-z` supported)  |
//! | `"[^a-z]"`    | one character NOT in the class (negated class)              |
//! | `"[a-z]+"`   | a maximal-munch run: the longest stretch of class characters |
//! | `"any"`      | a whole double-quoted string, matched as one unit           |
//!
//! ```
//! use std::collections::HashSet;
//! use RegExt::lexer::{Lexer, Token};
//!
//! let lexer = Lexer::new(HashSet::from(["ab".to_string()]));
//! assert_eq!(
//!     lexer.tokenize("ab ab"),
//!     vec![
//!         Token::Terminal("ab".to_string()),
//!         Token::Terminal("ab".to_string()),
//!         Token::EOF,
//!     ]
//! );
//! ```

use std::collections::HashSet;

/// A lexer token.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A token carrying the grammar symbol it matched.
    Terminal(String),
    /// End of input.
    EOF,
}

/// A lexer token with its byte span in the input text.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenSpan {
    pub token: Token,
    /// Byte offset of the first byte of the token.
    pub start: usize,
    /// Byte offset just past the last byte of the token.
    pub end: usize,
}

/// A maximal-munch tokenizer built from a grammar's terminal set.
pub struct Lexer {
    string_rules: Vec<String>,
    class_runs: Vec<(String, HashSet<char>)>,
    literals: Vec<String>,
    char_classes: Vec<(String, HashSet<char>)>,
    negated_classes: Vec<(String, HashSet<char>)>,
    /// Terminal emitted for a character that no literal, class, or class-run
    /// mentions. Present only when the grammar has a negated class.
    other: Option<String>,
}

impl Lexer {
    /// Build a lexer from the grammar's terminals (see the module docs for
    /// the recognized terminal shapes).
    pub fn new(terminals: HashSet<String>) -> Self {
        let mut string_rules = Vec::new();
        let mut class_runs = Vec::new();
        let mut literals = Vec::new();
        let mut char_classes = Vec::new();
        let mut negated_classes = Vec::new();
        let mut other = None;

        for terminal in terminals {
            // A leading NUL marks the synthetic "any other character" terminal
            // introduced for negated classes. It is not a user literal.
            if terminal.starts_with('\u{0}') {
                other = Some(terminal);
                continue;
            }
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
            } else if chars.len() >= 4
                && chars[0] == '['
                && chars[1] == '^'
                && chars[chars.len() - 1] == ']'
            {
                // A [^..] terminal is a negated class: one character that is
                // NOT a member. Membership excludes the set, so a literal
                // member is never tokenized as the negated class.
                negated_classes.push((terminal, Self::parse_class(&chars[2..chars.len() - 1])));
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
        negated_classes.sort_by(|a, b| a.0.cmp(&b.0));

        Self {
            string_rules,
            class_runs,
            literals,
            char_classes,
            negated_classes,
            other,
        }
    }

    fn parse_class(inner: &[char]) -> HashSet<char> {
        let text: String = inner.iter().collect();
        parse_class_members(&text).unwrap_or_default()
    }

    /// Tokenize and record the byte span of every token.
    pub fn tokenize_with_spans(&self, input: &str) -> Vec<TokenSpan> {
        let chars: Vec<char> = input.chars().collect();
        let mut byte_off = Vec::with_capacity(chars.len() + 1);
        let mut acc = 0;
        byte_off.push(0);
        for ch in &chars {
            acc += ch.len_utf8();
            byte_off.push(acc);
        }

        let mut spans = Vec::new();
        let mut i = 0;

        while i < chars.len() {
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }

            let (symbol, next) = match self.match_symbol(&chars, i) {
                Some(t) => t,
                // A negated class's OTHER terminal stands for every character
                // no literal or class mentioned. Otherwise an unknown character
                // is still emitted so the parser can reject it precisely.
                None => {
                    let symbol = match &self.other {
                        Some(other) => other.clone(),
                        None => chars[i].to_string(),
                    };
                    (symbol, i + 1)
                }
            };
            spans.push(TokenSpan {
                token: Token::Terminal(symbol),
                start: byte_off[i],
                end: byte_off[next],
            });
            i = next;
        }

        spans.push(TokenSpan {
            token: Token::EOF,
            start: input.len(),
            end: input.len(),
        });
        spans
    }

    /// Tokenize `input`; the result always ends with [`Token::EOF`].
    pub fn tokenize(&self, input: &str) -> Vec<Token> {
        self.tokenize_with_spans(input)
            .into_iter()
            .map(|span| span.token)
            .collect()
    }

    /// Longest match across every terminal kind. On a tie the higher rank
    /// wins: quoted string, then literal, then class-run, then class, then
    /// negated class. Equal rank keeps the earlier candidate, and those
    /// lists are sorted so the result is deterministic.
    fn match_symbol(&self, chars: &[char], i: usize) -> Option<(String, usize)> {
        let mut best_end = i;
        let mut best_rank: u8 = 0;
        let mut best_sym: Option<String> = None;
        let mut propose = |end: usize, rank: u8, sym: &str| {
            if end <= i {
                return;
            }
            let better =
                best_sym.is_none() || end > best_end || (end == best_end && rank > best_rank);
            if better {
                best_end = end;
                best_rank = rank;
                best_sym = Some(sym.to_string());
            }
        };

        if chars[i] == '"' {
            for rule in &self.string_rules {
                if let Some(end) = Self::quoted_end(chars, i) {
                    propose(end, 5, rule);
                }
            }
        }

        for (run, members) in &self.class_runs {
            if members.contains(&chars[i]) {
                let mut end = i + 1;
                while end < chars.len() && members.contains(&chars[end]) {
                    end += 1;
                }
                propose(end, 3, run);
            }
        }

        for literal in &self.literals {
            let len = literal.chars().count();
            if len == 0 || i + len > chars.len() {
                continue;
            }
            let slice: String = chars[i..i + len].iter().collect();
            if slice == *literal {
                propose(i + len, 4, literal);
            }
        }

        for (class, members) in &self.char_classes {
            if members.contains(&chars[i]) {
                propose(i + 1, 2, class);
            }
        }

        for (class, members) in &self.negated_classes {
            if !members.contains(&chars[i]) {
                propose(i + 1, 1, class);
            }
        }

        best_sym.map(|sym| (sym, best_end))
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

/// Members of a class body (`a`, `a-z`, a hyphen at either edge). A range
/// whose start sorts after its end is an error; equal endpoints are one
/// character. `inner` is the text between the brackets, without a leading
/// `^`.
pub(crate) fn parse_class_members(inner: &str) -> Result<HashSet<char>, String> {
    let chars: Vec<char> = inner.chars().collect();
    let mut members = HashSet::new();
    let mut i = 0;
    while i < chars.len() {
        if i + 2 < chars.len() && chars[i + 1] == '-' {
            let start = chars[i];
            let end = chars[i + 2];
            if start > end {
                return Err(format!("inverted character range '{start}-{end}' in class"));
            }
            for c in start..=end {
                members.insert(c);
            }
            i += 3;
        } else {
            members.insert(chars[i]);
            i += 1;
        }
    }
    Ok(members)
}
