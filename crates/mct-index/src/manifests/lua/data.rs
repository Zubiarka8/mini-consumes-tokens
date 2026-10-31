//! A reader for the subset of Lua that LuaRocks data files use: rockspecs,
//! tree `manifest` files and `luarocks.lock`.
//!
//! Nothing is evaluated. The reader folds exact constants (literals, tables,
//! `..` of strings, names bound earlier in the same file) and **stops** at the
//! first statement or expression it cannot fold exactly, reporting why and
//! where. Callers must treat everything after a stop as unknown: the text it
//! hides could run code that rewrites any value read so far.

use std::collections::HashMap;

/// Deepest table nesting accepted.
const MAX_DEPTH: usize = 64;
/// Most values (table entries and scalars) one file may produce.
const MAX_VALUES: usize = 250_000;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Value {
    Nil,
    Bool(bool),
    Number(String),
    Str(String),
    Table(Table),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Key {
    /// `{ "a", "b" }`: the 1-based position among positional entries.
    Position(usize),
    /// `name = v` and `["name"] = v`.
    Name(String),
    /// Any other key (`[1] = v`, `[true] = v`).
    Other,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(super) struct Table {
    pub entries: Vec<(Key, Value)>,
}

impl Value {
    pub(super) fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

impl Table {
    /// The value under a string key; the last entry wins, as in Lua.
    pub(super) fn get(&self, name: &str) -> Option<&Value> {
        self.entries
            .iter()
            .rev()
            .find_map(|(key, value)| match key {
                Key::Name(k) if k == name => Some(value),
                _ => None,
            })
    }

    pub(super) fn positional(&self) -> impl Iterator<Item = &Value> {
        self.entries.iter().filter_map(|(key, value)| match key {
            Key::Position(_) => Some(value),
            _ => None,
        })
    }

    /// String-keyed entries in source order. A repeated key appears once,
    /// with its last value.
    pub(super) fn named(&self) -> Vec<(&str, &Value)> {
        let mut out: Vec<(&str, &Value)> = Vec::new();
        for (key, value) in &self.entries {
            if let Key::Name(name) = key {
                match out.iter_mut().find(|(k, _)| *k == name) {
                    Some(slot) => slot.1 = value,
                    None => out.push((name, value)),
                }
            }
        }
        out
    }

    pub(super) fn has_other_keys(&self) -> bool {
        self.entries.iter().any(|(key, _)| *key == Key::Other)
    }
}

/// Why reading stopped before the end of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stop {
    pub reason: String,
    pub line: usize,
}

impl Stop {
    fn new(reason: impl Into<String>, line: usize) -> Self {
        Self {
            reason: reason.into(),
            line,
        }
    }
}

/// What was folded from one file.
#[derive(Debug, Default)]
pub(super) struct Chunk {
    /// Top-level names and their folded values, in the state reached when
    /// reading ended.
    pub env: HashMap<String, Value>,
    /// The value of a top-level `return`, for `luarocks.lock`.
    pub returned: Option<Value>,
    /// `Some` when the whole file was not folded exactly.
    pub stop: Option<Stop>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Str(String),
    Number(String),
    Punct(u8),
    Concat,
    Vararg,
}

/// Reads `source` without evaluating it.
pub(super) fn read(source: &str) -> Chunk {
    let (tokens, lex_error) = lex(source);
    let mut parser = Parser {
        tokens,
        pos: 0,
        env: HashMap::new(),
        returned: None,
        values: 0,
        depth: 0,
    };
    let mut stop = parser.chunk().err();
    if stop.is_none() {
        stop = lex_error;
    }
    Chunk {
        env: parser.env,
        returned: parser.returned,
        stop,
    }
}

// ---------------------------------------------------------------- lexer

fn lex(source: &str) -> (Vec<(Tok, usize)>, Option<Stop>) {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut line = 1;
    if bytes.first() == Some(&b'#') {
        while i < bytes.len() && bytes[i] != b'\n' {
            i += 1;
        }
    }
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\n' => {
                line += 1;
                i += 1;
            }
            b' ' | b'\t' | b'\r' | 0x0b | 0x0c => i += 1,
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i += 2;
                if let Some(level) = long_bracket_level(bytes, i) {
                    match read_long(bytes, i, level, &mut line) {
                        Some((_, next)) => i = next,
                        None => {
                            return (tokens, Some(Stop::new("unterminated long comment", line)))
                        }
                    }
                } else {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
            }
            b'"' | b'\'' => match read_quoted(bytes, i, &mut line) {
                Ok((text, next)) => {
                    tokens.push((Tok::Str(text), line));
                    i = next;
                }
                Err(reason) => return (tokens, Some(Stop::new(reason, line))),
            },
            b'[' if long_bracket_level(bytes, i).is_some() => {
                let level = long_bracket_level(bytes, i).unwrap_or(0);
                let start_line = line;
                match read_long(bytes, i, level, &mut line) {
                    Some((text, next)) => {
                        tokens.push((Tok::Str(text), start_line));
                        i = next;
                    }
                    None => return (tokens, Some(Stop::new("unterminated long string", line))),
                }
            }
            b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let d = bytes[i];
                    let hex =
                        source[start..].starts_with("0x") || source[start..].starts_with("0X");
                    let exponent = if hex {
                        matches!(bytes[i - 1], b'p' | b'P')
                    } else {
                        matches!(bytes[i - 1], b'e' | b'E')
                    };
                    let exponent_sign = (d == b'+' || d == b'-') && exponent;
                    if d.is_ascii_alphanumeric() || d == b'.' || d == b'_' || exponent_sign {
                        i += 1;
                    } else {
                        break;
                    }
                }
                tokens.push((Tok::Number(source[start..i].to_string()), line));
            }
            b'.' => {
                if bytes.get(i + 1) == Some(&b'.') {
                    if bytes.get(i + 2) == Some(&b'.') {
                        tokens.push((Tok::Vararg, line));
                        i += 3;
                    } else {
                        tokens.push((Tok::Concat, line));
                        i += 2;
                    }
                } else if bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    let start = i;
                    i += 1;
                    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.')
                    {
                        i += 1;
                    }
                    tokens.push((Tok::Number(source[start..i].to_string()), line));
                } else {
                    tokens.push((Tok::Punct(b'.'), line));
                    i += 1;
                }
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                tokens.push((Tok::Name(source[start..i].to_string()), line));
            }
            b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'#' | b'&' | b'~' | b'|' | b'<' | b'>'
            | b'=' | b'(' | b')' | b'{' | b'}' | b'[' | b']' | b';' | b':' | b',' => {
                tokens.push((Tok::Punct(c), line));
                i += 1;
            }
            _ => return (tokens, Some(Stop::new("unexpected character", line))),
        }
    }
    (tokens, None)
}

