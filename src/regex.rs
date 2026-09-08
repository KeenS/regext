//! Regex front end — patterns compile down to a CFG grammar and are matched
//! by the LALR(1) VM; a regex is thus a restricted, LALR(1) CFG.
//!
//! Supported pattern syntax:
//!   - literal characters (any run of plain characters)
//!   - `|`    alternation
//!   - `*`    zero or more repetitions (binds to the last character of a run)
//!   - `+`    one or more repetitions
//!   - `?`    zero or one occurrence
//!   - `(...)` capture group, numbered by opening parenthesis, starting at 1
//!   - `\x`   escapes a special character so it matches literally
//!
//! `.` (any character) and empty alternatives are rejected: they are either
//! not expressible as a CFG or degenerate, and the VM deliberately stays a
//! grammar engine.
//!
//! Semantics (inherited from [`CfgParser`]):
//!   - `is_match`   the ENTIRE input must match (anchored at both ends)
//!   - `find`       leftmost match at any position; the match extends as far
//!     as the grammar allows (leftmost start, longest match)
//!   - `captures`   the leftmost match plus the span of each capture group
//!
//! ```
//! use RegExt::{Match, Regex};
//!
//! let re = Regex::new("a(b)+c").unwrap();
//! assert!(re.is_match("abbbc"));          // anchored: whole input
//! assert_eq!(re.find("xyabbcz"), Some(Match::new(2, 6)));
//! let caps = re.captures("abbc").unwrap();
//! assert_eq!(caps.matches, vec![Some(Match::new(0, 4)), Some(Match::new(2, 3))]);
//! ```

use crate::cfg_parser::{Captures, CfgParser, Match};

/// Error returned by [`Regex::new`].
///
/// [`InvalidPattern`][Self::InvalidPattern] means the pattern syntax itself is
/// invalid in RegExt's dialect (for example a `.` or an empty alternative).
/// [`CompileError`][Self::CompileError] means the pattern is syntactically
/// fine but its CFG encoding is not conflict-free LALR(1), in which case the
/// message carries the number of table conflicts.
#[derive(Debug, Clone)]
pub enum RegexError {
    /// The pattern is not valid RegExt syntax (unbalanced groups, `.` any-char,
    /// empty alternative, quantifier without operand, dangling escape, ...).
    InvalidPattern(String),
    /// The pattern compiles to a grammar with LALR(1) conflicts, so it is
    /// refused rather than resolved in production order.
    CompileError(String),
}

impl std::fmt::Display for RegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegexError::InvalidPattern(s) => write!(f, "Invalid pattern: {}", s),
            RegexError::CompileError(s) => write!(f, "Compile error: {}", s),
        }
    }
}

impl std::error::Error for RegexError {}

// =============================================================================
// Pattern AST
// =============================================================================

#[derive(Debug, Clone)]
enum PNode {
    Lit(String),
    Seq(Vec<PNode>),
    Alt(Vec<PNode>),
    Star(Box<PNode>),
    Plus(Box<PNode>),
    Opt(Box<PNode>),
    /// A capture group: its 1-based id (in pattern order) and its body.
    Group(usize, Box<PNode>),
}

// =============================================================================
// Pattern parser (recursive descent: alt -> seq -> rep -> atom)
// =============================================================================

struct PParser {
    chars: Vec<char>,
    pos: usize,
    next_group: usize,
}

impl PParser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    /// The next capture-group id; groups are numbered in pattern order.
    fn next_group_id(&mut self) -> usize {
        let id = self.next_group;
        self.next_group += 1;
        id
    }
}

fn is_special(ch: char) -> bool {
    matches!(ch, '|' | '*' | '+' | '?' | '(' | ')' | '\\' | '.')
}

fn parse_pattern(pattern: &str) -> Result<PNode, RegexError> {
    let mut p = PParser {
        chars: pattern.chars().collect(),
        pos: 0,
        next_group: 1,
    };
    let root = parse_alternation(&mut p)?;
    if p.pos != p.chars.len() {
        return Err(RegexError::InvalidPattern(format!(
            "unexpected '{}' after pattern",
            p.chars[p.pos]
        )));
    }
    Ok(root)
}

fn parse_alternation(p: &mut PParser) -> Result<PNode, RegexError> {
    let mut alts = vec![parse_sequence(p)?];
    while p.peek() == Some('|') {
        p.bump();
        alts.push(parse_sequence(p)?);
    }
    match alts.len() {
        1 => Ok(alts.into_iter().next().unwrap()),
        _ => Ok(PNode::Alt(alts)),
    }
}

