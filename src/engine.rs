use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use enigo::{Direction, Enigo, Keyboard, Settings};
use uuid::Uuid;

use crate::audio::{self, AudioFrame};
use crate::band_mapper::BandMapper;
use crate::buttons::{SnesButton, enigo_key};
use crate::input_sink::DiffingInputSink;
use crate::pitch_detector::PitchDetector;
use crate::pitch_smoother::PitchSmoother;
use crate::profile::{BandTriggerMode, PitchBand, Profile};

const HISTORY_SECONDS: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalibrationPhase {
    Idle,
    Low,
    High,
    Done,
}

#[derive(Clone, Debug)]
pub struct PitchPoint {
    pub time: f64,
    pub note: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    /// True while the audio thread is accepting frames.
    #[allow(dead_code)]
    pub running: bool,
    pub note: Option<f64>,
    pub frequency_hz: Option<f64>,
    pub rms: f32,
    pub confidence: f32,
    pub voiced: bool,
    pub active_band_ids: HashSet<Uuid>,
    pub pressed: BTreeSet<SnesButton>,
    pub history: Vec<PitchPoint>,
    pub now: f64,
    pub calibrating: bool,
    pub calibration_phase: CalibrationPhase,
    pub input_error: Option<String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            running: false,
            note: None,
            frequency_hz: None,
            rms: 0.0,
            confidence: 0.0,
            voiced: false,
            active_band_ids: HashSet::new(),
            pressed: BTreeSet::new(),
            history: Vec::new(),
            now: 0.0,
            calibrating: false,
            calibration_phase: CalibrationPhase::Idle,
            input_error: None,
        }
    }
}

enum Command {
    Profile(Profile),
    Output(bool),
    TestMode(bool),
    Start,
    Stop,
    BeginCalibration,
    Shutdown,
}

struct Worker {
    detector: PitchDetector,
    smoother: PitchSmoother,
    mapper: BandMapper,
    sink: DiffingInputSink,
    enigo: Option<Enigo>,
    profile: Profile,
    output_enabled: bool,
    test_mode: bool,
    running: bool,
    history: Vec<PitchPoint>,
    last_note: Option<f64>,
    last_hz: Option<f64>,
    last_rms: f32,
    last_confidence: f32,
    last_voiced: bool,
    last_active: HashSet<Uuid>,
    last_pressed: BTreeSet<SnesButton>,
    calibrating: bool,
    phase: CalibrationPhase,
    calibration_samples: Vec<f64>,
    calibration_deadline: Option<f64>,
    calibration_low: Option<f64>,
    origin: Instant,
    input_error: Option<String>,
}

impl Worker {
    fn new(profile: Profile) -> Self {
        let settings = Settings {
            linux_delay: 0,
            ..Settings::default()
        };
        let (enigo, input_error) = match Enigo::new(&settings) {
            Ok(enigo) => (Some(enigo), None),
            Err(err) => (
                None,
                Some(format!("Couldn't create keyboard output: {err}")),
            ),
        };
        let smoother =
            PitchSmoother::with_thresholds(profile.gate_rms, profile.confidence_threshold);
        let mapper = BandMapper::new(
            profile.bands.clone(),
            profile.hysteresis_semitones,
            profile.min_hold_ms,
        );
        Self {
            detector: PitchDetector::with_sample_rate(44_100.0),
            smoother,
            mapper,
            sink: DiffingInputSink::default(),
            enigo,
            profile,
            output_enabled: false,
            test_mode: true,
            running: false,
            history: Vec::new(),
            last_note: None,
            last_hz: None,
            last_rms: 0.0,
            last_confidence: 0.0,
            last_voiced: false,
            last_active: HashSet::new(),
            last_pressed: BTreeSet::new(),
            calibrating: false,
            phase: CalibrationPhase::Idle,
            calibration_samples: Vec::new(),
            calibration_deadline: None,
            calibration_low: None,
            origin: Instant::now(),
            input_error,
        }
    }

