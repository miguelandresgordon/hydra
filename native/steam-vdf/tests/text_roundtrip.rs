use steam_vdf::{text, Entry, Value, VdfError};

const SAMPLE: &str = "\u{feff}// comment\n\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"name\" \"Quote \\\" and \\\\ path\"\n\t\"dup\" \"1\"\n\t\"dup\" \"2\"\n\tUserConfig { language english }\n}\n";

#[test]
fn parses_nested_duplicates_and_escapes() {
    let tree = text::parse(SAMPLE).unwrap();
    assert_eq!(tree.len(), 1);
    let Value::Map(app) = &tree[0].value else { panic!("not a map") };
    assert_eq!(app[1].value, Value::Str("Quote \" and \\ path".into()));
    assert_eq!(app.iter().filter(|e| e.key == "dup").count(), 2);
}

#[test]
fn write_then_parse_is_identity() {
    let tree = text::parse(SAMPLE).unwrap();
    let out = text::write(&tree);
    assert_eq!(text::parse(&out).unwrap(), tree);
    // writing is stable
    assert_eq!(text::write(&text::parse(&out).unwrap()), out);
}

#[test]
fn syntax_errors_carry_offsets() {
    assert!(matches!(text::parse("\"a\" {"), Err(VdfError::Syntax { .. })));
    assert!(matches!(text::parse("\"a\" \"b"), Err(VdfError::Syntax { .. })));
    assert!(matches!(text::parse("}"), Err(VdfError::Syntax { offset: 0, .. })));
    assert!(matches!(text::parse("\"a\""), Err(VdfError::Syntax { .. })));
}

#[test]
fn u32_values_are_written_as_strings() {
    let out = text::write(&[Entry::new("n", Value::U32(7))]);
    assert_eq!(text::parse(&out).unwrap()[0].value, Value::Str("7".into()));
}
