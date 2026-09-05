//! Audio preparation and playback, separate from clip persistence and GUI widgets.
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::domain::AudioTrim;
use crate::media::{JobContext, MAX_AUDIO_SECONDS, run_process};

pub struct AudioPreview {
    pub path: PathBuf,
    pub duration: f64,
    pub peaks: Vec<[f32; 2]>,
}

pub fn prepare_preview(
    source: &Path,
    portable_root: &Path,
    directory: &Path,
    context: &JobContext,
) -> Result<AudioPreview, String> {
    let ffmpeg = crate::converter::ffmpeg_path(portable_root)
        .ok_or("FFmpeg is missing from the portable tools folder.")?;
    let source = source.canonicalize().map_err(|error| error.to_string())?;
    let path = directory.join("preview.wav");
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-y",
            "-protocol_whitelist",
            "file,pipe",
            "-i",
        ])
        .arg(source)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-t",
            "3601",
            "-ac",
            "1",
            "-ar",
            "44100",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&path);
    context.report("Preparing audio preview...");
    run_process(&mut command, directory, context, Duration::from_secs(180))?;
    waveform(&path, context)
}

pub fn waveform(path: &Path, context: &JobContext) -> Result<AudioPreview, String> {
    let mut reader =
        hound::WavReader::open(path).map_err(|error| format!("Invalid preview audio: {error}"))?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 44100
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("Preview audio must be mono 44.1 kHz PCM WAV.".into());
    }
    let duration = f64::from(reader.duration()) / f64::from(spec.sample_rate);
    if duration <= 0.0 || duration > MAX_AUDIO_SECONDS {
        return Err("Use an audio source between one sample and 60 minutes long.".into());
    }
    context.report("Building waveform...");
    let bucket_size = (reader.len() as usize).div_ceil(2048).max(1);
    let mut peaks = Vec::new();
    let mut peak = [0.0_f32; 2];
    let mut count = 0;
    for (index, sample) in reader.samples::<i16>().enumerate() {
        if index % 44100 == 0 {
            context.check()?;
        }
        let sample = f32::from(sample.map_err(|error| error.to_string())?) / 32768.0;
        peak[0] = peak[0].min(sample);
        peak[1] = peak[1].max(sample);
        count += 1;
        if count == bucket_size {
            peaks.push(peak);
            peak = [0.0; 2];
            count = 0;
        }
    }
    if count > 0 {
        peaks.push(peak);
    }
    Ok(AudioPreview {
        path: path.to_owned(),
        duration,
        peaks,
    })
}

#[derive(Default)]
pub struct PreviewPlayer {
    #[cfg(windows)]
    playback: Option<(rodio::Player, rodio::MixerDeviceSink)>,
    start: f64,
}

impl PreviewPlayer {
    pub fn play(
        &mut self,
        preview: &AudioPreview,
        trim: AudioTrim,
        cursor: f64,
        volume: f32,
    ) -> Result<(), String> {
        let (start, end) = trim.range(preview.duration).map_err(str::to_owned)?;
        self.stop();
        self.start = cursor.clamp(start, (end - (1.0 / 44100.0)).max(start));
        #[cfg(windows)]
        {
            use rodio::Source;
            let file = File::open(&preview.path).map_err(|error| error.to_string())?;
            let mut decoder = rodio::Decoder::try_from(file).map_err(|error| error.to_string())?;
            decoder
                .try_seek(Duration::from_secs_f64(self.start))
                .map_err(|error| error.to_string())?;
            let mut device = rodio::DeviceSinkBuilder::open_default_sink()
                .map_err(|error| format!("Could not open the audio output: {error}"))?;
            device.log_on_drop(false);
            let player = rodio::Player::connect_new(device.mixer());
            player.set_volume(volume);
            player.append(decoder.take_duration(Duration::from_secs_f64(end - self.start)));
            self.playback = Some((player, device));
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = (File::open(&preview.path), end, volume);
            Err("Audio playback is available in the Windows application.".into())
        }
    }

    pub fn stop(&mut self) {
        #[cfg(windows)]
        {
            self.playback = None;
        }
    }
    pub fn active(&self) -> bool {
        #[cfg(windows)]
        {
            self.playback
                .as_ref()
                .is_some_and(|(player, _)| !player.empty())
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub fn paused(&self) -> bool {
        #[cfg(windows)]
        {
            self.playback
                .as_ref()
                .is_some_and(|(player, _)| player.is_paused())
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
    pub fn toggle_pause(&self) {
        #[cfg(windows)]
        if let Some((player, _)) = &self.playback {
            if player.is_paused() {
                player.play();
            } else {
                player.pause();
            }
        }
    }
    pub fn position(&self) -> f64 {
        #[cfg(windows)]
        if let Some((player, _)) = &self.playback {
            return self.start + player.get_pos().as_secs_f64();
        }
        self.start
    }
    pub fn set_volume(&self, volume: f32) {
        #[cfg(windows)]
        if let Some((player, _)) = &self.playback {
            player.set_volume(volume);
        }
        #[cfg(not(windows))]
        let _ = volume;
    }
}
