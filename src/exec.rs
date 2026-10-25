//! Interned shift/reduce interpreter.
//!
//! The LALR table is built over owned strings so conflict detection stays
//! exactly the sorted-item, last-write-wins construction. Matching then runs
//! on integer symbol ids: action and goto never allocate, and a find walks
//! each start at most once instead of re-tokenizing every character slice.
//! Conflict-free grammars also get a literal prefilter: a fixed-string search
//! when every match is one literal, a required-literal reject, a prefix
//! search (`str::find`, the Two-Way algorithm), and a byte-class scan of
//! FIRST.

use std::collections::{HashMap, HashSet};

use crate::engine::{Action, Reduction};
use crate::grammar::Symbol;
use crate::lalr::LalrTable;
use crate::lexer::{self, ByteSet, Lexeme, Lexer};

const UNKNOWN: u32 = u32::MAX;

/// A compiled grammar ready to match text.
pub(crate) struct Exec {
    eof_action: Vec<Action>,
    term_action: Vec<HashMap<u32, Action>>,
    goto: Vec<HashMap<u32, u32>>,
    prod_lhs: Vec<u32>,
    prod_rhs_len: Vec<usize>,
    /// Symbol id to the name stored on a [`Reduction`].
    names: Vec<String>,
    ids: HashMap<String, u32>,
    start_id: u32,
    /// `true` when the empty token sequence derives the start symbol.
    nullable: bool,
    /// Characters that can begin a non-empty match. `None` means the set is
    /// open-ended (a negated class or the OTHER fallback is in FIRST).
    first_chars: Option<HashSet<char>>,
    /// Literal and byte-class accelerators. Off when the table has a
    /// conflict: last-write-wins can accept a different language than the
    /// productions describe.
    accel: Option<Accel>,
}

struct Cursor {
    states: Vec<usize>,
    syms: Vec<u32>,
}

enum Undo {
    PopState,
    PopSym,
    PushState(usize),
    PushSym(u32),
}

enum Drive {
    Shifted,
    Accepted,
}

struct Step {
    /// Log length before this token was shifted.
    mark_before: usize,
    /// Match end before this token was consumed.
    end_before: usize,
    /// `(token start, token end)` when the token is more than one character,
    /// so a shorter tokenization of that span can still be a match.
    interior: Option<(usize, usize)>,
}

impl Exec {
    pub(crate) fn from_table(
        table: &LalrTable<String>,
        start: &str,
        first_chars: Option<HashSet<char>>,
        prods: &[(Symbol, Vec<Symbol>)],
        conflicts: usize,
    ) -> Self {
        let mut ids: HashMap<String, u32> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut intern = |symbol: &str| -> u32 {
            if let Some(&id) = ids.get(symbol) {
                return id;
            }
            let id = names.len() as u32;
            names.push(symbol.to_string());
            ids.insert(symbol.to_string(), id);
            id
        };

        let mut prod_lhs = Vec::with_capacity(table.productions().len());
        let mut prod_rhs_len = Vec::with_capacity(table.productions().len());
        for prod in table.productions() {
            prod_lhs.push(intern(&prod.lhs));
            prod_rhs_len.push(prod.rhs_len);
        }

        let n = table.row_count();
        let mut eof_action = vec![Action::Error; n];
        let mut term_action = vec![HashMap::new(); n];
        let mut goto = vec![HashMap::new(); n];
        for state in 0..n {
            eof_action[state] = table.eof_action(state);
            for (symbol, action) in table.terminal_actions(state) {
                term_action[state].insert(intern(symbol), *action);
            }
            for (symbol, next) in table.gotos(state) {
                goto[state].insert(intern(symbol), *next as u32);
            }
        }
        let start_id = intern(start);

        let mut exec = Self {
            eof_action,
            term_action,
            goto,
            prod_lhs,
            prod_rhs_len,
            names,
            ids,
            start_id,
            nullable: false,
            first_chars,
            accel: None,
        };
        exec.nullable = exec.accepts_ids(&[]);
        if conflicts == 0 {
            exec.accel = Some(Accel::analyze(prods, start, exec.first_chars.as_ref()));
        }
        exec
    }

    /// `Some` when a literal prefilter decides an anchored match.
    pub(crate) fn anchored_shortcut(&self, text: &str) -> Option<bool> {
        let accel = self.accel.as_ref()?;
        let core = text.trim();
        if core.is_empty() {
            return Some(self.nullable);
        }
        if let Some(lits) = &accel.literals {
            return Some(lits.iter().any(|lit| lit == core));
        }
        if accel
            .required
            .iter()
            .any(|lit| !core.contains(lit.as_str()))
        {
            return Some(false);
        }
        if !accel.prefix.is_empty() && !core.starts_with(accel.prefix.as_str()) {
            return Some(false);
        }
        if let Some(suffixes) = &accel.suffixes {
            if !suffixes.iter().any(|lit| core.ends_with(lit.as_str())) {
                return Some(false);
            }
        }
        None
    }

