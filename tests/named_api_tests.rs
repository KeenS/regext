use RegExt::{CfgParser, Engine, Expr, GrammarRegistry, LRCompiler, LalrTable, Symbol, expr};

fn run(table: &LalrTable<String>, input: Vec<String>) -> Result<String, String> {
    let mut engine = Engine::new(0);
    engine.run(table, input)
}

#[test]
fn test_named_api_basic() {
    let mut registry = GrammarRegistry::new();
    let _s = expr(r#""a" | "b""#).named("S", &mut registry).unwrap();
    let table = LRCompiler::compile::<String>(&registry);
    let result = run(&table, vec!["a".to_string()]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "S");
}

#[test]
fn test_named_api_sequence() {
    let mut registry = GrammarRegistry::new();
    let _e = expr(r#""a" "b" | "c""#).named("E", &mut registry).unwrap();
    let table = LRCompiler::compile::<String>(&registry);
    let result = run(&table, vec!["a".to_string(), "b".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn test_named_api_nested() {
    let mut registry = GrammarRegistry::new();
    let _s = expr("A B").named("S", &mut registry).unwrap();
    let _a = expr(r#""a" | "b""#).named("A", &mut registry).unwrap();
    let _b = expr(r#""c" | "d""#).named("B", &mut registry).unwrap();
    let table = LRCompiler::compile::<String>(&registry);
    let result = run(&table, vec!["a".to_string(), "c".to_string()]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "S");
}

#[test]
fn test_named_api_arithmetic() {
    let mut registry = GrammarRegistry::new();
    let _e = expr(r#"E "plus" T | T"#).named("E", &mut registry).unwrap();
    let _t = expr(r#"T "star" F | F"#).named("T", &mut registry).unwrap();
    let _f = expr(r#""lparen" E "rparen" | "num""#)
        .named("F", &mut registry)
        .unwrap();
    let table = LRCompiler::compile::<String>(&registry);
    let result = run(&table, vec!["num".to_string()]);
    assert!(result.is_ok());
    let result = run(
        &table,
        vec!["num".to_string(), "plus".to_string(), "num".to_string()],
    );
    assert!(result.is_ok());
}

#[test]
fn test_registry_start_symbol_and_production_count() {
    let mut registry = GrammarRegistry::new();
    let _s = expr(r#""a" | "b""#).named("S", &mut registry).unwrap();
    assert!(matches!(
        registry.start_symbol,
        Some(RegExt::Symbol::NonTerminal(_))
    ));
    assert_eq!(
        registry.productions.len(),
        2,
        "two alternatives -> two productions"
    );
}

#[test]
fn test_registry_records_terminals() {
    let mut registry = GrammarRegistry::new();
    Expr::str("a")
        .or(Expr::str("b"))
        .named("S", &mut registry)
        .unwrap();
    assert_eq!(registry.productions.len(), 2);
    assert!(registry.terminals.contains("a"));
    assert!(registry.terminals.contains("b"));
    assert_eq!(registry.terminals.len(), 2);

    let mut registry = GrammarRegistry::new();
    expr(r#""a" | "b" "c""#).named("S", &mut registry).unwrap();
    assert!(registry.terminals.contains("a"));
    assert!(registry.terminals.contains("b"));
    assert!(registry.terminals.contains("c"));
    assert_eq!(registry.terminals.len(), 3);

    // The recorded name is the class terminal, not its expanded letters.
    let mut registry = GrammarRegistry::new();
    expr(r#""[a-c]""#).named("S", &mut registry).unwrap();
    assert!(registry.terminals.contains("[a-c]"));
    assert_eq!(registry.terminals.len(), 1);
    let parser = CfgParser::from_registry(registry).unwrap();
    assert!(parser.is_match("b"));
    assert!(!parser.is_match("d"));

    let mut registry = GrammarRegistry::new();
    let err = Expr::str("").named("S", &mut registry).unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");
    assert!(registry.productions.is_empty());
}

#[test]
fn test_quantified_name_does_not_self_loop() {
    // The fresh nonterminal for `a*` must not be the name being registered.
    // `N1 -> N1` makes `is_match` diverge.
    let mut registry = GrammarRegistry::new();
    Expr::str("a").star().named("N1", &mut registry).unwrap();
    let self_loop = registry.productions.iter().any(|(lhs, rhs)| {
        *lhs == Symbol::NonTerminal("N1".to_string())
            && rhs.len() == 1
            && rhs[0] == Symbol::NonTerminal("N1".to_string())
    });
    assert!(!self_loop, "registering N1 must not add N1 -> N1");
    let parser = CfgParser::from_registry(registry).unwrap();
    assert_eq!(parser.conflict_count(), 0);
    assert!(parser.is_match(""));
    assert!(parser.is_match("a"));
    assert!(parser.is_match("aa"));
    assert!(!parser.is_match("b"));

    for (label, expr) in [
        ("plus", Expr::str("a").plus()),
        ("opt", Expr::str("a").opt()),
    ] {
        let mut registry = GrammarRegistry::new();
        expr.named("N1", &mut registry).unwrap();
        let self_loop = registry.productions.iter().any(|(lhs, rhs)| {
            *lhs == Symbol::NonTerminal("N1".to_string())
                && rhs.len() == 1
                && rhs[0] == Symbol::NonTerminal("N1".to_string())
        });
        assert!(!self_loop, "{label}: registering N1 must not add N1 -> N1");
        let parser = CfgParser::from_registry(registry).unwrap();
        assert_eq!(parser.conflict_count(), 0, "{label}");
        match label {
            "plus" => {
                assert!(!parser.is_match(""));
                assert!(parser.is_match("a"));
                assert!(parser.is_match("aa"));
            }
            "opt" => {
                assert!(parser.is_match(""));
                assert!(parser.is_match("a"));
                assert!(!parser.is_match("aa"));
            }
            _ => unreachable!(),
        }
        assert!(!parser.is_match("b"), "{label}");
    }
}

#[test]
fn test_fresh_name_avoids_referenced_nonterminal() {
    let mut registry = GrammarRegistry::new();
    Expr::call("N1").star().named("S", &mut registry).unwrap();
    Expr::str("a").named("N1", &mut registry).unwrap();
    let stole_n1 = registry.productions.iter().any(|(lhs, rhs)| {
        *lhs == Symbol::NonTerminal("N1".to_string())
            && rhs.len() == 2
            && rhs
                .iter()
                .all(|s| *s == Symbol::NonTerminal("N1".to_string()))
    });
    assert!(
        !stole_n1,
        "the star's fresh name must not be the referenced N1"
    );
    let parser = CfgParser::from_registry(registry).unwrap();
    assert_eq!(parser.conflict_count(), 0);
    assert!(parser.is_match(""));
    assert!(parser.is_match("a"));
    assert!(parser.is_match("aa"));
    assert!(!parser.is_match("b"));

    let mut registry = GrammarRegistry::new();
    Expr::call("N1").plus().named("S", &mut registry).unwrap();
    Expr::str("a").named("N1", &mut registry).unwrap();
    let stole_n1 = registry.productions.iter().any(|(lhs, rhs)| {
        *lhs == Symbol::NonTerminal("N1".to_string())
            && rhs.len() == 2
            && rhs
                .iter()
                .all(|s| *s == Symbol::NonTerminal("N1".to_string()))
    });
    assert!(
        !stole_n1,
        "the plus's fresh name must not be the referenced N1"
    );
    let parser = CfgParser::from_registry(registry).unwrap();
    assert_eq!(parser.conflict_count(), 0);
    assert!(!parser.is_match(""));
    assert!(parser.is_match("a"));
    assert!(parser.is_match("aa"));
    assert!(!parser.is_match("b"));
}
