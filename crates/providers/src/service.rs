//! [`Providers`]: the user's providers, each job's main and backup, keys,
//! the Test button, and each job built as one provider with fallback.

use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use recap_engine::{BatchStt, Fallback, FallbackNotice, LiveStt, Llm, ProviderError, ProviderErrorKind};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    ai::cli,
    config::{Choice, Config, ProviderConfig, TranscriptInput},
    job::{Job, Slot, capability_names},
    provider::{Provider, ProviderSettings},
    registry::{Family, Group, Kind, Preset, Registry},
    secrets::{SecretError, Secrets, mask},
};

#[derive(Debug, thiserror::Error)]
pub enum ProvidersError {
    #[error("no provider {0}")]
    UnknownProvider(String),
    #[error("no preset {0}")]
    UnknownPreset(String),
    #[error("{provider} can't do {job}")]
    CantDoJob { provider: String, job: &'static str },
    #[error("{0} always has a main provider")]
    NeedsMain(&'static str),
    #[error("{0} is built in")]
    BuiltIn(String),
    #[error(transparent)]
    Secrets(#[from] SecretError),
    #[error("providers.json: {0}")]
    Io(#[from] std::io::Error),
}

/// A provider as its form has it: saved (`id`) or new (from `preset`).
/// Fields left `None` keep what the provider (or its preset) has; a `key`
/// is the one typed, never shown back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ProviderForm {
    pub id: Option<String>,
    pub preset: String,
    pub name: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub command: Option<String>,
    pub input: Option<TranscriptInput>,
    pub key: Option<String>,
}

impl ProviderForm {
    pub fn new(preset: &str) -> Self {
        Self { preset: preset.into(), ..Default::default() }
    }
}

/// What the Test button shows: ✓ and the time, or the error.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TestOutcome {
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

impl TestOutcome {
    fn failed(error: impl Into<String>) -> Self {
        Self { ok: false, latency_ms: 0, error: Some(error.into()) }
    }
}

struct State {
    config: Config,
    /// The last Test of each saved provider, this run.
    tested: HashMap<String, TestOutcome>,
}

/// The providers' state. A panic while it was held leaves it whole (every
/// change is one assignment or a `retain`), so a poisoned lock is used as is:
/// a meeting must not lose its captions over an earlier panic.
fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The user's providers. Cheap to clone; every clone is the same.
#[derive(Clone)]
pub struct Providers {
    registry: Arc<Registry>,
    secrets: Arc<dyn Secrets>,
    path: Option<PathBuf>,
    state: Arc<Mutex<State>>,
}

/// One provider, resolved: its config, preset and kind.
struct Entry {
    config: ProviderConfig,
    preset: Preset,
    kind: Kind,
}

fn non_empty(s: Option<&String>) -> Option<&str> {
    s.map(String::as_str).filter(|s| !s.is_empty())
}

/// `name` as an id: lowercase words joined by `-`.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// `localhost:11434` of `http://localhost:11434/v1`.
fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or("")
}

impl Providers {
    /// The providers saved at `path` (none: not saved, tests), or the
    /// defaults: ElevenLabs then Deepgram for speech, Claude Code for the
    /// summary.
    pub fn open(registry: Registry, path: Option<PathBuf>, secrets: Arc<dyn Secrets>) -> Self {
        let config = path.as_deref().and_then(Config::load);
        let providers = Self {
            registry: Arc::new(registry),
            secrets,
            path,
            state: Arc::new(Mutex::new(State { config: Config::default(), tested: HashMap::new() })),
        };
        let config = config.unwrap_or_else(|| providers.defaults());
        lock(&providers.state).config = config;
        providers
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    fn defaults(&self) -> Config {
        let mut config = Config::default();
        if let Some(preset) = self.registry.preset("claude_code") {
            config.ai.push(Self::from_preset(preset, preset.id));
        }
        for job in Job::ALL {
            let eligible = self.eligible_in(&config, job);
            let backup = if job.family() == Family::Stt { eligible.get(1).cloned() } else { None };
            config.jobs.insert(job, Choice { main: eligible.first().cloned(), backup });
        }
        config
    }

    fn from_preset(preset: &Preset, id: &str) -> ProviderConfig {
        ProviderConfig {
            id: id.into(),
            preset: preset.id.into(),
            name: preset.name.into(),
            model: preset.model.into(),
            base_url: preset.base_url.into(),
            command: preset.command.into(),
            input: TranscriptInput::Stdin,
        }
    }

    fn speech(&self) -> impl Iterator<Item = &Preset> {
        self.registry.presets().iter().filter(|p| p.group == Group::Speech)
    }

    fn entry_in(&self, config: &Config, id: &str) -> Option<Entry> {
        let (config, preset) = match self.speech().find(|p| p.id == id) {
            Some(preset) => (Self::from_preset(preset, id), *preset),
            None => {
                let config = config.ai.iter().find(|p| p.id == id)?.clone();
                let preset = *self.registry.preset(&config.preset)?;
                (config, preset)
            }
        };
        let kind = *self.registry.kind(preset.kind)?;
        Some(Entry { config, preset, kind })
    }

    fn entry(&self, id: &str) -> Option<Entry> {
        self.entry_in(&lock(&self.state).config, id)
    }

    fn eligible_in(&self, config: &Config, job: Job) -> Vec<String> {
        let ids: Vec<String> = match job.family() {
            Family::Stt => self.speech().map(|p| p.id.to_string()).collect(),
            Family::Ai => config.ai.iter().map(|p| p.id.clone()).collect(),
        };
        ids.into_iter().filter(|id| self.entry_in(config, id).is_some_and(|e| job.takes(&e.kind))).collect()
    }

    /// The providers that can do `job`, in the order they are listed.
    pub fn eligible(&self, job: Job) -> Vec<String> {
        self.eligible_in(&lock(&self.state).config, job)
    }

    /// `job`'s main and backup.
    pub fn choice(&self, job: Job) -> (Option<String>, Option<String>) {
        let state = lock(&self.state);
        let choice = state.config.jobs.get(&job).cloned().unwrap_or_default();
        (choice.main, choice.backup)
    }

    fn persist(&self, config: &Config) -> Result<(), ProvidersError> {
        if let Some(path) = &self.path {
            config.save(path)?;
        }
        Ok(())
    }

    /// Make `id` the main or backup of `job` (`None`: no backup). The main
    /// and backup trade places when one is chosen for the other.
    pub fn set_job(&self, job: Job, slot: Slot, id: Option<&str>) -> Result<(), ProvidersError> {
        let mut state = lock(&self.state);
        if let Some(id) = id
            && !self.eligible_in(&state.config, job).iter().any(|e| e == id)
        {
            return Err(ProvidersError::CantDoJob { provider: id.into(), job: job.label() });
        }
        let choice = state.config.jobs.entry(job).or_default();
        let id = id.map(String::from);
        match slot {
            Slot::Main => {
                let Some(id) = id else { return Err(ProvidersError::NeedsMain(job.label())) };
                if choice.backup.as_ref() == Some(&id) {
                    choice.backup = choice.main.take();
                }
                choice.main = Some(id);
            }
            Slot::Backup => {
                if id.is_some() && id == choice.main {
                    let Some(backup) = choice.backup.take() else {
                        return Err(ProvidersError::NeedsMain(job.label()));
                    };
                    choice.main = Some(backup);
                }
                choice.backup = id;
            }
        }
        self.persist(&state.config)
    }

    /// Save a provider: a new one from its preset, or `form.id` changed.
    /// A speech engine only takes a key. Its key, if typed, goes to the
    /// secret store. Answers its id.
    pub fn save(&self, form: ProviderForm) -> Result<String, ProvidersError> {
        let preset =
            *self.registry.preset(&form.preset).ok_or_else(|| ProvidersError::UnknownPreset(form.preset.clone()))?;
        if preset.group == Group::Speech {
            if let Some(key) = non_empty(form.key.as_ref()) {
                self.secrets.set(preset.id, key)?;
            }
            return Ok(preset.id.into());
        }
        let mut state = lock(&self.state);
        let existing = form.id.as_ref().and_then(|id| state.config.ai.iter().position(|p| &p.id == id));
        let mut config = match existing {
            Some(at) => state.config.ai[at].clone(),
            None => {
                let name = non_empty(form.name.as_ref()).unwrap_or(preset.name);
                let base = if preset.fields.name { slug(name) } else { preset.id.into() };
                let base = if base.is_empty() { preset.id.to_string() } else { base };
                let taken = |id: &str| state.config.ai.iter().any(|p| p.id == id) || self.speech().any(|p| p.id == id);
                let id = (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|id| !taken(id));
                Self::from_preset(&preset, &id.expect("an id is free"))
            }
        };
        if let Some(name) = non_empty(form.name.as_ref()) {
            config.name = name.into();
        }
        for (field, value) in [
            (&mut config.model, &form.model),
            (&mut config.base_url, &form.base_url),
            (&mut config.command, &form.command),
        ] {
            if let Some(value) = value {
                *field = value.trim().into();
            }
        }
        if let Some(input) = form.input {
            config.input = input;
        }
        if let Some(key) = non_empty(form.key.as_ref()) {
            self.secrets.set(&config.id, key.trim())?;
        }
        let id = config.id.clone();
        match existing {
            Some(at) => state.config.ai[at] = config,
            None => state.config.ai.push(config),
        }
        state.tested.remove(&id);
        self.persist(&state.config)?;
        Ok(id)
    }

    /// Set `id`'s key (empty: forget it).
    pub fn set_key(&self, id: &str, key: &str) -> Result<(), ProvidersError> {
        if self.entry(id).is_none() {
            return Err(ProvidersError::UnknownProvider(id.into()));
        }
        match key.trim() {
            "" => self.secrets.delete(id)?,
            key => self.secrets.set(id, key)?,
        }
        lock(&self.state).tested.remove(id);
        Ok(())
    }

    /// Remove an added provider, from its jobs too (the backup steps up
    /// when the main goes), and forget its key.
    pub fn remove(&self, id: &str) -> Result<(), ProvidersError> {
        if self.speech().any(|p| p.id == id) {
            return Err(ProvidersError::BuiltIn(id.into()));
        }
        let mut state = lock(&self.state);
        let before = state.config.ai.len();
        state.config.ai.retain(|p| p.id != id);
        if state.config.ai.len() == before {
            return Err(ProvidersError::UnknownProvider(id.into()));
        }
        for choice in state.config.jobs.values_mut() {
            if choice.backup.as_deref() == Some(id) {
                choice.backup = None;
            }
            if choice.main.as_deref() == Some(id) {
                choice.main = choice.backup.take();
            }
        }
        state.tested.remove(id);
        self.secrets.delete(id)?;
        self.persist(&state.config)
    }

    fn stored_key(&self, id: &str) -> Option<String> {
        self.secrets.get(id).unwrap_or_else(|error| {
            eprintln!("recap: key of {id}: {error}");
            None
        })
    }

    fn settings_of(&self, config: &ProviderConfig, key: Option<String>) -> ProviderSettings {
        ProviderSettings {
            name: config.name.clone(),
            key,
            model: config.model.clone(),
            base_url: config.base_url.clone(),
            command: config.command.clone(),
            input: config.input,
        }
    }

    /// Saved provider `id`'s settings, its key read from the secret store.
    pub fn settings(&self, id: &str) -> Option<ProviderSettings> {
        let entry = self.entry(id)?;
        Some(self.settings_of(&entry.config, self.stored_key(id)))
    }

    /// Build saved provider `id`.
    pub fn build(&self, id: &str) -> Result<Arc<dyn Provider>, ProviderError> {
        let entry =
            self.entry(id).ok_or_else(|| ProviderError::new(ProviderErrorKind::Other, format!("no provider {id}")))?;
        if entry.preset.fields.key && self.stored_key(id).is_none() {
            return Err(ProviderError::new(ProviderErrorKind::Auth, format!("{} has no API key", entry.config.name)));
        }
        Ok((entry.kind.build)(&self.settings_of(&entry.config, self.stored_key(id))))
    }

    /// The Test button: build the provider as `form` has it (a typed key, or
    /// the saved one) and run its test, timed. A saved provider's outcome
    /// shows on its row.
    pub fn test(&self, form: ProviderForm) -> impl Future<Output = TestOutcome> + Send + 'static {
        let prepared = (|| {
            let saved = form.id.as_deref().and_then(|id| self.entry(id));
            let (mut config, preset, kind) = match saved {
                Some(Entry { config, preset, kind }) => (config, preset, kind),
                None => {
                    let preset = *self.registry.preset(&form.preset).ok_or("Unknown provider")?;
                    let kind = *self.registry.kind(preset.kind).ok_or("Unknown provider")?;
                    (Self::from_preset(&preset, preset.id), preset, kind)
                }
            };
            if let Some(name) = non_empty(form.name.as_ref()) {
                config.name = name.into();
            }
            for (field, value) in [
                (&mut config.model, &form.model),
                (&mut config.base_url, &form.base_url),
                (&mut config.command, &form.command),
            ] {
                if let Some(value) = value {
                    *field = value.trim().into();
                }
            }
            if let Some(input) = form.input {
                config.input = input;
            }
            let key = non_empty(form.key.as_ref())
                .map(|k| k.trim().to_string())
                .or_else(|| form.id.as_deref().and_then(|id| self.stored_key(id)));
            if preset.fields.key && key.is_none() {
                return Err("Add an API key");
            }
            Ok((kind.build)(&self.settings_of(&config, key)))
        })();
        let (state, id) = (self.state.clone(), form.id.clone());
        async move {
            let provider = match prepared {
                Ok(provider) => provider,
                Err(error) => return TestOutcome::failed(error),
            };
            let started = Instant::now();
            let result = provider.test().await;
            let latency_ms = started.elapsed().as_millis() as u64;
            let outcome = match result {
                Ok(()) => TestOutcome { ok: true, latency_ms, error: None },
                Err(error) => TestOutcome { ok: false, latency_ms, error: Some(error.message) },
            };
            if let Some(id) = id {
                lock(&state).tested.insert(id, outcome.clone());
            }
            outcome
        }
    }

    /// `job` as one provider: its main's engine, with its backup's (if it
    /// offers one) to fall back to.
    fn job<T: ?Sized>(
        &self,
        job: Job,
        engine: impl Fn(&dyn Provider) -> Option<Arc<T>>,
    ) -> Result<(Arc<T>, Option<Arc<T>>), ProviderError> {
        let (main, backup) = self.choice(job);
        let main = main
            .ok_or_else(|| ProviderError::new(ProviderErrorKind::Other, format!("{} has no provider", job.label())))?;
        let built = self.build(&main)?;
        let main_engine = engine(built.as_ref()).ok_or_else(|| built.not_offered(job.label()))?;
        let backup = backup.and_then(|id| self.build(&id).ok()).and_then(|b| engine(b.as_ref()));
        Ok((main_engine, backup))
    }

    /// Live captions, falling back from main to backup.
    pub fn live(
        &self,
        on_switch: impl Fn(FallbackNotice) + Send + Sync + 'static,
    ) -> Result<Fallback<dyn LiveStt>, ProviderError> {
        let (main, backup) = self.job(Job::Live, |p| p.live())?;
        Ok(Fallback::new(main, backup).on_switch(on_switch))
    }

    /// The after-Stop transcript, falling back from main to backup.
    pub fn batch(
        &self,
        on_switch: impl Fn(FallbackNotice) + Send + Sync + 'static,
    ) -> Result<Fallback<dyn BatchStt>, ProviderError> {
        let (main, backup) = self.job(Job::Transcript, |p| p.batch())?;
        Ok(Fallback::new(main, backup).on_switch(on_switch))
    }

    /// Summary and names, falling back from main to backup.
    pub fn llm(
        &self,
        on_switch: impl Fn(FallbackNotice) + Send + Sync + 'static,
    ) -> Result<Fallback<dyn Llm>, ProviderError> {
        let (main, backup) = self.job(Job::Summary, |p| p.llm())?;
        Ok(Fallback::new(main, backup).on_switch(on_switch))
    }

    /// What the Providers page shows (`ui/host.luau` has the shape). Keys
    /// only masked.
    pub fn view(&self) -> serde_json::Value {
        let state = lock(&self.state);
        let config = &state.config;
        let jobs: Vec<_> = Job::ALL
            .iter()
            .map(|&job| {
                let choice = config.jobs.get(&job).cloned().unwrap_or_default();
                json!({
                    "id": job.id(),
                    "label": job.label(),
                    "family": job.family(),
                    "needs": job.needs_names(),
                    "main": choice.main,
                    "backup": choice.backup,
                })
            })
            .collect();
        let row = |id: &str| -> Option<serde_json::Value> {
            let Entry { config, preset, kind } = self.entry_in(config, id)?;
            let key_mask = if preset.fields.key {
                self.stored_key(id).map(|k| mask(&k)).unwrap_or_default()
            } else {
                String::new()
            };
            let subtitle =
                if kind.id == "cli" { cli::program(&config.command).to_string() } else { config.model.clone() };
            let trailing = if !key_mask.is_empty() {
                key_mask.clone()
            } else if !preset.fields.key && !config.base_url.is_empty() {
                host(&config.base_url).to_string()
            } else {
                String::new()
            };
            Some(json!({
                "id": id,
                "preset": preset.id,
                "kind": kind.id,
                "family": kind.family,
                "name": config.name,
                "logo": preset.logo,
                "capabilities": kind.capabilities,
                "abilities": capability_names(kind.capabilities),
                "fields": preset.fields,
                "key_hint": preset.key_hint,
                "key_mask": key_mask,
                "subtitle": subtitle,
                "trailing": trailing,
                "model": config.model,
                "base_url": config.base_url,
                "command": config.command,
                "input": config.input,
                "tested": state.tested.get(id),
            }))
        };
        let stt: Vec<_> = self.speech().filter_map(|p| row(p.id)).collect();
        let ai: Vec<_> = config.ai.iter().filter_map(|p| row(&p.id)).collect();
        let presets: Vec<_> = self.registry.presets().iter().filter(|p| p.group != Group::Speech).collect();
        json!({ "jobs": jobs, "stt": stt, "ai": ai, "presets": presets })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemorySecrets;

    #[test]
    fn a_panic_while_the_state_was_held_leaves_the_providers_usable() {
        let providers = Providers::open(Registry::builtin(), None, Arc::new(MemorySecrets::default()));
        let before = providers.choice(Job::Live);
        let held = providers.clone();
        let _ = std::thread::spawn(move || {
            let _state = held.state.lock();
            panic!("mid-change");
        })
        .join();
        assert!(providers.state.is_poisoned());
        assert_eq!(providers.choice(Job::Live), before);
        providers.set_job(Job::Live, Slot::Backup, None).unwrap();
        assert_eq!(providers.choice(Job::Live).1, None);
        assert!(providers.view()["stt"].is_array());
    }
}
