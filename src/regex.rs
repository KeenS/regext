//! Regex engine — patterns are encoded as CFGs and matched with the VM.

use crate::cfg_parser::CfgParser;

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
    /// The original pattern string this regex was built from.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

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
            Some(Captures {
                matches: vec![Match::new(0, text.len())],
            })
        } else {
            None
        }
    }
}