    fn now(&self) -> f64 {
        self.origin.elapsed().as_secs_f64()
    }

    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Shutdown => {
                self.release();
                return true;
            }
            Command::Profile(profile) => {
                self.smoother.gate_rms = profile.gate_rms;
                self.smoother.confidence_threshold = profile.confidence_threshold;
                self.mapper.configure(&profile);
                self.profile = profile;
            }
            Command::Output(enabled) => {
                self.output_enabled = enabled;
                if !enabled {
                    self.release();
                }
            }
            Command::TestMode(enabled) => {
                self.test_mode = enabled;
                if enabled {
                    self.release();
                }
            }
            Command::Start => {
                self.running = true;
            }
            Command::Stop => {
                self.running = false;
                self.release();
                self.smoother.reset();
                self.mapper.reset();
                self.clear_live();
            }
            Command::BeginCalibration => {
                self.calibrating = true;
                self.phase = CalibrationPhase::Low;
                self.calibration_samples.clear();
                self.calibration_low = None;
                self.calibration_deadline = Some(self.now() + 5.0);
                self.running = true;
            }
        }
        false
    }

    fn clear_live(&mut self) {
        self.last_note = None;
        self.last_hz = None;
        self.last_rms = 0.0;
        self.last_confidence = 0.0;
        self.last_voiced = false;
        self.last_active.clear();
        self.last_pressed.clear();
    }

    fn process(&mut self, frame: AudioFrame) -> Option<Vec<PitchBand>> {
        if !self.running {
            return None;
        }
        if (self.detector.sample_rate - frame.sample_rate).abs() > 1.0 {
            self.detector = PitchDetector::with_sample_rate(frame.sample_rate);
        }
        let detection = self.detector.detect(&frame.samples);
        let smoothed = self.smoother.process(detection);
        let now = self.now();
        let mapped = self.mapper.map(smoothed.note, now);
        let calibrated = self.tick_calibration(smoothed.note, now);

        let should_emit = self.output_enabled && !self.test_mode && self.running;
        if should_emit {
            self.emit(&mapped.buttons);
        } else {
            self.release();
        }

        self.last_note = smoothed.note;
        self.last_hz = smoothed.frequency_hz;
        self.last_rms = smoothed.rms;
        self.last_confidence = smoothed.confidence;
        self.last_voiced = smoothed.is_voiced;
        self.last_active = mapped.active_band_ids;
        self.last_pressed = mapped.buttons;
        self.history.push(PitchPoint {
            time: now,
            note: smoothed.note,
        });
        let cutoff = now - HISTORY_SECONDS;
        self.history.retain(|point| point.time >= cutoff);
        calibrated
    }

    fn emit(&mut self, buttons: &BTreeSet<SnesButton>) {
        let events = self.sink.apply(buttons);
        self.post(events);
    }

    fn release(&mut self) {
        let events = self.sink.release_all();
        self.post(events);
    }

    fn post(&mut self, events: Vec<(SnesButton, bool)>) {
        let Some(enigo) = self.enigo.as_mut() else {
            return;
        };
        for (button, down) in events {
            let Some(key) = enigo_key(self.profile.key_code(button)) else {
                continue;
            };
            let direction = if down {
                Direction::Press
            } else {
                Direction::Release
            };
            let _ = enigo.key(key, direction);
        }
    }

    fn tick_calibration(&mut self, note: Option<f64>, now: f64) -> Option<Vec<PitchBand>> {
        if !self.calibrating {
            return None;
        }
        let (Some(note), Some(deadline)) = (note, self.calibration_deadline) else {
            return None;
        };
        self.calibration_samples.push(note);
        if now < deadline {
            return None;
        }
        let value = percentile(
            &self.calibration_samples,
            if self.phase == CalibrationPhase::Low {
                0.2
            } else {
                0.8
            },
            if self.phase == CalibrationPhase::Low {
                55.0
            } else {
                72.0
            },
        );
        if self.phase == CalibrationPhase::Low {
            self.calibration_low = Some(value);
            self.phase = CalibrationPhase::High;
            self.calibration_samples.clear();
            self.calibration_deadline = Some(now + 5.0);
            None
        } else {
            let low = self.calibration_low.unwrap_or(55.0);
            let high = value.max(low + 4.0);
            self.phase = CalibrationPhase::Done;
            self.calibrating = false;
            let bands = calibrated_bands(low, high);
            self.profile.bands = bands.clone();
            self.mapper.configure(&self.profile);
            Some(bands)
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            running: self.running,
            note: self.last_note,
            frequency_hz: self.last_hz,
            rms: self.last_rms,
            confidence: self.last_confidence,
            voiced: self.last_voiced,
            active_band_ids: self.last_active.clone(),
            pressed: self.last_pressed.clone(),
            history: self.history.clone(),
            now: self.now(),
            calibrating: self.calibrating,
            calibration_phase: self.phase,
            input_error: self.input_error.clone(),
        }
    }
}

