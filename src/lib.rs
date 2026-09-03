//! parser_vm: a small configuration-grammar VM.
//!
//! Grammars are compiled to LALR(1) action/goto tables and evaluated by a
//! two-stack shift/reduce engine. Regex patterns are supported by encoding
//! them as CFGs.
//!
//! Modules:
//! - `engine` — the shift/reduce table trait and engine
//! - `grammar` — symbols, expressions, and the production registry
//! - `lalr` — the LALR(1) compiler (de Remer's construction)
//! - `lexer` — maximal-munch tokenizer for grammar terminals
//! - `regex` — regex front end (patterns encoded as CFGs)
//! - `cfg_parser` — the `CfgParser` string-grammar front end

pub mod cfg_parser;
pub mod engine;
pub mod grammar;
pub mod lalr;
pub mod lexer;
pub mod regex;

pub use cfg_parser::{CfgError, CfgParser};
pub use engine::{Action, Engine, Production, Table};
pub use grammar::{Expr, GrammarRegistry, NamedExpr, Symbol, expr};
pub use lalr::{LRCompiler, LalrTable};
pub use lexer::{Lexer, Token};
pub use regex::{Captures, Match, Regex, RegexError};