fn parse_sequence(p: &mut PParser) -> Result<PNode, RegexError> {
    let mut seq = Vec::new();
    while let Some(ch) = p.peek() {
        if ch == '|' || ch == ')' {
            break;
        }
        seq.push(parse_repetition(p)?);
    }
    match seq.len() {
        0 => Err(RegexError::InvalidPattern(
            "empty alternative in pattern".to_string(),
        )),
        1 => Ok(seq.into_iter().next().unwrap()),
        _ => Ok(PNode::Seq(seq)),
    }
}

fn parse_repetition(p: &mut PParser) -> Result<PNode, RegexError> {
    let atom = parse_atom(p)?;
    match p.peek() {
        Some('*') => {
            p.bump();
            Ok(PNode::Star(Box::new(atom)))
        }
        Some('+') => {
            p.bump();
            Ok(PNode::Plus(Box::new(atom)))
        }
        Some('?') => {
            p.bump();
            Ok(PNode::Opt(Box::new(atom)))
        }
        _ => Ok(atom),
    }
}

fn parse_atom(p: &mut PParser) -> Result<PNode, RegexError> {
    match p.peek() {
        None => Err(RegexError::InvalidPattern(
            "unexpected end of pattern".to_string(),
        )),
        Some('(') => {
            p.bump();
            let id = p.next_group_id();
            let inner = parse_alternation(p)?;
            if p.peek() != Some(')') {
                return Err(RegexError::InvalidPattern(
                    "unbalanced '(' in pattern".to_string(),
                ));
            }
            p.bump();
            Ok(PNode::Group(id, Box::new(inner)))
        }
        Some('.') => Err(RegexError::InvalidPattern(
            "'.': any-character is not expressible as a CFG".to_string(),
        )),
        Some('*') | Some('+') | Some('?') => Err(RegexError::InvalidPattern(
            "quantifier with no operand".to_string(),
        )),
        Some('\\') => {
            p.bump();
            match p.peek() {
                Some(ch) => {
                    p.bump();
                    Ok(PNode::Lit(ch.to_string()))
                }
                None => Err(RegexError::InvalidPattern(
                    "dangling escape at end of pattern".to_string(),
                )),
            }
        }
        Some(_) => parse_literal_run(p),
    }
}

/// Consume a run of plain characters. A quantifier right after the run
/// binds to the run's LAST character only (`ab*` = `a` then `b*`).
fn parse_literal_run(p: &mut PParser) -> Result<PNode, RegexError> {
    let start = p.pos;
    while let Some(ch) = p.peek() {
        if is_special(ch) {
            break;
        }
        p.bump();
    }
    let run: String = p.chars[start..p.pos].iter().collect();
    match p.peek() {
        Some('*') | Some('+') | Some('?') => {
            let quantifier = p.peek().unwrap();
            p.bump();
            let base: String = run.chars().take(run.chars().count() - 1).collect();
            let last: String = run.chars().skip(run.chars().count() - 1).collect();
            let rep = match quantifier {
                '*' => PNode::Star(Box::new(PNode::Lit(last))),
                '+' => PNode::Plus(Box::new(PNode::Lit(last))),
                _ => PNode::Opt(Box::new(PNode::Lit(last))),
            };
            let mut seq = Vec::new();
            if !base.is_empty() {
                seq.push(PNode::Lit(base));
            }
            seq.push(rep);
            Ok(match seq.len() {
                1 => seq.into_iter().next().unwrap(),
                _ => PNode::Seq(seq),
            })
        }
        _ => Ok(PNode::Lit(run)),
    }
}

// =============================================================================
// CFG code generation
// =============================================================================

/// The generated grammar string plus the capture-group nonterminals in
/// pattern order.
struct Cfg {
    grammar: String,
    groups: Vec<String>,
}

struct Lowerer {
    rules: Vec<(String, String)>,
    n_counter: usize,
    groups: Vec<(usize, String)>,
}

impl Lowerer {
    fn fresh_n(&mut self) -> String {
        self.n_counter += 1;
        format!("N{}", self.n_counter)
    }

