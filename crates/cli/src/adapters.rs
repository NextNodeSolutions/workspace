use workspace_core::port::{Profile, Provider};
use workspace_profile_agent::{Claude, Pi};
use workspace_provider_boat::Boat;

use crate::config::{Profiles, Providers};

type Registry<F> = &'static [(&'static str, F)];

const PROVIDERS: Registry<fn(Providers) -> Box<dyn Provider>> =
    &[("boat", |options| Box::new(Boat::new(options.boat)))];
const PROFILES: Registry<fn(Profiles) -> Box<dyn Profile>> = &[
    ("pi", |options| Box::new(Pi::new(options.pi))),
    ("claude", |options| Box::new(Claude::new(options.claude))),
];

#[derive(Debug, thiserror::Error)]
#[error("unknown {kind} `{name}`; available: {available}")]
pub struct UnknownAdapter {
    kind: &'static str,
    name: String,
    available: String,
}

#[derive(Debug, thiserror::Error)]
#[error("several providers exist, so pick one with --provider: {available}")]
pub struct SeveralProviders {
    available: String,
}

pub fn sole_provider() -> Result<String, SeveralProviders> {
    match PROVIDERS {
        [(name, _)] => Ok((*name).to_owned()),
        _ => Err(SeveralProviders {
            available: list_names(PROVIDERS),
        }),
    }
}

pub fn provider(name: &str, options: Providers) -> Result<Box<dyn Provider>, UnknownAdapter> {
    let create = find("provider", PROVIDERS, name)?;
    Ok(create(options))
}

pub fn profile(name: &str, options: Profiles) -> Result<Box<dyn Profile>, UnknownAdapter> {
    let create = find("profile", PROFILES, name)?;
    Ok(create(options))
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
