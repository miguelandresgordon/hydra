use std::fs;
use std::path::PathBuf;

use steam_vdf::shortcut::shortcuts_mut;
use steam_vdf::{binary, Entry, Value, VdfError};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    fs::read(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

#[test]
fn real_and_synthetic_fixtures_roundtrip_byte_for_byte() {
    for name in [
        "real-13-bytes.vdf",
        "real-mac-empty.vdf",
        "200-entries.vdf",
        "sparse-indexes.vdf",
    ] {
        let original = fixture(name);
        let tree = binary::parse(&original).unwrap_or_else(|e| panic!("{name}: {e}"));
        let written = binary::write(&tree).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(written, original, "{name} is not byte-identical");
    }
}

#[test]
fn sparse_indexes_are_preserved() {
    let tree = binary::parse(&fixture("sparse-indexes.vdf")).unwrap();
    let mut tree = tree;
    let keys: Vec<_> = shortcuts_mut(&mut tree).unwrap().iter().map(|e| e.key.clone()).collect();
    assert_eq!(keys, ["0", "5", "99"]);
}

#[test]
fn utf8_and_quotes_survive() {
    let tree = binary::parse(&fixture("200-entries.vdf")).unwrap();
    let json = serde_json::to_string(&tree).unwrap();
    assert!(json.contains("ñandú"));
    assert!(json.contains("\\\"x\\\""));
}

#[test]
fn unknown_type_reports_offset() {
    // root: 0x07 is not a supported tag
    assert_eq!(
        binary::parse(&[0x07, b'a', 0, 0, 0, 0, 0, 0, 0, 0, 0x08]),
        Err(VdfError::UnknownType { offset: 0, tag: 0x07 })
    );
    let mut nested = fixture("real-13-bytes.vdf");
    nested.insert(11, 0x07);
    assert!(matches!(binary::parse(&nested), Err(VdfError::UnknownType { tag: 0x07, .. })));
}

#[test]
fn truncated_input_errors_at_every_length() {
    let original = fixture("sparse-indexes.vdf");
    for len in 0..original.len() {
        assert!(binary::parse(&original[..len]).is_err(), "prefix of {len} bytes parsed");
    }
}

#[test]
fn trailing_bytes_and_invalid_utf8_are_errors() {
    let mut data = fixture("real-13-bytes.vdf");
    data.push(0);
    assert!(matches!(binary::parse(&data), Err(VdfError::TrailingBytes { .. })));
    assert!(matches!(
        binary::parse(&[0x01, 0xff, 0, 0, 0x08]),
        Err(VdfError::InvalidUtf8 { offset: 1 })
    ));
}

#[test]
fn excessive_nesting_is_rejected_not_overflowed() {
    let mut data = Vec::new();
    for _ in 0..10_000 {
        data.extend_from_slice(&[0x00, b'a', 0]);
    }
    assert!(matches!(binary::parse(&data), Err(VdfError::TooDeep { .. })));
}

#[test]
fn nul_in_strings_is_rejected_on_write() {
    let tree = vec![Entry::new("a\0b", Value::U32(1))];
    assert!(matches!(binary::write(&tree), Err(VdfError::EmbeddedNul { .. })));
}

#[test]
fn backup_edit_write_reparse_flow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shortcuts.vdf");
    let original = fixture("sparse-indexes.vdf");
    fs::write(&path, &original).unwrap();

    let mut tree = binary::parse(&fs::read(&path).unwrap()).unwrap();
    for e in shortcuts_mut(&mut tree).unwrap()[0].value_map_mut() {
        if e.key == "AppName" {
            e.value = Value::Str("Renombrado".into());
        }
    }
    let bytes = binary::write(&tree).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert_eq!(binary::parse(&fs::read(&path).unwrap()).unwrap(), tree);
}

trait ValueMapMut {
    fn value_map_mut(&mut self) -> &mut Vec<Entry>;
}
impl ValueMapMut for Entry {
    fn value_map_mut(&mut self) -> &mut Vec<Entry> {
        match &mut self.value {
            Value::Map(m) => m,
            _ => panic!("not a map"),
        }
    }
}
