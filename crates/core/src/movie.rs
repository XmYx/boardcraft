//! Movie export: the animatic as an MP4 (H.264 video, AAC audio) with optional burn-ins.
//!
//! Every frame is rendered as playback shows it (camera moves applied, see
//! [`crate::stills::render_frame_from`]), the burn-ins are drawn into it (panel number top left,
//! timecode top right, dialogue at the bottom; text through [`crate::text`]), and it is encoded with
//! `craft-video`'s H.264 encoder into an MP4 written by `craft-media`'s `Mp4Writer` (moov first).
//! The audio track ([`crate::audio::Mixer`]) is mixed at 48 kHz stereo and encoded with
//! `craft-audio`'s AAC-LC encoder, interleaved with the video. No external tools are used.

use std::io::{Read, Seek, Write};
use std::path::Path;

use craft_media::isobmff::{AvcConfig, Brand, Mp4Writer, SampleEntry, TrackConfig, WriteSample, WriterOptions};
use craft_video::h264;
use serde::{Deserialize, Serialize};

use crate::audio::{Mixer, decode_all};
use crate::error::{Error, Result};
use crate::model::Project;
use crate::stills::render_frame_from;
use crate::text::{TextRenderer, blend_mask, fill_rect};

/// Audio rate of exported movies.
pub const AUDIO_RATE: u32 = 48_000;
/// AAC bit rate (stereo).
pub const AUDIO_BITRATE: u32 = 192_000;

/// Output picture size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovieSize {
    /// The board's canvas size.
    #[default]
    Board,
    /// Half the canvas size (smaller files for review).
    Half,
}

/// Movie export settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MovieOptions {
    #[serde(default)]
    pub size: MovieSize,
    /// Draw the panel number, timecode and dialogue into the picture.
    #[serde(default = "yes")]
    pub burn_in: bool,
    /// The panel burn-in; `{label}` is replaced with the panel number (`3.2`). Translated by the caller.
    #[serde(default = "panel_label")]
    pub panel_label: String,
    /// H.264 constant quality (lower is better; 18–28 is usual).
    #[serde(default = "quality")]
    pub quality: f32,
}

fn yes() -> bool {
    true
}

fn panel_label() -> String {
    "Panel {label}".into()
}

fn quality() -> f32 {
    20.0
}

impl Default for MovieOptions {
    fn default() -> Self {
        Self { size: MovieSize::Board, burn_in: true, panel_label: panel_label(), quality: quality() }
    }
}

/// What an export wrote.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MovieReport {
    pub frames: u64,
    pub width: u32,
    pub height: u32,
    /// Whether the movie has a sound track.
    pub audio: bool,
    /// Sound clips that could not be decoded (left out).
    pub audio_errors: Vec<String>,
}

/// The movie's picture size for a board: even sides (H.264 4:2:0), halved for [`MovieSize::Half`].
pub fn movie_size(project: &Project, size: MovieSize) -> (u32, u32) {
    let (w, h) = (project.settings.width, project.settings.height);
    let (w, h) = if size == MovieSize::Half { (w / 2, h / 2) } else { (w, h) };
    ((w & !1).max(16), (h & !1).max(16))
}

/// `HH:MM:SS:FF` (non-drop) of `frame` at the board's rate.
pub fn timecode(frame: u64, fps: f64) -> String {
    let base = (fps.round() as u64).max(1);
    let (ff, s) = (frame % base, frame / base);
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60, ff)
}

fn video_err(e: impl std::fmt::Display) -> Error {
    Error::Video(e.to_string())
}

fn mux_err(e: craft_media::isobmff::Error) -> Error {
    match e {
        craft_media::isobmff::Error::Io(e) => Error::io("movie", e),
        e => Error::Video(e.to_string()),
    }
}

