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
//! run, so `ab*` is `a` then `b*`), bounded repetitions `X{n}`, `X{n,}`,
//! `X{n,m}` (max bound 100), character classes `[a-z]` / `[^a-z]`
//! (single members or `a-z` ranges), the aliases `\d` = `[0-9]` and
//! `\w` = `[a-zA-Z0-9_]`, `(...)` capture groups numbered by opening
//! parenthesis from 1, non-capturing `(?:P)` and atomic `(?>P)` groups
//! (neither numbers; in this non-backtracking engine `(?>P)` ≡ `(?:P)`),
//! possessive quantifiers `a*+` `a++` `a?+` (accepted as sugar, identical
//! to the plain ones because the engine never backtracks), and the
//! zero-width assertions `(?=P)`, `(?!P)`, `(?<=P)`, `(?<!P)` — see the
//! [`regex`] module for their semantics.
//!
//! Rejected at compile time:
//!
//! - `.` (any character) — a CFG's terminals are a finite set of literal
//!   strings, so "any Unicode character" is not expressible;
//! - `\s` — whitespace is the token separator (the lexer skips it), so no
//!   terminal can match it;
//! - empty alternatives (`a|`), unbalanced parentheses, dangling escapes,
//!   a `]` outside a class, an inverted class range (`[z-a]`),
//!   quantifiers with no operand, malformed repetition bounds (`{}`, `{,3}`,
//!   `a{3,2}`, bound above 100, a quantifier after a bound such as `a{2}+`);
//! - patterns whose lowering is not conflict-free LALR(1), such as `(a)+a`
//!   (a genuine shift/reduce ambiguity) — see [`RegexError::CompileError`];
//! - assertions inside assertions (`a(?=b(?=c))`), empty assertion bodies
//!   (`(?=)`), and assertion bodies that are not conflict-free LALR(1)
//!   (`(?=(a)+a)`).
//!
//! ## Matching semantics
//!
//! - [`Regex::is_match`] / [`CfgParser::is_match`] — anchored on the token
//!   stream. The lexer skips whitespace, so `" test "` matches `test`.
//! - [`Regex::find`] / [`CfgParser::find`] — leftmost start, longest at that
//!   start; byte offsets into the input. Skipped whitespace is not part of
//!   the span (`find(" test ")` is `(1, 5)`).
//! - [`Regex::captures`] / [`CfgParser::captures`] — the leftmost match plus
//!   each group's span. A capture is the input range covered by the group's
//!   *last reduction* in the winning derivation: `(a)+` captures the last
//!   iteration, `(a+)` the whole run, `a(b)?c` captures `None` for the group
//!   when the option is skipped, and `x(y*)` captures a zero-width span when
//!   the star matches empty.
//!
//! ## Complement
//!
//! The complement of a regular language is regular, so RegExt exposes it as
//! [`Regex::not`]: `re.not().is_match(t) == !re.is_match(t)`. It is a
//! language-level (engine) operation, not a pattern operator — the complement
//! of most patterns needs a "any character" terminal that the finite-terminal
//! dialect cannot write, the same reason `.` is rejected. `re.not().find(t)`
//! returns the leftmost slice the wrapped pattern **rejects**, and its
//! `captures` report every group as `None` (a rejected slice has no winning
//! derivation). Complement pairs cancel: `re.not().not()` behaves exactly
//! like `re`.
//!
//! ## Intersection
//!
//! The intersection of two regular languages is regular, so RegExt exposes
//! it as [`Regex::and`]: `e1.and(e2).is_match(t) == e1.is_match(t) &&
//! e2.is_match(t)`. There is no pattern that spells "pattern `A` and pattern
//! `B`" together, so `and` — like `not` — composes compiled regexes at the
//! engine level. The conjunction is on the *language*: `e1.and(e2).find(t)`
//! returns the leftmost slice (longest at that start) accepted by **both**
//! sides of the same text, even when the two sides tokenize it differently
//! (e.g. `ab` is one terminal of `"ab"` but two of `"a"`). `captures`
//! reports the left side's group spans first, then the right side's — each
//! side's winning derivation supplies its own groups, and a side with no
//! winning derivation (e.g. a complement) reports its groups as `None`.
//! `and` composes with `not`: `e1.and(e2).not()` is everything except the
//! intersection, and `e1.and(e2).not().not()` cancels back to `e1.and(e2)`.
//!
//! Both are `Regex` features only: context-free languages are *not* closed
//! under complement or intersection (the complement of `{aⁿbⁿ}` is not
//! context-free), so `Expr` has no `not` / `and` counterpart — and the
//! zero-width assertions are `Regex`-engine machinery, not part of the
//! grammar layer.
//!
//! ## Composable
//!
//! Both front ends also accept **composed expressions** — build the
//! grammar or pattern from named pieces with `or` / `then` instead of one
//! long string:
//!
//! ```
//! use RegExt::{CfgParser, Expr, GrammarRegistry, Pattern, Regex};
//!
//! // Regex: `[+-]?\d{1,3}`, built piece by piece.
//! let re = Regex::from_pattern(
//!     Pattern::lit("+")
//!         .or(Pattern::lit("-"))
//!         .opt()
//!         .then(Pattern::digit().between(1, 3)),
//! )
//! .unwrap();
//! assert!(re.is_match("-123"));
//!
//! // CFG: `S -> "x" G "y"; G -> "a" | "b"`, from composable `Expr`s.
//! let mut registry = GrammarRegistry::new();
//! Expr::str("x")
//!     .then(Expr::call("G"))
//!     .then(Expr::str("y"))
//!     .named("S", &mut registry).unwrap();
//! Expr::str("a").or(Expr::str("b")).named("G", &mut registry).unwrap();
//! let parser = CfgParser::from_registry(registry).unwrap();
//! assert!(parser.is_match("xay"));
//! assert!(!parser.is_match("xy"));
//!
//! // `Expr` carries the same quantifier surface as `Pattern` — `a{1,3}`
//! // over digits, registered with its fresh nonterminals alongside.
//! let mut digits = GrammarRegistry::new();
//! Expr::digit().between(1, 3).named("D", &mut digits).unwrap();
//! let dparser = CfgParser::from_registry(digits).unwrap();
//! assert!(dparser.is_match("42"));
//! assert!(!dparser.is_match("1234"));
//! ```
//!
//! A composable pattern compiles through the same validation and lowering
//! as its string form, so the two are behaviorally identical.
//!
//! ## The CFG layer
//!
//! [`CfgParser`] accepts grammars in a small string format — productions are
//! `;`-separated, the **first LHS is the start symbol**, quoted symbols are
//! terminals and unquoted symbols nonterminals, `|` is alternation and
//! `epsilon` is empty. `;`, `|`, whitespace, and `->` split only outside
//! quotes; `\"` and `\\` escape inside a terminal, and `""` is rejected:
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
//! runs (`"[a-z]+"`), or `"any"` (a whole double-quoted string). Positive and
//! negated classes expand to character terminals before the table is built;
//! the lexer then takes the longest match across every terminal kind. See
//! [`Lexer`] for the tokenization rules.
//!
//! ## Modules
//!
//! - [`regex`] — the `Regex` front end: pattern parsing, the composable [`Pattern`] expressions, AST, CFG lowering.
//! - [`cfg_parser`] — the `CfgParser` grammar-string front end and
//!   `Match` / `Captures`.
//! - [`lexer`] — the maximal-munch tokenizer for grammar terminals.
//! - [`lalr`] — the de Remer LALR(1) compiler.
//! - [`engine`] — the shift/reduce `Table` trait and two-stack engine.
//! - [`grammar`] — symbols, composable [`Expr`] expressions (`or` / `then` / quantifiers / bounds), and the production registry.
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
mod exec;
pub mod grammar;
pub mod lalr;
pub mod lexer;
pub mod regex;

pub use cfg_parser::{Captures, CfgError, CfgParser, Match};
pub use engine::{Action, Engine, Production, Reduction, Table};
pub use grammar::{Expr, GrammarRegistry, NamedExpr, Symbol, expr};
pub use lalr::{LRCompiler, LalrTable};
pub use lexer::{Lexer, Token};
pub use regex::{AssertKind, Pattern, Regex, RegexError};
