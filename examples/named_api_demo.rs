use parser_vm::{GrammarRegistry, LRCompiler, Engine, expr};

fn main() {
    println!("=== Named API Demo ===\n");
    
    println!("Using expr().named() API for building grammars\n");
    
    // Example 1: Simple grammar
    println!("1. Simple grammar with named API:");
    let mut registry = GrammarRegistry::new();
    let _s = expr("a | b").named("S", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    let result = engine.run(&table, vec!["a".to_string()]);
    println!("   S -> a | b");
    println!("   Parse 'a': {:?}", result.is_ok());
    
    // Example 2: Nested grammar
    println!("\n2. Nested grammar:");
    let mut registry = GrammarRegistry::new();
    let _s = expr("A B").named("S", &mut registry);
    let _a = expr("a | b").named("A", &mut registry);
    let _b = expr("c | d").named("B", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    let tests = vec![
        (vec!["a".to_string(), "c".to_string()], true),
        (vec!["b".to_string(), "d".to_string()], true),
        (vec!["a".to_string(), "x".to_string()], false),
    ];
    
    for (input, expected) in tests {
        let result = engine.run(&table, input.clone());
        let status = if result.is_ok() == expected { "✓" } else { "✗" };
        println!("   {} {:?} -> {}", status, input, result.is_ok());
    }
    
    // Example 3: Arithmetic
    println!("\n3. Arithmetic grammar:");
    let mut registry = GrammarRegistry::new();
    let _e = expr("E plus T | T").named("E", &mut registry);
    let _t = expr("T star F | F").named("T", &mut registry);
    let _f = expr("lparen E rparen | num").named("F", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    let tests = vec![
        (vec!["num".to_string()], true),
        (vec!["num".to_string(), "plus".to_string(), "num".to_string()], true),
        (vec!["lparen".to_string(), "num".to_string(), "rparen".to_string()], true),
    ];
    
    for (input, expected) in tests {
        let result = engine.run(&table, input.clone());
        let status = if result.is_ok() == expected { "✓" } else { "✗" };
        println!("   {} {:?} -> {}", status, input, result.is_ok());
    }
    
    println!("\nNamed API provides fluent builder pattern for grammars");
    println!("Order matters: first registered non-terminal becomes start symbol");
}
