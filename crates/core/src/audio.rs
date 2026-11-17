//! The audio track under the animatic.
//!
//! A project holds any number of [`AudioClip`]s on one track (clips may overlap; they are mixed).
//! The source file of a clip is kept unchanged inside the `.boardcraft` archive
//! (`audio/<clip id>.<ext>`) and decoded on demand with `craft-audio` (craft-libs) to planar `f32`.
//!
//! - Time is whole frames at the project rate, like panel durations: a clip starts at
//!   [`AudioClip::start`] on the animatic and skips [`AudioClip::trim_in`] frames of its source.
//! - **Sync on retime:** a clip anchored to a panel ([`AudioAnchor`]) starts `offset` frames after
//!   that panel's first frame; [`Project::sync_audio`] (run after every command) moves it when panel
//!   durations or order change. A clip whose panel is deleted keeps its place and loses the anchor.
//! - [`Mixer`] mixes the track at any output rate for playback and movie export; [`Waveform`] is a
//!   min/max overview for drawing (craft-audio `peaks`).

use std::collections::HashMap;
use std::sync::Arc;

/// Decoded audio (planar `f32`), and the file extensions the decoder reads (for file pickers).
pub use craft_audio::{AudioBuffer, DECODE_EXTENSIONS as AUDIO_EXTENSIONS};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::{Id, Project};

/// Largest audio file kept in a project (and read back from an archive entry).
pub const MAX_AUDIO_BYTES: u64 = 1 << 30;
/// Most decoded samples (frames × channels) of one clip: 2^28 `f32` (1 GiB), about 46 minutes of
/// 48 kHz stereo.
pub const MAX_DECODED_SAMPLES: u64 = 1 << 28;
/// Most clips on a board's track.
pub const MAX_CLIPS: usize = 256;
/// Gain range in decibels.
pub const MIN_GAIN_DB: f32 = -60.0;
pub const MAX_GAIN_DB: f32 = 24.0;
/// Furthest a clip may start: one day at the highest frame rate.
pub const MAX_START: u64 = 24 * 3600 * crate::model::MAX_FPS as u64;

/// Ties a clip to a panel: the clip starts `offset` frames after the panel's first frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioAnchor {
    pub panel: Id,
    #[serde(default)]
    pub offset: i64,
}

/// One sound file placed on the animatic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioClip {
    pub id: Id,
    pub name: String,
    /// The source's file extension (`wav`, `mp3`, `m4a`, …), lower case: the archive entry's name
    /// and a decoding hint.
    pub ext: String,
    /// First frame on the animatic timeline.
    #[serde(default)]
    pub start: u64,
    /// Frames of the source skipped at its start.
    #[serde(default)]
    pub trim_in: u64,
    /// Gain in decibels ([`MIN_GAIN_DB`]..=[`MAX_GAIN_DB`]).
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub anchor: Option<AudioAnchor>,
    /// Source sample rate, channels and length in sample frames, measured on import (exports and
    /// drawing use them without decoding).
    #[serde(default)]
    pub sample_rate: u32,
    #[serde(default)]
    pub channels: u16,
    #[serde(default)]
    pub source_frames: u64,
    /// The source file's bytes. Saved as an archive entry, not in the JSON.
    #[serde(skip)]
    pub data: Arc<Vec<u8>>,
}

/// A file extension safe for an archive entry name: lower-case ASCII letters and digits, at most 8.
pub fn sanitize_ext(ext: &str) -> String {
    let e: String = ext.trim_start_matches('.').chars().filter(char::is_ascii_alphanumeric).take(8).collect::<String>().to_ascii_lowercase();
    if e.is_empty() { "bin".into() } else { e }
}

impl AudioClip {
    /// The archive entry holding the source: `audio/<id>.<ext>`.
    pub fn entry_name(&self) -> String {
        format!("audio/{}.{}", self.id.0, self.ext)
    }

    /// Source length in seconds.
    pub fn source_seconds(&self) -> f64 {
        if self.sample_rate == 0 { 0.0 } else { self.source_frames as f64 / self.sample_rate as f64 }
    }

    /// Frames the clip plays on the animatic at `fps` (the source after `trim_in`).
    pub fn length_frames(&self, fps: f64) -> u64 {
        let total = (self.source_seconds() * fps.max(0.0)).ceil();
        let total = if total.is_finite() { total as u64 } else { 0 };
        total.saturating_sub(self.trim_in)
    }

    /// One past the last frame the clip plays.
    pub fn end(&self, fps: f64) -> u64 {
        self.start.saturating_add(self.length_frames(fps))
    }

