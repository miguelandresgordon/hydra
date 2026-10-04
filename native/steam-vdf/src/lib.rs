//! Lossless Steam VDF codec.
//!
//! Both formats are modelled as an ordered tree ([`Value`]) so that entry
//! order, sparse indexes and duplicate keys survive a parse/write round trip.

pub mod binary;
pub mod error;
pub mod shortcut;
pub mod text;

pub use error::VdfError;

use serde::{Deserialize, Serialize};

/// A VDF value. Maps keep insertion order and may repeat keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
pub enum Value {
    Map(Vec<Entry>),
    Str(String),
    U32(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub value: Value,
}

impl Entry {
    pub fn new(key: impl Into<String>, value: Value) -> Self {
        Self { key: key.into(), value }
    }
}
