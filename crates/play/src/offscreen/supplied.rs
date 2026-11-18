//! A value a run supplies for one named global, from `play --global NAME=v0,v1,v2,v3`.
//!
//! The value goes into the globals a frame starts from as a fallback, as the host's main light
//! does (see `add_main_light`): a name the world's globals already hold keeps its value, and a
//! `SetGlobal` the script records replaces it from the place in the frame where it is recorded.
//! A value is a vector of one to four floats; the components the option leaves out are 0. A run
//! that supplies none is unchanged.

use std::fmt;

use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;

use super::builtins::{MAIN_LIGHT_COLOR, MAIN_LIGHT_POSITION};

/// The most floats a supplied value holds.
const MOST_COMPONENTS: usize = 4;

/// One named global and the vector a run gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct SuppliedGlobal {
    name: String,
    value: [f32; MOST_COMPONENTS],
    /// How many leading components of `value` the option listed.
    listed: usize,
}

impl SuppliedGlobal {
    /// Reads `NAME=v0,v1,v2,v3` with one to four finite floats; the error says what is wrong.
    ///
    /// # Errors
    /// Fails when the text has no `=`, the name is not an identifier (letters, digits and `_`,
    /// not starting with a digit), or the value is not one to four finite floats.
    pub fn parse(text: &str) -> Result<Self, String> {
        let (name, values) = text
            .split_once('=')
            .ok_or_else(|| "it is not NAME=v0,v1,v2,v3".to_owned())?;
        let name = name.trim();
        let identifier = name
            .chars()
            .all(|character| character == '_' || character.is_ascii_alphanumeric())
            && !name.starts_with(|character: char| character.is_ascii_digit());
        if name.is_empty() || !identifier {
            return Err(format!(
                "the name `{name}` is not letters, digits and `_` that do not start with a digit"
            ));
        }
        let mut value = [0.0; MOST_COMPONENTS];
        let mut listed = 0;
        for component in values.split(',') {
            let number = component
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|number| number.is_finite())
                .ok_or_else(|| format!("{name}: `{}` is not a finite number", component.trim()))?;
            *value.get_mut(listed).ok_or_else(|| {
                format!("{name}: a value has 1 to {MOST_COMPONENTS} floats, this has more")
            })? = number;
            listed += 1;
        }
        Ok(Self {
            name: name.to_owned(),
            value,
            listed,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value as the vector the global holds, with 0 after the listed components.
    #[must_use]
    pub fn value(&self) -> [f32; MOST_COMPONENTS] {
        self.value
    }
}

/// `unity_LightData=(1, 1, 1, 1)`: the name and the components the option listed.
impl fmt::Display for SuppliedGlobal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let listed: Vec<String> = self.value[..self.listed]
            .iter()
            .map(f32::to_string)
            .collect();
        write!(formatter, "{}=({})", self.name, listed.join(", "))
    }
}

/// Adds each of `supplied` to `layer` as a vector, unless the layer already holds that name.
pub fn add_supplied(layer: &mut PropertyLayer, supplied: &[SuppliedGlobal]) {
    for global in supplied {
        if layer.values.get(&global.name).is_none() {
            layer
                .values
                .set(&global.name, PropertyValue::Vector(global.value));
        }
    }
}

/// The names the host can put in a frame's starting globals: its main light and the globals the
/// run supplies. A shader's declared default for one of them is read only when no global has it.
#[must_use]
pub fn host_filled(supplied: &[SuppliedGlobal]) -> Vec<&str> {
    [MAIN_LIGHT_POSITION, MAIN_LIGHT_COLOR]
        .into_iter()
        .chain(supplied.iter().map(SuppliedGlobal::name))
        .collect()
}

