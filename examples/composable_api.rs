use parser_vm::{Engine, GrammarRegistry, LRCompiler, SLRTable, expr};

fn and(left: &str, right: &str) -> String {
    format!("{} {}", left, right)
}

fn or(left: &str, right: &str) -> String {
    format!("{} | {}", left, right)
}

fn run(table: &SLRTable<String>, input: Vec<String>) -> Result<String, String> {
    let mut engine = Engine::new(0);
    engine.run(table, input)
}

fn main() {
    // Build the grammar with and/or combinators
    let mut registry = GrammarRegistry::new();

    let e_expr = or(&and("E", "+ T"), "T");
    let t_expr = or(&and("T", "* F"), "F");
    let f_expr = or("( E )", "NUM");

    let _e = expr(&e_expr).named("E", &mut registry);
    let _t = expr(&t_expr).named("T", &mut registry);
    let _f = expr(&f_expr).named("F", &mut registry);
    let _num = expr("1 | 2 | 3").named("NUM", &mut registry);

    let table = LRCompiler::compile::<String>(&registry);

    // Each run needs a fresh engine; the start symbol is E.
    assert_eq!(run(&table, vec!["1".to_string()]).as_deref(), Ok("E"), "single number");
    assert_eq!(
        run(&table, vec!["1".to_string(), "+".to_string(), "2".to_string()]).as_deref(),
        Ok("E"),
        "addition"
    );
    assert_eq!(
        run(&table, vec!["(".to_string(), "1".to_string(), "+".to_string(), "2".to_string(), ")".to_string()]).as_deref(),
        Ok("E"),
        "parenthesized"
    );
    assert!(
        run(&table, vec!["1".to_string(), "+".to_string()]).is_err(),
        "trailing operator must be rejected"
    );

    println!("composable_api: 4 assertions passed");
}