    /// Linear gain.
    pub fn gain(&self) -> f32 {
        db_to_gain(self.gain_db)
    }

    /// Clamp hostile values.
    pub fn sanitize(&mut self) {
        self.ext = sanitize_ext(&self.ext);
        self.gain_db = if self.gain_db.is_finite() { self.gain_db.clamp(MIN_GAIN_DB, MAX_GAIN_DB) } else { 0.0 };
        self.start = self.start.min(MAX_START);
        self.trim_in = self.trim_in.min(MAX_START);
        if let Some(a) = &mut self.anchor {
            a.offset = a.offset.clamp(-(MAX_START as i64), MAX_START as i64);
        }
        if self.name.chars().count() > 200 {
            self.name = self.name.chars().take(200).collect();
        }
    }
}

fn db_to_gain(db: f32) -> f32 {
    if db.is_finite() { 10f32.powf(db.clamp(MIN_GAIN_DB, MAX_GAIN_DB) / 20.0) } else { 1.0 }
}

/// Decode audio file bytes (`ext` is a hint; the contents decide) with the clip size limit.
pub fn decode_bytes(bytes: &[u8], ext: &str) -> Result<AudioBuffer> {
    let opts = craft_audio::DecodeOptions::default().with_max_samples(MAX_DECODED_SAMPLES);
    let (_, buf) = craft_audio::decode_with(bytes, Some(ext), &opts).map_err(|e| Error::Audio(e.to_string()))?;
    if buf.sample_rate == 0 || buf.channels.is_empty() {
        return Err(Error::Audio("the file has no audio".into()));
    }
    Ok(buf)
}

/// Decode a clip's source.
pub fn decode(clip: &AudioClip) -> Result<AudioBuffer> {
    decode_bytes(&clip.data, &clip.ext)
}

/// Decode every clip of a project. Clips that fail are left out and reported (`name: error`).
pub fn decode_all(project: &Project) -> (HashMap<Id, Arc<AudioBuffer>>, Vec<String>) {
    let mut out = HashMap::new();
    let mut errors = Vec::new();
    for clip in &project.audio {
        match decode(clip) {
            Ok(b) => {
                out.insert(clip.id, Arc::new(b));
            }
            Err(e) => errors.push(format!("{}: {e}", clip.name)),
        }
    }
    (out, errors)
}

impl Project {
    /// Add a sound file as a clip starting at `start`: decoded once to check it and measure it.
    pub fn add_audio(&mut self, name: &str, ext: &str, bytes: Vec<u8>, start: u64) -> Result<Id> {
        if self.audio.len() >= MAX_CLIPS {
            return Err(Error::Unsupported(format!("a board holds at most {MAX_CLIPS} sound clips")));
        }
        if bytes.len() as u64 > MAX_AUDIO_BYTES {
            return Err(Error::Audio(format!("{name} is larger than {} MB", MAX_AUDIO_BYTES >> 20)));
        }
        let ext = sanitize_ext(ext);
        let buf = decode_bytes(&bytes, &ext)?;
        let id = self.alloc();
        let mut clip = AudioClip {
            id,
            name: name.to_string(),
            ext,
            start,
            trim_in: 0,
            gain_db: 0.0,
            anchor: None,
            sample_rate: buf.sample_rate,
            channels: buf.num_channels(),
            source_frames: buf.frames() as u64,
            data: Arc::new(bytes),
        };
        clip.sanitize();
        self.audio.push(clip);
        Ok(id)
    }

    pub fn audio_clip(&self, id: Id) -> Option<&AudioClip> {
        self.audio.iter().find(|c| c.id == id)
    }

    pub fn audio_clip_mut(&mut self, id: Id) -> Option<&mut AudioClip> {
        self.audio.iter_mut().find(|c| c.id == id)
    }

    /// Move a clip to start at `start`. An anchored clip keeps its panel and takes the new offset.
    pub fn move_audio(&mut self, id: Id, start: u64) -> Result<()> {
        let start = start.min(MAX_START);
        let anchor_start = self.audio_clip(id).ok_or_else(|| Error::NotFound(format!("sound clip {}", id.0)))?.anchor.and_then(|a| self.panel_start(a.panel));
        if let Some(clip) = self.audio_clip_mut(id) {
            clip.start = start;
            if let (Some(a), Some(ps)) = (&mut clip.anchor, anchor_start) {
                a.offset = start as i64 - ps as i64;
            }
        }
        Ok(())
    }

