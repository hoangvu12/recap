//! Jobs (GLOSSARY): what providers are used for, and what each needs.

use recap_engine::Capabilities;
use serde::{Deserialize, Serialize};

use crate::registry::{Family, Kind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    /// Live captions: live STT.
    Live,
    /// The after-Stop transcript: batch STT with speakers and word times.
    Transcript,
    /// Summary and names: an AI provider.
    Summary,
}

/// A job's main provider, or its backup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Main,
    Backup,
}

impl Job {
    pub const ALL: [Job; 3] = [Job::Live, Job::Transcript, Job::Summary];

    pub fn id(self) -> &'static str {
        match self {
            Job::Live => "live",
            Job::Transcript => "transcript",
            Job::Summary => "summary",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|j| j.id() == id)
    }

    /// As the Providers page says it.
    pub fn label(self) -> &'static str {
        match self {
            Job::Live => "Live captions",
            Job::Transcript => "Transcript",
            Job::Summary => "Summary and names",
        }
    }

    pub fn family(self) -> Family {
        match self {
            Job::Live | Job::Transcript => Family::Stt,
            Job::Summary => Family::Ai,
        }
    }

    /// The capabilities a speech engine needs for it.
    pub fn needs(self) -> Capabilities {
        match self {
            Job::Live => Capabilities { live: true, ..Default::default() },
            // Speakers are named from the diarized system channel, and the
            // two channels merge by word times (DESIGN §4.3).
            Job::Transcript => Capabilities { batch: true, diarize: true, word_timestamps: true, ..Default::default() },
            Job::Summary => Capabilities::default(),
        }
    }

    /// The names of [`Job::needs`], as the UI checks them.
    pub fn needs_names(self) -> Vec<&'static str> {
        capability_names(self.needs())
    }

    /// Whether providers of `kind` can do this job.
    pub fn takes(self, kind: &Kind) -> bool {
        let (need, have) = (self.needs(), kind.capabilities);
        kind.family == self.family()
            && (!need.live || have.live)
            && (!need.batch || have.batch)
            && (!need.diarize || have.diarize)
            && (!need.keyterms || have.keyterms)
            && (!need.word_timestamps || have.word_timestamps)
    }
}

/// The capability flags that are set, by name.
pub fn capability_names(c: Capabilities) -> Vec<&'static str> {
    [
        (c.live, "live"),
        (c.batch, "batch"),
        (c.diarize, "diarize"),
        (c.keyterms, "keyterms"),
        (c.word_timestamps, "word_timestamps"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect()
}
