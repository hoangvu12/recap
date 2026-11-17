//! The recording: stereo Ogg/Opus, channel 0 Me, channel 1 Them, 16 kHz,
//! 32 kbps (about 15 MB an hour). Written as it goes: a page a second, synced
//! to disk every few seconds. Every Ogg page has a CRC, so a file cut off by a
//! crash or a kill still decodes up to its last whole page.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::Path;

use ogg::writing::{PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels};

use crate::{AudioFrame, SAMPLE_RATE, Source};

/// 20 ms Opus frames.
const FRAME: usize = SAMPLE_RATE as usize / 50;
/// Granule positions count 48 kHz samples, whatever the input rate.
const GRANULE_PER_FRAME: u64 = 960;
/// A page a second: what a cut-off file loses at most, besides the sync wait.
const FRAMES_PER_PAGE: u64 = 50;
/// Synced to disk every this many pages (seconds).
const PAGES_PER_SYNC: u64 = 2;
const BITRATE: i32 = 32_000;
const SERIAL: u32 = 0x5245_4341; // "RECA"

/// Writes a meeting's audio to disk. Feed it the frames a source hands out
/// (Me and Them in step); [`finish`](Recorder::finish) closes the stream.
pub struct Recorder {
    writer: PacketWriter<'static, BufWriter<File>>,
    encoder: opus::Encoder,
    me: VecDeque<i16>,
    them: VecDeque<i16>,
    frames: u64,
    /// The encoder's delay, in 16 kHz samples.
    lookahead: usize,
    pcm: Vec<i16>,
    packet: Vec<u8>,
}

impl Recorder {
    pub fn create(path: &Path) -> io::Result<Recorder> {
        let mut encoder =
            opus::Encoder::new(SAMPLE_RATE, Channels::Stereo, Application::Voip).map_err(io::Error::other)?;
        encoder.set_bitrate(Bitrate::Bits(BITRATE)).map_err(io::Error::other)?;
        let lookahead = encoder.get_lookahead().map_err(io::Error::other)?;
        let mut writer = PacketWriter::new(BufWriter::new(File::create(path)?));

        // OpusHead (RFC 7845 §5.1): version 1, 2 channels, pre-skip in 48 kHz
        // samples, the input rate, no gain, mapping family 0.
        let mut head = b"OpusHead".to_vec();
        head.push(1);
        head.push(2);
        head.extend((lookahead as u16 * 3).to_le_bytes());
        head.extend(SAMPLE_RATE.to_le_bytes());
        head.extend(0i16.to_le_bytes());
        head.push(0);
        writer.write_packet(head, SERIAL, PacketWriteEndInfo::EndPage, 0)?;
        let vendor = b"recap";
        let mut tags = b"OpusTags".to_vec();
        tags.extend((vendor.len() as u32).to_le_bytes());
        tags.extend(vendor);
        tags.extend(0u32.to_le_bytes());
        writer.write_packet(tags, SERIAL, PacketWriteEndInfo::EndPage, 0)?;
        let mut recorder = Recorder {
            writer,
            encoder,
            me: VecDeque::new(),
            them: VecDeque::new(),
            frames: 0,
            lookahead: lookahead.max(0) as usize,
            pcm: vec![0; FRAME * 2],
            packet: vec![0; 4000],
        };
        recorder.sync()?;
        Ok(recorder)
    }

    pub fn write(&mut self, frame: &AudioFrame) -> io::Result<()> {
        match frame.source {
            Source::Me => self.me.extend(&frame.samples),
            Source::Them => self.them.extend(&frame.samples),
        }
        while self.me.len() >= FRAME && self.them.len() >= FRAME {
            self.encode(PacketWriteEndInfo::NormalPacket)?;
        }
        Ok(())
    }

    /// The rest, and the end of the stream; synced to disk.
    pub fn finish(mut self) -> io::Result<()> {
        // The encoder runs `lookahead` behind: zeros push the last audio out.
        let len = self.me.len().max(self.them.len()) + self.lookahead;
        let len = len.div_ceil(FRAME).max(1) * FRAME;
        self.me.resize(len, 0);
        self.them.resize(len, 0);
        while self.me.len() > FRAME {
            self.encode(PacketWriteEndInfo::NormalPacket)?;
        }
        self.encode(PacketWriteEndInfo::EndStream)?;
        self.sync()
    }

    fn encode(&mut self, mut end: PacketWriteEndInfo) -> io::Result<()> {
        for (i, (me, them)) in self.me.drain(..FRAME).zip(self.them.drain(..FRAME)).enumerate() {
            self.pcm[i * 2] = me;
            self.pcm[i * 2 + 1] = them;
        }
        let len = self.encoder.encode(&self.pcm, &mut self.packet).map_err(io::Error::other)?;
        self.frames += 1;
        let page_done = self.frames.is_multiple_of(FRAMES_PER_PAGE);
        if page_done && end == PacketWriteEndInfo::NormalPacket {
            end = PacketWriteEndInfo::EndPage;
        }
        let granule = self.frames * GRANULE_PER_FRAME;
        self.writer.write_packet(self.packet[..len].to_vec(), SERIAL, end, granule)?;
        if page_done && (self.frames / FRAMES_PER_PAGE).is_multiple_of(PAGES_PER_SYNC) {
            self.sync()?;
        }
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        let file = self.writer.inner_mut();
        file.flush()?;
        file.get_ref().sync_data()
    }
}

/// A recording read back: each side as 16 kHz mono.
#[derive(Clone, Debug, Default)]
pub struct Recording {
    pub me: Vec<i16>,
    pub them: Vec<i16>,
}

impl Recording {
    /// Decode a recording, as much of it as is whole: a file cut off mid-write
    /// gives everything up to its last complete page.
    pub fn read(path: &Path) -> io::Result<Recording> {
        let mut reader = ogg::PacketReader::new(BufReader::new(File::open(path)?));
        let mut decoder = opus::Decoder::new(SAMPLE_RATE, Channels::Stereo).map_err(io::Error::other)?;
        let mut pre_skip = 0usize;
        let mut recording = Recording::default();
        let mut pcm = vec![0i16; SAMPLE_RATE as usize / 1000 * 120 * 2];
        let mut index = 0;
        // A read error is the cut-off tail: what came before stands.
        while let Ok(Some(packet)) = reader.read_packet() {
            index += 1;
            match index {
                1 => {
                    let head = &packet.data;
                    if head.len() < 19 || &head[..8] != b"OpusHead" {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "not an Ogg/Opus file"));
                    }
                    // Pre-skip is in 48 kHz samples.
                    pre_skip = u16::from_le_bytes([head[10], head[11]]) as usize * SAMPLE_RATE as usize / 48_000;
                }
                2 => {} // OpusTags
                _ => {
                    let Ok(n) = decoder.decode(&packet.data, &mut pcm, false) else { break };
                    for [me, them] in pcm[..n * 2].as_chunks::<2>().0 {
                        recording.me.push(*me);
                        recording.them.push(*them);
                    }
                }
            }
        }
        let skip = pre_skip.min(recording.me.len());
        recording.me.drain(..skip);
        recording.them.drain(..skip);
        Ok(recording)
    }
}
