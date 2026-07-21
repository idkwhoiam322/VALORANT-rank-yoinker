use serde_json::Value;

/// Extract the first string value from `v` across multiple possible field names.
/// Returns `None` if none of the keys hold a string.
pub(crate) fn first_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(s) = v.get(*key).and_then(|v| v.as_str()) {
            return Some(s);
        }
    }
    None
}