/// RGBA8 to planar 4:2:0 BT.709 limited range.
fn to_yuv(rgba: &[u8], w: usize, h: usize, y: &mut [u8], u: &mut [u8], v: &mut [u8]) {
    const KR: f32 = 0.2126;
    const KB: f32 = 0.0722;
    const KG: f32 = 1.0 - KR - KB;
    let px = |x: usize, yy: usize| {
        let i = (yy * w + x) * 4;
        rgba.get(i..i + 3).map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0]).unwrap_or([0.0; 3])
    };
    for yy in 0..h {
        for x in 0..w {
            let [r, g, b] = px(x, yy);
            if let Some(o) = y.get_mut(yy * w + x) {
                *o = (16.0 + 219.0 * (KR * r + KG * g + KB * b)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    let cw = w / 2;
    for cy in 0..h / 2 {
        for cx in 0..cw {
            let mut acc = [0.0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = px(cx * 2 + dx, cy * 2 + dy);
                for c in 0..3 {
                    acc[c] += p[c] / 4.0;
                }
            }
            let [r, g, b] = acc;
            let luma = KR * r + KG * g + KB * b;
            if let Some(o) = u.get_mut(cy * cw + cx) {
                *o = (128.0 + 224.0 * (b - luma) / (2.0 * (1.0 - KB))).round().clamp(0.0, 255.0) as u8;
            }
            if let Some(o) = v.get_mut(cy * cw + cx) {
                *o = (128.0 + 224.0 * (r - luma) / (2.0 * (1.0 - KR))).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// The burn-ins of one frame.
struct BurnIn<'a> {
    text: TextRenderer<'a>,
    px: f32,
}

impl BurnIn<'_> {
    fn draw(&self, rgba: &mut [u8], w: usize, h: usize, panel: &str, tc: &str, dialogue: &str) {
        let pad = (self.px * 0.4).round() as i64;
        let band = (self.px * 1.6).round() as i64;
        let shade = [0, 0, 0, 150];
        let white = [255, 255, 255, 255];
        // Top band: panel number left, timecode right.
        fill_rect(rgba, w, h, 0, 0, w as i64, band, shade);
        let (ascent, descent) = self.text.vertical(self.px);
        let top = ((band as f32 - ascent - descent) / 2.0).round() as i64 - 1;
        let m = self.text.render(panel, self.px);
        blend_mask(rgba, w, h, &m, pad, top, white);
        let m = self.text.render(tc, self.px);
        blend_mask(rgba, w, h, &m, w as i64 - pad - m.width as i64, top, white);
        // Bottom: up to two lines of dialogue, centred.
        let lines = self.text.wrap(dialogue, self.px, w as f32 - 4.0 * pad as f32, 2);
        if lines.is_empty() {
            return;
        }
        let line_h = (self.px * 1.3).round() as i64;
        let box_h = line_h * lines.len() as i64 + pad * 2;
        fill_rect(rgba, w, h, 0, h as i64 - box_h, w as i64, h as i64, shade);
        for (i, l) in lines.iter().enumerate() {
            let m = self.text.render(l, self.px);
            let x = (w as i64 - m.width as i64) / 2;
            blend_mask(rgba, w, h, &m, x, h as i64 - box_h + pad + i as i64 * line_h, white);
        }
    }
}

/// Encode the animatic into `out` (any `Read + Write + Seek`; a file or a `Cursor`). `font` is the
/// face for burn-ins. `progress(done, total)` is called after every frame; returning `false`
/// cancels the export with [`Error::Cancelled`].
pub fn encode_movie<W: Read + Write + Seek>(
    project: &Project,
    out: W,
    opts: &MovieOptions,
    font: &[u8],
    font_index: u32,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(W, MovieReport)> {
    let total = project.total_frames();
    if total == 0 {
        return Err(Error::Unsupported("the board has no panels".into()));
    }
    let (w, h) = movie_size(project, opts.size);
    let (n, d) = project.settings.rate();
    let fps = project.settings.fps_f64();
    let burn = if opts.burn_in { Some(BurnIn { text: TextRenderer::new(font, font_index)?, px: (h as f32 / 24.0).clamp(8.0, 72.0) }) } else { None };

    let mut cfg = h264::EncoderConfig::new(w, h, n, d);
    cfg.format = h264::PacketFormat::LengthPrefixed;
    cfg.bframes = 0;
    cfg.rate = h264::RateControl::Crf(if opts.quality.is_finite() { opts.quality.clamp(0.0, 51.0) } else { 20.0 });
    cfg.keyint = (fps.round() as u32).saturating_mul(10).max(1);
    cfg.writer = crate::GENERATOR.to_string();
    let mut enc = h264::Encoder::new(cfg).map_err(video_err)?;
    let avc = AvcConfig::parse(&enc.avcc()).map_err(mux_err)?;

    let mut options = WriterOptions::new(Brand::Mp4);
    options.metadata.push(("©nam".into(), project.name.clone()));
    options.metadata.push(("©too".into(), crate::GENERATOR.into()));
    let mut mux = Mp4Writer::new(out, options).map_err(mux_err)?;
    // Video timescale: 1000 ticks a frame at whole rates, the rate's numerator for 1000/1001 rates.
    let (timescale, tick) = if d == 1 { (n.saturating_mul(1000), 1000u32) } else { (n, d) };
    let video = mux.add_track(TrackConfig::new(SampleEntry::avc(avc, w as u16, h as u16), timescale)).map_err(mux_err)?;

    // Sound.
    let mut report = MovieReport { frames: total, width: w, height: h, ..Default::default() };
    let mixer = if project.audio.is_empty() {
        Mixer::default()
    } else {
        let (decoded, errors) = decode_all(project);
        report.audio_errors = errors;
        Mixer::new(project, &decoded)
    };
    let mut aac = None;
    if !mixer.is_empty() {
        let e = craft_audio::aac::Encoder::new(craft_audio::aac::EncoderConfig::cbr(AUDIO_RATE, 2, AUDIO_BITRATE)).map_err(|e| Error::Audio(e.to_string()))?;
        let mut tc = TrackConfig::new(SampleEntry::aac(e.audio_specific_config(), 2, AUDIO_RATE), AUDIO_RATE);
        tc.media_start = Some(i64::from(e.priming_samples()));
        tc.roll_distance = Some(-1);
        let track = mux.add_track(tc).map_err(mux_err)?;
        aac = Some((e, track));
        report.audio = true;
    }
    let mut audio_pos = 0u64;
    let mut mix = Vec::new();
    let mut write_audio = |mux: &mut Mp4Writer<W>, aac: &mut Option<(craft_audio::aac::Encoder, usize)>, until: u64, last: bool| -> Result<()> {
        let Some((e, track)) = aac else { return Ok(()) };
        let mut packets = Vec::new();
        if until > audio_pos {
            let frames = (until - audio_pos) as usize;
            mix.resize(frames * 2, 0.0);
            mixer.render(AUDIO_RATE, 2, audio_pos, &mut mix);
            let left: Vec<f32> = mix.iter().step_by(2).copied().collect();
            let right: Vec<f32> = mix.iter().skip(1).step_by(2).copied().collect();
            packets = e.encode(&[&left, &right]);
            audio_pos = until;
        }
        if last {
            packets.extend(e.flush());
        }
        for p in packets {
            mux.write_sample(*track, WriteSample { data: &p, duration: 1024, composition_offset: 0, is_sync: true }).map_err(mux_err)?;
        }
        Ok(())
    };

    let (wu, hu) = (w as usize, h as usize);
    let (mut y, mut u, mut v) = (vec![0u8; wu * hu], vec![0u8; wu * hu / 4], vec![0u8; wu * hu / 4]);
    let mut frame_no = 0u64;
    let write_packets = |mux: &mut Mp4Writer<W>, packets: Vec<h264::Packet>| -> Result<()> {
        for p in packets {
            let offset = (p.pts - p.dts).saturating_mul(tick as i64);
            let sample =
                WriteSample { data: &p.data, duration: tick, composition_offset: offset.clamp(i32::MIN as i64, i32::MAX as i64) as i32, is_sync: p.keyframe };
            mux.write_sample(video, sample).map_err(mux_err)?;
        }
        Ok(())
    };
    for (_, panel) in project.panels() {
        let composite = panel.composite(&project.settings);
        let label = opts.panel_label.replace("{label}", &project.panel_label(panel.id));
        let still = (!panel.camera.moves()).then(|| render_frame_from(&composite, panel, 0.0, w, h));
        for f in 0..panel.duration.max(1) {
            let pic = match &still {
                Some(s) => s.clone(),
                None => render_frame_from(&composite, panel, f as f32, w, h),
            };
            let mut rgba = pic.data().to_vec();
            if let Some(b) = &burn {
                b.draw(&mut rgba, wu, hu, &label, &timecode(frame_no, fps), &panel.dialogue);
            }
            to_yuv(&rgba, wu, hu, &mut y, &mut u, &mut v);
            let frame = h264::YuvFrame { y: &y, u: &u, v: &v, y_stride: wu, uv_stride: wu / 2 };
            let packets = enc.encode(&frame, frame_no as i64).map_err(video_err)?;
            write_packets(&mut mux, packets)?;
            frame_no += 1;
            let until = (frame_no as u128 * AUDIO_RATE as u128 * d as u128 / n.max(1) as u128) as u64;
            write_audio(&mut mux, &mut aac, until, false)?;
            if !progress(frame_no, total) {
                return Err(Error::Cancelled);
            }
        }
    }
    write_packets(&mut mux, enc.flush())?;
    let end = (total as u128 * AUDIO_RATE as u128 * d as u128 / n.max(1) as u128) as u64;
    write_audio(&mut mux, &mut aac, end, true)?;
    let out = mux.finish_faststart().map_err(mux_err)?;
    Ok((out, report))
}

/// [`encode_movie`] into `path`, atomically: a failed or cancelled export leaves no partial file
/// and an earlier file of that name untouched. The movie is built in memory (animatics are mostly
/// still pictures, so a few megabytes a minute) so its index can go first ("fast start").
pub fn write_movie(
    project: &Project,
    path: &Path,
    opts: &MovieOptions,
    font: &[u8],
    font_index: u32,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<MovieReport> {
    let (out, report) = encode_movie(project, std::io::Cursor::new(Vec::new()), opts, font, font_index, progress)?;
    craft_atomic::atomic_write(path, &out.into_inner()).map_err(|e| Error::io(path, e))?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::tone_wav;
    use crate::fountain;
    use crate::model::Settings;

    fn board() -> Project {
        let script = fountain::parse("INT. A\n\nA door opens.\n\nBOB\nHello there, this is a line.\n\nEXT. B\n\nRain.\n");
        let mut p = fountain::project_from_script(&script, Settings { width: 96, height: 54, fps: 12, ntsc: false });
        for panel in p.scenes.iter_mut().flat_map(|s| s.panels.iter_mut()) {
            panel.duration = 6;
        }
        p
    }

    fn decode_first_frame(bytes: &[u8]) -> (u32, u32, Vec<u8>, usize, bool) {
        let file = craft_media::isobmff::open(bytes.to_vec()).unwrap();
        let vt = file.tracks.iter().position(|t| matches!(t.codec(), Some(craft_media::isobmff::CodecConfig::Avc(_)))).unwrap();
        let Some(craft_media::isobmff::CodecConfig::Avc(cfg)) = file.tracks[vt].codec() else { unreachable!() };
        let mut dec = h264::decode::Decoder::from_avcc(&cfg.to_bytes()).unwrap();
        let has_audio = file.tracks.iter().any(|t| matches!(t.codec(), Some(craft_media::isobmff::CodecConfig::Aac(_))));
        let samples = file.tracks[vt].samples.len();
        let mut pics = Vec::new();
        for i in 0..samples {
            let s = file.read_sample(&bytes.to_vec(), vt, i).unwrap();
            pics.extend(dec.decode(&s, i as i64).unwrap());
            if !pics.is_empty() {
                break;
            }
        }
        pics.extend(dec.flush());
        let pic = pics.into_iter().min_by_key(|p| p.pts).unwrap();
        (pic.width, pic.height, pic.y.clone(), samples, has_audio)
    }

    #[test]
    fn movie_decodes_back_with_burn_ins_and_sound() {
        let mut p = board();
        p.add_audio("tone", "wav", tone_wav(440.0, 1.0, 44_100, 1), 0).unwrap();
        let mut calls = 0;
        let (out, report) = encode_movie(&p, std::io::Cursor::new(Vec::new()), &MovieOptions::default(), epaint_default_fonts::UBUNTU_LIGHT, 0, &mut |_, _| {
            calls += 1;
            true
        })
        .unwrap();
        let bytes = out.into_inner();
        assert_eq!(report.frames, 18);
        assert_eq!(calls, 18);
        assert!(report.audio);
        let (w, h, y, samples, has_audio) = decode_first_frame(&bytes);
        assert_eq!((w, h), (96, 54));
        assert_eq!(samples, 18);
        assert!(has_audio);
        // Paper in the middle of the picture (BT.709 luma of the paper colour ≈ 229), a dark band
        // with light text at the top.
        let mid = y[27 * 96 + 48];
        assert!((215..=240).contains(&mid), "{mid}");
        let top: Vec<u8> = (0..96).map(|x| y[2 * 96 + x]).collect();
        assert!(top.iter().copied().min().unwrap() < 140, "{top:?}");
        // Without burn-ins the top row is paper too; half size halves the picture.
        let opts = MovieOptions { burn_in: false, size: MovieSize::Half, ..Default::default() };
        let (out, _) = encode_movie(&p, std::io::Cursor::new(Vec::new()), &opts, b"", 0, &mut |_, _| true).unwrap();
        let (w, h, y, _, _) = decode_first_frame(&out.into_inner());
        assert_eq!((w, h), (48, 26));
        assert!(y[48 + 24] > 200);
    }

    #[test]
    fn cancel_leaves_no_file() {
        let p = board();
        let dir = std::env::temp_dir().join(format!("boardcraft-movie-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.mp4");
        let r = write_movie(&p, &path, &MovieOptions::default(), epaint_default_fonts::UBUNTU_LIGHT, 0, &mut |done, _| done < 3);
        assert!(matches!(r, Err(Error::Cancelled)), "{r:?}");
        assert!(!path.exists());
        let r = write_movie(&p, &path, &MovieOptions::default(), epaint_default_fonts::UBUNTU_LIGHT, 0, &mut |_, _| true).unwrap();
        assert!(!r.audio);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[4..8], b"ftyp");
        // A bad font fails cleanly when burn-ins are on.
        assert!(write_movie(&p, &dir.join("b.mp4"), &MovieOptions::default(), b"nope", 0, &mut |_, _| true).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn timecodes() {
        assert_eq!(timecode(0, 24.0), "00:00:00:00");
        assert_eq!(timecode(24 * 3661 + 5, 24.0), "01:01:01:05");
        assert_eq!(timecode(30, 29.97), "00:00:01:00");
    }
}
