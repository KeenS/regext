use parser_vm::{GrammarRegistry, LRCompiler, Engine, expr};

fn and(left: &str, right: &str) -> String {
    format!("{} {}", left, right)
}

fn or(left: &str, right: &str) -> String {
    format!("{} | {}", left, right)
}

fn main() {
    println!("=== Composable API Demo ===\n");
    
    println!("Building grammar using and/or combinators\n");
    
    let mut registry = GrammarRegistry::new();
    
    // Using composable APIs
    let e_expr = or("E + T", "T");
    let t_expr = or("T * F", "F");
    let f_expr = or("( E )", "NUM");
    
    let _e = expr(&e_expr).named("E", &mut registry);
    let _t = expr(&t_expr).named("T", &mut registry);
    let _f = expr(&f_expr).named("F", &mut registry);
    let _num = expr("1 | 2 | 3").named("NUM", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    
    let tests = vec![
        vec!["1".to_string()],
        vec!["1".to_string(), "+".to_string(), "2".to_string()],
        vec!["(".to_string(), "1".to_string(), "+".to_string(), "2".to_string(), ")".to_string()],
    ];
    
    for input in tests {
        let result = engine.run(&table, input.clone());
        println!("  {:?} -> {}", input, result.is_ok());
    }
    
    println!("\nComposable APIs: and(), or() for building grammars");
}