/// The summary line a run that supplies globals prints, or `None` when it supplies none.
#[must_use]
pub fn supplied_line(supplied: &[SuppliedGlobal]) -> Option<String> {
    if supplied.is_empty() {
        return None;
    }
    let listed: Vec<String> = supplied.iter().map(ToString::to_string).collect();
    Some(format!(
        "globals supplied for this run: {}",
        listed.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::{SuppliedGlobal, add_supplied, supplied_line};
    use worldrunner_render_program::layers::PropertyLayer;
    use worldrunner_render_program::properties::PropertyValue;

    fn parsed(text: &str) -> SuppliedGlobal {
        SuppliedGlobal::parse(text).expect("parses")
    }

    /// The components of `text`'s value, compared as the exact floats the text names.
    fn assert_value(text: &str, expected: [f32; 4]) {
        assert_eq!(
            parsed(text).value().map(f32::to_bits),
            expected.map(f32::to_bits),
            "{text}"
        );
    }

    #[test]
    fn four_floats_name_a_whole_vector() {
        let global = parsed("unity_LightData=1,1,1,1");
        assert_eq!(global.name(), "unity_LightData");
        assert_value("unity_LightData=1,1,1,1", [1.0; 4]);
        assert_eq!(global.to_string(), "unity_LightData=(1, 1, 1, 1)");
    }

    #[test]
    fn one_to_four_floats_fill_the_leading_components_and_the_rest_are_zero() {
        assert_value("a=2", [2.0, 0.0, 0.0, 0.0]);
        assert_value("a=2,3", [2.0, 3.0, 0.0, 0.0]);
        assert_value("a=2,3,4", [2.0, 3.0, 4.0, 0.0]);
        assert_value("a=2,3,4,5", [2.0, 3.0, 4.0, 5.0]);
        assert_eq!(parsed("a=2,3").to_string(), "a=(2, 3)");
    }

    #[test]
    fn signs_fractions_exponents_and_spaces_are_numbers() {
        assert_value("_Name9 = -0.5, 1e2 ,.25", [-0.5, 100.0, 0.25, 0.0]);
        assert_eq!(parsed("_Name9=1").name(), "_Name9");
    }

    #[test]
    fn a_malformed_value_is_refused_by_what_is_wrong() {
        for text in ["", "novalue", "=1,2", "1abc=1", "a b=1", "a-b=1", "a.b=1"] {
            assert!(SuppliedGlobal::parse(text).is_err(), "`{text}`");
        }
        let name = SuppliedGlobal::parse("1abc=1").unwrap_err();
        assert!(name.contains("1abc"), "{name}");
        let none = SuppliedGlobal::parse("a=").unwrap_err();
        assert!(none.contains("not a finite number"), "{none}");
        let word = SuppliedGlobal::parse("unity_LightData=1,one").unwrap_err();
        assert!(
            word.contains("unity_LightData") && word.contains("one"),
            "{word}"
        );
        let many = SuppliedGlobal::parse("a=1,2,3,4,5").unwrap_err();
        assert!(many.contains("1 to 4"), "{many}");
        for text in ["a=1,,2", "a=1,", "a=nan", "a=inf", "a=-inf"] {
            assert!(SuppliedGlobal::parse(text).is_err(), "`{text}`");
        }
    }

    #[test]
    fn a_supplied_value_fills_a_name_the_layer_does_not_hold_and_keeps_one_it_holds() {
        let mut layer = PropertyLayer::default();
        layer
            .values
            .set("held", PropertyValue::Vector([9.0, 9.0, 9.0, 9.0]));
        add_supplied(&mut layer, &[parsed("held=1,1,1,1"), parsed("free=2,3")]);
        assert_eq!(
            layer.values.get("held"),
            Some(&PropertyValue::Vector([9.0; 4]))
        );
        assert_eq!(
            layer.values.get("free"),
            Some(&PropertyValue::Vector([2.0, 3.0, 0.0, 0.0]))
        );
    }

    #[test]
    fn the_summary_line_lists_every_supplied_value_and_is_absent_without_any() {
        assert_eq!(supplied_line(&[]), None);
        assert_eq!(
            supplied_line(&[parsed("unity_LightData=1,1,1,1")]).as_deref(),
            Some("globals supplied for this run: unity_LightData=(1, 1, 1, 1)")
        );
        assert_eq!(
            supplied_line(&[parsed("a=1"), parsed("b=0.5,2")]).as_deref(),
            Some("globals supplied for this run: a=(1), b=(0.5, 2)")
        );
    }
}
