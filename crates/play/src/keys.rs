//! Desktop key names to the managed `KeyCode` values the native input queue carries.

/// The engine's keys, in the order of `UnityEngine.KeyCode` after `None`, with the
/// desktop host's logical name for each.
const KEYS: [&str; 33] = [
    "space",
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "arrowup",
    "arrowdown",
    "arrowleft",
    "arrowright",
    "enter",
    "escape",
];

/// The managed `KeyCode` for a desktop key name, if the engine delivers that key.
#[must_use]
pub fn key_code(name: &str) -> Option<u16> {
    let index = KEYS.iter().position(|key| *key == name)?;
    u16::try_from(index + 1).ok()
}

#[cfg(test)]
mod tests {
    use super::{KEYS, key_code};

    /// The managed enum is the source of truth; its declaration must list the same keys.
    #[test]
    fn names_follow_the_managed_key_code_order() {
        let source = include_str!("../../../csharp/Disunity/Input.cs");
        let start = source.find("public enum KeyCode").expect("KeyCode enum");
        let body = &source[source[start..].find('{').expect("enum body") + start + 1..];
        let members: Vec<&str> = body[..body.find('}').expect("enum end")]
            .split(',')
            .map(str::trim)
            .filter(|member| !member.is_empty())
            .collect();
        let expected: Vec<String> = KEYS
            .iter()
            .map(|key| match *key {
                "space" => "Space".to_owned(),
                "arrowup" => "UpArrow".to_owned(),
                "arrowdown" => "DownArrow".to_owned(),
                "arrowleft" => "LeftArrow".to_owned(),
                "arrowright" => "RightArrow".to_owned(),
                "enter" => "Return".to_owned(),
                "escape" => "Escape".to_owned(),
                letter => letter.to_uppercase(),
            })
            .collect();
        assert_eq!(members[0], "None");
        assert_eq!(members[1..], expected);
        assert_eq!(key_code("space"), Some(1));
        assert_eq!(key_code("w"), Some(24));
        assert_eq!(key_code("escape"), Some(33));
        assert_eq!(key_code("shift"), None);
    }
}
