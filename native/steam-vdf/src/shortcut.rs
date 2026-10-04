//! Helpers for the `shortcuts` map of `shortcuts.vdf`.

use crate::{Entry, Value, VdfError};

/// Returns the entries of the top-level `shortcuts` map (key is case-insensitive).
pub fn shortcuts_mut(root: &mut [Entry]) -> Result<&mut Vec<Entry>, VdfError> {
    root.iter_mut()
        .find_map(|e| match &mut e.value {
            Value::Map(m) if e.key.eq_ignore_ascii_case("shortcuts") => Some(m),
            _ => None,
        })
        .ok_or(VdfError::NoShortcuts)
}
