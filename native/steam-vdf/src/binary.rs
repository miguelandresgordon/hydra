//! Binary VDF (`shortcuts.vdf`): tags 0x00 map, 0x01 string, 0x02 u32, 0x08 end.

use crate::{Entry, Value, VdfError};

const TAG_MAP: u8 = 0x00;
const TAG_STR: u8 = 0x01;
const TAG_U32: u8 = 0x02;
const TAG_END: u8 = 0x08;
const MAX_DEPTH: usize = 64;

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, VdfError> {
        let b = *self
            .buf
            .get(self.pos)
            .ok_or(VdfError::UnexpectedEof { offset: self.pos })?;
        self.pos += 1;
        Ok(b)
    }

    fn cstr(&mut self) -> Result<String, VdfError> {
        let start = self.pos;
        let rest = self.buf.get(start..).unwrap_or(&[]);
        let len = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(VdfError::UnexpectedEof { offset: self.buf.len() })?;
        let s = std::str::from_utf8(&rest[..len])
            .map_err(|_| VdfError::InvalidUtf8 { offset: start })?;
        self.pos = start + len + 1;
        Ok(s.to_owned())
    }

    fn u32(&mut self) -> Result<u32, VdfError> {
        let end = self.pos.checked_add(4).ok_or(VdfError::UnexpectedEof { offset: self.pos })?;
        let s = self
            .buf
            .get(self.pos..end)
            .ok_or(VdfError::UnexpectedEof { offset: self.pos })?;
        self.pos = end;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn map(&mut self, depth: usize) -> Result<Vec<Entry>, VdfError> {
        if depth > MAX_DEPTH {
            return Err(VdfError::TooDeep { offset: self.pos, max: MAX_DEPTH });
        }
        let mut out = Vec::new();
        loop {
            let tag_offset = self.pos;
            match self.byte()? {
                TAG_END => return Ok(out),
                TAG_MAP => {
                    let key = self.cstr()?;
                    out.push(Entry::new(key, Value::Map(self.map(depth + 1)?)));
                }
                TAG_STR => {
                    let key = self.cstr()?;
                    out.push(Entry::new(key, Value::Str(self.cstr()?)));
                }
                TAG_U32 => {
                    let key = self.cstr()?;
                    out.push(Entry::new(key, Value::U32(self.u32()?)));
                }
                tag => return Err(VdfError::UnknownType { offset: tag_offset, tag }),
            }
        }
    }
}

/// Parses a binary VDF document into its root map.
pub fn parse(buf: &[u8]) -> Result<Vec<Entry>, VdfError> {
    let mut reader = Reader { buf, pos: 0 };
    let root = reader.map(0)?;
    if reader.pos != buf.len() {
        return Err(VdfError::TrailingBytes { offset: reader.pos });
    }
    Ok(root)
}

fn push_cstr(s: &str, context: &str, out: &mut Vec<u8>) -> Result<(), VdfError> {
    if s.as_bytes().contains(&0) {
        return Err(VdfError::EmbeddedNul { context: context.to_owned() });
    }
    out.extend_from_slice(s.as_bytes());
    out.push(0);
    Ok(())
}

fn write_map(entries: &[Entry], out: &mut Vec<u8>) -> Result<(), VdfError> {
    for Entry { key, value } in entries {
        match value {
            Value::Map(children) => {
                out.push(TAG_MAP);
                push_cstr(key, key, out)?;
                write_map(children, out)?;
            }
            Value::Str(s) => {
                out.push(TAG_STR);
                push_cstr(key, key, out)?;
                push_cstr(s, key, out)?;
            }
            Value::U32(n) => {
                out.push(TAG_U32);
                push_cstr(key, key, out)?;
                out.extend_from_slice(&n.to_le_bytes());
            }
        }
    }
    out.push(TAG_END);
    Ok(())
}

/// Serializes a root map. Fails only if a key or string contains a NUL byte.
pub fn write(root: &[Entry]) -> Result<Vec<u8>, VdfError> {
    let mut out = Vec::new();
    write_map(root, &mut out)?;
    Ok(out)
}
