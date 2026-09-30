use RegExt::{Engine, GrammarRegistry, LRCompiler, expr};

#[derive(Debug, Clone, PartialEq)]
enum Arith {
    Plus(Box<Arith>, Box<Arith>),
    Mult(Box<Arith>, Box<Arith>),
    Num(i64),
}

fn main() {
    // Registration order matters: the first non-terminal is the start symbol.
    let mut registry = GrammarRegistry::new();
    let _e = expr(r#"E "+" T | T"#).named("E", &mut registry).unwrap();
    let _t = expr(r#"T "*" F | F"#).named("T", &mut registry).unwrap();
    let _f = expr(r#""(" E ")" | NUM"#).named("F", &mut registry).unwrap();
    let _num = expr(r#""1" | "2" | "3""#).named("NUM", &mut registry).unwrap();

    let table = LRCompiler::compile::<String>(&registry);
    let mut engine = Engine::new(0);

    let input = vec!["1".to_string(), "+".to_string(), "2".to_string()];
    let result = engine.run(&table, input);
    assert_eq!(
        result.as_deref(),
        Ok("E"),
        "'1 + 2' must parse to the start symbol"
    );

    // Goal of AST mapping (roadmap): reduce the parse to a Rust data structure.
    let expected = Arith::Plus(Box::new(Arith::Num(1)), Box::new(Arith::Num(2)));
    let mult_example = Arith::Mult(Box::new(Arith::Num(2)), Box::new(Arith::Num(3)));
    println!(
        "ast_mapping: 1 assertion passed; target AST for '1 + 2': {:#?}",
        expected
    );
    println!("ast_mapping: target AST for '2 * 3': {:#?}", mult_example);
}