    /// Leftmost-longest covered match. Conflict-free grammars use the literal
    /// prefilter; conflicted grammars scan candidate starts.
    pub(crate) fn find_in(&self, lexer: &Lexer, text: &str) -> Option<(usize, usize)> {
        let Some(accel) = self.accel.as_ref() else {
            return self.find_general(lexer, text);
        };
        self.find_fast(lexer, text, accel)
    }

    fn find_general(&self, lexer: &Lexer, text: &str) -> Option<(usize, usize)> {
        if self.nullable {
            return self.longest_match(lexer, text, 0).map(|end| (0, end));
        }
        let mut start = 0;
        while start < text.len() {
            if self.may_start(text, start) {
                if let Some(end) = self.longest_match(lexer, text, start) {
                    if end > start {
                        return Some((start, end));
                    }
                }
            }
            start = lexer::next_char_end(text, start);
        }
        None
    }

    fn find_fast(&self, lexer: &Lexer, text: &str, accel: &Accel) -> Option<(usize, usize)> {
        if self.nullable {
            if let Some(lits) = &accel.literals {
                let end = longest_literal_at(text, 0, lits).unwrap_or(0);
                return Some((0, end));
            }
            if !fast_may_start(text, 0, accel.first_bytes) || missing_literal(&accel.required, text)
            {
                return Some((0, 0));
            }
            return self.longest_match(lexer, text, 0).map(|end| (0, end));
        }
        if let Some(lits) = &accel.literals {
            return find_literals(text, lits);
        }
        if missing_literal(&accel.required, text) {
            return None;
        }
        if !accel.prefix.is_empty() {
            return self.find_prefixed(lexer, text, &accel.prefix);
        }
        self.find_scanned(lexer, text, accel.first_bytes)
    }

    fn find_prefixed(&self, lexer: &Lexer, text: &str, prefix: &str) -> Option<(usize, usize)> {
        let mut from = 0;
        while from < text.len() {
            let Some(rel) = text[from..].find(prefix) else {
                return None;
            };
            let start = from + rel;
            if let Some(end) = self.longest_match(lexer, text, start) {
                if end > start {
                    return Some((start, end));
                }
            }
            let next = lexer::next_char_end(text, start);
            if next <= start {
                break;
            }
            from = next;
        }
        None
    }

    fn find_scanned(
        &self,
        lexer: &Lexer,
        text: &str,
        first_bytes: Option<ByteSet>,
    ) -> Option<(usize, usize)> {
        let bytes = text.as_bytes();
        let mut start = 0;
        while start < bytes.len() {
            match first_bytes {
                Some(bits) => {
                    if !bits.contains(bytes[start]) {
                        start += 1;
                        continue;
                    }
                }
                None => {
                    if lexer::char_at(text, start).is_whitespace() {
                        start = lexer::next_char_end(text, start);
                        continue;
                    }
                }
            }
            if let Some(end) = self.longest_match(lexer, text, start) {
                if end > start {
                    return Some((start, end));
                }
            }
            start = lexer::next_char_end(text, start);
        }
        None
    }

    /// `false` when no non-empty covered slice can start at `start`.
    pub(crate) fn may_start(&self, text: &str, start: usize) -> bool {
        if start >= text.len() {
            return false;
        }
        let ch = lexer::char_at(text, start);
        if ch.is_whitespace() {
            return false;
        }
        match &self.first_chars {
            None => true,
            Some(chars) => chars.contains(&ch),
        }
    }

    pub(crate) fn accepts_text(&self, lexer: &Lexer, text: &str) -> bool {
        let mut ids = Vec::new();
        lexer.scan(text, |lexeme, _, _| ids.push(self.id_of(lexeme)));
        self.accepts_ids(&ids)
    }

    pub(crate) fn accepts_ids(&self, tokens: &[u32]) -> bool {
        if tokens.iter().any(|&id| id == UNKNOWN) {
            return false;
        }
        let mut cursor = Cursor::new();
        let mut index = 0;
        loop {
            let state = match cursor.states.last() {
                Some(&state) => state,
                None => return false,
            };
            let look = tokens.get(index).copied();
            match self.action(state, look) {
                Action::Shift(next) => {
                    let Some(id) = look else {
                        return false;
                    };
                    index += 1;
                    cursor.syms.push(id);
                    cursor.states.push(next);
                }
                Action::Reduce(prod) => {
                    if !cursor.reduce(self, prod) {
                        return false;
                    }
                }
                Action::Accept => return cursor.syms.last().copied() == Some(self.start_id),
                Action::Error => return false,
            }
        }
    }

