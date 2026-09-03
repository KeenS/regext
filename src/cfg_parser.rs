//! `CfgParser`: grammar-string front end (lexer + LALR(1) table + engine).

use std::collections::HashSet;

use crate::engine::Engine;
use crate::grammar::{GrammarRegistry, Symbol};
use crate::lalr::{LRCompiler, LalrTable};
use crate::lexer::{Lexer, Token};

#[derive(Debug, Clone)]
pub struct CfgError {
    message: String,
}

impl std::fmt::Display for CfgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CFG Error: {}", self.message)
    }
}

impl std::error::Error for CfgError {}

pub struct CfgParser {
    table: LalrTable<String>,
    start_symbol: String,
    terminals: HashSet<String>,
}

impl CfgParser {
    pub fn new(grammar: &str) -> Result<Self, CfgError> {
        let mut registry = GrammarRegistry::new();
        let mut start_symbol = None;

        for prod_str in grammar.split(';') {
            let prod_str = prod_str.trim();
            if prod_str.is_empty() {
                continue;
            }

            if let Some(idx) = prod_str.find("->") {
                let lhs = prod_str[..idx].trim();
                let rhs = prod_str[idx + 2..].trim();

                if start_symbol.is_none() {
                    start_symbol = Some(lhs.to_string());
                }

                let expr = GrammarRegistry::expr(rhs);
                registry.register(lhs, expr);
            } else {
                return Err(CfgError {
                    message: format!("Invalid production: {}", prod_str),
                });
            }
        }

        let start_sym = start_symbol.ok_or_else(|| CfgError {
            message: "No start symbol".to_string(),
        })?;
        // Terminals are the terminal symbols that actually occur in productions,
        // so choice/sequence syntax like `abc|def` yields `abc` and `def`.
        let terminals: HashSet<String> = registry
            .productions
            .iter()
            .flat_map(|(_, rhs)| rhs.iter())
            .filter_map(|sym| match sym {
                Symbol::Terminal(t) => Some(t.clone()),
                Symbol::NonTerminal(_) => None,
            })
            .collect();
        let table = LRCompiler::compile::<String>(&registry);

        Ok(Self {
            table,
            start_symbol: start_sym,
            terminals,
        })
    }

    pub fn is_match(&self, text: &str) -> bool {
        let lexer = Lexer::new(self.terminals.clone());
        let symbols: Vec<String> = lexer
            .tokenize(text)
            .into_iter()
            .filter_map(|token| match token {
                Token::Terminal(symbol) => Some(symbol),
                Token::EOF => None,
            })
            .collect();
        let mut engine = Engine::new(0);
        match engine.run(&self.table, symbols) {
            Ok(result) => result == self.start_symbol,
            Err(_) => false,
        }
    }

    /// Number of (state, symbol) action slots where two different LALR(1)
    /// actions collided and were resolved in production order. Zero means
    /// the grammar is conflict-free LALR(1); nonzero means the grammar is
    /// not LALR(1) and behavior on the colliding inputs depends on the
    /// order of the productions.
    pub fn conflict_count(&self) -> usize {
        self.table.conflict_count()
    }
}
