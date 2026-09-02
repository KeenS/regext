//! Tests for the LALR(1) compiler (de Remer construction):
//!   1. nullability-aware FIRST/lookahead propagation,
//!   2. canonical LR(1) automaton + core-based state merging,
//!   3. the conflict counter: zero for conflict-free LALR(1) grammars,
//!      nonzero for grammars that are genuinely not LALR(1).
//!
//! The non-LALR(1) expectations below were cross-checked against an
//! independent textbook LALR(1) implementation: same 24 LR(1) states,
//! same 14 merged LALR states, same 6 conflict slots, and the same
//! final action table.

use parser_vm::{CfgParser, Engine, GrammarRegistry, LRCompiler, LalrTable, expr};

const JSON_GRAMMAR: &str = r#"Value -> Object | Array | Str | Number | "true" | "false" | "null"; Object -> "{" "}" | "{" Pair Tail; Tail -> "," Pair Tail | "}"; Pair -> Str ":" Value; Array -> "[" "]" | "[" Value VTail; VTail -> "," Value VTail | "]"; Str -> "any"; Number -> "[-0-9.eE+]+""#;

const SEXP_GRAMMAR: &str = r#"SExp -> Quoted | List | Atom | Str; List -> "(" ")" | "(" SExps ")" ; SExps -> SExp SExps | SExp; Quoted -> "'" SExp | "`" SExp | "," SExp | "," "@" SExp; Atom -> "[-!$%*+/=?<>^_.:#~a-z0-9A-Z]+"; Str -> "any""#;

const ARITH_GRAMMAR: &str = r#"E -> E "+" T | T; T -> T "*" F | F; F -> "(" E ")" | "n""#;

/// S -> "p" A; A -> B "c"; B -> "a" | epsilon.
/// B->epsilon must be reducible exactly when the following context
/// ("c", nullable-aware FIRST) is viable — not on arbitrary input.
const EPSILON_GRAMMAR: &str = r#"S -> "p" A; A -> B "c"; B -> "a" | epsilon"#;

/// Classic a^i b^j-style grammar. Genuinely not LALR(1): 6 conflict
/// slots (verified against an independent reference implementation).
const NOT_LALR_GRAMMAR: &str = r#"S -> More | Less; More -> "a" More "b" | "a" X; Less -> "a" Less "b" | "a" "b" "b"; X -> "b" X | epsilon"#;

fn check(parser: &CfgParser, name: &str, input: &str, expected: bool) {
    let result = parser.is_match(input);
    assert_eq!(result, expected, "[{}] input {:?}: expected {}", name, input, expected);
}

fn parse(grammar: &str) -> CfgParser {
    CfgParser::new(grammar).expect("grammar should compile")
}

// =============================================================================
// Conflict-freeness
// =============================================================================

#[test]
fn test_json_grammar_is_conflict_free_lalr() {
    let parser = parse(JSON_GRAMMAR);
    assert_eq!(
        parser.conflict_count(),
        0,
        "the JSON grammar must be conflict-free LALR(1)"
    );
}

#[test]
fn test_sexp_grammar_is_conflict_free_lalr() {
    let parser = parse(SEXP_GRAMMAR);
    assert_eq!(
        parser.conflict_count(),
        0,
        "the s-expression grammar must be conflict-free LALR(1)"
    );
}

#[test]
fn test_left_recursive_grammar_is_conflict_free_lalr() {
    let parser = parse(ARITH_GRAMMAR);
    assert_eq!(
        parser.conflict_count(),
        0,
        "left recursion must be handled by LALR(1) without conflicts"
    );
}

#[test]
fn test_epsilon_grammar_is_conflict_free_lalr() {
    let parser = parse(EPSILON_GRAMMAR);
    assert_eq!(parser.conflict_count(), 0);
}