    /// Reductions of `tokens`, in the order the engine performs them.
    /// `None` when the sequence does not derive the start symbol.
    pub(crate) fn trace_ids(&self, tokens: &[u32]) -> Option<Vec<Reduction<String>>> {
        if tokens.iter().any(|&id| id == UNKNOWN) {
            return None;
        }
        let mut cursor = Cursor::new();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut reductions: Vec<Reduction<String>> = Vec::new();
        let mut index = 0;
        loop {
            let state = *cursor.states.last()?;
            let look = tokens.get(index).copied();
            match self.action(state, look) {
                Action::Shift(next) => {
                    let id = look?;
                    index += 1;
                    cursor.syms.push(id);
                    cursor.states.push(next);
                    let at = index - 1;
                    spans.push((at, at + 1));
                }
                Action::Reduce(prod) => {
                    let len = self.prod_rhs_len[prod];
                    let (start, end) = if len == 0 {
                        match spans.last() {
                            Some((_, frontier)) => (*frontier, *frontier),
                            None => (0, 0),
                        }
                    } else {
                        let right = spans.pop()?;
                        let mut left = right.0;
                        for _ in 1..len {
                            left = spans.pop()?.0;
                        }
                        (left, right.1)
                    };
                    if !cursor.reduce(self, prod) {
                        return None;
                    }
                    let lhs_id = self.prod_lhs[prod];
                    reductions.push(Reduction {
                        lhs: self.names[lhs_id as usize].clone(),
                        start,
                        end,
                    });
                    spans.push((start, end));
                }
                Action::Accept => {
                    if cursor.syms.last().copied() == Some(self.start_id) {
                        return Some(reductions);
                    }
                    return None;
                }
                Action::Error => return None,
            }
        }
    }

    /// Longest token-index prefix of `tokens` that derives the start symbol,
    /// including the empty prefix when the grammar is nullable.
    pub(crate) fn longest_prefix(&self, tokens: &[u32]) -> Option<usize> {
        let mut cursor = Cursor::new();
        let mut log = Vec::new();
        // Log length before each token that was shifted. Undoing to
        // `marks[i]` restores the state after `i` tokens.
        let mut marks = Vec::with_capacity(tokens.len());
        for &id in tokens {
            if id == UNKNOWN {
                break;
            }
            let mark = log.len();
            if self.drive(&mut cursor, Some(id), &mut log).is_err() {
                undo_to(&mut cursor, &mut log, mark);
                break;
            }
            marks.push(mark);
        }
        // One end-of-input check at the furthest token, then only walk
        // backward when that prefix does not derive. A hit on the whole
        // stream is therefore a single parse.
        let mut consumed = marks.len();
        if self.eof_accepts(&mut cursor, &mut log) {
            return Some(consumed);
        }
        while consumed > 0 {
            consumed -= 1;
            undo_to(&mut cursor, &mut log, marks[consumed]);
            if self.eof_accepts(&mut cursor, &mut log) {
                return Some(consumed);
            }
        }
        None
    }

    pub(crate) fn id_of_name(&self, name: &str) -> u32 {
        self.ids.get(name).copied().unwrap_or(UNKNOWN)
    }

    /// Leftmost-longest covered match of `text` that starts at `start`.
    /// A nullable grammar also reports the empty match at `start`.
    pub(crate) fn longest_match(&self, lexer: &Lexer, text: &str, start: usize) -> Option<usize> {
        let mut cursor = Cursor::new();
        let mut log = Vec::new();
        self.search(
            lexer,
            text,
            start,
            text.len(),
            start,
            start,
            &mut cursor,
            &mut log,
        )
    }

