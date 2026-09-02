use parser_vm::{Engine, GrammarRegistry, LRCompiler, LalrTable, expr};

fn run(table: &LalrTable<String>, input: Vec<String>) -> Result<String, String> {
    let mut engine = Engine::new(0);
    engine.run(table, input)
}

fn main() {
    // 1. Simple grammar: the first registered non-terminal is the start symbol
    let mut registry = GrammarRegistry::new();
    let _s = expr(r#""a" | "b""#).named("S", &mut registry);

    let table = LRCompiler::compile::<String>(&registry);
    assert_eq!(run(&table, vec!["a".to_string()]).as_deref(), Ok("S"), "S -> a");
    assert_eq!(run(&table, vec!["b".to_string()]).as_deref(), Ok("S"), "S -> b");

    // 2. Nested grammar: S -> A B; A -> a | b; B -> c | d
    let mut registry = GrammarRegistry::new();
    let _s = expr("A B").named("S", &mut registry);
    let _a = expr(r#""a" | "b""#).named("A", &mut registry);
    let _b = expr(r#""c" | "d""#).named("B", &mut registry);

    let table = LRCompiler::compile::<String>(&registry);
    assert_eq!(run(&table, vec!["a".to_string(), "c".to_string()]).as_deref(), Ok("S"), "A=a B=c");
    assert_eq!(run(&table, vec!["b".to_string(), "d".to_string()]).as_deref(), Ok("S"), "A=b B=d");
    assert!(run(&table, vec!["a".to_string(), "x".to_string()]).is_err(), "unknown B must be rejected");

    // 3. Arithmetic grammar with symbolic terminals
    let mut registry = GrammarRegistry::new();
    let _e = expr(r#"E "plus" T | T"#).named("E", &mut registry);
    let _t = expr(r#"T "star" F | F"#).named("T", &mut registry);
    let _f = expr(r#""lparen" E "rparen" | "num""#).named("F", &mut registry);

    let table = LRCompiler::compile::<String>(&registry);
    assert_eq!(run(&table, vec!["num".to_string()]).as_deref(), Ok("E"), "single num");
    assert_eq!(
        run(&table, vec!["num".to_string(), "plus".to_string(), "num".to_string()]).as_deref(),
        Ok("E"),
        "num plus num"
    );
    assert_eq!(
        run(&table, vec!["lparen".to_string(), "num".to_string(), "rparen".to_string()]).as_deref(),
        Ok("E"),
        "lparen num rparen"
    );

    println!("named_api_demo: 7 assertions passed");
}