    /// Emit the grammar symbols for `node`, registering any named
    /// nonterminals (alternations, repetitions, groups) it introduces.
    fn emit(&mut self, node: &PNode) -> Vec<String> {
        match node {
            PNode::Lit(s) => vec![format!("\"{}\"", s)],
            PNode::Seq(els) => els.iter().flat_map(|e| self.emit(e)).collect(),
            PNode::Alt(alts) => {
                let name = self.fresh_n();
                let rhs = alts
                    .iter()
                    .map(|a| self.emit(a).join(" "))
                    .collect::<Vec<_>>()
                    .join(" | ");
                self.rules.push((name.clone(), rhs));
                vec![name]
            }
            PNode::Star(base) => {
                let body = self.emit(base).join(" ");
                let name = self.fresh_n();
                self.rules
                    .push((name.clone(), format!("{} {}", body, name)));
                self.rules.push((name.clone(), "epsilon".to_string()));
                vec![name]
            }
            PNode::Plus(base) => {
                let body = self.emit(base).join(" ");
                let name = self.fresh_n();
                self.rules
                    .push((name.clone(), format!("{} {}", body, name)));
                self.rules.push((name.clone(), body));
                vec![name]
            }
            PNode::Opt(base) => {
                let body = self.emit(base).join(" ");
                let name = self.fresh_n();
                self.rules.push((name.clone(), "epsilon".to_string()));
                self.rules.push((name.clone(), body));
                vec![name]
            }
            PNode::Group(id, inner) => {
                let name = format!("G{}", id);
                let rhs = self.emit(inner).join(" ");
                self.rules.push((name.clone(), rhs));
                self.groups.push((*id, name.clone()));
                vec![name]
            }
        }
    }
}

/// Lower the pattern AST into a CFG grammar string (start symbol `S`
/// first) and the ordered capture-group names.
fn lower(node: &PNode) -> Cfg {
    let mut lowerer = Lowerer {
        rules: Vec::new(),
        n_counter: 0,
        groups: Vec::new(),
    };
    let root = lowerer.emit(node);
    let mut grammar = format!("S -> {}", root.join(" "));
    for (lhs, rhs) in &lowerer.rules {
        grammar.push_str(&format!("; {} -> {}", lhs, rhs));
    }
    // Order capture groups by their pattern-order id (the emit traversal
    // visits nested groups depth-first, which is not pattern order).
    let mut groups = lowerer.groups;
    groups.sort_by_key(|(id, _)| *id);
    let group_names = groups.into_iter().map(|(_, name)| name).collect();
    Cfg {
        grammar,
        groups: group_names,
    }
}

// =============================================================================
// Regex
// =============================================================================

/// A compiled regular expression: a pattern lowered to a CFG grammar and
/// matched by the LALR(1) engine.
///
/// `Regex` is a restricted, honest dialect of regular expressions — see the
/// crate-level documentation for the pattern language, the matching semantics
/// (anchored [`is_match`][Self::is_match], leftmost [`find`][Self::find]), and
/// the capture-group semantics.
pub struct Regex {
    pattern: String,
    cfg: CfgParser,
    groups: Vec<String>,
}

impl Regex {
    /// Compile a pattern: parse, lower to a CFG, build the LALR(1) table.
    /// Fails with `InvalidPattern` on bad syntax and with `CompileError`
    /// when the encoding is not conflict-free LALR(1).
    pub fn new(pattern: &str) -> Result<Self, RegexError> {
        let ast = parse_pattern(pattern)?;
        let cfg = lower(&ast);
        let parser =
            CfgParser::new(&cfg.grammar).map_err(|e| RegexError::CompileError(e.to_string()))?;
        if parser.conflict_count() > 0 {
            return Err(RegexError::CompileError(format!(
                "pattern compiles to a grammar with {} LALR(1) conflict(s)",
                parser.conflict_count()
            )));
        }
        Ok(Self {
            pattern: pattern.to_string(),
            cfg: parser,
            groups: cfg.groups,
        })
    }

    /// The original pattern string, unchanged.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Number of capture groups in the pattern.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// The ENTIRE input must match (anchored at both ends).
    pub fn is_match(&self, text: &str) -> bool {
        self.cfg.is_match(text)
    }

    /// Leftmost match at any position (leftmost start, longest match).
    pub fn find(&self, text: &str) -> Option<Match> {
        self.cfg.find(text)
    }

    /// Leftmost match plus the span of each capture group, in pattern
    /// order; a group that did not participate is `None`.
    pub fn captures(&self, text: &str) -> Option<Captures> {
        let groups: Vec<&str> = self.groups.iter().map(|s| s.as_str()).collect();
        self.cfg.captures(text, &groups)
    }
}