    /// Longest accepting end in `text[pos..limit]`, with the cursor holding
    /// the tokens already consumed and `end_byte` the end of those tokens.
    /// Restores `cursor` and `log` to their entry state.
    fn search(
        &self,
        lexer: &Lexer,
        text: &str,
        mut pos: usize,
        limit: usize,
        origin: usize,
        mut end_byte: usize,
        cursor: &mut Cursor,
        log: &mut Vec<Undo>,
    ) -> Option<usize> {
        let entry = log.len();
        let mut steps: Vec<Step> = Vec::new();
        // A match found strictly inside the token that would not shift.
        let mut blocked: Option<usize> = None;

        loop {
            if pos >= limit || pos >= text.len() {
                break;
            }
            let scan_at = if pos == origin {
                if lexer::char_at(text, pos).is_whitespace() {
                    break;
                }
                pos
            } else {
                let skipped = lexer::skip_ws(text, pos, limit);
                if skipped >= limit || skipped >= text.len() {
                    break;
                }
                skipped
            };

            if let Some(hit) = lexer.best_match(text, scan_at, limit) {
                let mark = log.len();
                let id = self.id_of_name(hit.name);
                if matches!(self.drive(cursor, Some(id), log), Ok(Drive::Shifted)) {
                    let interior = (lexer::prev_char_boundary(text, hit.end) > scan_at)
                        .then_some((scan_at, hit.end));
                    steps.push(Step {
                        mark_before: mark,
                        end_before: end_byte,
                        interior,
                    });
                    pos = hit.end;
                    end_byte = hit.end;
                    continue;
                }
                undo_to(cursor, log, mark);
                blocked =
                    self.shorter_than(lexer, text, scan_at, hit.end, origin, end_byte, cursor, log);
                break;
            }

            let ch = lexer::char_at(text, scan_at);
            let end = scan_at + ch.len_utf8();
            if end > limit {
                break;
            }
            let id = match lexer.other_name() {
                Some(name) => self.id_of_name(name),
                None => self.id_of_char(ch),
            };
            let mark = log.len();
            if matches!(self.drive(cursor, Some(id), log), Ok(Drive::Shifted)) {
                steps.push(Step {
                    mark_before: mark,
                    end_before: end_byte,
                    interior: None,
                });
                pos = end;
                end_byte = end;
                continue;
            }
            undo_to(cursor, log, mark);
            break;
        }

        // End-of-input is checked at the furthest greedy token first. When
        // that slice derives, every shorter covered slice at this start is a
        // shorter match, so the scan does not revisit each token.
        let mut best = if self.eof_accepts(cursor, log) {
            Some(end_byte)
        } else {
            None
        };
        if let Some(end) = blocked {
            if best.is_none_or(|have| end > have) {
                best = Some(end);
            }
        }
        if best.is_some_and(|have| have >= end_byte) {
            undo_to(cursor, log, entry);
            return best;
        }

        while let Some(step) = steps.pop() {
            undo_to(cursor, log, step.mark_before);
            if self.eof_accepts(cursor, log) {
                if best.is_none_or(|have| step.end_before > have) {
                    best = Some(step.end_before);
                }
                break;
            }
            if let Some((token_at, token_end)) = step.interior {
                if let Some(end) = self.shorter_than(
                    lexer,
                    text,
                    token_at,
                    token_end,
                    origin,
                    step.end_before,
                    cursor,
                    log,
                ) {
                    if best.is_none_or(|have| end > have) {
                        best = Some(end);
                    }
                    if best.is_some_and(|have| have > step.end_before) {
                        break;
                    }
                }
            }
        }

        undo_to(cursor, log, entry);
        best
    }

    /// Accepting ends strictly inside a greedy token `[pos, hit_end)`.
    fn shorter_than(
        &self,
        lexer: &Lexer,
        text: &str,
        pos: usize,
        hit_end: usize,
        origin: usize,
        end_byte: usize,
        cursor: &mut Cursor,
        log: &mut Vec<Undo>,
    ) -> Option<usize> {
        let prev = lexer::prev_char_boundary(text, hit_end);
        if prev <= pos {
            return None;
        }
        self.search(lexer, text, pos, prev, origin, end_byte, cursor, log)
    }

    fn id_of(&self, lexeme: Lexeme<'_>) -> u32 {
        match lexeme {
            Lexeme::Term(name) => self.id_of_name(name),
            Lexeme::Raw(ch) => self.id_of_char(ch),
        }
    }

    fn id_of_char(&self, ch: char) -> u32 {
        let mut buf = [0u8; 4];
        let text = ch.encode_utf8(&mut buf);
        self.ids.get(text).copied().unwrap_or(UNKNOWN)
    }

    fn action(&self, state: usize, look: Option<u32>) -> Action {
        let Some(actions) = self.term_action.get(state) else {
            return Action::Error;
        };
        match look {
            None => self.eof_action.get(state).copied().unwrap_or(Action::Error),
            Some(id) => actions.get(&id).copied().unwrap_or(Action::Error),
        }
    }

    /// Missing gotos are state 0, matching [`LalrTable`]'s historical lookup.
    fn goto(&self, state: usize, symbol: u32) -> usize {
        self.goto
            .get(state)
            .and_then(|row| row.get(&symbol).copied())
            .unwrap_or(0) as usize
    }

    fn eof_accepts(&self, cursor: &mut Cursor, log: &mut Vec<Undo>) -> bool {
        let mark = log.len();
        let accepted = matches!(self.drive(cursor, None, log), Ok(Drive::Accepted));
        undo_to(cursor, log, mark);
        accepted
    }

