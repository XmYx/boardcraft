//! Sound output for animatic playback through `cpal`: plays the mixed audio track from a position
//! until stopped. Opening the device never blocks the UI for long and never panics; without an
//! output device (a headless machine, CI) `play` returns an error the app shows in the status bar.

use std::sync::Arc;

use boardcraft_core::audio::Mixer;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

/// A playing output stream, or none.
#[derive(Default)]
pub struct AudioOut {
    stream: Option<cpal::Stream>,
}

impl AudioOut {
    pub fn is_playing(&self) -> bool {
        self.stream.is_some()
    }

    /// Play `mixer` from `seconds` into the animatic on the default output device.
    pub fn play(&mut self, mixer: Arc<Mixer>, seconds: f64) -> Result<(), String> {
        self.stop();
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or_else(|| "no sound output device".to_string())?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate();
        let channels = usize::from(config.channels()).max(1);
        let from = (seconds.max(0.0) * rate as f64) as u64;
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config.config(), mixer, rate, channels, from),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config.config(), mixer, rate, channels, from),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config.config(), mixer, rate, channels, from),
            cpal::SampleFormat::I32 => build::<i32>(&device, &config.config(), mixer, rate, channels, from),
            other => return Err(format!("unsupported output sample format {other}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        // Dropping the stream stops it.
        self.stream = None;
    }
}

fn build<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    mixer: Arc<Mixer>,
    rate: u32,
    channels: usize,
    from: u64,
) -> Result<cpal::Stream, String> {
    let mut pos = from;
    let mut buf: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                buf.resize(out.len(), 0.0);
                mixer.render(rate, channels, pos, &mut buf);
                for (o, s) in out.iter_mut().zip(&buf) {
                    *o = T::from_sample(*s);
                }
                pos = pos.saturating_add((out.len() / channels) as u64);
            },
            |e| log::warn!("sound output: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_and_stop_never_panic_without_a_device() {
        let mut out = AudioOut::default();
        // On a machine without sound this is an error; with one it plays silence. Either way it
        // returns.
        let r = out.play(Arc::new(Mixer::default()), 0.0);
        assert_eq!(out.is_playing(), r.is_ok());
        out.stop();
        assert!(!out.is_playing());
    }
}