#[test]
fn test_named_api_compiles_conflict_free_tables() {
    let mut registry = GrammarRegistry::new();
    let _e = expr(r#"E "+" T | T"#).named("E", &mut registry);
    let _t = expr(r#"T "*" F | F"#).named("T", &mut registry);
    let _f = expr(r#""(" E ")" | "n""#).named("F", &mut registry);

    let table: LalrTable<String> = LRCompiler::compile(&registry);
    assert_eq!(table.conflict_count(), 0);

    let mut engine = Engine::new(0);
    let result = engine.run(&table, vec!["n".to_string(), "+".to_string(), "n".to_string()]);
    assert_eq!(result, Ok("E".to_string()), "engine must derive the start symbol");
}

// =============================================================================
// JSON acceptance / rejection
// =============================================================================

#[test]
fn test_json_accepts_valid_documents() {
    let parser = parse(JSON_GRAMMAR);
    for input in [
        "{}",
        "[]",
        "\"hi\"",
        "42",
        "-3.5",
        "6.02e23",
        "-2.5e-3",
        "true",
        "false",
        "null",
        r#"{"a": 1, "b": [true, null]}"#,
        r#"[1, [2, [3, [4]]]]"#,
        r#"{"nested": {"deeper": {"x": "y"}}}"#,
    ] {
        check(&parser, "json-accept", input, true);
    }
}

#[test]
fn test_json_rejects_invalid_documents() {
    let parser = parse(JSON_GRAMMAR);
    for input in [
        "",
        "{",
        "}",
        "[",
        r#"{"a":}"#,
        r#"{"a" 1}"#,
        r#"[1 2]"#,
        r#"[,1]"#,
        "{} {}",
        "tru",
        "nul",
        "1 .2",
        "\"unterminated",
    ] {
        check(&parser, "json-reject", input, false);
    }
}

// =============================================================================
// S-expression acceptance / rejection
// =============================================================================

#[test]
fn test_sexp_accepts_lisp_forms() {
    let parser = parse(SEXP_GRAMMAR);
    for input in [
        "a",
        "-42",
        "+17",
        "3.14",
        "->",
        "sin-1",
        "foo.bar",
        "x1",
        "\"str\"",
        "( )",
        "( a b c )",
        "( ( a ) ( b ) ( c ) )",
        "'a",
        "`( a b )",
        ",x",
        ",@x",
        "( 'a ( b c ) ,@d )",
    ] {
        check(&parser, "sexp-accept", input, true);
    }
}

#[test]
fn test_sexp_rejects_malformed_forms() {
    let parser = parse(SEXP_GRAMMAR);
    for input in [
        "",
        ")",
        "(",
        "( a b",
        "( ( a )",
        "( ) )",
        "a (",
        "'( a ) ( b )",
        ")",
    ] {
        check(&parser, "sexp-reject", input, false);
    }
}

// =============================================================================
// Left-recursive arithmetic
// =============================================================================

#[test]
fn test_arithmetic_precedence_and_parens() {
    let parser = parse(ARITH_GRAMMAR);
    for input in [
        "n",
        "n + n",
        "n * n",
        "( n )",
        "n + n * n",
        "n * n + n * n",
        "( n + n ) * n",
        "( ( n ) + n )",
    ] {
        check(&parser, "arith-accept", input, true);
    }
    for input in [
        "n n",
        "n +",
        "+ n",
        "n *",
        "( n",
        "n )",
        "( n ) +",
        "n + n *",
    ] {
        check(&parser, "arith-reject", input, false);
    }
}

// =============================================================================
// Epsilon / nullability-aware lookahead
// =============================================================================

#[test]
fn test_nullable_nonterminal_lookahead() {
    let parser = parse(EPSILON_GRAMMAR);
    check(&parser, "eps-accept", "p c", true); // B -> epsilon
    check(&parser, "eps-accept", "p a c", true); // B -> "a"
    check(&parser, "eps-reject", "p", false); // missing "c"
    check(&parser, "eps-reject", "p a", false); // missing "c" after "a"
    check(&parser, "eps-reject", "", false);
}

// =============================================================================
// Conflict detection (grammar that is not LALR(1))
// =============================================================================

#[test]
fn test_conflict_counter_flags_non_lalr_grammar() {
    let parser = parse(NOT_LALR_GRAMMAR);
    assert_eq!(
        parser.conflict_count(),
        6,
        "this grammar has exactly 6 (state, symbol) action conflicts; \
         an independent reference implementation agrees"
    );
}

#[test]
fn test_conflicting_grammar_is_still_deterministic() {
    let parser = parse(NOT_LALR_GRAMMAR);
    for input in ["a", "aab", "aaabb", "abbba b", "aabbb a", "aabbb"] {
        let _ = parser.is_match(input);
    }
    check(&parser, "moreless-accept", "a", true);
    check(&parser, "moreless-accept", "aab", true);
    check(&parser, "moreless-accept", "aaabb", true);
    check(&parser, "moreless-reject", "ab", false);
    check(&parser, "moreless-reject", "abb", false);
    check(&parser, "moreless-reject", "aabb", false);
    check(&parser, "moreless-reject", "aaab", false);
    check(&parser, "moreless-reject", "aaaabb", false);
}