    /// Reduce while `look` says so, then shift it or accept. `Err` leaves
    /// the logged edits in place; the caller undoes them.
    fn drive(
        &self,
        cursor: &mut Cursor,
        look: Option<u32>,
        log: &mut Vec<Undo>,
    ) -> Result<Drive, ()> {
        loop {
            let state = match cursor.states.last() {
                Some(&state) => state,
                None => return Err(()),
            };
            match self.action(state, look) {
                Action::Reduce(prod) => {
                    if !cursor.reduce_logged(self, prod, log) {
                        return Err(());
                    }
                }
                Action::Shift(next) => {
                    let Some(id) = look else {
                        return Err(());
                    };
                    cursor.push_sym(id, log);
                    cursor.push_state(next, log);
                    return Ok(Drive::Shifted);
                }
                Action::Accept => {
                    if cursor.syms.last().copied() == Some(self.start_id) {
                        return Ok(Drive::Accepted);
                    }
                    return Err(());
                }
                Action::Error => return Err(()),
            }
        }
    }
}

impl Cursor {
    fn new() -> Self {
        Self {
            states: vec![0],
            syms: Vec::new(),
        }
    }

    fn push_state(&mut self, state: usize, log: &mut Vec<Undo>) {
        self.states.push(state);
        log.push(Undo::PopState);
    }

    fn push_sym(&mut self, symbol: u32, log: &mut Vec<Undo>) {
        self.syms.push(symbol);
        log.push(Undo::PopSym);
    }

    fn reduce(&mut self, exec: &Exec, prod: usize) -> bool {
        let len = exec.prod_rhs_len[prod];
        if self.syms.len() < len || self.states.len() < len {
            return false;
        }
        for _ in 0..len {
            self.syms.pop();
            self.states.pop();
        }
        let state = match self.states.last() {
            Some(&state) => state,
            None => return false,
        };
        let lhs = exec.prod_lhs[prod];
        let next = exec.goto(state, lhs);
        self.syms.push(lhs);
        self.states.push(next);
        true
    }

    fn reduce_logged(&mut self, exec: &Exec, prod: usize, log: &mut Vec<Undo>) -> bool {
        let len = exec.prod_rhs_len[prod];
        if self.syms.len() < len || self.states.len() < len {
            return false;
        }
        for _ in 0..len {
            let symbol = self.syms.pop().expect("length checked");
            let state = self.states.pop().expect("length checked");
            log.push(Undo::PushSym(symbol));
            log.push(Undo::PushState(state));
        }
        let state = match self.states.last() {
            Some(&state) => state,
            None => return false,
        };
        let lhs = exec.prod_lhs[prod];
        let next = exec.goto(state, lhs);
        self.push_sym(lhs, log);
        self.push_state(next, log);
        true
    }
}

fn undo_to(cursor: &mut Cursor, log: &mut Vec<Undo>, mark: usize) {
    while log.len() > mark {
        match log.pop().expect("mark is within the log") {
            Undo::PopState => {
                cursor.states.pop();
            }
            Undo::PopSym => {
                cursor.syms.pop();
            }
            Undo::PushState(state) => cursor.states.push(state),
            Undo::PushSym(symbol) => cursor.syms.push(symbol),
        }
    }
}

/// Characters that can begin a non-empty derivation of `start`, after class
/// expansion. `None` when that set is open-ended.
pub(crate) fn first_chars(prods: &[(Symbol, Vec<Symbol>)], start: &str) -> Option<HashSet<char>> {
    let mut nullable: HashSet<Symbol> = HashSet::new();
    let mut first: HashMap<Symbol, HashSet<Symbol>> = HashMap::new();
    loop {
        let (before_null, before_first) = (nullable.clone(), first.clone());
        for (lhs, rhs) in prods {
            let mut all_nullable = true;
            for sym in rhs {
                match sym {
                    Symbol::Terminal(_) => {
                        first.entry(lhs.clone()).or_default().insert(sym.clone());
                        all_nullable = false;
                        break;
                    }
                    Symbol::End => {
                        all_nullable = false;
                        break;
                    }
                    Symbol::NonTerminal(_) => {
                        if let Some(set) = first.get(sym).cloned() {
                            first.entry(lhs.clone()).or_default().extend(set);
                        }
                        if !nullable.contains(sym) {
                            all_nullable = false;
                            break;
                        }
                    }
                }
            }
            if all_nullable {
                nullable.insert(lhs.clone());
            }
        }
        if nullable == before_null && first == before_first {
            break;
        }
    }

    let start_sym = Symbol::NonTerminal(start.to_string());
    let Some(set) = first.get(&start_sym) else {
        return Some(HashSet::new());
    };
    let mut chars = HashSet::new();
    for sym in set {
        let Symbol::Terminal(name) = sym else {
            continue;
        };
        match lexer::terminal_start_chars(name) {
            None => return None,
            Some(more) => chars.extend(more),
        }
    }
    Some(chars)
}

