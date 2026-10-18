//! The adapters this binary can plug into the core. Adding one is an adapter
//! crate, a dependency in this crate's Cargo.toml and one line in a registry.

use workspace_core::port::{Profile, Provider};

type Registry<T> = &'static [(&'static str, fn() -> T)];

const PROVIDERS: Registry<Box<dyn Provider>> = &[];
const PROFILES: Registry<Box<dyn Profile>> = &[];

#[derive(Debug, thiserror::Error)]
#[error("unknown {kind} `{name}`; available: {available}")]
pub struct UnknownAdapter {
    kind: &'static str,
    name: String,
    available: String,
}

pub fn provider(name: &str) -> Result<Box<dyn Provider>, UnknownAdapter> {
    build("provider", PROVIDERS, name)
}

pub fn profile(name: &str) -> Result<Box<dyn Profile>, UnknownAdapter> {
    build("profile", PROFILES, name)
}

fn build<T>(kind: &'static str, registry: Registry<T>, name: &str) -> Result<T, UnknownAdapter> {
    let entry = registry.iter().find(|(key, _)| *key == name);
    entry
        .map(|(_, create)| create())
        .ok_or_else(|| UnknownAdapter {
            kind,
            name: name.to_owned(),
            available: list_names(registry),
        })
}

fn list_names<T>(registry: Registry<T>) -> String {
    if registry.is_empty() {
        return "none".to_owned();
    }
    let names: Vec<&str> = registry.iter().map(|(key, _)| *key).collect();
    names.join(", ")
}
