//! `providers.json`: the user's AI providers and each job's main and
//! backup. Never a key (those are in the secret store).

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use crate::job::Job;

/// How a command-line provider gets the transcript.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptInput {
    #[default]
    Stdin,
    /// Written to a file, its path in `{transcript_file}`.
    File,
}

/// A provider the user added (AI). Speech engines need no entry: they are
/// the registry's, by their preset id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub id: String,
    pub preset: String,
    pub name: String,
    pub model: String,
    pub base_url: String,
    pub command: String,
    pub input: TranscriptInput,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    pub main: Option<String>,
    pub backup: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub ai: Vec<ProviderConfig>,
    pub jobs: BTreeMap<Job, Choice>,
}

impl Default for Config {
    fn default() -> Self {
        Self { version: 1, ai: Vec::new(), jobs: BTreeMap::new() }
    }
}

impl Config {
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&text).map_err(|e| eprintln!("recap: {}: {e}", path.display())).ok()
    }

    /// Written whole, then moved over the old file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(tmp, path)
    }
}