/// `[`, any number of `=`, `[` at `i`: the number of `=`.
fn long_bracket_level(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes.get(i) != Some(&b'[') {
        return None;
    }
    let mut level = 0;
    while bytes.get(i + 1 + level) == Some(&b'=') {
        level += 1;
    }
    (bytes.get(i + 1 + level) == Some(&b'[')).then_some(level)
}

/// The text of a long bracket starting at `start` and the index after it.
fn read_long(
    bytes: &[u8],
    start: usize,
    level: usize,
    line: &mut usize,
) -> Option<(String, usize)> {
    let mut i = start + level + 2;
    if bytes.get(i) == Some(&b'\r') {
        i += 1;
    }
    if bytes.get(i) == Some(&b'\n') {
        *line += 1;
        i += 1;
    }
    let body_start = i;
    while i < bytes.len() {
        if bytes[i] == b']' {
            let closes = (1..=level).all(|k| bytes.get(i + k) == Some(&b'='))
                && bytes.get(i + level + 1) == Some(&b']');
            if closes {
                let text = String::from_utf8_lossy(&bytes[body_start..i]).into_owned();
                return Some((text, i + level + 2));
            }
        }
        if bytes[i] == b'\n' {
            *line += 1;
        }
        i += 1;
    }
    None
}

fn read_quoted(bytes: &[u8], start: usize, line: &mut usize) -> Result<(String, usize), String> {
    let quote = bytes[start];
    let mut out: Vec<u8> = Vec::new();
    let mut i = start + 1;
    while i < bytes.len() {
        let c = bytes[i];
        if c == quote {
            return Ok((String::from_utf8_lossy(&out).into_owned(), i + 1));
        }
        match c {
            b'\n' => return Err("unterminated string".to_string()),
            b'\\' => {
                i += 1;
                let Some(&e) = bytes.get(i) else {
                    return Err("unterminated string".to_string());
                };
                match e {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'a' => out.push(0x07),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0c),
                    b'v' => out.push(0x0b),
                    b'\\' | b'"' | b'\'' => out.push(e),
                    b'\n' => {
                        *line += 1;
                        out.push(b'\n');
                    }
                    b'z' => {
                        while bytes.get(i + 1).is_some_and(u8::is_ascii_whitespace) {
                            if bytes[i + 1] == b'\n' {
                                *line += 1;
                            }
                            i += 1;
                        }
                    }
                    b'x' => {
                        let hex = bytes
                            .get(i + 1..i + 3)
                            .and_then(|h| std::str::from_utf8(h).ok());
                        let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) else {
                            return Err("invalid \\x escape".to_string());
                        };
                        out.push(byte);
                        i += 2;
                    }
                    b'u' => {
                        if bytes.get(i + 1) != Some(&b'{') {
                            return Err("invalid \\u escape".to_string());
                        }
                        let Some(close) = bytes[i + 2..].iter().position(|b| *b == b'}') else {
                            return Err("invalid \\u escape".to_string());
                        };
                        let digits =
                            std::str::from_utf8(&bytes[i + 2..i + 2 + close]).unwrap_or("");
                        let Some(ch) = u32::from_str_radix(digits, 16)
                            .ok()
                            .and_then(char::from_u32)
                        else {
                            return Err("invalid \\u escape".to_string());
                        };
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        i += 2 + close;
                    }
                    b'0'..=b'9' => {
                        let mut value = 0u32;
                        let mut count = 0;
                        while count < 3 && bytes.get(i).is_some_and(u8::is_ascii_digit) {
                            value = value * 10 + u32::from(bytes[i] - b'0');
                            i += 1;
                            count += 1;
                        }
                        i -= 1;
                        let Ok(byte) = u8::try_from(value) else {
                            return Err("decimal escape too large".to_string());
                        };
                        out.push(byte);
                    }
                    _ => return Err("invalid escape sequence".to_string()),
                }
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    Err("unterminated string".to_string())
}

// --------------------------------------------------------------- parser

const KEYWORDS: [&str; 22] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

struct Parser {
    tokens: Vec<(Tok, usize)>,
    pos: usize,
    env: HashMap<String, Value>,
    returned: Option<Value>,
    values: usize,
    depth: usize,
}

type Step<T> = Result<T, Stop>;

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn peek_at(&self, offset: usize) -> Option<&Tok> {
        self.tokens.get(self.pos + offset).map(|(t, _)| t)
    }

    fn line(&self) -> usize {
        self.tokens
            .get(self.pos)
            .or_else(|| self.tokens.last())
            .map_or(1, |(_, l)| *l)
    }

    fn dynamic<T>(&self, what: &str) -> Step<T> {
        Err(Stop::new(what, self.line()))
    }

    fn is_punct(&self, byte: u8) -> bool {
        self.peek() == Some(&Tok::Punct(byte))
    }

    fn count(&mut self) -> Step<()> {
        self.values += 1;
        if self.values > MAX_VALUES {
            return self.dynamic(&format!("more than {MAX_VALUES} values"));
        }
        Ok(())
    }

    fn chunk(&mut self) -> Step<()> {
        while let Some(tok) = self.peek().cloned() {
            match tok {
                Tok::Punct(b';') => self.pos += 1,
                Tok::Name(name) if name == "local" => {
                    self.pos += 1;
                    self.local()?;
                }
                Tok::Name(name) if name == "return" => {
                    self.pos += 1;
                    let value = self.expr()?;
                    if self.is_punct(b';') {
                        self.pos += 1;
                    }
                    if self.peek().is_some() {
                        return self.dynamic("text after `return`");
                    }
                    self.returned = Some(value);
                }
                Tok::Name(name) if KEYWORDS.contains(&name.as_str()) => {
                    return self.dynamic(&format!("statement `{name}` is not data"));
                }
                Tok::Name(_) => self.assignment()?,
                _ => return self.dynamic("unexpected token"),
            }
        }
        Ok(())
    }

    fn name(&mut self) -> Step<String> {
        match self.peek().cloned() {
            Some(Tok::Name(n)) if !KEYWORDS.contains(&n.as_str()) => {
                self.pos += 1;
                Ok(n)
            }
            _ => self.dynamic("expected a name"),
        }
    }

    fn local(&mut self) -> Step<()> {
        let name = self.name()?;
        if self.is_punct(b',') || self.is_punct(b'<') {
            return self.dynamic("multiple or attributed local declaration");
        }
        if self.is_punct(b'=') && self.peek_at(1) != Some(&Tok::Punct(b'=')) {
            self.pos += 1;
            let value = self.expr()?;
            self.env.insert(name, value);
        } else {
            self.env.insert(name, Value::Nil);
        }
        Ok(())
    }

    fn assignment(&mut self) -> Step<()> {
        let first = self.name()?;
        let mut path: Vec<String> = Vec::new();
        loop {
            if self.is_punct(b'.') {
                self.pos += 1;
                path.push(self.name()?);
            } else if self.is_punct(b'[') {
                if let (Some(Tok::Str(key)), Some(Tok::Punct(b']'))) =
                    (self.peek_at(1).cloned(), self.peek_at(2))
                {
                    self.pos += 3;
                    path.push(key);
                } else {
                    return self.dynamic("assignment to a computed key");
                }
            } else {
                break;
            }
        }
        if !self.is_punct(b'=') || self.peek_at(1) == Some(&Tok::Punct(b'=')) {
            return self.dynamic("call or unsupported statement");
        }
        self.pos += 1;
        let value = self.expr()?;
        if path.is_empty() {
            self.env.insert(first, value);
            return Ok(());
        }
        let line = self.line();
        let Some(Value::Table(mut table)) = self.env.remove(&first) else {
            return Err(Stop::new("assignment into an unknown table", line));
        };
        let result = set_path(&mut table, &path, value);
        self.env.insert(first, Value::Table(table));
        result.map_err(|reason| Stop::new(reason, line))
    }

    fn expr(&mut self) -> Step<Value> {
        let mut value = self.operand()?;
        while self.peek() == Some(&Tok::Concat) {
            self.pos += 1;
            let right = self.operand()?;
            value = match (value, right) {
                (Value::Str(a), Value::Str(b)) => Value::Str(a + &b),
                _ => return self.dynamic("concatenation of non-constant values"),
            };
        }
        match self.peek() {
            None | Some(Tok::Punct(b';' | b',' | b'}' | b']')) => Ok(value),
            Some(Tok::Name(n)) if n != "and" && n != "or" => Ok(value),
            _ => self.dynamic("call, index or operator in an expression"),
        }
    }

    fn operand(&mut self) -> Step<Value> {
        self.count()?;
        let Some(tok) = self.peek().cloned() else {
            return self.dynamic("missing expression");
        };
        self.pos += 1;
        match tok {
            Tok::Str(s) => Ok(Value::Str(s)),
            Tok::Number(n) => Ok(Value::Number(n)),
            Tok::Punct(b'-') => match self.peek().cloned() {
                Some(Tok::Number(n)) => {
                    self.pos += 1;
                    Ok(Value::Number(format!("-{n}")))
                }
                _ => self.dynamic("unary minus on a non-number"),
            },
            Tok::Punct(b'{') => self.table(),
            Tok::Name(n) => match n.as_str() {
                "nil" => Ok(Value::Nil),
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ if KEYWORDS.contains(&n.as_str()) => {
                    self.pos -= 1;
                    self.dynamic(&format!("`{n}` in an expression"))
                }
                _ => match self.env.get(&n) {
                    Some(v) => Ok(v.clone()),
                    None => {
                        self.pos -= 1;
                        self.dynamic(&format!("name `{n}` has no constant value"))
                    }
                },
            },
            _ => {
                self.pos -= 1;
                self.dynamic("unsupported expression")
            }
        }
    }

    fn table(&mut self) -> Step<Value> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.dynamic(&format!("tables nested deeper than {MAX_DEPTH}"));
        }
        let result = self.table_body();
        self.depth -= 1;
        result
    }

    fn table_body(&mut self) -> Step<Value> {
        let mut table = Table::default();
        let mut position = 0;
        loop {
            self.count()?;
            if self.is_punct(b'}') {
                self.pos += 1;
                return Ok(Value::Table(table));
            }
            let key = if self.is_punct(b'[') && self.peek_at(1) != Some(&Tok::Punct(b'[')) {
                self.pos += 1;
                let key = self.expr()?;
                if !self.is_punct(b']') {
                    return self.dynamic("expected `]`");
                }
                self.pos += 1;
                if !self.is_punct(b'=') {
                    return self.dynamic("expected `=` after a key");
                }
                self.pos += 1;
                Some(match key {
                    Value::Str(s) => Key::Name(s),
                    _ => Key::Other,
                })
            } else if matches!(self.peek(), Some(Tok::Name(n)) if !KEYWORDS.contains(&n.as_str()))
                && self.peek_at(1) == Some(&Tok::Punct(b'='))
                && self.peek_at(2) != Some(&Tok::Punct(b'='))
            {
                let name = self.name()?;
                self.pos += 1;
                Some(Key::Name(name))
            } else {
                None
            };
            let value = self.expr()?;
            let key = key.unwrap_or_else(|| {
                position += 1;
                Key::Position(position)
            });
            table.entries.push((key, value));
            if self.is_punct(b',') || self.is_punct(b';') {
                self.pos += 1;
            } else if !self.is_punct(b'}') {
                return self.dynamic("expected `,` or `}` in a table");
            }
        }
    }
}

