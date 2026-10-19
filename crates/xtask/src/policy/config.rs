use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

pub const FILE: &str = "crates/xtask/policy/explicit.toml";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub types: Vec<Registered>,
    pub literal_scopes: Vec<String>,
    pub owners: Vec<Owner>,
    pub coverage: Vec<Coverage>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Dto,
    Runtime,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registered {
    pub name: String,
    pub file: String,
    pub role: Role,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub file: String,
    pub symbol: String,
    pub reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coverage {
    pub dto: String,
    pub field: String,
    pub test: String,
}

pub fn read(root: &Path) -> Result<Config, String> {
    let text =
        std::fs::read_to_string(root.join(FILE)).map_err(|e| format!("cannot read {FILE}: {e}"))?;
    let config: Config = toml::from_str(&text).map_err(|e| format!("invalid {FILE}: {e}"))?;
    if config.types.is_empty() || config.literal_scopes.is_empty() {
        return Err("explicit policy registry and literal scopes must not be empty".into());
    }
    let mut names = BTreeSet::new();
    for entry in &config.types {
        syn::parse_str::<syn::Ident>(&entry.name)
            .map_err(|e| format!("invalid registered identifier: {e}"))?;
        if !names.insert(&entry.name) {
            return Err(format!("duplicate registered type {}", entry.name));
        }
        super::source::contained(root, &entry.file)?;
    }
    for file in &config.literal_scopes {
        super::source::contained(root, file)?;
    }
    let mut owners = BTreeSet::new();
    for owner in &config.owners {
        if !config.literal_scopes.contains(&owner.file)
            || owner.reason.trim().is_empty()
            || !owners.insert((&owner.file, &owner.symbol))
        {
            return Err(format!(
                "invalid or duplicate literal owner {}::{}",
                owner.file, owner.symbol
            ));
        }
    }
    Ok(config)
}
