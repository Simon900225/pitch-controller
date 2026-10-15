use std::sync::mpsc::SyncSender;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

pub const BUFFER_SIZE: usize = 2048;
pub const HOP_SIZE: usize = 512;

pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub sample_rate: f64,
}

struct Ring {
    samples: Vec<f32>,
    since_emit: usize,
}

impl Ring {
    fn push(&mut self, input: &[f32]) -> Vec<Vec<f32>> {
        self.samples.extend_from_slice(input);
        self.since_emit += input.len();
        if self.samples.len() > BUFFER_SIZE * 2 {
            let drop_n = self.samples.len() - BUFFER_SIZE;
            self.samples.drain(..drop_n);
        }
        let mut out = Vec::new();
        while self.since_emit >= HOP_SIZE && self.samples.len() >= BUFFER_SIZE {
            self.since_emit -= HOP_SIZE;
            let start = self.samples.len() - BUFFER_SIZE;
            out.push(self.samples[start..].to_vec());
        }
        out
    }
}

pub fn start_input(tx: SyncSender<AudioFrame>) -> Result<(Stream, f64), String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No microphone found".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|err| format!("Couldn't open the microphone: {err}"))?;
    let sample_rate = supported.sample_rate() as f64;
    let channels = supported.channels() as usize;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();

    let err_fn = |err| eprintln!("microphone error: {err}");
    let stream = match format {
        SampleFormat::F32 => {
            build::<f32>(&device, &config, channels, sample_rate, tx, |s| s, err_fn)?
        }
        SampleFormat::F64 => build::<f64>(
            &device,
            &config,
            channels,
            sample_rate,
            tx,
            |s| s as f32,
            err_fn,
        )?,
        SampleFormat::I16 => build::<i16>(
            &device,
            &config,
            channels,
            sample_rate,
            tx,
            |s| s as f32 / i16::MAX as f32,
            err_fn,
        )?,
        SampleFormat::U16 => build::<u16>(
            &device,
            &config,
            channels,
            sample_rate,
            tx,
            |s| (s as f32 - 32768.0) / 32768.0,
            err_fn,
        )?,
        SampleFormat::I32 => build::<i32>(
            &device,
            &config,
            channels,
            sample_rate,
            tx,
            |s| s as f32 / i32::MAX as f32,
            err_fn,
        )?,
        other => return Err(format!("Unsupported microphone format: {other:?}")),
    };
    stream
        .play()
        .map_err(|err| format!("Couldn't start the microphone: {err}"))?;
    Ok((stream, sample_rate))
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    channels: usize,
    sample_rate: f64,
    tx: SyncSender<AudioFrame>,
    convert: fn(T) -> f32,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, String>
where
    T: cpal::SizedSample + 'static,
{
    let mut ring = Ring {
        samples: Vec::new(),
        since_emit: 0,
    };
    device
        .build_input_stream(
            config.clone(),
            move |data: &[T], _| {
                let frames = data.len() / channels.max(1);
                let mut mono = Vec::with_capacity(frames);
                for frame in 0..frames {
                    let mut sum = 0.0;
                    for channel in 0..channels.max(1) {
                        sum += convert(data[frame * channels.max(1) + channel]);
                    }
                    mono.push(sum / channels.max(1) as f32);
                }
                for samples in ring.push(&mono) {
                    let _ = tx.try_send(AudioFrame {
                        samples,
                        sample_rate,
                    });
                }
            },
            err_fn,
            None,
        )
        .map_err(|err| format!("Couldn't open the microphone: {err}"))
}