    /// Anchor a clip to a panel at its current offset from the panel's start (`None`: free the clip).
    pub fn set_audio_anchor(&mut self, id: Id, panel: Option<Id>) -> Result<()> {
        let panel_start = match panel {
            Some(p) => Some((p, self.panel_start(p).ok_or_else(|| Error::NotFound(format!("panel {}", p.0)))?)),
            None => None,
        };
        let clip = self.audio_clip_mut(id).ok_or_else(|| Error::NotFound(format!("sound clip {}", id.0)))?;
        clip.anchor = panel_start.map(|(panel, ps)| AudioAnchor { panel, offset: clip.start as i64 - ps as i64 });
        Ok(())
    }

    /// Keep anchored clips with their panels (sync on retime). Anchors to missing panels are dropped.
    pub fn sync_audio(&mut self) {
        let starts: HashMap<Id, u64> = {
            let mut t = 0u64;
            let mut m = HashMap::new();
            for (_, p) in self.panels() {
                m.insert(p.id, t);
                t += p.duration.max(1) as u64;
            }
            m
        };
        for clip in &mut self.audio {
            let Some(a) = clip.anchor else { continue };
            match starts.get(&a.panel) {
                Some(&ps) => clip.start = (ps as i64).saturating_add(a.offset).clamp(0, MAX_START as i64) as u64,
                None => clip.anchor = None,
            }
        }
    }
}

/// A min/max overview of a clip's source for drawing (mono mix of its channels).
#[derive(Clone, Debug, Default)]
pub struct Waveform {
    sample_rate: u32,
    mono: Vec<f32>,
    peaks: craft_audio::peaks::Peaks,
}

impl Waveform {
    pub fn new(buf: &AudioBuffer) -> Self {
        let frames = buf.frames();
        let n = buf.channels.len().max(1) as f32;
        let mono: Vec<f32> = (0..frames).map(|i| buf.channels.iter().map(|c| c.get(i).copied().unwrap_or(0.0)).sum::<f32>() / n).collect();
        let peaks = craft_audio::peaks::Peaks::build(&mono);
        Self { sample_rate: buf.sample_rate, mono, peaks }
    }

    /// Source length in seconds.
    pub fn seconds(&self) -> f64 {
        if self.sample_rate == 0 { 0.0 } else { self.mono.len() as f64 / self.sample_rate as f64 }
    }

    /// (min, max) of each of `cols` columns `step` seconds wide from `from` seconds of the source;
    /// `None` outside the source.
    pub fn columns(&self, from: f64, step: f64, cols: usize) -> Vec<Option<(f32, f32)>> {
        let sr = self.sample_rate as f64;
        self.peaks.spans(&self.mono, from * sr, step * sr, 0.0, cols)
    }
}

/// One clip ready to mix.
struct MixClip {
    /// Seconds on the animatic where the clip starts.
    start: f64,
    /// Seconds of the source skipped.
    trim: f64,
    /// Seconds the clip plays.
    length: f64,
    gain: f32,
    buf: Arc<AudioBuffer>,
}

/// Mixes the audio track: every clip, resampled (linear) to the output rate, gained and summed.
#[derive(Default)]
pub struct Mixer {
    clips: Vec<MixClip>,
}

