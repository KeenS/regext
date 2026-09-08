# RegExt

**RegExt** — *extended regular expressions* — is a small, dependency-free Rust
crate in which regular expressions are not a special case but a **restricted,
LALR(1) context-free grammar**. A pattern is parsed into an AST, lowered into a
CFG grammar string, compiled by a genuine de Remer LALR(1) construction, and
matched by a two-stack shift/reduce engine.

The guiding principle is **honesty about what a CFG can and cannot express**:
where the pattern language and the CFG language diverge, RegExt *refuses to
compile* with a precise error instead of silently picking a parse or pretending
a feature exists.

- **Zero dependencies.** Everything — lexer, LALR(1) compiler, shift/reduce
  engine, and the regex front end — lives in `src/` with no external crates.
- **Stable Rust**, edition 2024.
- **90 tests** across the public API, the LALR(1) compiler (checked against an
  independent LR(1) reference), the lexer, and the regex layer; **13 runnable
  examples** that assert their own behavior.

---

## Contents

- [What makes RegExt different](#what-makes-regext-different)
- [Requirements](#requirements)
- [Quick start](#quick-start)
- [Pattern language](#pattern-language)
- [Matching semantics](#matching-semantics)
- [Capture groups](#capture-groups)
- [What RegExt will not do](#what-regext-will-not-do)
- [The CFG layer](#the-cfg-layer)
- [How it works](#how-it-works)
- [API reference](#api-reference)
- [Examples](#examples)
- [Tests](#tests)
- [Development](#development)

---

## What makes RegExt different

Most regex engines are **backtracking** machines: they try an interpretation of
the pattern, fail, and rewind. RegExt has no backtracking. A pattern is first
turned into a **grammar**, then matched by a deterministic **LALR(1)** table
drive. Two consequences follow:

1. **Every accepted pattern is conflict-free LALR(1).** If a pattern lowers to a
   grammar with a shift/reduce conflict, `Regex::new` returns a `CompileError`
   naming the conflict count. There is no "it happens to work" path.
2. **Capture groups have grammar semantics.** A group is a nonterminal, and a
   capture is the input span covered by the group's *last reduction* in the
   winning derivation. This naturally reproduces the intuitive regex behavior
   for the cases people care about (last iteration of `(a)+`, whole run of
   `(a+)`, `None` for an unparticipated group, zero-width for an empty star)
   — but it is *defined* by the derivation, not by ad-hoc backtracking rules.

RegExt is not trying to be a drop-in `regex` crate. It is a demonstration that
a useful, well-defined subset of regular expressions is a *subset of CFGs*, and
that you can build the whole thing — compiler and all — in a few hundred lines
with no dependencies.

---

## Requirements

- A stable Rust toolchain (developed and tested on the current stable, edition
  2024).
- `cargo`.

That is all. There is no `Cargo.lock`-driven dependency graph to worry about —
`[dependencies]` is empty.

---

## Quick start

```rust
use RegExt::{Match, Regex};

let re = Regex::new("a(b)+c").unwrap();

// `is_match` is ANCHORED: the whole input must match.
assert!(re.is_match("abbbc"));
assert!(!re.is_match("xabbbc"));      // substring does not satisfy is_match

// `find` locates the leftmost match anywhere in the text.
assert_eq!(re.find("xyabbcz"), Some(Match::new(2, 6)));

// `captures` gives the whole match plus each group's span, in pattern order.
let caps = re.captures("abbc").unwrap();
//          matches[0]  = whole match  (0, 4)
//          matches[1]  = group (b)    last iteration -> (2, 3)
assert_eq!(caps.matches, vec![Some(Match::new(0, 4)), Some(Match::new(2, 3))]);
```

Run one of the bundled examples to see it in action:

```sh
cargo run --example regex_demo
cargo run --example regex_api_demo
```

---

## Pattern language

The supported pattern syntax:

| Syntax  | Meaning                                          |
|:-------:|--------------------------------------------------|
| `abc`   | literal characters (any run of plain characters) |
| `a\|b`  | alternation — matches `a` or `b`                 |
| `\x`    | escape — `\x` matches the literal character `x`  |
| `a*`    | zero or more (`a`, `aa`, …, or empty)            |
| `a+`    | one or more                                      |
| `a?`    | zero or one                                      |
| `(…)`   | capture group, numbered by opening paren, from 1 |

### Quantifier binding

A quantifier binds to the **last character of a literal run**:

- `ab*` means `a` followed by `b*` — **not** `(ab)*`.
  - matches `a`, `ab`, `abb`; rejects `abab`, `b`.
- To repeat a whole chunk, group it: `(ab)*`.

### Alternation and the lexer

Overlapping literal terminals are tokenized **longest-first** (maximal munch),
so the branch that consumes the most input at a position wins:

- `a|ab` on `ab` → matches `(0, 2)` as the `ab` branch (the input `ab` is a
  single `ab` token), and on `a` → `(0, 1)`.

### Escapes

`\x` escapes any following character so it matches literally. This is how you
match special characters: `a\*b`, `a\|b`, `a\\`. A dangling escape at the end of
the pattern (`a\`) is rejected.

---

## Matching semantics

### `is_match` is anchored

`is_match(text)` requires the **entire** input to match. This differs from the
`regex` crate, where `is_match` performs a substring search. In RegExt:

- `Regex::new("test").is_match("a test b")` → `false`
- `Regex::new("test").is_match("test")`     → `true`

If you want to search anywhere in the text, use `find` or `captures`.

### `find`: leftmost start, longest match

`find(text)` returns the **leftmost** match (earliest start position wins), and
at that start it extends **as far as the grammar allows** (longest at that
position). It returns byte offsets, half-open `[start, end)`.

- `a+` on `baab`  → `(1, 3)` — leftmost of the two candidate matches.
- `a+` on `aaab`  → `(0, 3)` — longest at the leftmost start.
- `a*` on `bbb`   → `(0, 0)` — an empty (zero-width) match at position 0.
- `a*` on `baaab` → `(0, 0)` — leftmost (empty) beats a longer match further right.
- `é` on `aéb`    → `(1, 3)` — spans are **byte** offsets (`é` is two UTF-8 bytes).

### No match

When no position yields a match, `find` and `captures` return `None`.

---

## Capture groups

A group is a nonterminal in the lowered grammar. `captures(text)` returns
`Captures { matches }` where:

- `matches[0]` is the whole match, and
- `matches[i]` (for `i ≥ 1`) is the span of the **i-th group in pattern order**,
  or `None` if that group did not participate in the winning derivation.

The span of a group is the input range covered by the group's **last reduction**
in the derivation. This gives the intuitive regex semantics for the common
cases, and they are *derived*, not special-cased:

| Pattern     | Input   | Whole | Group 1                         | Why                                                        |
|-------------|---------|-------|---------------------------------|------------------------------------------------------------|
| `(a)+b`     | `aab`   | `(0,3)` | `(1,2)`                      | `+` wraps the group → group is the **last iteration**      |
| `(a+)`      | `aabb`* | `(0,2)` | `(0,2)`                       | group wraps `+` → group is the **whole run** (*see below)  |
| `a(b)?c`    | `ac`    | `(0,2)` | `None`                          | group is *inside* the option; empty branch → no capture    |
| `a(b)?c`    | `abc`   | `(0,3)` | `(1,2)`                       | the option took the `b` branch                              |
| `x(y*)(z)`  | `xzz`   | `(0,2)` | `(1,1)` (zero-width), `z` `(1,2)` | group wraps `*`; empty star still participates          |
| `((a)+)b`   | `aab`   | `(0,3)` | outer `(0,2)`, inner `(1,2)`   | nested groups, numbered by opening paren (outer = 1)       |

\* e.g. `(a+)` alone on `aa` captures the whole `aa` as group 1.

**Group numbering** follows opening-parenthesis order, starting at 1 — the same
order as in classic regex engines. `group_count()` returns the total.

**Non-participating groups are `None`** (reported, not omitted), so
`matches.len()` is always `1 + group_count()` on a successful match.

---

## What RegExt will not do

RegExt deliberately stays a grammar engine. The following are **rejected at
compile time**, not approximated:

| Input                     | Error                  | Reason                                              |
|---------------------------|------------------------|-----------------------------------------------------|
| `h.llo`, `.`, `a.*b`      | `InvalidPattern`       | `.` (any-character) is not a finite, enumerable set of CFG terminals |
| `a\|`, `\|b`, `\|`        | `InvalidPattern`       | empty alternative                                   |
| `(a`, `(a)(b`, `a(b`      | `InvalidPattern`       | unbalanced parentheses                              |
| `a)`                      | `InvalidPattern`       | trailing junk                                       |
| `a\`                      | `InvalidPattern`       | dangling escape                                     |
| `*`, `**`, `+`, `?`       | `InvalidPattern`       | quantifier with no operand                          |
| `(a)+a`                   | `CompileError`         | lowers to a grammar with LALR(1) conflicts          |
| `a**`                     | `CompileError`         | lowers to a grammar with LALR(1) conflicts          |

The `CompileError` path is the "honesty" guarantee: `Regex::new` compiles the
lowered grammar and, if `conflict_count() > 0`, returns

```
Compile error: pattern compiles to a grammar with N LALR(1) conflict(s)
```

rather than letting the parser silently resolve a shift/reduce conflict in
production order. Patterns like `(a)+a` genuinely *need* the parser to guess how
far `a+` reaches before the trailing `a` — a real ambiguity — so they are
refused, and a conflict-free rewrite (e.g. `(aa)+`) is the way forward.

Error types:

```rust
pub enum RegexError {
    InvalidPattern(String),  // bad pattern syntax (see table above)
    CompileError(String),    // valid syntax, but not LALR(1)
}
```

---

## The CFG layer

`Regex` is a convenience front end. The real engine is **`CfgParser`**, which
accepts a grammar in a small string format. This is where RegExt shows it is a
*context-free* system, not merely a regex one.

### Grammar syntax

- Productions are separated by `;` (a trailing `;` is fine).
- Each production is `LHS -> RHS`. **The first LHS is the start symbol.**
- RHS symbols are whitespace-separated. A **quoted** symbol is a terminal; an
  **unquoted** symbol is a nonterminal.
- `|` in an RHS is alternation (becomes separate productions).
- `epsilon` is the empty symbol.

```rust
let parser = CfgParser::new(r#"S -> "x" G "y"; G -> "a" | "b""#).unwrap();
assert!(parser.is_match("xay"));
assert!(parser.is_match("xby"));
assert!(!parser.is_match("xab"));
```

### Terminal forms

The lexer recognizes these terminal shapes in the input text:

| Terminal     | Matches                                                     |
|--------------|-------------------------------------------------------------|
| `"literal"`  | that exact literal                                          |
| `"[a-z]"`    | a single character from the class (e.g. `[a-z]`, `[A-Za-z0-9_]`) |
| `"[a-z]+"`   | a **maximal-munch run**: the longest stretch of class chars |
| `"any"`      | any double-quoted string token (a whole `"…"` as one unit)  |

Character classes support ranges (`a-z`) and single members. Whitespace in the
input is skipped between tokens.

```rust
// A "word then number" token — impossible to express as a plain literal regex.
let parser = CfgParser::new(r#"S -> Word Num; Word -> "[a-z]+" ; Num -> "[0-9]+""#).unwrap();
assert!(parser.is_match("abc42"));
assert!(!parser.is_match("42abc"));
```

### `conflict_count`

`CfgParser::conflict_count()` reports how many `(state, symbol)` table slots
had colliding actions. **Zero** means the grammar is conflict-free LALR(1);
non-zero means it is not LALR(1) and behavior on colliding inputs depends on
production order. `Regex::new` uses this to refuse non-LALR(1) patterns.

### `CfgParser` API

- `CfgParser::new(grammar: &str) -> Result<CfgParser, CfgError>`
- `is_match(text) -> bool` — anchored, whole-input
- `find(text) -> Option<Match>` — leftmost, longest
- `captures(text, groups: &[&str]) -> Option<Captures>` — spans of the named nonterminals, in the order given
- `conflict_count() -> usize`

`CfgParser` is also the cleanest way to demonstrate recursion, which flat regex
engines often fudge:

```rust
// Recursive arithmetic expression.
let parser = CfgParser::new(
    r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | NUM; NUM -> "1" | "2""#,
)
.unwrap();
assert!(parser.is_match("1 + 2 * 1"));
```

---

## How it works

```
 pattern  ──▶  PParser (recursive descent: alt → seq → rep → atom)
              │  produces a PNode AST (Lit/Seq/Alt/Star/Plus/Opt/Group)
              ▼
           lower()   ──▶  CFG grammar string  (S -> …; N1 -> …; G1 -> …)
              │
              ▼
        CfgParser::new
              │  GrammarRegistry: productions, terminals, start symbol
              ▼
        LRCompiler (de Remer LALR(1))  ──▶  LalrTable (action/goto)
              │
              ▼
   input text ──▶ Lexer (maximal-munch over the terminal set)
              │  tokens + byte spans
              ▼
        Engine (two-stack shift/reduce)
              │  run / run_traced (tracks each reduction's input span)
              ▼
        Match / Captures  (byte offsets)
```

### Lowering: what a pattern actually becomes

Each syntactic construct that must be *named* (alternation, repetition, group)
gets its own nonterminal. Repetitions use the standard right-recursive
encodings (`N -> b N | b` for `+`, `N -> b N | ε` for `*`, `N -> b | ε` for `?`),
which are all LALR(1)-friendly. Verified lowered forms:

| Pattern    | Lowered grammar                                              |
|------------|--------------------------------------------------------------|
| `a(b)?c`   | `S -> "a" N1 "c"; G1 -> "b"; N1 -> epsilon; N1 -> G1`       |
| `a(b)+c`   | `S -> "a" N1 "c"; G1 -> "b"; N1 -> G1 N1; N1 -> G1`         |
| `(a)+b`    | `S -> N1 "b"; G1 -> "a"; N1 -> G1 N1; N1 -> G1`             |
| `a\|ab`    | `S -> N1; N1 -> "a" \| "ab"`                                |
| `ab?`      | `S -> "a" N1; N1 -> epsilon; N1 -> "b"`                     |
| `x(y*)(z)` | `S -> "x" G1 G2; N1 -> "y" N1; N1 -> epsilon; G1 -> N1; G2 -> "z"` |

Note how `(a)+b` and `a(b)+c` differ: in `(a)+b` the **repetition wraps the
group** (`N1 -> G1 N1`), so the group is the repeated unit and captures the last
iteration; in `a(b)+c` the **group wraps the repetition** (`G1 -> …`), so the
group captures the whole run. Both are correct, and both fall straight out of
the lowering with no special cases.

### Why the encodings are LALR(1)

The star/plus/opt encodings are left/right-recursive in a way that keeps the
post-token states conflict-free: the decision to continue (shift the base) or
terminate (reduce) is resolved by the *next* token, so there is no
shift/reduce ambiguity. That is precisely why `(a)+a` *is* ambiguous (after
reducing the inner `a+`, the parser cannot know whether the following `a`
belongs to the `+` or to the literal) and is therefore rejected.

---

## API reference

Re-exported at the crate root:

```rust
pub use cfg_parser::{Captures, CfgError, CfgParser, Match};
pub use engine::{Action, Engine, Production, Reduction, Table};
pub use grammar::{Expr, GrammarRegistry, NamedExpr, Symbol, expr};
pub use lalr::{LRCompiler, LalrTable};
pub use lexer::{Lexer, Token};
pub use regex::{Regex, RegexError};
```

### `regex`

- `Regex::new(pattern: &str) -> Result<Regex, RegexError>` — parse, lower, compile; refuses non-LALR(1) encodings.
- `pattern() -> &str` — the original pattern.
- `group_count() -> usize` — number of capture groups.
- `is_match(text) -> bool` — anchored, whole-input match.
- `find(text) -> Option<Match>` — leftmost, longest match.
- `captures(text) -> Option<Captures>` — whole match + each group's span.

`RegexError { InvalidPattern(String), CompileError(String) }`.

### `cfg_parser`

- `CfgParser::new(grammar: &str) -> Result<CfgParser, CfgError>`
- `is_match(text) -> bool`
- `find(text) -> Option<Match>`
- `captures(text, groups: &[&str]) -> Option<Captures>`
- `conflict_count() -> usize`
- `Match { start: usize, end: usize }` — byte offsets, half-open; `Match::new`.
- `Captures { matches: Vec<Option<Match>> }` — `matches[0]` is the whole match.
- `CfgError` — carries a human-readable message.

### `lexer`

- `Lexer::new(terminals: HashSet<String>) -> Lexer`
- `tokenize(text) -> Vec<Token>`
- `tokenize_with_spans(text) -> Vec<TokenSpan>`
- `Token { Terminal(String), EOF }`
- `TokenSpan { token, start, end }`

### `engine`

- `Table<S>` trait: `action(state, symbol) -> Action`, `goto(state, symbol) -> usize`, `production(id) -> &Production<S>`.
- `Action { Shift(usize), Reduce(usize), Accept, Error }`
- `Production<S> { lhs, rhs_len, id }`
- `Reduction<S> { lhs, start, end }` — a reduction and the input range it covered.
- `Engine<S>`: `new(initial_state)`, `run(table, input) -> Result<S, String>`,
  `run_traced(table, input) -> Result<(S, Vec<Reduction<S>>), String>`.

### `lalr`

- `LRCompiler::compile::<S>(registry: &GrammarRegistry) -> LalrTable<S>`
- `LalrTable<S>` (implements `Table<S>`): `conflict_count() -> usize`.

### `grammar`

- `GrammarRegistry { productions, start_symbol, terminals }`, `new()`, `register(name, expr) -> Symbol`, `expr(input) -> Expr`.
- `Symbol { Terminal(String), NonTerminal(String) }`
- `Expr { Terminal, NonTerminal, Sequence, Choice, Empty }`
- `NamedExpr`, and a free `expr(input)` helper.

---

## Examples

Runnable, self-asserting demonstrations (each prints its assertion count):

| Example                 | Shows                                                        |
|-------------------------|--------------------------------------------------------------|
| `regex_demo`            | the `Regex` surface: anchored `is_match`, `find`, captures, group spans, honest rejection |
| `regex_api_demo`        | `Regex` + `CfgParser` side by side, groups, last-iteration capture |
| `cfg_parser_demo`       | `CfgParser` grammar strings, terminals, recursion            |
| `json_parser`           | a **genuine JSON parser** (RFC 8259 values) as a CFG         |
| `s_expression_parser`   | a **genuine S-expression (Lisp) parser** on the LALR(1) engine |
| `arithmetic_parser`     | arithmetic expressions with precedence and parentheses        |
| `no_spaces_arithmetic`  | arithmetic where operator/operand tokens must not be separated by spaces |
| `no_spaces_demo`        | the maximal-munch `[a-z]+` / `[0-9]+` terminal behavior       |
| `composable_api`        | composing `Engine`, `GrammarRegistry`, and `LRCompiler` directly |
| `ast_mapping`           | mapping a parse onto an explicit AST                          |
| `named_api_demo`        | the `NamedExpr` / `register` building blocks                  |
| `terminal_expressions`  | terminal (literal / class / class-run) matching               |
| `demo`                  | a minimal end-to-end smoke test                               |

Run any one:

```sh
cargo run --example json_parser
```

Run them all:

```sh
for f in examples/*.rs; do cargo run --quiet --example "$(basename "$f" .rs)"; done
```

---

## Tests

`cargo test` runs **90 tests** across five integration suites:

| Suite                         | Count | Covers                                   |
|-------------------------------|:-----:|------------------------------------------|
| `tests/regex_tests.rs`        |   35  | the pattern dialect, anchored `is_match`, `find`, `captures` with group semantics, and honest rejections |
| `tests/cfg_tests.rs`          |   21  | `CfgParser` grammar parsing, terminals, recursion, `find`/`captures` |
| `tests/lexer_integration_test.rs` | 16 | maximal-munch tokenization, classes, class-runs, `"any"` |
| `tests/lalr_tests.rs`         |   13  | the LALR(1) compiler, validated against an independent LR(1) reference (state counts and conflict behavior) |
| `tests/named_api_tests.rs`    |    5  | the `NamedExpr` / `register` API          |

All examples additionally assert their own behavior when run, giving end-to-end
coverage of the public API.

---

## Development

```sh
cargo build                # build the library + examples
cargo test --no-fail-fast  # run the full test suite
cargo clippy --all-targets # lints (clean)
cargo fmt --check          # formatting (clean)
```

The codebase is organized as:

| Module           | Responsibility                                            |
|------------------|-----------------------------------------------------------|
| `src/regex.rs`   | pattern parsing + AST + CFG lowering + `Regex` facade      |
| `src/cfg_parser.rs` | grammar-string front end, `Match`/`Captures`, `find`/`captures` |
| `src/lalr.rs`    | de Remer LALR(1) compiler                                  |
| `src/grammar.rs` | symbols, expressions, production registry                  |
| `src/engine.rs`  | the shift/reduce table trait and two-stack engine          |
| `src/lexer.rs`   | maximal-munch tokenizer for grammar terminals              |
| `src/lib.rs`     | module wiring and re-exports                               |

---

*RegExt: extended regular expressions, honestly compiled to LALR(1).*
