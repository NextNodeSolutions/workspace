use workspace_core::port::{Profile, Provider};
use workspace_provider_boat::Boat;

use crate::config::Providers;

type Registry<F> = &'static [(&'static str, F)];

const PROVIDERS: Registry<fn(Providers) -> Box<dyn Provider>> =
    &[("boat", |options| Box::new(Boat::new(options.boat)))];
const PROFILES: Registry<fn() -> Box<dyn Profile>> = &[];

#[derive(Debug, thiserror::Error)]
#[error("unknown {kind} `{name}`; available: {available}")]
pub struct UnknownAdapter {
    kind: &'static str,
    name: String,
    available: String,
}

#[derive(Debug, thiserror::Error)]
pub enum NoProvider {
    #[error("no provider chosen: run `workspace setup`, or pass --provider")]
    NotSetUp,
    #[error("several providers exist, so pick one with --provider: {available}")]
    Several { available: String },
}

pub fn sole_provider() -> Result<String, NoProvider> {
    match PROVIDERS {
        [(name, _)] => Ok((*name).to_owned()),
        _ => Err(NoProvider::Several {
            available: list_names(PROVIDERS),
        }),
    }
}

pub fn provider(name: &str, options: Providers) -> Result<Box<dyn Provider>, UnknownAdapter> {
    let create = find("provider", PROVIDERS, name)?;
    Ok(create(options))
}

pub fn profile(name: &str) -> Result<Box<dyn Profile>, UnknownAdapter> {
    let create = find("profile", PROFILES, name)?;
    Ok(create())
}

fn find<F: Copy>(
    kind: &'static str,
    registry: Registry<F>,
    name: &str,
) -> Result<F, UnknownAdapter> {
    let entry = registry.iter().find(|(key, _)| *key == name);
    entry
        .map(|&(_, create)| create)
        .ok_or_else(|| UnknownAdapter {
            kind,
            name: name.to_owned(),
            available: list_names(registry),
        })
}

fn list_names<F>(registry: Registry<F>) -> String {
    if registry.is_empty() {
        return "none".to_owned();
    }
    let names: Vec<&str> = registry.iter().map(|(key, _)| *key).collect();
    names.join(", ")
}