impl Mixer {
    /// The clips of `project` whose decoded sources are in `decoded`.
    pub fn new(project: &Project, decoded: &HashMap<Id, Arc<AudioBuffer>>) -> Self {
        let fps = project.settings.fps_f64().max(1e-6);
        let clips = project
            .audio
            .iter()
            .filter_map(|c| {
                let buf = decoded.get(&c.id)?.clone();
                let trim = c.trim_in as f64 / fps;
                let length = (buf.duration_secs() - trim).max(0.0);
                (length > 0.0).then(|| MixClip { start: c.start as f64 / fps, trim, length, gain: c.gain(), buf })
            })
            .collect();
        Self { clips }
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    /// Where the last clip ends, in seconds.
    pub fn end_seconds(&self) -> f64 {
        self.clips.iter().map(|c| c.start + c.length).fold(0.0, f64::max)
    }

    /// Mix `out.len() / channels` frames into `out` (interleaved), the first at output sample
    /// `from` (`from / rate` seconds into the animatic). `out` is overwritten.
    pub fn render(&self, rate: u32, channels: usize, from: u64, out: &mut [f32]) {
        out.iter_mut().for_each(|s| *s = 0.0);
        if rate == 0 || channels == 0 {
            return;
        }
        let frames = out.len() / channels;
        let t0 = from as f64 / rate as f64;
        let t1 = (from + frames as u64) as f64 / rate as f64;
        for clip in &self.clips {
            if clip.start >= t1 || clip.start + clip.length <= t0 {
                continue;
            }
            let src_rate = clip.buf.sample_rate as f64;
            let src_frames = clip.buf.frames();
            let nsrc = clip.buf.channels.len();
            if nsrc == 0 || src_frames == 0 {
                continue;
            }
            for (i, frame) in out.chunks_exact_mut(channels).enumerate() {
                let t = (from + i as u64) as f64 / rate as f64 - clip.start;
                if t < 0.0 || t >= clip.length {
                    continue;
                }
                let pos = (t + clip.trim) * src_rate;
                let i0 = pos.floor() as usize;
                if i0 >= src_frames {
                    continue;
                }
                let frac = (pos - i0 as f64) as f32;
                let i1 = (i0 + 1).min(src_frames - 1);
                let at = |ch: usize| {
                    let c = &clip.buf.channels[ch.min(nsrc - 1)];
                    let a = c.get(i0).copied().unwrap_or(0.0);
                    let b = c.get(i1).copied().unwrap_or(a);
                    a + (b - a) * frac
                };
                if channels == 1 {
                    frame[0] += (0..nsrc).map(at).sum::<f32>() / nsrc as f32 * clip.gain;
                } else {
                    for (c, s) in frame.iter_mut().enumerate() {
                        *s += at(c) * clip.gain;
                    }
                }
            }
        }
        for s in out.iter_mut() {
            *s = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
        }
    }
}

/// A sine tone as a 16-bit PCM WAV file (tests, demos): `seconds` long at `rate`, `channels` wide.
pub fn tone_wav(freq: f32, seconds: f32, rate: u32, channels: u16) -> Vec<u8> {
    let frames = (seconds.max(0.0) * rate as f32) as u32;
    let channels = channels.max(1);
    let data_len = frames * channels as u32 * 2;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    w.extend_from_slice(&(channels * 2).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let v = ((i as f32 / rate as f32 * freq * std::f32::consts::TAU).sin() * 0.5 * i16::MAX as f32) as i16;
        for _ in 0..channels {
            w.extend_from_slice(&v.to_le_bytes());
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Command, apply};

    fn board() -> (Project, Vec<Id>) {
        let mut p = Project::blank("t");
        let s = p.scenes[0].id;
        let a = p.scenes[0].panels[0].id;
        let b = p.add_panel(s, Some(a)).unwrap();
        let c = p.add_panel(s, Some(b)).unwrap();
        (p, vec![a, b, c])
    }

    #[test]
    fn add_measures_and_mixes() {
        let (mut p, _) = board();
        let id = p.add_audio("tone", "WAV", tone_wav(440.0, 1.0, 22_050, 1), 24).unwrap();
        let clip = p.audio_clip(id).unwrap();
        assert_eq!((clip.sample_rate, clip.channels, clip.source_frames), (22_050, 1, 22_050));
        assert_eq!(clip.ext, "wav");
        assert_eq!(clip.length_frames(24.0), 24);
        let (decoded, errors) = decode_all(&p);
        assert!(errors.is_empty());
        let mixer = Mixer::new(&p, &decoded);
        assert!((mixer.end_seconds() - 2.0).abs() < 1e-6);
        // Silence before the clip, the tone after its start (one second in at 48 kHz).
        let mut out = vec![0.0f32; 2 * 4800];
        mixer.render(48_000, 2, 0, &mut out);
        assert!(out.iter().all(|s| *s == 0.0));
        mixer.render(48_000, 2, 48_000, &mut out);
        let peak = out.iter().fold(0.0f32, |a, s| a.max(s.abs()));
        assert!((0.4..=0.55).contains(&peak), "{peak}");
        // Gain applies.
        p.audio_clip_mut(id).unwrap().gain_db = -20.0;
        let mixer = Mixer::new(&p, &decoded);
        mixer.render(48_000, 2, 48_000, &mut out);
        let quiet = out.iter().fold(0.0f32, |a, s| a.max(s.abs()));
        assert!(quiet < peak / 5.0, "{quiet} vs {peak}");
        // A waveform of the clip.
        let w = Waveform::new(decoded.get(&id).unwrap());
        let cols = w.columns(0.0, 0.1, 12);
        assert_eq!(cols.len(), 12);
        assert!(cols[0].unwrap().1 > 0.4);
        assert!(cols[11].is_none());
    }

    #[test]
    fn anchored_clips_follow_retimes() {
        let (mut p, ids) = board();
        let clip = p.add_audio("line", "wav", tone_wav(220.0, 0.5, 8_000, 2), 0).unwrap();
        apply(&mut p, &Command::MoveAudio { clip, start: 100 }).unwrap();
        apply(&mut p, &Command::SetAudioAnchor { clip, panel: Some(ids[2]) }).unwrap();
        assert_eq!(p.audio_clip(clip).unwrap().anchor.unwrap().offset, 4, "panel 3 starts at 96");
        apply(&mut p, &Command::SetDuration { panel: ids[0], frames: 10 }).unwrap();
        assert_eq!(p.audio_clip(clip).unwrap().start, 10 + 48 + 4);
        apply(&mut p, &Command::MovePanel { panel: ids[2], later: false }).unwrap();
        assert_eq!(p.audio_clip(clip).unwrap().start, 10 + 4);
        // Moving an anchored clip changes its offset, not its panel.
        apply(&mut p, &Command::MoveAudio { clip, start: 12 }).unwrap();
        assert_eq!(p.audio_clip(clip).unwrap().anchor, Some(AudioAnchor { panel: ids[2], offset: 2 }));
        // Deleting the panel frees the clip where it is.
        apply(&mut p, &Command::DeletePanel { panel: ids[2] }).unwrap();
        let c = p.audio_clip(clip).unwrap();
        assert_eq!((c.start, c.anchor), (12, None));
        // Unanchored clips stay put.
        apply(&mut p, &Command::SetDuration { panel: ids[0], frames: 40 }).unwrap();
        assert_eq!(p.audio_clip(clip).unwrap().start, 12);
        apply(&mut p, &Command::SetAudioGain { clip, gain_db: f32::NAN }).unwrap();
        assert_eq!(p.audio_clip(clip).unwrap().gain_db, 0.0);
        apply(&mut p, &Command::SetAudioTrim { clip, trim_in: 3 }).unwrap();
        assert_eq!(p.audio_clip(clip).unwrap().trim_in, 3);
        apply(&mut p, &Command::RemoveAudio { clip }).unwrap();
        assert!(p.audio.is_empty());
        assert!(apply(&mut p, &Command::RemoveAudio { clip }).is_err());
    }

    #[test]
    fn add_audio_command_reads_a_file() {
        let (mut p, ids) = board();
        let dir = std::env::temp_dir().join(format!("boardcraft-audio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Line 1.wav");
        std::fs::write(&path, tone_wav(330.0, 0.25, 16_000, 1)).unwrap();
        let cmd = crate::command::parse(&format!(r#"{{"cmd":"add_audio","path":{:?},"panel":{}}}"#, path.to_str().unwrap(), ids[1].0)).unwrap();
        let out = apply(&mut p, &cmd).unwrap();
        let c = p.audio_clip(out.created.unwrap()).unwrap();
        assert_eq!(c.name, "Line 1");
        assert_eq!(c.start, 48);
        assert_eq!(c.anchor, Some(AudioAnchor { panel: ids[1], offset: 0 }));
        // Not audio, or no file: an error and no change.
        std::fs::write(&path, b"not audio at all").unwrap();
        let before = p.clone();
        assert!(apply(&mut p, &cmd).is_err());
        assert!(
            apply(&mut p, &Command::AddAudio { path: dir.join("missing.wav").to_string_lossy().into(), name: String::new(), start: None, panel: None })
                .is_err()
        );
        assert_eq!(p, before);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn hostile_audio_never_panics() {
        let wav = tone_wav(440.0, 0.1, 8_000, 1);
        let mut seed = 0x1234_5678_u32;
        for cut in (0..wav.len()).step_by(7) {
            let _ = std::panic::catch_unwind(|| decode_bytes(&wav[..cut], "wav")).expect("truncation panicked");
        }
        for _ in 0..200 {
            let mut m = wav.clone();
            for _ in 0..3 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let i = seed as usize % m.len().min(64);
                m[i] ^= (seed >> 8) as u8 | 1;
            }
            let _ = std::panic::catch_unwind(|| decode_bytes(&m, "wav")).expect("bit flip panicked");
        }
        let mut c = AudioClip {
            id: Id(1),
            name: "x".repeat(1000),
            ext: "../../evil".into(),
            start: u64::MAX,
            trim_in: u64::MAX,
            gain_db: f32::INFINITY,
            anchor: Some(AudioAnchor { panel: Id(9), offset: i64::MIN }),
            sample_rate: 0,
            channels: 0,
            source_frames: u64::MAX,
            data: Arc::default(),
        };
        c.sanitize();
        assert_eq!(c.ext, "evil");
        assert_eq!(c.gain_db, 0.0);
        assert_eq!(c.length_frames(24.0), 0);
        assert!(c.name.chars().count() <= 200);
    }
}