/// Search accelerators derived from a conflict-free grammar.
struct Accel {
    /// Literals that occur in every non-empty derivation, longest first.
    required: Vec<String>,
    /// Longest string every possible first literal starts with.
    prefix: String,
    /// Set when every non-empty yield is exactly one of these literals,
    /// longest first.
    literals: Option<Vec<String>>,
    /// Literal terminals that can end a non-empty yield. `None` when a
    /// non-literal terminal can be last, so a suffix check would be wrong.
    suffixes: Option<Vec<String>>,
    /// Leading bytes of FIRST. `None` when FIRST is open-ended.
    first_bytes: Option<ByteSet>,
}

impl Accel {
    fn analyze(
        prods: &[(Symbol, Vec<Symbol>)],
        start: &str,
        first_chars: Option<&HashSet<char>>,
    ) -> Self {
        let grouped = group_productions(prods);
        let nullable = nullable_names(prods);
        let required = required_literals(prods, start, &nullable);
        let front = edge_literals(prods, &nullable, false);
        let back = edge_literals(prods, &nullable, true);
        let prefix = match front.get(start) {
            Some(LitSet::Closed(set)) => longest_common_prefix(set),
            _ => String::new(),
        };
        let suffixes = match back.get(start) {
            Some(LitSet::Open) => None,
            Some(LitSet::Closed(set)) => Some(set.iter().cloned().collect()),
            None => Some(Vec::new()),
        };
        Self {
            required,
            prefix,
            literals: pure_literals(&grouped, start),
            suffixes,
            first_bytes: first_chars.map(ByteSet::from_first_chars),
        }
    }
}

fn fast_may_start(text: &str, start: usize, first_bytes: Option<ByteSet>) -> bool {
    if start >= text.len() {
        return false;
    }
    match first_bytes {
        None => !lexer::char_at(text, start).is_whitespace(),
        Some(bits) => bits.contains(text.as_bytes()[start]),
    }
}

fn missing_literal(required: &[String], text: &str) -> bool {
    required.iter().any(|lit| !text.contains(lit.as_str()))
}

/// Longest literal in `lits` (already longest-first) that matches at `at`.
fn longest_literal_at(text: &str, at: usize, lits: &[String]) -> Option<usize> {
    let rest = text.get(at..)?;
    for lit in lits {
        if rest.starts_with(lit.as_str()) {
            return Some(at + lit.len());
        }
    }
    None
}

/// Leftmost start at which any literal matches, then the longest literal there.
fn find_literals(text: &str, lits: &[String]) -> Option<(usize, usize)> {
    if lits.is_empty() {
        return None;
    }
    if lits.len() == 1 {
        let lit = &lits[0];
        return text.find(lit.as_str()).map(|at| (at, at + lit.len()));
    }
    if lits.iter().all(|lit| lit.len() == 1 && lit.is_ascii()) {
        let mut bits = ByteSet::new();
        for lit in lits {
            bits.insert(lit.as_bytes()[0]);
        }
        return text
            .bytes()
            .position(|byte| bits.contains(byte))
            .map(|at| (at, at + 1));
    }
    if lits.len() <= 8 {
        let mut best: Option<usize> = None;
        for lit in lits {
            if let Some(at) = text.find(lit.as_str()) {
                best = Some(best.map_or(at, |have| have.min(at)));
            }
        }
        let at = best?;
        return longest_literal_at(text, at, lits).map(|end| (at, end));
    }
    let mut bits = ByteSet::new();
    for lit in lits {
        bits.insert(lit.as_bytes()[0]);
    }
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bits.contains(bytes[index])
            && text.is_char_boundary(index)
            && let Some(end) = longest_literal_at(text, index, lits)
        {
            return Some((index, end));
        }
        index += 1;
    }
    None
}

fn group_productions(prods: &[(Symbol, Vec<Symbol>)]) -> HashMap<&str, Vec<&[Symbol]>> {
    let mut map: HashMap<&str, Vec<&[Symbol]>> = HashMap::new();
    for (lhs, rhs) in prods {
        if let Symbol::NonTerminal(name) = lhs {
            map.entry(name.as_str()).or_default().push(rhs.as_slice());
        }
    }
    map
}

fn nullable_names(prods: &[(Symbol, Vec<Symbol>)]) -> HashSet<String> {
    let mut nullable = HashSet::new();
    loop {
        let before = nullable.len();
        for (lhs, rhs) in prods {
            let Symbol::NonTerminal(name) = lhs else {
                continue;
            };
            if nullable.contains(name) {
                continue;
            }
            let all = rhs.iter().all(|sym| match sym {
                Symbol::NonTerminal(n) => nullable.contains(n),
                _ => false,
            });
            if all {
                nullable.insert(name.clone());
            }
        }
        if nullable.len() == before {
            break;
        }
    }
    nullable
}

