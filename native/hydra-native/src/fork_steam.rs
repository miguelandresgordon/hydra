//! Fork: napi bindings over the pure `steam-vdf` crate.
//!
//! Trees cross the JS boundary as JSON strings (`{ type, value }` nodes, see
//! `steam_vdf::Value`) so key order and sparse indexes are preserved.

use std::fs;
use std::path::Path;

use napi::bindgen_prelude::Error;
use napi_derive::napi;
use steam_vdf::{binary, text, Entry};

fn err(context: impl std::fmt::Display, cause: impl std::fmt::Display) -> Error {
    Error::from_reason(format!("{context}: {cause}"))
}

fn to_json(tree: &[Entry]) -> napi::Result<String> {
    serde_json::to_string(tree).map_err(|e| err("could not serialize VDF tree", e))
}

fn from_json(json: &str) -> napi::Result<Vec<Entry>> {
    serde_json::from_str(json).map_err(|e| err("invalid VDF tree JSON", e))
}

/// Reads and parses a binary `shortcuts.vdf`, returning the tree as JSON.
#[napi]
pub fn steam_shortcuts_read(path: String) -> napi::Result<String> {
    let bytes = fs::read(&path).map_err(|e| err(format!("could not read {path}"), e))?;
    let tree = binary::parse(&bytes).map_err(|e| err(format!("could not parse {path}"), e))?;
    to_json(&tree)
}

/// Serializes a tree (JSON) to binary VDF, verifies it reparses to the same
/// tree, then replaces `path` atomically (tmp file + rename).
///
/// Backups and Steam-running checks are the caller's responsibility.
#[napi]
pub fn steam_shortcuts_write(path: String, tree_json: String) -> napi::Result<()> {
    let tree = from_json(&tree_json)?;
    let bytes = binary::write(&tree).map_err(|e| err("could not serialize shortcuts", e))?;
    let reparsed = binary::parse(&bytes).map_err(|e| err("serialized shortcuts failed validation", e))?;
    if reparsed != tree {
        return Err(Error::from_reason(
            "serialized shortcuts failed validation: reparsed tree differs".to_string(),
        ));
    }

    let target = Path::new(&path);
    let mut tmp_name = target.as_os_str().to_owned();
    tmp_name.push(".hydra.tmp");
    let tmp = Path::new(&tmp_name);
    let write_tmp = || -> std::io::Result<()> {
        let file = fs::File::create(tmp)?;
        std::io::Write::write_all(&mut &file, &bytes)?;
        file.sync_all()?;
        fs::rename(tmp, target)
    };
    write_tmp().map_err(|e| {
        let _ = fs::remove_file(tmp);
        err(format!("could not write {path}"), e)
    })
}

/// Parses text VDF (configset, `.acf`, `loginusers.vdf`) into a JSON tree.
#[napi]
pub fn steam_vdf_text_parse(content: String) -> napi::Result<String> {
    let tree = text::parse(&content).map_err(|e| err("could not parse text VDF", e))?;
    to_json(&tree)
}

/// Serializes a JSON tree as text VDF.
#[napi]
pub fn steam_vdf_text_write(tree_json: String) -> napi::Result<String> {
    Ok(text::write(&from_json(&tree_json)?))
}
