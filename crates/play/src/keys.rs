//! Desktop key names to the managed `KeyCode` values the native input queue carries.

/// Keys the desktop host names outside the letter, digit and function-key runs: the host's
/// logical name, the managed `KeyCode` member and its standard value. The host reports the
/// logical key without its side, so a modifier maps to the left key (the engine's choice).
const NAMED: [(&str, &str, u16); 33] = [
    ("space", "Space", 32),
    ("enter", "Return", 13),
    ("escape", "Escape", 27),
    ("backspace", "Backspace", 8),
    ("tab", "Tab", 9),
    ("delete", "Delete", 127),
    ("insert", "Insert", 277),
    ("home", "Home", 278),
    ("end", "End", 279),
    ("pageup", "PageUp", 280),
    ("pagedown", "PageDown", 281),
    ("arrowup", "UpArrow", 273),
    ("arrowdown", "DownArrow", 274),
    ("arrowright", "RightArrow", 275),
    ("arrowleft", "LeftArrow", 276),
    ("shift", "LeftShift", 304),
    ("control", "LeftControl", 306),
    ("alt", "LeftAlt", 308),
    ("capslock", "CapsLock", 301),
    ("numlock", "Numlock", 300),
    ("scrolllock", "ScrollLock", 302),
    ("pause", "Pause", 19),
    ("-", "Minus", 45),
    ("=", "Equals", 61),
    ("[", "LeftBracket", 91),
    ("]", "RightBracket", 93),
    ("\\", "Backslash", 92),
    (";", "Semicolon", 59),
    ("'", "Quote", 39),
    (",", "Comma", 44),
    (".", "Period", 46),
    ("/", "Slash", 47),
    ("`", "BackQuote", 96),
];

/// The managed `KeyCode` for a desktop key name, if the engine delivers that key.
#[must_use]
pub fn key_code(name: &str) -> Option<u16> {
    if let Some((_, _, code)) = NAMED.iter().find(|(host, _, _)| *host == name) {
        return Some(*code);
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        return match first {
            'a'..='z' => Some(97 + u16::try_from(u32::from(first) - u32::from('a')).ok()?),
            '0'..='9' => Some(48 + u16::try_from(u32::from(first) - u32::from('0')).ok()?),
            _ => None,
        };
    }
    let number: u16 = name.strip_prefix('f')?.parse().ok()?;
    (1..=12).contains(&number).then_some(281 + number)
}

#[cfg(test)]
mod tests {
    use super::{NAMED, key_code};
    use std::collections::BTreeMap;

    /// The managed enum is the source of truth: its numeric `Name = value` members.
    fn managed_members() -> BTreeMap<String, u16> {
        let source = include_str!("../../../csharp/Disunity/KeyCode.cs");
        let start = source.find("public enum KeyCode").expect("KeyCode enum");
        let body = &source[source[start..].find('{').expect("enum body") + start + 1..];
        body[..body.find('}').expect("enum end")]
            .split(',')
            .filter_map(|member| {
                let (name, value) = member.split_once('=')?;
                Some((name.trim().to_owned(), value.trim().parse().ok()?))
            })
            .collect()
    }

    fn expect_member(host: &str, member: &str, members: &BTreeMap<String, u16>) {
        let value = members
            .get(member)
            .unwrap_or_else(|| panic!("{member} is not a KeyCode member"));
        assert_eq!(
            key_code(host),
            Some(*value),
            "{host} should map to {member}"
        );
    }

    #[test]
    fn named_keys_map_to_the_managed_values() {
        let members = managed_members();
        for (host, member, _) in NAMED {
            expect_member(host, member, &members);
        }
    }

    #[test]
    fn letters_digits_and_function_keys_map_to_the_managed_values() {
        let members = managed_members();
        for letter in 'a'..='z' {
            expect_member(
                &letter.to_string(),
                &letter.to_ascii_uppercase().to_string(),
                &members,
            );
        }
        for digit in 0..=9 {
            expect_member(&digit.to_string(), &format!("Alpha{digit}"), &members);
        }
        for number in 1..=12 {
            expect_member(&format!("f{number}"), &format!("F{number}"), &members);
        }
    }

    #[test]
    fn standard_values_are_the_ones_games_carry() {
        assert_eq!(key_code("enter"), Some(13));
        assert_eq!(key_code("a"), Some(97));
        assert_eq!(key_code("arrowup"), Some(273));
        assert_eq!(key_code("shift"), Some(304));
        assert_eq!(key_code("f1"), Some(282));
    }

    #[test]
    fn names_the_engine_does_not_deliver_map_to_nothing() {
        for name in ["", "unidentified", "f0", "f13", "ff", "A", "10", "meta"] {
            assert_eq!(key_code(name), None, "{name:?}");
        }
    }
}
