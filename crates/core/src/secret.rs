//! Secret values (passwords, UUIDs, REALITY keys, short IDs, obfs passwords).
//! `Secret` never prints its value via `Debug`/`Display` and is not
//! serializable, so it cannot leak into logs, errors or the UI by accident.
//! The only ways out are [`Secret::expose`] (for config generation / keychain)
//! and [`Secret::masked`] (for the UI).

use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw value. Only for writing sing-box configs and the keychain.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// UI-safe form, see [`mask`].
    pub fn masked(&self) -> String {
        mask(&self.0)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(••••)")
    }
}

/// Masked form for display: first 4 characters + `…••••` if the value has at
/// least 8 characters, otherwise just `••••` (empty stays empty).
pub fn mask(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value.chars().count() >= 8 {
        let head: String = value.chars().take(4).collect();
        format!("{head}…••••")
    } else {
        "••••".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_value() {
        let s = Secret::new("super-secret-password");
        assert_eq!(format!("{s:?}"), "Secret(••••)");
        assert!(!format!("{:?}", Some(s)).contains("super"));
    }

    #[test]
    fn masking() {
        assert_eq!(mask("a1b2c3d4e5"), "a1b2…••••");
        assert_eq!(mask("short"), "••••");
        assert_eq!(mask(""), "");
        assert_eq!(mask("密码密码密码密码"), "密码密码…••••");
    }
}
