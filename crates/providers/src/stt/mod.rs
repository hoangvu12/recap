//! Speech-to-text engines: one file per vendor, each with its `KIND` (the
//! adapter's builder and capabilities) and its `PRESET` (name, logo, key
//! hint). Add a vendor here and to [`ALL`]; the Providers page lists it and
//! the job pickers offer it wherever its capabilities fit.

pub mod deepgram;
pub mod elevenlabs;

use crate::registry::{Kind, Preset};

/// Every speech engine, in the order the Providers page lists them.
pub const ALL: [(Kind, Preset); 2] = [(elevenlabs::KIND, elevenlabs::PRESET), (deepgram::KIND, deepgram::PRESET)];