#[derive(Clone)]
struct Req {
    /// `true` when the symbol has a finite non-empty yield.
    nonempty: bool,
    /// Literals in every non-empty yield. Empty means those yields share
    /// no single literal.
    must: HashSet<String>,
}

fn required_literals(
    prods: &[(Symbol, Vec<Symbol>)],
    start: &str,
    nullable: &HashSet<String>,
) -> Vec<String> {
    let mut universe = HashSet::new();
    let mut names = HashSet::new();
    for (lhs, rhs) in prods {
        if let Symbol::NonTerminal(name) = lhs {
            names.insert(name.clone());
        }
        for sym in rhs {
            if let Symbol::Terminal(name) = sym
                && let Some(lit) = lexer::literal_needle(name)
            {
                universe.insert(lit.to_string());
            }
        }
    }
    // Top of the intersection lattice. Productions shrink it.
    let mut info: HashMap<String, Req> = names
        .into_iter()
        .map(|name| {
            (
                name,
                Req {
                    nonempty: true,
                    must: universe.clone(),
                },
            )
        })
        .collect();
    let grouped = group_productions(prods);
    loop {
        let snapshot = info.clone();
        let mut changed = false;
        for (name, rhss) in &grouped {
            let mut nonempty = false;
            let mut must: Option<HashSet<String>> = None;
            for rhs in rhss {
                let seq = sequence_req(rhs, nullable, &snapshot);
                if !seq.nonempty {
                    continue;
                }
                nonempty = true;
                must = Some(match must {
                    None => seq.must,
                    Some(prev) => {
                        let mut both = prev;
                        both.retain(|lit| seq.must.contains(lit));
                        both
                    }
                });
            }
            let next = if nonempty {
                Req {
                    nonempty: true,
                    must: must.unwrap_or_default(),
                }
            } else {
                Req {
                    nonempty: false,
                    must: HashSet::new(),
                }
            };
            let Some(entry) = info.get_mut(*name) else {
                continue;
            };
            if entry.nonempty != next.nonempty || entry.must != next.must {
                *entry = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let Some(req) = info.get(start) else {
        return Vec::new();
    };
    if !req.nonempty {
        return Vec::new();
    }
    let mut out: Vec<String> = req.must.iter().cloned().collect();
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    out
}

/// Literals forced by one production's non-empty yields.
///
/// A non-nullable symbol is in every yield, so its literals are required.
/// When every symbol is nullable, a non-empty yield can be any one of them,
/// so only the intersection is required. `S -> N` with `N` nullable therefore
/// requires whatever `N` requires, which is how `([a|b])*` avoids demanding
/// both `a` and `|`.
fn sequence_req(rhs: &[Symbol], nullable: &HashSet<String>, info: &HashMap<String, Req>) -> Req {
    let mut any_nonempty = false;
    let mut saw_non_nullable = false;
    let mut forced = HashSet::new();
    let mut optional = Vec::new();
    for sym in rhs {
        match sym {
            Symbol::End => {}
            Symbol::Terminal(text) => {
                any_nonempty = true;
                saw_non_nullable = true;
                if let Some(lit) = lexer::literal_needle(text) {
                    forced.insert(lit.to_string());
                }
            }
            Symbol::NonTerminal(name) => {
                let Some(req) = info.get(name) else {
                    continue;
                };
                if !nullable.contains(name) {
                    any_nonempty = true;
                    saw_non_nullable = true;
                    forced.extend(req.must.iter().cloned());
                } else if req.nonempty {
                    any_nonempty = true;
                    optional.push(req.must.clone());
                }
            }
        }
    }
    if !any_nonempty {
        return Req {
            nonempty: false,
            must: HashSet::new(),
        };
    }
    if !saw_non_nullable {
        let mut sets = optional.into_iter();
        forced = sets.next().unwrap_or_default();
        for set in sets {
            forced.retain(|lit| set.contains(lit));
        }
    }
    Req {
        nonempty: true,
        must: forced,
    }
}

#[derive(Clone)]
enum LitSet {
    Open,
    Closed(HashSet<String>),
}

fn edge_literals(
    prods: &[(Symbol, Vec<Symbol>)],
    nullable: &HashSet<String>,
    from_right: bool,
) -> HashMap<String, LitSet> {
    let mut edge: HashMap<String, LitSet> = HashMap::new();
    for (lhs, _) in prods {
        if let Symbol::NonTerminal(name) = lhs {
            edge.entry(name.clone())
                .or_insert_with(|| LitSet::Closed(HashSet::new()));
        }
    }
    loop {
        let mut changed = false;
        for (lhs, rhs) in prods {
            let Symbol::NonTerminal(name) = lhs else {
                continue;
            };
            let contrib = sequence_edge(rhs, nullable, &edge, from_right);
            let entry = edge.get_mut(name).expect("every lhs is seeded");
            if merge_lits(entry, contrib) {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    edge
}

fn merge_lits(entry: &mut LitSet, contrib: LitSet) -> bool {
    match entry {
        LitSet::Open => false,
        LitSet::Closed(_) if matches!(contrib, LitSet::Open) => {
            *entry = LitSet::Open;
            true
        }
        LitSet::Closed(have) => {
            let LitSet::Closed(more) = contrib else {
                return false;
            };
            let before = have.len();
            have.extend(more);
            have.len() != before
        }
    }
}

fn sequence_edge(
    rhs: &[Symbol],
    nullable: &HashSet<String>,
    edge: &HashMap<String, LitSet>,
    from_right: bool,
) -> LitSet {
    if rhs.is_empty() {
        return LitSet::Closed(HashSet::new());
    }
    let mut acc = HashSet::new();
    let last = rhs.len() - 1;
    for step in 0..rhs.len() {
        let sym = if from_right {
            &rhs[last - step]
        } else {
            &rhs[step]
        };
        match sym {
            Symbol::End => {}
            Symbol::Terminal(text) => {
                if let Some(lit) = lexer::literal_needle(text) {
                    acc.insert(lit.to_string());
                    return LitSet::Closed(acc);
                }
                return LitSet::Open;
            }
            Symbol::NonTerminal(name) => {
                match edge.get(name) {
                    Some(LitSet::Open) => return LitSet::Open,
                    Some(LitSet::Closed(set)) => acc.extend(set.iter().cloned()),
                    None => {}
                }
                if !nullable.contains(name) {
                    return LitSet::Closed(acc);
                }
            }
        }
    }
    LitSet::Closed(acc)
}

fn longest_common_prefix(set: &HashSet<String>) -> String {
    let mut iter = set.iter();
    let Some(first) = iter.next() else {
        return String::new();
    };
    let mut prefix = first.clone();
    for other in iter {
        while !other.starts_with(&prefix) {
            prefix.pop();
            if prefix.is_empty() {
                return prefix;
            }
        }
    }
    prefix
}

#[derive(Clone)]
enum Yield {
    Complex,
    One(HashSet<String>),
}

enum Memo {
    Visiting,
    Done(Yield),
}

enum Piece {
    Lit(String),
    Many(HashSet<String>),
}

fn pure_literals(prods: &HashMap<&str, Vec<&[Symbol]>>, start: &str) -> Option<Vec<String>> {
    let mut memo = HashMap::new();
    match yields(start, prods, &mut memo) {
        Yield::Complex => None,
        Yield::One(set) => {
            let mut lits: Vec<String> = set.into_iter().collect();
            lits.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
            Some(lits)
        }
    }
}

fn yields<'a>(
    name: &'a str,
    prods: &HashMap<&'a str, Vec<&'a [Symbol]>>,
    memo: &mut HashMap<&'a str, Memo>,
) -> Yield {
    match memo.get(name) {
        Some(Memo::Visiting) => return Yield::Complex,
        Some(Memo::Done(done)) => return done.clone(),
        None => {}
    }
    memo.insert(name, Memo::Visiting);
    let Some(rhss) = prods.get(name) else {
        let empty = Yield::One(HashSet::new());
        memo.insert(name, Memo::Done(empty.clone()));
        return empty;
    };
    let mut acc = HashSet::new();
    for rhs in rhss {
        let mut pieces = Vec::new();
        let mut complex = false;
        for sym in *rhs {
            match sym {
                Symbol::End => {}
                Symbol::Terminal(text) => match lexer::literal_needle(text) {
                    Some(lit) => pieces.push(Piece::Lit(lit.to_string())),
                    None => {
                        complex = true;
                        break;
                    }
                },
                Symbol::NonTerminal(n) => match yields(n, prods, memo) {
                    Yield::Complex => {
                        complex = true;
                        break;
                    }
                    Yield::One(set) if set.is_empty() => {}
                    Yield::One(set) => pieces.push(Piece::Many(set)),
                },
            }
            if pieces.len() > 1 {
                complex = true;
                break;
            }
        }
        if complex || pieces.len() > 1 {
            memo.insert(name, Memo::Done(Yield::Complex));
            return Yield::Complex;
        }
        match pieces.pop() {
            None => {}
            Some(Piece::Lit(lit)) => {
                acc.insert(lit);
            }
            Some(Piece::Many(set)) => acc.extend(set),
        }
    }
    let done = Yield::One(acc);
    memo.insert(name, Memo::Done(done.clone()));
    done
}
