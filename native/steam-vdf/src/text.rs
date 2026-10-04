//! Text VDF (`configset_controller_*.vdf`, `appmanifest_*.acf`, `loginusers.vdf`).
//!
//! Parsing is order- and duplicate-preserving. Numbers stay strings (text VDF
//! has no types). Comments and original whitespace are not preserved.

use crate::{Entry, Value, VdfError};

const MAX_DEPTH: usize = 64;

struct Parser<'a> {
    src: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.pos).copied()
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_whitespace() => self.pos += 1,
                Some(b'/') if self.src.as_bytes().get(self.pos + 1) == Some(&b'/') => {
                    while !matches!(self.peek(), Some(b'\n') | None) {
                        self.pos += 1;
                    }
                }
                _ => return,
            }
        }
    }

    fn token(&mut self) -> Result<String, VdfError> {
        if self.peek() == Some(b'"') {
            self.pos += 1;
            let mut out = String::new();
            let mut chars = self.src[self.pos..].char_indices();
            while let Some((i, c)) = chars.next() {
                match c {
                    '"' => {
                        self.pos += i + 1;
                        return Ok(out);
                    }
                    '\\' => match chars.next() {
                        Some((_, 'n')) => out.push('\n'),
                        Some((_, 't')) => out.push('\t'),
                        Some((_, '\\')) => out.push('\\'),
                        Some((_, '"')) => out.push('"'),
                        // Unknown escape: keep it verbatim, as Steam does for paths.
                        Some((_, other)) => {
                            out.push('\\');
                            out.push(other);
                        }
                        None => break,
                    },
                    c => out.push(c),
                }
            }
            return Err(VdfError::Syntax { offset: self.src.len(), message: "unterminated string" });
        }
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() || matches!(b, b'{' | b'}' | b'"') {
                break;
            }
            self.pos += 1;
        }
        if start == self.pos {
            return Err(VdfError::Syntax { offset: start, message: "expected a key" });
        }
        Ok(self.src[start..self.pos].to_owned())
    }

    fn map(&mut self, depth: usize, nested: bool) -> Result<Vec<Entry>, VdfError> {
        if depth > MAX_DEPTH {
            return Err(VdfError::TooDeep { offset: self.pos, max: MAX_DEPTH });
        }
        let mut out = Vec::new();
        loop {
            self.skip_trivia();
            match self.peek() {
                None if nested => {
                    return Err(VdfError::Syntax { offset: self.pos, message: "missing closing brace" })
                }
                None => return Ok(out),
                Some(b'}') if nested => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'}') => {
                    return Err(VdfError::Syntax { offset: self.pos, message: "unexpected closing brace" })
                }
                Some(b'{') => {
                    return Err(VdfError::Syntax { offset: self.pos, message: "unexpected opening brace" })
                }
                Some(_) => {}
            }
            let key = self.token()?;
            self.skip_trivia();
            if self.peek() == Some(b'{') {
                self.pos += 1;
                out.push(Entry::new(key, Value::Map(self.map(depth + 1, true)?)));
            } else if self.peek().is_none() {
                return Err(VdfError::Syntax { offset: self.pos, message: "key without value" });
            } else if matches!(self.peek(), Some(b'}')) {
                return Err(VdfError::Syntax { offset: self.pos, message: "key without value" });
            } else {
                out.push(Entry::new(key, Value::Str(self.token()?)));
            }
        }
    }
}

/// Parses a text VDF document into its root entries (a leading BOM is skipped).
pub fn parse(src: &str) -> Result<Vec<Entry>, VdfError> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    Parser { src, pos: 0 }.map(0, false)
}

fn escape(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_map(entries: &[Entry], depth: usize, out: &mut String) {
    let indent = "\t".repeat(depth);
    for Entry { key, value } in entries {
        out.push_str(&indent);
        escape(key, out);
        match value {
            Value::Map(children) => {
                out.push('\n');
                out.push_str(&indent);
                out.push_str("{\n");
                write_map(children, depth + 1, out);
                out.push_str(&indent);
                out.push_str("}\n");
            }
            Value::Str(s) => {
                out.push_str("\t\t");
                escape(s, out);
                out.push('\n');
            }
            Value::U32(n) => {
                out.push_str("\t\t");
                escape(&n.to_string(), out);
                out.push('\n');
            }
        }
    }
}

/// Serializes entries as text VDF (tab-indented, `\n` line endings).
pub fn write(root: &[Entry]) -> String {
    let mut out = String::new();
    write_map(root, 0, &mut out);
    out
}
