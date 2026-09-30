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
- **133 tests** across the public API, the LALR(1) compiler (checked against
  an independent LR(1) reference), the lexer, and the regex layer (including
  zero-width assertions, character classes, bounded repetition, and
  non-capturing/possessive forms); **17 runnable examples** that assert their
  own behavior.

---

## Contents

- [What makes RegExt different](#what-makes-regext-different)
- [Requirements](#requirements)
- [Quick start](#quick-start)
- [Pattern language](#pattern-language)
- [Character classes and aliases](#character-classes-and-aliases)
- [Bounded repetition](#bounded-repetition)
- [Non-capturing, atomic, and possessive forms](#non-capturing-atomic-and-possessive-forms)
- [Matching semantics](#matching-semantics)
- [Capture groups](#capture-groups)
- [Zero-width assertions](#zero-width-assertions-lookaround)
- [Complement (not())](#complement-not)
- [Intersection (and())](#intersection-and)
- [Composable API](#composable-api)
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
| `[a-z]` | character class — one member or an `a-z` range   |
| `[^a-z]`| negated class — one character *not* in the class |
| `\d`    | alias for `[0-9]`                                |
| `\w`    | alias for `[a-zA-Z0-9_]`                         |
| `X{n}`  | exactly `n` copies of `X` (bound ≤ 100)          |
| `X{n,}` | at least `n` copies of `X`                       |
| `X{n,m}`| between `n` and `m` copies of `X` (`m ≥ n`)      |
| `(…)`    | capture group, numbered by opening paren, from 1   |
| `(?:…)`  | non-capturing group — no group number consumed     |
| `(?>…)`  | atomic group — identical to `(?:…)` in this engine |
| `a*+` `a++` `a?+` | possessive quantifiers — identical to the plain ones (no backtracking) |
| `(?=P)`  | positive lookahead — asserts `P` follows, consumes nothing |
| `(?!P)`  | negative lookahead — asserts `P` does not follow           |
| `(?<=P)` | positive lookbehind — asserts `P` precedes, consumes nothing |
| `(?<!P)` | negative lookbehind — asserts `P` does not precede          |

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

## Character classes and aliases

A character class is a **single-character atom** — the finite-set case that is
textbook regular. `[` starts a class, `]` ends it; a leading `^` negates it;
members are single characters or `a-z` ranges:

| Pattern    | Matches                                             |
|------------|-----------------------------------------------------|
| `[abc]`    | exactly one of `a`, `b`, `c`                        |
| `[a-z]+`   | a run of lowercase letters                          |
| `[^a-z]`   | one character *not* a lowercase letter              |
| `[^0-9]*`  | a run of non-digits (may be empty)                  |
| `[a\|b]`   | one of `a`, `\|`, `b` — special chars are members inside a class |

Quantifiers bind to the whole class naturally, because a class is one atom:
`[a-z]*`, `[^abc]+`, `[0-9]{3}` all work without special-casing. Lowering is
straight to the lexer's terminal forms — `[a-z]` lowers to the single quoted
terminal `"[a-z]"` (and `[^a-z]` to `"[^a-z]"`), so matching is a property of
the maximal-munch lexer, not of the pattern layer.

A class of exactly one multibyte character is char-exact: `[é]` matches
exactly `é` (and `[^é]` everything else).

### Predefined aliases

| Alias | Expands to      | Meaning                          |
|-------|-----------------|----------------------------------|
| `\d`  | `[0-9]`         | a digit                          |
| `\w`  | `[a-zA-Z0-9_]`  | a word character (letter, digit, or `_`) |

Both lower exactly like the equivalent class atom, so `\d+`, `\w{2,}`,
`a\d*b`, etc. all behave as their class forms would.

`\s` is **rejected** with an explicit error: whitespace is the token separator
in RegExt's stream — the lexer skips it — so no terminal can ever match it.

---

## Bounded repetition

`X{n}`, `X{n,}`, `X{n,m}` repeat an atom a bounded number of times:

| Pattern   | Meaning                                    |
|-----------|--------------------------------------------|
| `a{2}`    | exactly two `a`s                           |
| `a{2,}`   | at least two `a`s                          |
| `a{2,3}`  | two or three `a`s                          |
| `ab{2}`   | `a` then `bb` — the bound binds to the **last character** of a run, like `*` `+` `?` |

Semantics:

- **Greedy within the bound** — at a given start, the match takes the longest
  alternative inside the bound: `(a){2,3}` on `aaa` captures group 1 as the
  last `a` of a three-copy derivation.
- **Groups inside a bound** reuse the same group id for every copy, so
  `(a){2}` captures the **last iteration** — the same "last reduction wins"
  rule as `(a)+`, with no special handling.
- **The cap is 100.** `n` and `m` must be ≤ 100; anything above is an
  `InvalidPattern`. Bounds unroll to a finite set of alternatives, so the cap
  keeps the generated grammar finite and small.

Rejected bounds: `{}`, `{,3}`, `{a}`, `a{3,2}` (upper below lower), `a{101}`
(above the cap), `a{2}+` (a quantifier after a bound), and `a{2}{3}`.

A bound whose unrolling is genuinely ambiguous is refused as a `CompileError`
— e.g. `a{2,3}a`: after two `a`s the parser cannot know from one token whether
the 2-copy alternative is finished or a third `a` belongs to the 3-copy one.

---

## Non-capturing, atomic, and possessive forms

### Non-capturing groups `(?:…)`

Group without consuming a number: `(?:a)(b)` has **one** group, and `(b)` is
group 1. The body is inlined into the grammar (syntactic sugar over
sequence/alternation), so matching is identical to the ungrouped pattern —
and it cannot introduce LALR(1) conflicts a plain sequence wouldn't. Empty
bodies are rejected: `(?:)` is an `InvalidPattern`.

### Atomic groups `(?>…)`

`(?>P)` is a **non-capturing** group whose body is structurally a plain group
body. In a backtracking engine, atomicity controls *how* the group is
retried; RegExt never backtracks, so

```
(?>P)  ≡  (?:P)
```

— exactly what the engine can express.

### Possessive quantifiers `a*+`, `a++`, `a?+`

Possessive quantifiers exist only to forbid backtracking into the repetition.
Because this engine never backtracks, they are **language-identical** to the
plain quantifiers:

```
a*+  ≡  a*        a++  ≡  a+        a?+  ≡  a?
```

Accepting them keeps the dialect honest about what the construct would do in a
backtracking engine, at zero risk. A second possessive marker is rejected:
`a*++` is an `InvalidPattern`.

---

## Zero-width assertions (lookaround)

`(?=P)`, `(?!P)`, `(?<=P)`, and `(?<!P)` are supported. An assertion is
evaluated at the position where it sits in the winning derivation, against the
**full input text** (never a slice of it):

- **Lookahead** `(?=P)` / `(?!P)`: the body `P` must (must not) match a
  **prefix** of the text remaining at that position.
- **Lookbehind** `(?<=P)` / `(?<!P)`: the body `P` must (must not) match a
  **suffix** of the text consumed before that position.
- The body always matches **as far as possible** (a greedy `a+` body claims
  the whole run) and **consumes nothing**: the match span contains only the
  tokens the main pattern consumes.

| Pattern      | Input  | Result                                   | Why                                                        |
|--------------|--------|------------------------------------------|------------------------------------------------------------|
| `a(?=b)`     | `ab`   | `find` → `(0,1)`                          | the match is the `a`; the `b` is looked at, not consumed   |
| `a(?=b)`     | `ab`   | `is_match` → `false`                      | anchored: the whole input is `ab`, but only `a` is consumed |
| `a*(?=b)`    | `xaab` | `find` → `(1,3)`                          | greedy `a*`, asserting the trailing `b`                    |
| `a(?!b)c`    | `ac`   | match                                    | `b` must not follow the `a`; `c` may                        |
| `a(?!b)c`    | `abc`  | no match                                 | `b` does follow the `a`                                     |
| `(?<=a)b`    | `xab`  | `find` → `(2,3)`                          | the `b` after an `a`; the `a` is not part of the match      |
| `(?<!a)b`    | `xbb`  | `find` → `(1,2)`                          | the first `b` is not preceded by an `a`                     |
| `(?<=(a+))b` | `xaab` | whole `(3,4)`, group 1 `(1,3)`            | groups inside assertion bodies are numbered globally and capture |

An assertion on an alternative branch that is **not taken** is vacuously
satisfied, and its groups are reported as `None`: `a|(?=b)(c)` on `a` gives
whole `(0,1)` and group 1 `None`.

Honest rejections: an **assertion inside an assertion** is `InvalidPattern`
(`a(?=b(?=c))`), an **empty assertion body** is `InvalidPattern` (`(?=)`), and
an assertion body that is not conflict-free LALR(1) is a `CompileError`
(`(?=(a)+a` — 1 conflict).

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

## Complement (not())

The complement of a regular language is regular — invert a DFA's accepting
states and you are done. RegExt exposes this as `Regex::not()`:

```rust
use RegExt::Regex;

let not_a = Regex::new("a").unwrap().not();
assert!(not_a.is_match("b"));     // `b` is not `a`
assert!(!not_a.is_match("a"));    // `a` is `a`

// Double negation cancels exactly.
let re = Regex::new("a(b)+c").unwrap().not().not();
assert!(re.is_match("abbc"));
```

`re.not().is_match(t) == !re.is_match(t)` — the complement is a **boolean
operation on the anchored language**.

### Why it is an engine operation, not a pattern

Almost no complement can be written as a RegExt pattern. The stream's
alphabet is the whole Unicode space — the lexer emits any unknown character
as a single-character token — so "everything except `L`" needs a universal
character set that the finite-terminal dialect cannot write (the same reason
`.` is rejected). `not()` is therefore realized at the engine level instead
of by re-lowering:

| Method          | Behavior on `re.not()`                                            |
|-----------------|-------------------------------------------------------------------|
| `is_match(t)`   | `!re.is_match(t)`                                                 |
| `find(t)`       | the **leftmost slice** (longest at that start) that `re` **rejects**; `None` when every slice of `t` is in `re`'s language |
| `captures(t)`   | that slice, with **every group `None`** — a rejected slice has no winning derivation, so no group participates |
| `group_count()` | inherited from the wrapped pattern, so `matches.len() == 1 + group_count()` still holds |

```rust
use RegExt::{Match, Regex};

let not_a = Regex::new("a").unwrap().not();
assert_eq!(not_a.find("xa"), Some(Match::new(0, 2))); // `xa` is not `a`
assert_eq!(not_a.find("a"), Some(Match::new(0, 0)));  // the empty slice is not `a`

let re = Regex::new("a(b)+c").unwrap().not();
let caps = re.captures("ab").unwrap();
assert_eq!(caps.matches, vec![Some(Match::new(0, 2)), None]); // group never participates
```

Complement pairs cancel: `re.not().not()` behaves exactly like `re`,
captures included.

---

## Intersection (and())

The intersection of two regular languages is regular — build the product of
their DFAs and you are done. RegExt exposes this as `Regex::and()`:

```rust
use RegExt::Regex;

let re = Regex::new("a|b").unwrap().and(Regex::new("a|c").unwrap());
assert!(re.is_match("a"));      // the only string in both languages
assert!(!re.is_match("b"));
assert!(!re.is_match("c"));

// `and` composes with `not`.
let re = Regex::new("a").unwrap().and(Regex::new("b").unwrap()).not();
assert!(re.is_match("x"));      // everything except the (empty) intersection
```

`e1.and(e2).is_match(t) == e1.is_match(t) && e2.is_match(t)` — the
intersection is a **boolean operation on the anchored languages**.

### Why it is an engine operation, not a pattern

No RegExt pattern can spell "pattern `A` and pattern `B`" together, so `and`
— like `not` — composes *compiled* regexes at the engine level. The
conjunction is on the **language**, not on aligned token slices: the two
sides may tokenize the same text differently (e.g. `ab` is one terminal of
`"ab"` but two tokens of `"a"`), and a slice is accepted exactly when both
sides accept that same slice of the same text:

| Method          | Behavior on `e1.and(e2)`                                             |
|-----------------|----------------------------------------------------------------------|
| `is_match(t)`   | `e1.is_match(t) && e2.is_match(t)`                                   |
| `find(t)`       | the **leftmost slice** (longest at that start) accepted by **both** sides of the same text |
| `captures(t)`   | that slice, then the left side's group spans (in its pattern order), then the right side's — each side's winning derivation supplies its own groups |
| `group_count()` | the sum of both sides, so `matches.len() == 1 + group_count()` still holds |

A side with no winning derivation for the slice (for example a complement
— its wrapped regex rejected the slice) reports its groups as `None`;
`e1.and(e2).not().not()` cancels back to `e1.and(e2)`, captures included.

```rust
use RegExt::{Match, Regex};

// The sides' groups are concatenated: left first, then right.
let re = Regex::new("(a)(b)").unwrap().and(Regex::new("a(b)").unwrap());
assert_eq!(re.group_count(), 3);
let caps = re.captures("xab").unwrap();
assert_eq!(
    caps.matches,
    vec![
        Some(Match::new(1, 3)), // the whole slice
        Some(Match::new(1, 2)), // left group 1: `a`
        Some(Match::new(2, 3)), // left group 2: `b`
        Some(Match::new(2, 3))  // right group 1: `b`
    ]
);

// `ab` ∩ `a` = ∅, even though `ab` is one terminal and `a` another.
let empty = Regex::new("ab").unwrap().and(Regex::new("a").unwrap());
assert!(empty.captures("aab").is_none());
```

---

## Composable API

Both front ends also accept **composed expressions**: instead of one long
pattern string or grammar string, build the language from named pieces with
`or` (choice) and `then` (juxtaposition), and compile the result.

### Regex: `Pattern` + `Regex::from_pattern`

`Pattern` is the pattern language as a value. Every string-dialect feature
has a composable spelling:

| pattern string | composable |
|----------------|------------|
| `ab` | `Pattern::lit("a").then(Pattern::lit("b"))` |
| `a\|b` | `Pattern::lit("a").or(Pattern::lit("b"))` |
| `a*` `a+` `a?` | `a.star()` `a.plus()` `a.opt()` |
| `a{2}` `a{2,}` `a{2,5}` | `a.times(2)` `a.at_least(2)` `a.between(2, 5)` |
| `[a-z]` / `[^a-z]` | `Pattern::class("a-z")` / `Pattern::neg_class("a-z")` |
| `\d` / `\w` | `Pattern::digit()` / `Pattern::word()` |
| `(P)` | `P.group()` — capture group, numbered in pattern order |
| `(?:P)` | `P.non_capturing()` |
| `(?=P)` `(?!P)` `(?<=P)` `(?<!P)` | `P.lookahead()` `P.neg_lookahead()` `P.lookbehind()` `P.neg_lookbehind()` |
| *(no spelling)* | `Pattern::epsilon()` — the empty pattern |

```rust
use RegExt::{Pattern, Regex};

// `[+-]?\d{1,3}` — the optional sign is `or` + `opt`, the bound `between`.
let sign = Pattern::lit("+").or(Pattern::lit("-"));
let re = Regex::from_pattern(sign.opt().then(Pattern::digit().between(1, 3)))
    .unwrap();
assert!(re.is_match("+17"));
assert!(re.is_match("-123"));
assert!(!re.is_match("1234"));

// Capture groups are the `.group()` pieces, in pattern order:
// `\d{1,3}(\.\d{1,3}){3}` with each segment captured.
let seg = Pattern::digit().between(1, 3).group();
let dot = Pattern::lit(".");
let ip = Regex::from_pattern(
    seg.clone().then(dot.clone()).then(seg.clone())
        .then(dot.clone()).then(seg.clone()).then(dot.clone()).then(seg),
).unwrap();
assert_eq!(ip.group_count(), 4);
```

`Regex::from_pattern` runs the **exact same validation and lowering** as
`Regex::new`, so a composable pattern and its string form behave
identically (`examples/regex_composable.rs` asserts this on a battery of
inputs, and `tests/composable_api_tests.rs` formalizes it). `not()` composes
over the result as well. The only extra checks: repetition bounds must
respect the same 100-copy cap and be ordered (a memberless class such as
`Pattern::class("")` is refused), since the string parser used to catch
those on the way in.

### CFG: `Expr` + `CfgParser::from_registry`

`Expr` (the grammar RHS language) gains the same combinators — plus
constructors and a `named` registration:

| grammar string | composable |
|----------------|------------|
| `"a"` `B` | `Expr::str("a")` `Expr::call("B")` |
| `A B` | `Expr::call("A").then(Expr::call("B"))` |
| `A \| B` | `Expr::call("A").or(Expr::call("B"))` |
| `epsilon` | `Expr::epsilon()` |
| registration | `expr.named("NAME", &mut registry)` |

```rust
use RegExt::{CfgParser, Expr, GrammarRegistry};

// E -> E "+" T | T;  T -> T "*" F | F;  F -> "(" E ")" | "1" | "2"
let e = Expr::call("E")
    .then(Expr::str("+"))
    .then(Expr::call("T"))
    .or(Expr::call("T"));
let t = Expr::call("T")
    .then(Expr::str("*"))
    .then(Expr::call("F"))
    .or(Expr::call("F"));
let f = Expr::str("(")
    .then(Expr::call("E"))
    .then(Expr::str(")"))
    .or(Expr::str("1"))
    .or(Expr::str("2"));

// First registration is the start symbol.
let mut registry = GrammarRegistry::new();
e.named("E", &mut registry);
t.named("T", &mut registry);
f.named("F", &mut registry);

let parser = CfgParser::from_registry(registry).unwrap();
assert!(parser.is_match("1 + 2 * 1"));
assert!(!parser.is_match("1 +"));
```

`CfgParser::from_registry` produces the same start symbol, terminal set, and
LALR(1) table as `CfgParser::new` for the equivalent grammar string — same
`is_match` / `find` / `captures` (named pieces are the capture groups).
`or` / `then` flatten, so `a.or(b).or(c)` is one choice; and the string
`expr(...)` pieces and the `Expr` combinators mix freely
(`expr(r#""a" | "b""#).or(Expr::str("c"))`).

---

## What RegExt will not do

RegExt deliberately stays a grammar engine. The following are **rejected at
compile time**, not approximated:

| Input                     | Error                  | Reason                                              |
|---------------------------|------------------------|-----------------------------------------------------|
| `h.llo`, `.`, `a.*b`      | `InvalidPattern`       | `.` (any-character) is not a finite, enumerable set of CFG terminals |
| `\s`, `a\sb`              | `InvalidPattern`       | whitespace is the token separator (the lexer skips it), so no terminal can match it |
| `[]`, `[a[b]`, `[a`       | `InvalidPattern`       | empty class, nested `[`, or unclosed class |
| `a\|`, `\|b`, `\|`        | `InvalidPattern`       | empty alternative                                   |
| `(a`, `(a)(b`, `a(b`      | `InvalidPattern`       | unbalanced parentheses                              |
| `a)`                      | `InvalidPattern`       | trailing junk                                       |
| `a\`                      | `InvalidPattern`       | dangling escape                                     |
| `*`, `**`, `+`, `?`       | `InvalidPattern`       | quantifier with no operand                          |
| `a{}`, `a{,3}`, `a{x}`    | `InvalidPattern`       | malformed repetition bound                          |
| `a{3,2}`                  | `InvalidPattern`       | upper bound below lower                             |
| `a{101}`                  | `InvalidPattern`       | bound above the cap of 100                          |
| `a{2}+`, `a{2}{3}`        | `InvalidPattern`       | a quantifier/bound after a bound                    |
| `a*++`                    | `InvalidPattern`       | a second possessive marker                          |
| `(?:)`                    | `InvalidPattern`       | empty non-capturing body                            |
| `(a)+a`                   | `CompileError`         | lowers to a grammar with LALR(1) conflicts          |
| `a**`                     | `CompileError`         | lowers to a grammar with LALR(1) conflicts          |
| `a{2,3}a`                 | `CompileError`         | the unrolled alternatives leave a genuine LALR(1) conflict |
| `(?=)`, `(?<=)b`          | `InvalidPattern`       | empty assertion body                                |
| `a(?=b(?=c))`             | `InvalidPattern`       | an assertion inside an assertion                    |
| `(?=(a)+a)`               | `CompileError`         | the assertion body itself is not LALR(1) (1 conflict) |

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
| `"[^a-z]"`   | a single character **not** in the class (negated)           |
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
- `token_spans(text) -> Vec<TokenSpan>` — tokenization with byte spans; the substrate every match is built on
- `find_tokens_with(text, ok)` — leftmost search accepting a candidate `(start, end)` only when `ok` says so (`Regex` uses it for assertion checks)
- `slice_reductions(text, start, end) -> Option<Vec<Reduction>>` — the winning derivation of a token slice
- `prefix_match(text) -> Option<Match>` — the longest prefix (left-anchored); the primitive lookahead assertions are built on
- `suffix_match(text) -> Option<Match>` — the longest suffix (right-anchored); the primitive lookbehind assertions are built on
- `match_bytes(text, start, end) -> Match` — the byte span of a token slice
- `reduction_bytes(text, slice_start, r) -> Match` — map a derivation reduction onto byte offsets

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
              │  produces a PNode AST (Lit/Seq/Alt/Star/Plus/Opt/Group/Assert/
              │                        Class/Bound/NoCap)
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
| `[a-z]`    | `S -> "[a-z]"` — a class is a single quoted terminal        |
| `[^a-z]`   | `S -> "[^a-z]"` — a negated class is a single quoted terminal |
| `a{2}`     | `S -> "a" "a"` — an exact bound unrolls to copies           |
| `a{2,}`    | `S -> "a" "a" N1; N1 -> "a" N1; N1 -> epsilon` — copies plus `a*` |
| `a{2,3}`   | `S -> N1; N1 -> "a" "a"; N1 -> "a" "a" "a"` — one nonterminal, one production per copy count |
| `(a){2}`   | `S -> G1 G1; G1 -> "a"` — the group id is reused, so the capture is the last copy |
| `(?:ab)`   | `S -> "a" "b"` — the body is inlined, no nonterminal        |
| `a*+`      | identical lowering to `a*` — possession is sugar without backtracking |

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
pub use regex::{AssertKind, Pattern, Regex, RegexError};
```

### `regex`

- `Regex::new(pattern: &str) -> Result<Regex, RegexError>` — parse, lower, compile; refuses non-LALR(1) encodings.
- `Regex::from_pattern(pattern: Pattern) -> Result<Regex, RegexError>` — the composable front end: same validation and lowering as `new` (see [Composable API](#composable-api)).
- `Pattern` — the composable pattern AST: `lit`, `class`, `neg_class`, `digit`, `word`, `epsilon`, `or`, `then`, `star`, `plus`, `opt`, `times`, `at_least`, `between`, `group`, `non_capturing`, `lookahead`, `neg_lookahead`, `lookbehind`, `neg_lookbehind`.
- `pattern() -> &str` — the original pattern.
- `not() -> Regex` — the complement language (`is_match` negated; see [Complement](#complement-not)).
- `group_count() -> usize` — number of capture groups (including groups inside assertion bodies).
- `is_match(text) -> bool` — anchored, whole-input match.
- `find(text) -> Option<Match>` — leftmost, longest match.
- `captures(text) -> Option<Captures>` — whole match + each group's span.

`RegexError { InvalidPattern(String), CompileError(String) }`.

### `cfg_parser`

- `CfgParser::new(grammar: &str) -> Result<CfgParser, CfgError>`
- `CfgParser::from_registry(registry: GrammarRegistry) -> Result<CfgParser, CfgError>` — compile a composable registry (see [Composable API](#composable-api))
- `is_match(text) -> bool`
- `find(text) -> Option<Match>`
- `captures(text, groups: &[&str]) -> Option<Captures>`
- `conflict_count() -> usize`
- `prefix_match(text) -> Option<Match>` / `suffix_match(text) -> Option<Match>` — left/right-anchored longest match
- `find_tokens_with(text, ok)`, `slice_reductions(text, start, end)`, `token_spans(text)`, `match_bytes(text, start, end)`, `reduction_bytes(text, slice_start, r)` — the derivation primitives `Regex` builds assertions on
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
- `Expr { Terminal, NonTerminal, Sequence, Choice, Empty }` — with constructors `str`, `call`, `epsilon` and the combinators `or` / `then` / `named`.
- `NamedExpr`, and a free `expr(input)` helper; `NamedExpr::or` / `NamedExpr::then` mix string pieces with `Expr` pieces.

---

## Examples

Runnable, self-asserting demonstrations (each prints its assertion count):

| Example                 | Shows                                                        |
|-------------------------|--------------------------------------------------------------|
| `regex_demo`            | the `Regex` surface: anchored `is_match`, `find`, captures, group spans, honest rejection |
| `regex_api_demo`        | `Regex` + `CfgParser` side by side, groups, last-iteration capture |
| `assertions_demo`       | zero-width assertions: lookahead/lookbehind, greedy bodies, groups inside assertions, honest rejections |
| `classes_demo`          | character classes, `\d`/`\w` aliases, bounded repetition, non-capturing/atomic groups, possessive sugar, honest rejections |
| `cfg_parser_demo`       | `CfgParser` grammar strings, terminals, recursion            |
| `json_parser`           | a **genuine JSON parser** (RFC 8259 values) as a CFG         |
| `s_expression_parser`   | a **genuine S-expression (Lisp) parser** on the LALR(1) engine |
| `arithmetic_parser`     | arithmetic expressions with precedence and parentheses        |
| `innermost_div`         | innermost `<div>` text extraction: regular find + loop, well-nested parsing in the CFG layer |
| `no_spaces_arithmetic`  | arithmetic where operator/operand tokens must not be separated by spaces |
| `no_spaces_demo`        | the maximal-munch `[a-z]+` / `[0-9]+` terminal behavior       |
| `composable_api`        | the composable CFG API: `Expr::or` / `Expr::then`, `CfgParser::from_registry`, parity with grammar strings |
| `regex_composable`      | the composable regex API: `Pattern` combinators + `Regex::from_pattern`, parity with `Regex::new`, captures, `not()` |
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

`cargo test` runs **141 tests** across five integration suites (plus 10 doctests):

| Suite                         | Count | Covers                                   |
|-------------------------------|:-----:|------------------------------------------|
| `tests/regex_tests.rs`        |   86  | the pattern dialect, anchored `is_match`, `find`, `captures` with group semantics, zero-width assertions, character classes and aliases, bounded repetition, non-capturing/atomic/possessive forms, the `not()` complement, and honest rejections |
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