/// Assigns `value` at `path` inside `table`, creating only the last key.
fn set_path(table: &mut Table, path: &[String], value: Value) -> Result<(), String> {
    let Some((head, rest)) = path.split_first() else {
        return Ok(());
    };
    if rest.is_empty() {
        match table
            .entries
            .iter_mut()
            .rev()
            .find(|(k, _)| matches!(k, Key::Name(n) if n == head))
        {
            Some(slot) => slot.1 = value,
            None => table.entries.push((Key::Name(head.clone()), value)),
        }
        return Ok(());
    }
    match table
        .entries
        .iter_mut()
        .rev()
        .find(|(k, _)| matches!(k, Key::Name(n) if n == head))
    {
        Some((_, Value::Table(inner))) => set_path(inner, rest, value),
        _ => Err("assignment into an unknown table".to_string()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn string(v: &Value) -> &str {
        v.as_str().unwrap()
    }

    fn table(v: &Value) -> &Table {
        match v {
            Value::Table(t) => t,
            other => panic!("not a table: {other:?}"),
        }
    }

    #[test]
    fn folds_constants_comments_and_every_string_form() {
        let chunk = read(
            "-- c\n--[==[ long\ncomment ]==]\nlocal v = \"1.0\"\nname = 'a' .. v\n\
             text = [[ x\ndependencies = {} ]]\nesc = \"a\\n\\x41\\65\\u{e9}\\z   b\"\n\
             t = { \"p\"; q = 1, [\"a.b\"] = [=[z]=], }\n",
        );
        assert!(chunk.stop.is_none(), "{:?}", chunk.stop);
        assert_eq!(string(&chunk.env["name"]), "a1.0");
        assert_eq!(string(&chunk.env["text"]), " x\ndependencies = {} ");
        assert_eq!(string(&chunk.env["esc"]), "a\nAAéb");
        let t = table(&chunk.env["t"]);
        assert_eq!(t.positional().count(), 1);
        assert_eq!(string(t.get("a.b").unwrap()), "z");
    }

    #[test]
    fn dotted_assignment_folds_into_an_existing_table() {
        let chunk = read("build = { modules = {} }\nbuild.modules.x = \"x.lua\"\nbuild[\"type\"] = \"builtin\"\n");
        assert!(chunk.stop.is_none());
        let build = table(&chunk.env["build"]);
        let modules = table(build.get("modules").unwrap());
        assert_eq!(string(modules.get("x").unwrap()), "x.lua");
        assert_eq!(string(build.get("type").unwrap()), "builtin");
    }

    #[test]
    fn code_stops_reading_and_keeps_what_was_folded_before() {
        for source in [
            "a = \"x\"\nif jit then b = 1 end\n",
            "a = \"x\"\nb = f()\n",
            "a = \"x\"\nt = {}\nt[#t + 1] = \"y\"\n",
            "a = \"x\"\nb = a .. other\n",
            "a = \"x\"\nb = 1 + 2\n",
            "a = \"x\"\nfunction g() end\n",
            "a = \"x\"\nb = unknown\n",
            "a = \"x\"\nb = ...\n",
        ] {
            let chunk = read(source);
            assert!(chunk.stop.is_some(), "{source}");
            assert_eq!(string(&chunk.env["a"]), "x", "{source}");
        }
    }

    #[test]
    fn syntax_problems_are_stops_not_panics() {
        for source in [
            "a = \"open",
            "a = [[open",
            "a = {",
            "a = ",
            "a = 'x\\q'",
            "a = $",
        ] {
            assert!(read(source).stop.is_some(), "{source}");
        }
    }

    #[test]
    fn deep_nesting_is_a_stop() {
        let deep = format!("a = {}{}", "{".repeat(200), "}".repeat(200));
        let chunk = read(&deep);
        assert!(chunk.stop.unwrap().reason.contains("deeper"));
    }

    #[test]
    fn return_table_is_read_for_lockfiles() {
        let chunk = read("return { dependencies = { lpeg = \"1.1.0-2\" } }\n");
        assert!(chunk.stop.is_none());
        let returned = chunk.returned.unwrap();
        let deps = table(&returned).get("dependencies").unwrap();
        assert_eq!(string(table(deps).get("lpeg").unwrap()), "1.1.0-2");
    }
}
