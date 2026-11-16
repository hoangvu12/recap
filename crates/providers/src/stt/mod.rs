//! Speech-to-text engines: one file per vendor, each with its `KIND` (the
//! adapter's builder and capabilities) and its `PRESET` (name, logo, key
//! hint). Add a vendor here and to [`ALL`]; the Providers page lists it and
//! the job pickers offer it wherever its capabilities fit.

pub mod deepgram;
pub mod elevenlabs;

use recap_engine::SAMPLE_RATE;

use crate::registry::{Kind, Preset};

/// Every speech engine, in the order the Providers page lists them.
pub const ALL: [(Kind, Preset); 2] = [(elevenlabs::KIND, elevenlabs::PRESET), (deepgram::KIND, deepgram::PRESET)];

/// At most 50 keyterms of at most 20 characters (ElevenLabs' realtime
/// limits; every engine gets the same list).
fn keyterms(terms: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for term in terms {
        let term: String = term.trim().chars().take(20).collect();
        if !term.is_empty() && !out.contains(&term) {
            out.push(term);
        }
        if out.len() == 50 {
            break;
        }
    }
    out
}

/// 16 kHz mono 16-bit PCM as a WAV file (a side, uploaded after Stop).
fn wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Diarized speaker ids as `S1`… in order of first appearance.
#[derive(Default)]
struct Speakers(Vec<String>);

impl Speakers {
    fn label(&mut self, id: String) -> String {
        let n = match self.0.iter().position(|s| *s == id) {
            Some(i) => i + 1,
            None => {
                self.0.push(id);
                self.0.len()
            }
        };
        format!("S{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyterms_are_capped_trimmed_and_unique() {
        let terms: Vec<String> =
            [" Priya ", "Priya", "", "abcdefghijklmnopqrstuvwxyz"].iter().map(|s| s.to_string()).collect();
        assert_eq!(keyterms(&terms), ["Priya", "abcdefghijklmnopqrst"]);
    }

    #[test]
    fn speakers_are_numbered_by_first_appearance() {
        let mut speakers = Speakers::default();
        let labels: Vec<String> = ["b", "a", "b", "c"].iter().map(|id| speakers.label(id.to_string())).collect();
        assert_eq!(labels, ["S1", "S2", "S1", "S3"]);
    }
}
