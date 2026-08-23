use parser_vm::{GrammarRegistry, LRCompiler, Engine, expr};

#[derive(Debug, Clone, PartialEq)]
enum Arith {
    Plus(Box<Arith>, Box<Arith>),
    Mult(Box<Arith>, Box<Arith>),
    Num(i64),
}

fn main() {
    println!("=== AST Mapping Demo ===\n");
    
    println!("Parsing arithmetic expressions to AST\n");
    
    let mut registry = GrammarRegistry::new();
    
    // Build grammar with AST mapping
    let _num = expr("1 | 2 | 3").named("NUM", &mut registry);
    let _f = expr("( E ) | NUM").named("F", &mut registry);
    let _t = expr("T * F | F").named("T", &mut registry);
    let _e = expr("E + T | T").named("E", &mut registry);
    
    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);
    
    let input = vec!["1".to_string(), "+".to_string(), "2".to_string()];
    let result = engine.run(&table, input);
    
    println!("Input: 1 + 2");
    println!("Parse result: {:?}", result.is_ok());
    println!("AST mapping would convert parse tree to Rust data structures");
    
    println!("\nAST example:");
    println!("  Plus(Box::new(Num(1)), Box::new(Num(2)))");
}
