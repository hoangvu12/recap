//! A meeting's folder: the files are the truth the database is built from.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};

use crate::{MeetingInfo, Result, Segment, StoreError, Summary, TranscriptKind};

pub(crate) const INFO: &str = "info.json";
pub(crate) const MY_NOTES: &str = "my_notes.md";
pub(crate) const SUMMARY: &str = "summary.json";
pub(crate) const SUMMARY_MD: &str = "summary.md";

pub(crate) fn transcript_file(kind: TranscriptKind) -> &'static str {
    match kind {
        TranscriptKind::Live => "live.ndjson",
        TranscriptKind::Refined => "refined.ndjson",
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> StoreError + '_ {
    move |source| StoreError::Io { path: path.to_owned(), source }
}

fn json(path: &Path) -> impl FnOnce(serde_json::Error) -> StoreError + '_ {
    move |source| StoreError::Json { path: path.to_owned(), source }
}

/// Write `bytes` to `path` whole or not at all: a temporary file beside it,
/// then a rename over it.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes).map_err(io(&tmp))?;
    fs::rename(&tmp, path).map_err(io(path))
}

pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(json(path))?;
    write_atomic(path, &bytes)
}

/// The JSON file at `path`, or `None` when there is none.
pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(json(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(path)(e)),
    }
}

/// The text file at `path`, or empty when there is none.
pub(crate) fn read_text(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(io(path)(e)),
    }
}

fn ndjson(segments: &[Segment]) -> Vec<u8> {
    let mut out = Vec::new();
    for segment in segments {
        serde_json::to_writer(&mut out, segment).expect("a segment serializes");
        out.push(b'\n');
    }
    out
}

/// Append `segments` to the live file, one JSON object a line.
pub(crate) fn append_segments(path: &Path, segments: &[Segment]) -> Result<()> {
    let mut file = fs::OpenOptions::new().create(true).append(true).open(path).map_err(io(path))?;
    file.write_all(&ndjson(segments)).map_err(io(path))
}

pub(crate) fn write_segments(path: &Path, segments: &[Segment]) -> Result<()> {
    write_atomic(path, &ndjson(segments))
}

/// The segments of an ndjson file. A line that doesn't parse (the app died
/// while writing it) and what follows it are left out.
pub(crate) fn read_segments(path: &Path) -> Result<Vec<Segment>> {
    Ok(read_text(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map_while(|line| serde_json::from_str(line).ok())
        .collect())
}

/// Everything a folder holds for the index.
pub(crate) struct Folder {
    pub info: MeetingInfo,
    pub live: Vec<Segment>,
    pub refined: Vec<Segment>,
    pub my_notes: String,
    pub summary: Option<Summary>,
}

/// The meeting in `dir`, or `None` when it has no info.json (not a
/// meeting, or one whose first write never finished).
pub(crate) fn read_folder(dir: &Path) -> Result<Option<Folder>> {
    let Some(info) = read_json::<MeetingInfo>(&dir.join(INFO))? else {
        return Ok(None);
    };
    Ok(Some(Folder {
        info,
        live: read_segments(&dir.join(transcript_file(TranscriptKind::Live)))?,
        refined: read_segments(&dir.join(transcript_file(TranscriptKind::Refined)))?,
        my_notes: read_text(&dir.join(MY_NOTES))?,
        summary: read_json(&dir.join(SUMMARY))?,
    }))
}

/// The meeting folders under `meetings`, in name order.
pub(crate) fn meeting_dirs(meetings: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(meetings)
        .map_err(io(meetings))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .map(|entry| entry.path())
        .collect();
    dirs.sort();
    Ok(dirs)
}

pub(crate) fn create_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir(path)
}

pub(crate) fn remove_dir(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(path)(e)),
        _ => Ok(()),
    }
}

pub(crate) fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(io(path))
}

pub(crate) fn write_text(path: &Path, text: &str) -> Result<()> {
    write_atomic(path, text.as_bytes())
}
