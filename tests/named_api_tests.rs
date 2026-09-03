use parser_vm::{Engine, GrammarRegistry, LRCompiler, LalrTable, expr};

fn run(table: &LalrTable<String>, input: Vec<String>) -> Result<String, String> {
    let mut engine = Engine::new(0);
    engine.run(table, input)
}

#[test]
fn test_named_api_basic() {
    let mut registry = GrammarRegistry::new();
    let _s = expr(r#""a" | "b""#).named("S", &mut registry);
    let table = LRCompiler::compile::<String>(&registry);
    let result = run(&table, vec!["a".to_string()]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "S");
}

#[test]
fn test_named_api_sequence() {
    let mut registry = GrammarRegistry::new();
    let _e = expr(r#""a" "b" | "c""#).named("E", &mut registry);
    let table = LRCompiler::compile::<String>(&registry);
    let result = run(&table, vec!["a".to_string(), "b".to_string()]);
    assert!(result.is_ok());
}

#[test]
fn test_named_api_nested() {
    let mut registry = GrammarRegistry::new();
    let _s = expr("A B").named("S", &mut registry);
    let _a = expr(r#""a" | "b""#).named("A", &mut registry);
    let _b = expr(r#""c" | "d""#).named("B", &mut registry);
    let table = LRCompiler::compile::<String>(&registry);
    let result = run(&table, vec!["a".to_string(), "c".to_string()]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "S");
}

#[test]
fn test_named_api_arithmetic() {
    let mut registry = GrammarRegistry::new();
    let _e = expr(r#"E "plus" T | T"#).named("E", &mut registry);
    let _t = expr(r#"T "star" F | F"#).named("T", &mut registry);
    let _f = expr(r#""lparen" E "rparen" | "num""#).named("F", &mut registry);
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
    let _s = expr(r#""a" | "b""#).named("S", &mut registry);
    assert!(matches!(
        registry.start_symbol,
        Some(parser_vm::Symbol::NonTerminal(_))
    ));
    assert_eq!(
        registry.productions.len(),
        2,
        "two alternatives -> two productions"
    );
}