fn percentile(samples: &[f64], fraction: f64, fallback: f64) -> f64 {
    if samples.is_empty() {
        return fallback;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((sorted.len() - 1) as f64 * fraction) as usize;
    sorted[idx]
}

fn calibrated_bands(low: f64, high: f64) -> Vec<PitchBand> {
    let span = high - low;
    let edges: Vec<f64> = [0.0, 0.28, 0.52, 0.72, 1.0]
        .into_iter()
        .map(|t| low + span * t)
        .collect();
    let templates = [
        ("Walk", crate::profile::set([SnesButton::Right]), 0.55),
        (
            "Run",
            crate::profile::set([SnesButton::Right, SnesButton::Y]),
            0.35,
        ),
        (
            "Run+Jump",
            crate::profile::set([SnesButton::Right, SnesButton::Y, SnesButton::B]),
            0.12,
        ),
        ("Jump", crate::profile::set([SnesButton::B]), 0.95),
    ];
    templates
        .into_iter()
        .enumerate()
        .map(|(i, (name, buttons, hue))| {
            let mut band = PitchBand::new(name, edges[i], edges[i + 1], buttons, hue);
            band.trigger = BandTriggerMode::Hold;
            band
        })
        .collect()
}

/// Latest analysis, written by the audio thread and read by the UI.
#[derive(Clone)]
struct Published {
    snapshot: Snapshot,
    /// Bands produced by the most recent calibration, if the UI has not taken them yet.
    calibrated: Option<Vec<PitchBand>>,
}

pub struct Engine {
    cmd_tx: Option<Sender<Command>>,
    frame_tx: Option<SyncSender<AudioFrame>>,
    published: Arc<std::sync::Mutex<Published>>,
    worker: Option<JoinHandle<()>>,
    stream: Option<cpal::Stream>,
    pub running: bool,
    pub output_enabled: bool,
    pub test_mode: bool,
    pub audio_error: Option<String>,
    snapshot: Snapshot,
}

impl Engine {
    pub fn new(profile: Profile) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (frame_tx, frame_rx) = mpsc::sync_channel(8);
        let published = Arc::new(std::sync::Mutex::new(Published {
            snapshot: Snapshot::default(),
            calibrated: None,
        }));
        let published_worker = Arc::clone(&published);
        let worker = thread::Builder::new()
            .name("pitch-audio".to_string())
            .spawn(move || worker_loop(profile, cmd_rx, frame_rx, published_worker))
            .expect("audio thread");
        Self {
            cmd_tx: Some(cmd_tx),
            frame_tx: Some(frame_tx),
            published,
            worker: Some(worker),
            stream: None,
            running: false,
            output_enabled: false,
            test_mode: true,
            audio_error: None,
            snapshot: Snapshot::default(),
        }
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Pull the latest analysis and any bands written by calibration.
    pub fn poll(&mut self) -> Option<Vec<PitchBand>> {
        let mut published = self.published.lock().unwrap_or_else(|err| err.into_inner());
        self.snapshot = published.snapshot.clone();
        published.calibrated.take()
    }

    pub fn set_profile(&self, profile: Profile) {
        self.send(Command::Profile(profile));
    }

    pub fn set_output(&mut self, enabled: bool) {
        self.output_enabled = enabled;
        self.send(Command::Output(enabled));
    }

    pub fn set_test_mode(&mut self, enabled: bool) {
        self.test_mode = enabled;
        self.send(Command::TestMode(enabled));
    }

    pub fn start(&mut self) {
        if self.running {
            return;
        }
        let Some(tx) = self.frame_tx.clone() else {
            return;
        };
        match audio::start_input(tx) {
            Ok((stream, _)) => {
                self.stream = Some(stream);
                self.running = true;
                self.audio_error = None;
                if !self.test_mode {
                    self.set_output(true);
                }
                self.send(Command::Start);
            }
            Err(err) => {
                self.running = false;
                self.audio_error = Some(err);
            }
        }
    }

    pub fn stop(&mut self) {
        self.stream = None;
        self.running = false;
        self.send(Command::Stop);
    }

    pub fn toggle_output(&mut self) {
        self.set_output(!self.output_enabled);
    }

    pub fn begin_calibration(&mut self) {
        if !self.running {
            self.start();
        }
        if self.running {
            self.send(Command::BeginCalibration);
        }
    }

    fn send(&self, command: Command) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(command);
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Command::Shutdown);
        }
        self.stream.take();
        self.frame_tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn worker_loop(
    profile: Profile,
    cmd_rx: Receiver<Command>,
    frame_rx: Receiver<AudioFrame>,
    published: Arc<std::sync::Mutex<Published>>,
) {
    let mut worker = Worker::new(profile);

    loop {
        let mut shutdown = false;
        loop {
            match cmd_rx.try_recv() {
                Ok(command) => {
                    if worker.handle(command) {
                        shutdown = true;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    worker.release();
                    return;
                }
            }
        }
        if shutdown {
            break;
        }

        let calibrated = match frame_rx.recv_timeout(Duration::from_millis(15)) {
            Ok(frame) => worker.process(frame),
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => None,
        };
        if let Ok(mut slot) = published.lock() {
            slot.snapshot = worker.snapshot();
            if calibrated.is_some() {
                slot.calibrated = calibrated;
            }
        }
    }
    worker.release();
}
