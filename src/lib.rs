#![allow(non_snake_case)] // the project name is intentionally `RegExt`

//! # RegExt — extended regular expressions
//!
//! **RegExt** treats regular expressions as a restricted subset of
//! context-free grammars: a pattern is parsed into an AST, lowered to a CFG
//! grammar string, compiled with a genuine de Remer LALR(1) construction, and
//! matched by a two-stack shift/reduce engine. There is no backtracking and no
//! dependency on any external crate.
//!
//! The guiding principle is **honesty about what a CFG can express**: where
//! the pattern language and the CFG language diverge, compilation is refused
//! with a precise error instead of silently picking a parse.
//!
//! ## Quick start
//!
//! ```
//! use RegExt::{Match, Regex};
//!
//! let re = Regex::new("a(b)+c").unwrap();
//!
//! // `is_match` is ANCHORED: the entire input must match.
//! assert!(re.is_match("abbbc"));
//! assert!(!re.is_match("xabbbc"));
//!
//! // `find` locates the leftmost match anywhere in the text — leftmost
//! // start wins, then the match extends as far as the grammar allows.
//! // Spans are byte offsets, half-open [start, end).
//! assert_eq!(re.find("xyabbcz"), Some(Match::new(2, 6)));
//!
//! // `captures` adds the span of each group in pattern order; a group that
//! // did not participate is `None`.
//! let caps = re.captures("abbc").unwrap();
//! assert_eq!(caps.matches, vec![Some(Match::new(0, 4)), Some(Match::new(2, 3))]);
//! ```
//!
//! ## Pattern language
//!
//! Literal characters, `\x` escapes, `a|b` alternation, the quantifiers
//! `*` `+` `?` (a quantifier binds to the **last character** of a literal
//! run, so `ab*` is `a` then `b*`), and `(...)` capture groups numbered by
//! opening parenthesis from 1.
//!
//! Rejected at compile time:
//!
//! - `.` (any character) — a CFG's terminals are a finite set of literal
//!   strings, so "any Unicode character" is not expressible;
//! - empty alternatives (`a|`), unbalanced parentheses, dangling escapes,
//!   quantifiers with no operand;
//! - patterns whose lowering is not conflict-free LALR(1), such as `(a)+a`
//!   (a genuine shift/reduce ambiguity) — see [`RegexError::CompileError`].
//!
//! ## Matching semantics
//!
//! - [`Regex::is_match`] / [`CfgParser::is_match`] — anchored, whole input.
//! - [`Regex::find`] / [`CfgParser::find`] — leftmost start, longest at that
//!   start; byte offsets into the input.
//! - [`Regex::captures`] / [`CfgParser::captures`] — the leftmost match plus
//!   each group's span. A capture is the input range covered by the group's
//!   *last reduction* in the winning derivation: `(a)+` captures the last
//!   iteration, `(a+)` the whole run, `a(b)?c` captures `None` for the group
//!   when the option is skipped, and `x(y*)` captures a zero-width span when
//!   the star matches empty.
//!
//! ## The CFG layer
//!
//! [`CfgParser`] accepts grammars in a small string format — productions are
//! `;`-separated, the **first LHS is the start symbol**, quoted symbols are
//! terminals and unquoted symbols nonterminals, `|` is alternation and
//! `epsilon` is empty:
//!
//! ```
//! use RegExt::CfgParser;
//!
//! let parser = CfgParser::new(r#"S -> "x" G "y"; G -> "a" | "b""#).unwrap();
//! assert!(parser.is_match("xay"));
//! assert_eq!(parser.find("zzxbyq"), Some(RegExt::Match::new(2, 5)));
//! ```
//!
//! Terminals may also be character classes (`"[a-z]"`), maximal-munch class
//! runs (`"[a-z]+"`), or `"any"` (a whole double-quoted string). See
//! [`Lexer`] for the tokenization rules.
//!
//! ## Modules
//!
//! - [`regex`] — the `Regex` front end: pattern parsing, AST, CFG lowering.
//! - [`cfg_parser`] — the `CfgParser` grammar-string front end and
//!   `Match` / `Captures`.
//! - [`lexer`] — the maximal-munch tokenizer for grammar terminals.
//! - [`lalr`] — the de Remer LALR(1) compiler.
//! - [`engine`] — the shift/reduce `Table` trait and two-stack engine.
//! - [`grammar`] — symbols, expressions, and the production registry.
//!
//! ## Example grammars
//!
//! ```
//! use RegExt::CfgParser;
//!
//! // Recursive arithmetic with precedence and parentheses.
//! let arith = CfgParser::new(
//!     r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | NUM; NUM -> "1" | "2""#,
//! ).unwrap();
//! assert!(arith.is_match("1 + 2 * 1"));
//! assert!(!arith.is_match("1 +"));
//!
//! // A word followed by a number, as a single "token stream".
//! let word_num = CfgParser::new(
//!     r#"S -> Word Num; Word -> "[a-z]+" ; Num -> "[0-9]+""#,
//! ).unwrap();
//! assert!(word_num.is_match("abc42"));
//! assert!(!word_num.is_match("42abc"));
//! ```

pub mod cfg_parser;
pub mod engine;
pub mod grammar;
pub mod lalr;
pub mod lexer;
pub mod regex;

pub use cfg_parser::{Captures, CfgError, CfgParser, Match};
pub use engine::{Action, Engine, Production, Reduction, Table};
pub use grammar::{Expr, GrammarRegistry, NamedExpr, Symbol, expr};
pub use lalr::{LRCompiler, LalrTable};
pub use lexer::{Lexer, Token};
pub use regex::{Regex, RegexError};
