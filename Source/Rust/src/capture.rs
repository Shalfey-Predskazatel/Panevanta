use crate::{
    cadence::{self, Probe},
    engine::{self, EngineApi, Parameters},
    nr,
    preview::Preview,
    sound::Cue,
    win::{self, Display, Overlay, SourceId},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows::{
    Foundation::TypedEventHandler,
    Graphics::{Capture::*, DirectX::DirectXPixelFormat, SizeInt32},
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0},
        System::{
            Threading::{CreateWaitableTimerExW, SetWaitableTimer, WaitForSingleObject},
            WinRT::{
                Graphics::Capture::IGraphicsCaptureItemInterop, RO_INIT_MULTITHREADED,
                RoInitialize, RoUninitialize,
            },
        },
    },
    core::{PCWSTR, factory},
};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub input: i32,
    pub multiplier: i32,
    pub height: i32,
    pub quality: i32,
    pub sr: bool,
    pub scale: f32,
    pub hdr: bool,
    pub peak: i32,
    pub nr: bool,
    pub nr_detail: bool,
    pub nr_resolution: i32,
    pub nr_style: u32,
    pub nr_passes: u32,
    pub nr_tone: i32,
    pub nr_structure: i32,
    pub nr_skin: i32,
    pub nr_auto_mask: bool,
    pub nr_ui_correction: bool,
    pub streamer: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input: 0,
            multiplier: 4,
            height: 1080,
            quality: 1,
            sr: true,
            scale: 1.5,
            hdr: true,
            peak: 0,
            nr: true,
            nr_detail: true,
            nr_resolution: 480,
            nr_style: 0,
            nr_passes: 1,
            nr_tone: 100,
            nr_structure: 100,
            nr_skin: 100,
            nr_auto_mask: true,
            nr_ui_correction: true,
            streamer: false,
        }
    }
}

impl Settings {
    pub fn normalized(mut self) -> Self {
        let default = Self::default();
        let tone = 0..=100;
        macro_rules! correct {
            ($field:ident, $values:expr) => {
                if !$values.contains(&self.$field) {
                    self.$field = default.$field;
                }
            };
        }
        correct!(input, [0, 24, 25, 30, 48, 50, 60, 72, 90, 120]);
        correct!(multiplier, 0..=6);
        correct!(height, [0, 1080, 900, 720]);
        correct!(quality, 0..=2);
        correct!(scale, [1.01, 1.25, 1.5, 2.]);
        correct!(peak, [0, 250, 350, 500, 750, 1000]);
        correct!(nr_resolution, [0, 480, 720, 900, 1080, 1440, 2160]);
        correct!(nr_style, 0..=2);
        correct!(nr_passes, 1..=3);
        correct!(nr_tone, tone);
        correct!(nr_structure, tone);
        correct!(nr_skin, tone);
        self
    }

    pub fn validated(self) -> Result<Self> {
        (self.normalized() == self)
            .then_some(self)
            .ok_or_else(|| "Неподдерживаемое значение настройки.".into())
    }

    fn any(self) -> bool {
        self.multiplier != 1 || self.sr || self.hdr || self.nr
    }

    fn pipeline(self) -> Self {
        let defaults = Self::default();
        Self {
            nr: defaults.nr,
            nr_detail: defaults.nr_detail,
            nr_resolution: defaults.nr_resolution,
            nr_style: defaults.nr_style,
            nr_passes: defaults.nr_passes,
            nr_tone: defaults.nr_tone,
            nr_structure: defaults.nr_structure,
            nr_skin: defaults.nr_skin,
            nr_auto_mask: defaults.nr_auto_mask,
            nr_ui_correction: defaults.nr_ui_correction,
            ..self
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Request {
    source: SourceId,
    settings: Settings,
    panel: usize,
    generation: u64,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub phase: &'static str,
    pub message: String,
    pub input: i32,
    pub factor: i32,
    pub hz: f64,
    pub display_hdr: bool,
    pub hdr_output: bool,
    pub auto_input: bool,
    pub cadence_locked: bool,
    pub cadence_observed: Option<f64>,
    pub cadence_error: Option<String>,
    pub capture_fps: Option<f64>,
    pub present_fps: Option<f64>,
    pub latency_ms: Option<f64>,
    pub drops: u64,
    pub resets: u32,
    pub backward_frames: u32,
    pub width: i32,
    pub height: i32,
    pub overlay: bool,
    pub bypassed: bool,
    pub log: Option<PathBuf>,
    pub log_available: bool,
    pub nr_width: u32,
    pub nr_height: u32,
    pub nr_ms: Option<f64>,
    pub nr_loading: bool,
    pub nr_error: Option<String>,
    pub streamer: bool,
    pub preview: bool,
}

struct Shared {
    request: Option<Request>,
    status: Status,
    generation: u64,
    worker_generation: Option<u64>,
    ready_generation: Option<u64>,
    sound: bool,
    closed: bool,
}

impl Shared {
    fn new(sound: bool) -> Self {
        Self {
            request: None,
            status: idle("Выберите окно или экран."),
            generation: 0,
            worker_generation: None,
            ready_generation: None,
            sound,
            closed: false,
        }
    }
}

pub struct Controller {
    state: Arc<(Mutex<Shared>, Condvar)>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Controller {
    pub fn new(program: PathBuf, sound: bool) -> Self {
        let state = Arc::new((Mutex::new(Shared::new(sound)), Condvar::new()));
        let worker_state = Arc::clone(&state);
        let worker = thread::spawn(move || work(&program, &worker_state));
        Self {
            state,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn toggle(
        &self,
        source: Option<SourceId>,
        settings: Settings,
        panel: usize,
        start_only: bool,
    ) -> Result<()> {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap();
        if state.request.is_some() {
            if !start_only {
                stop_request(&mut state, "Обработка остановлена.");
            }
        } else if state.worker_generation.is_some() {
            return Ok(());
        } else {
            let source = source.ok_or("Выберите источник захвата.")?;
            if settings.streamer && source.screen {
                return Err("Для режима стримера выберите окно.".into());
            }
            if !settings.streamer && !settings.any() {
                return Err("Включите хотя бы один RTX-эффект.".into());
            }
            if source.bounds().is_none() {
                return Err("Источник недоступен. Обновите список.".into());
            }
            state.generation += 1;
            state.request = Some(Request {
                source,
                settings,
                panel,
                generation: state.generation,
            });
            state.status = Status {
                phase: "starting",
                message: "Запускаю захват и RTX…".into(),
                ..Default::default()
            };
            if settings.streamer {
                Cue::Stream
            } else {
                Cue::Start
            }
            .play(state.sound);
        }
        wake.notify_one();
        Ok(())
    }

    pub fn stop(&self) {
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap();
        if state.request.is_some() {
            stop_request(&mut state, "Обработка остановлена.");
            wake.notify_one();
        }
    }

    pub fn configure(&self, settings: Settings, sound: bool) {
        let mut state = self.state.0.lock().unwrap();
        state.sound = sound;
        if let Some(request) = &mut state.request {
            request.settings = Settings {
                streamer: request.settings.streamer,
                ..settings
            };
            self.state.1.notify_one();
        }
    }

    pub fn status(&self) -> Status {
        self.state.0.lock().unwrap().status.clone()
    }

    pub fn shutdown(&self) {
        let (lock, wake) = &*self.state;
        {
            let mut state = lock.lock().unwrap();
            state.request = None;
            state.closed = true;
            wake.notify_one();
        }
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}

fn idle(message: &str) -> Status {
    Status {
        phase: "idle",
        message: message.into(),
        ..Default::default()
    }
}

fn stop_request(state: &mut Shared, message: &str) {
    state.request = None;
    if state.worker_generation.is_some() {
        state.status.phase = "stopping";
        state.status.message = message.into();
    } else {
        state.status = idle(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_start_while_worker_is_waiting_returns_to_idle() {
        let mut state = Shared::new(false);
        state.status.phase = "starting";
        stop_request(&mut state, "Обработка остановлена.");
        assert_eq!(state.status.phase, "idle");

        state.worker_generation = Some(1);
        stop_request(&mut state, "Обработка остановлена.");
        assert_eq!(state.status.phase, "stopping");
    }
}

fn work(program: &Path, state: &(Mutex<Shared>, Condvar)) {
    let apartment = Apartment::enter();
    let mut attempt = 0;
    loop {
        let request = {
            let (lock, wake) = state;
            let mut shared = lock.lock().unwrap();
            while shared.request.is_none() && !shared.closed {
                shared = wake.wait(shared).unwrap();
            }
            if shared.closed {
                return;
            }
            let request = shared.request.unwrap();
            shared.worker_generation = Some(request.generation);
            request
        };
        let result = match &apartment {
            Ok(_) => run(program, state, request, &mut attempt),
            Err(err) => Err(err.clone()),
        };
        let mut shared = state.0.lock().unwrap();
        shared.worker_generation = None;
        if shared.closed {
            return;
        }
        let cancelled = shared.request.is_none();
        shared.request = None;
        let last_log = shared.status.log.take();
        let log_available = shared.status.log_available;
        let played_ready = shared.ready_generation == Some(request.generation);
        let (status, cue) = match (cancelled, result) {
            (true, _) => (
                idle(&shared.status.message),
                played_ready.then_some(Cue::Stop),
            ),
            (false, Ok(())) => (
                idle("Обработка остановлена."),
                played_ready.then_some(Cue::Stop),
            ),
            (false, Err(message)) => (
                Status {
                    phase: "error",
                    message,
                    ..Default::default()
                },
                Some(Cue::Error),
            ),
        };
        shared.status = status;
        shared.status.log = last_log;
        shared.status.log_available = log_available;
        let sound = shared.sound;
        drop(shared);
        if let Some(cue) = cue {
            cue.play(sound);
        }
    }
}

fn run(
    program: &Path,
    state: &(Mutex<Shared>, Condvar),
    first: Request,
    attempt: &mut u64,
) -> Result<()> {
    let api = EngineApi::load(program)?;
    let preview = if first.settings.streamer {
        let source = first.source.bounds().ok_or("Источник недоступен.")?;
        let area = first
            .source
            .work_area()
            .ok_or("Не найден монитор для окна вывода.")?;
        Some(Preview::new(
            area,
            (source.right - source.left, source.bottom - source.top),
            first.panel,
        )?)
    } else {
        None
    };
    let mut size = None;
    let mut resets = 0;
    let mut locked = 0;
    let mut auto = None;
    loop {
        let Some(request) = state
            .0
            .lock()
            .unwrap()
            .request
            .filter(|r| r.generation == first.generation)
        else {
            return Ok(());
        };
        if !request.settings.streamer && !request.settings.any() {
            auto.take();
            publish(
                state,
                request,
                &Status {
                    phase: "running",
                    bypassed: true,
                    message: "Эффекты выключены. Выберите эффект для возобновления обработки."
                        .into(),
                    ..Default::default()
                },
            );
            let shared = state.0.lock().unwrap();
            drop(
                state
                    .1
                    .wait_while(shared, |shared| {
                        shared.request.is_some_and(|active| {
                            active.generation == request.generation && !active.settings.any()
                        })
                    })
                    .unwrap(),
            );
            continue;
        }
        if request.settings.input == 0 {
            if auto.is_none() && locked == 0 {
                auto = Some(Automatic::start(program.to_path_buf(), request.source));
            }
        } else {
            auto.take();
            locked = 0;
        }
        let input = match (request.settings.input, locked) {
            (0, 0) => 30,
            (0, rate) | (rate, _) => rate,
        };
        let display = win::display(request.source);
        let output_display = preview
            .as_ref()
            .map_or(display, |window| win::display(window.monitor_source()));
        let factor = if request.settings.multiplier == 0 {
            cadence::multiplier(input, output_display.hz)
        } else {
            request.settings.multiplier
        };
        *attempt += 1;
        let started = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let log = std::env::temp_dir().join(format!(
            "panevanta-rust-{}-{started}-{attempt}.log",
            std::process::id()
        ));
        let mut confirmed_rate = locked;
        let result = session(
            &api,
            program,
            state,
            request,
            size,
            display,
            output_display,
            preview.as_ref(),
            input,
            factor,
            &mut confirmed_rate,
            auto.as_ref(),
            &log,
            resets,
        )?;
        if confirmed_rate != 0 {
            locked = confirmed_rate;
            auto.take();
        }
        match result {
            End::Stop => return Ok(()),
            End::Change => {}
            End::Size(current) => {
                size = Some(current);
                auto.take();
            }
            End::Cadence(value) => {
                locked = value;
                auto.take();
            }
            End::Lag => resets += 1,
        }
    }
}

enum End {
    Stop,
    Change,
    Size(SizeInt32),
    Cadence(i32),
    Lag,
}

fn session(
    api: &EngineApi,
    program: &Path,
    state: &(Mutex<Shared>, Condvar),
    request: Request,
    size: Option<SizeInt32>,
    display: Display,
    output_display: Display,
    preview: Option<&Preview>,
    input: i32,
    factor: i32,
    confirmed_rate: &mut i32,
    probe: Option<&Automatic>,
    log: &Path,
    resets: u32,
) -> Result<End> {
    let mut status = Status {
        phase: "starting",
        message: "Подготавливаю вывод…".into(),
        input,
        factor,
        hz: output_display.hz,
        display_hdr: output_display.hdr,
        streamer: preview.is_some(),
        preview: preview.is_some_and(Preview::visible),
        auto_input: request.settings.input == 0,
        cadence_locked: *confirmed_rate != 0,
        resets,
        log: Some(log.to_owned()),
        ..Default::default()
    };
    publish(state, request, &status);
    let capture = Capture::new(api, request.source, size)?;
    let size = capture.size;
    if let Some(window) = preview {
        window.set_aspect(size.Width, size.Height);
    }
    let settings = request.settings;
    let mut monitor_size = request.source.monitor_size().unwrap_or((1920, 1080));
    let mut nr_pass = nr::Live::default();
    let pipeline = settings.pipeline();
    let height = if settings.height == 0 {
        size.Height
    } else {
        size.Height.min(settings.height)
    };
    let width = if height == size.Height {
        size.Width
    } else {
        (((size.Width as f64 * height as f64 / size.Height as f64).round() as i32) & !1).max(16)
    };
    status.width = width;
    status.height = height;
    let parameters = Parameters {
        width,
        height,
        fps: input,
        factor,
        quality: settings.quality,
        sr: settings.sr.into(),
        hdr: settings.hdr.into(),
        display_hdr: if preview.is_some() {
            output_display.hdr
        } else {
            display.hdr
        }
        .into(),
        peak: settings.peak,
        white: display.white,
        scale: settings.scale,
    };
    let mut overlay =
        if preview.is_none() {
            let bounds = request.source.bounds().ok_or("Источник недоступен.")?;
            Some(Overlay::new(bounds).map_err(|e| {
                format!("Не удалось создать слой вывода, исключённый из захвата. {e}")
            })?)
        } else {
            None
        };
    let renderer_window = preview
        .map(Preview::render)
        .unwrap_or_else(|| overlay.as_ref().unwrap().render);
    let mut engine = None;
    let mut clock = request
        .source
        .screen
        .then(|| Clock::new(input))
        .transpose()?;
    let mut metrics = Metrics::default();
    let frequency = win::frequency();
    let started = Instant::now();
    let mut last_ui = started;
    let mut last_display = started;
    let mut last_time = 0;
    let mut first_tick = 0;
    let mut anchor = 0;
    let mut previous_tick = 0;
    let mut sequence_offset = 0;
    let mut previous_nr = None;
    let mut captured = 0;
    let mut sample_times = [(0, 0); 1024];
    let mut pending_settings: Option<(Settings, Instant)> = None;
    let mut hdr_scanned = false;

    loop {
        let current = state.0.lock().unwrap().request;
        let Some(current) = current.filter(|active| active.generation == request.generation) else {
            return Ok(End::Stop);
        };
        if preview.is_some_and(Preview::closed) {
            return Ok(End::Stop);
        }
        if preview.is_none() && !current.settings.any() {
            return Ok(End::Change);
        }
        let other_settings = current.settings.pipeline();
        if other_settings != pipeline {
            match pending_settings {
                Some((value, since)) if value == other_settings => {
                    if since.elapsed() >= Duration::from_millis(120) {
                        return Ok(End::Change);
                    }
                }
                _ => pending_settings = Some((other_settings, Instant::now())),
            }
        } else {
            pending_settings = None;
        }

        if *confirmed_rate == 0
            && let Some(rate) = probe
                .map(|probe| probe.rate.load(Ordering::Relaxed))
                .filter(|&rate| rate > 0)
        {
            if rate != input {
                return Ok(End::Cadence(rate));
            }
            *confirmed_rate = rate;
            status.cadence_locked = true;
        }
        if capture.closed.load(Ordering::Relaxed) {
            return Err("Источник закрыт.".into());
        }
        if last_display.elapsed() >= Duration::from_secs(4) {
            last_display = Instant::now();
            monitor_size = request.source.monitor_size().unwrap_or(monitor_size);
            if win::display(request.source) != display
                || preview
                    .is_some_and(|window| win::display(window.monitor_source()) != output_display)
            {
                return Ok(End::Change);
            }
        }

        win::pump_messages();
        if last_ui.elapsed() >= Duration::from_millis(240) {
            last_ui = Instant::now();
            if let Some(probe) = probe {
                let observed = f64::from_bits(probe.observed.load(Ordering::Relaxed));
                status.cadence_observed = (observed > 0.).then_some(observed);
                status.cadence_error = probe.error.lock().unwrap().clone();
            }
            metrics.read(log, &mut status, captured, &sample_times);
            if !hdr_scanned && captured > 0 {
                let contents = fs::read_to_string(log).unwrap_or_default();
                hdr_scanned = contents.contains("Panevanta native");
                status.log_available = hdr_scanned;
                status.hdr_output =
                    contents.contains("Output color space RGB_FULL_G2084_NONE_P2020");
            }
            if status.phase == "starting" && hdr_scanned && metrics.last_qpc > 0 {
                status.phase = "running";
                status.message = if settings.hdr && !status.hdr_output {
                    "Обработка работает. Для вывода HDR включите HDR в Windows.".into()
                } else {
                    "Обработка работает.".into()
                };
            }
            if status.phase == "starting" && started.elapsed() > Duration::from_secs(22) {
                return Err("Движок не вывел первый кадр. Проверьте журнал.".into());
            }
            if (overlay.as_ref().is_some_and(Overlay::visible)
                || preview.is_some_and(Preview::drawable))
                && started.elapsed() >= Duration::from_secs(8)
                && status
                    .capture_fps
                    .is_some_and(|value| value >= input as f64 * 0.5)
                && (metrics.late >= 3
                    || (metrics.last_qpc > 0 && win::qpc() - metrics.last_qpc > frequency * 2))
            {
                return Ok(End::Lag);
            }
            if let Some(overlay) = &mut overlay {
                let bounds = request.source.bounds();
                overlay.update(
                    status.phase == "running"
                        && bounds.is_some()
                        && request.source.can_show(request.panel),
                    bounds,
                    request.panel,
                );
                status.overlay = overlay.visible();
            }
            if let Some(window) = preview {
                if status.phase == "running" && !window.visible() {
                    window.show();
                }
                status.preview = window.visible();
            }
            publish(state, request, &status);
        }

        let mut tick = if let Some(clock) = &mut clock {
            clock.wait()?
        } else {
            let _ = capture.pending.recv_timeout(Duration::from_millis(80));
            0
        };
        let Some(frame) = capture.latest() else {
            if let Some(renderer) = &engine {
                engine::Engine::submit(renderer, None, 0, 0)?;
            }
            continue;
        };
        let current_size = frame.0.ContentSize().map_err(message)?;
        if current_size != size {
            return if valid_size(current_size) {
                Ok(End::Size(current_size))
            } else {
                Err("Источник изменил размер на неподдерживаемый.".into())
            };
        }
        let time = frame.time()?;
        if time <= last_time || win::qpc() - time > frequency / 5 {
            continue;
        }
        if clock.is_none() {
            if anchor == 0 {
                anchor = time;
            }
            tick = 1 + ((time - anchor) as f64 * input as f64 / frequency as f64).round() as i64;
            if tick <= previous_tick {
                continue;
            }
        }
        let mut texture = engine::texture(&frame.0.Surface().map_err(message)?).map_err(message)?;
        let live = current.settings;
        let nr_limit = match live.nr_resolution {
            0 => monitor_size,
            height => ((height * 16 / 9) & !1, height),
        };
        let nr_size = nr::dimensions(size.Width, size.Height, nr_limit);
        let nr_options = live.into();
        let start = Instant::now();
        status.nr_loading = false;
        match nr_pass.frame(program, &texture, nr_size, display, live.nr, nr_options) {
            Ok(Some(processed)) => {
                texture = processed;
                let ms = start.elapsed().as_secs_f64() * 1000.;
                status.nr_ms = Some(status.nr_ms.map_or(ms, |value| value * 0.85 + ms * 0.15));
                status.nr_width = nr_size.0;
                status.nr_height = nr_size.1;
                status.nr_error = None;
            }
            result => {
                status.nr_width = 0;
                status.nr_height = 0;
                status.nr_ms = None;
                status.nr_loading = live.nr && result.is_ok();
                if let Err(error) = result {
                    if status.nr_error.as_deref() != Some(error.as_str()) {
                        status.nr_error = Some(error);
                    }
                } else {
                    status.nr_error = None;
                }
            }
        }
        if engine.is_none() {
            engine = Some(api.open(&texture, renderer_window, &parameters, log)?);
            if win::qpc() - time > frequency / 5 {
                continue;
            }
        }
        if first_tick == 0 {
            first_tick = tick;
        }
        let current_nr = (status.nr_height > 0).then_some((nr_size, nr_options));
        if captured > 0 && previous_nr != current_nr {
            // Do not interpolate between raw and NR images, or across a changed NR profile.
            sequence_offset += 1;
        }
        let sequence = tick - first_tick + 1 + sequence_offset;
        engine
            .as_ref()
            .unwrap()
            .submit(Some(&texture), sequence, time)?;
        sample_times[sequence as usize & 1023] = (sequence, time);
        captured += 1;
        previous_nr = current_nr;
        last_time = time;
        previous_tick = tick;
    }
}

fn publish(state: &(Mutex<Shared>, Condvar), request: Request, status: &Status) {
    let mut shared = state.0.lock().unwrap();
    if shared
        .request
        .is_some_and(|active| active.generation == request.generation)
    {
        shared.status = status.clone();
        let ready =
            status.phase == "running" && shared.ready_generation != Some(request.generation);
        if ready {
            shared.ready_generation = Some(request.generation);
        }
        let sound = shared.sound;
        drop(shared);
        if ready {
            Cue::Ready.play(sound);
        }
    }
}

struct Apartment;
impl Apartment {
    fn enter() -> Result<Self> {
        unsafe {
            RoInitialize(RO_INIT_MULTITHREADED).map_err(message)?;
        }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}

struct Capture {
    item: GraphicsCaptureItem,
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    size: SizeInt32,
    closed: Arc<AtomicBool>,
    pending: Receiver<()>,
    arrived_token: i64,
    closed_token: i64,
}

impl Capture {
    fn new(api: &EngineApi, source: SourceId, size: Option<SizeInt32>) -> Result<Self> {
        if !GraphicsCaptureSession::IsSupported().map_err(message)? {
            return Err("Windows Graphics Capture недоступен.".into());
        }
        let interop =
            factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(message)?;
        let item: GraphicsCaptureItem = unsafe {
            if source.screen {
                interop.CreateForMonitor(source.monitor())
            } else {
                interop.CreateForWindow(source.window())
            }
        }
        .map_err(message)?;
        let size = size.unwrap_or(item.Size().map_err(message)?);
        if !valid_size(size) {
            return Err("Неподдерживаемый размер источника.".into());
        }
        let device = api
            .capture_device()
            .map_err(|e| format!("Не удалось создать устройство D3D11 NVIDIA. {e}"))?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &device,
            DirectXPixelFormat::R16G16B16A16Float,
            2,
            size,
        )
        .map_err(message)?;
        let session = pool.CreateCaptureSession(&item).map_err(message)?;
        session.SetIsCursorCaptureEnabled(false).map_err(message)?;
        let (sender, pending) = mpsc::sync_channel(1);
        let arrived_token = pool
            .FrameArrived(&TypedEventHandler::new(move |_, _| {
                let _ = sender.try_send(());
                Ok(())
            }))
            .map_err(message)?;
        let closed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&closed);
        let closed_token = item
            .Closed(&TypedEventHandler::new(move |_, _| {
                flag.store(true, Ordering::Relaxed);
                Ok(())
            }))
            .map_err(message)?;
        let capture = Self {
            item,
            session,
            pool,
            size,
            closed,
            pending,
            arrived_token,
            closed_token,
        };
        capture.session.StartCapture().map_err(message)?;
        Ok(capture)
    }

    fn latest(&self) -> Option<Frame> {
        let first = self.pool.TryGetNextFrame().ok().map(Frame);
        self.pool.TryGetNextFrame().ok().map(Frame).or(first)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.pool.RemoveFrameArrived(self.arrived_token);
        let _ = self.item.RemoveClosed(self.closed_token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

struct Frame(Direct3D11CaptureFrame);
impl Frame {
    fn time(&self) -> Result<i64> {
        let ticks = self.0.SystemRelativeTime().map_err(message)?.Duration;
        Ok((ticks as i128 * win::frequency() as i128 / 10_000_000) as i64)
    }
}
impl Drop for Frame {
    fn drop(&mut self) {
        let _ = self.0.Close();
    }
}

fn valid_size(size: SizeInt32) -> bool {
    (16..=8192).contains(&size.Width) && (16..=8192).contains(&size.Height)
}
fn message(error: windows::core::Error) -> String {
    error.to_string()
}

struct Clock {
    timer: HANDLE,
    last: i64,
    start: i64,
    input: i64,
}
impl Clock {
    fn new(input: i32) -> Result<Self> {
        let timer = unsafe {
            CreateWaitableTimerExW(None, PCWSTR::null(), 2, 0x100002)
                .or_else(|_| CreateWaitableTimerExW(None, PCWSTR::null(), 0, 0x100002))
        }
        .map_err(message)?;
        Ok(Self {
            timer,
            last: 0,
            start: win::qpc(),
            input: input.into(),
        })
    }
    fn wait(&mut self) -> Result<i64> {
        let now = win::qpc();
        let frequency = win::frequency();
        self.last = (self.last + 1).max((now - self.start) * self.input / frequency);
        let until = self.start + self.last * frequency / self.input - now;
        let delay = -(until * 10_000_000 / frequency).max(1);
        unsafe {
            SetWaitableTimer(self.timer, &delay, 0, None, None, false).map_err(message)?;
            if WaitForSingleObject(self.timer, 200) != WAIT_OBJECT_0 {
                return Err("Таймер захвата недоступен.".into());
            }
        }
        Ok(self.last)
    }
}
impl Drop for Clock {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.timer);
        }
    }
}

struct Automatic {
    stop: Arc<AtomicBool>,
    rate: Arc<AtomicI32>,
    observed: Arc<AtomicU64>,
    error: Arc<Mutex<Option<String>>>,
    worker: Option<JoinHandle<()>>,
}
impl Automatic {
    fn start(program: PathBuf, source: SourceId) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let rate = Arc::new(AtomicI32::new(0));
        let observed = Arc::new(AtomicU64::new(0));
        let error = Arc::new(Mutex::new(None));
        let (cancel, result) = (Arc::clone(&stop), Arc::clone(&rate));
        let (last_observation, failure) = (Arc::clone(&observed), Arc::clone(&error));
        let worker = thread::spawn(move || {
            let run = || -> Result<()> {
                let _apartment = Apartment::enter()?;
                let api = EngineApi::load(&program)?;
                let capture = Capture::new(&api, source, None)?;
                let mut probe = None;
                while !cancel.load(Ordering::Relaxed) && !capture.closed.load(Ordering::Relaxed) {
                    let _ = capture.pending.recv_timeout(Duration::from_millis(80));
                    let Some(frame) = capture.latest() else {
                        continue;
                    };
                    if frame.0.ContentSize().map_err(message)? != capture.size {
                        return Ok(());
                    }
                    let texture =
                        engine::texture(&frame.0.Surface().map_err(message)?).map_err(message)?;
                    if probe.is_none() {
                        probe = Some(Probe::new(&texture, capture.size).map_err(message)?);
                    }
                    let probe = probe.as_mut().unwrap();
                    let measured = probe.measure(&texture, frame.time()?).map_err(message)?;
                    last_observation.store(probe.observed.to_bits(), Ordering::Relaxed);
                    if let Some(fps) = measured {
                        result.store(fps, Ordering::Relaxed);
                        return Ok(());
                    }
                }
                Ok(())
            };
            if let Err(error) = run() {
                *failure.lock().unwrap() = Some(error);
            }
        });
        Self {
            stop,
            rate,
            observed,
            error,
            worker: Some(worker),
        }
    }
}
impl Drop for Automatic {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Default)]
struct Metrics {
    capture: Rate,
    present: Rate,
    last_qpc: i64,
    last_pts: Option<f64>,
    late: u32,
    log: Option<File>,
    pending: Vec<u8>,
}

impl Metrics {
    fn read(&mut self, log: &Path, status: &mut Status, captured: u64, times: &[(i64, i64); 1024]) {
        let now = win::qpc();
        let frequency = win::frequency();
        status.capture_fps = self.capture.update(now, captured);
        if self.log.is_none() {
            self.log = File::open(log).ok();
        }
        let sample = self.log.as_mut().and_then(|file| {
            // Retain a half-written line for the next poll and read only bytes the renderer appended.
            file.read_to_end(&mut self.pending).ok()?;
            let end = self.pending.iter().rposition(|byte| *byte == b'\n')? + 1;
            let sample = String::from_utf8_lossy(&self.pending[..end])
                .lines()
                .rev()
                .find_map(render_sample);
            self.pending.drain(..end);
            sample
        });
        if self.last_qpc > 0 && now - self.last_qpc > frequency * 2 {
            status.present_fps = None;
            status.latency_ms = None;
        }
        let Some((frames, time, pts, drops)) = sample else {
            return;
        };
        if now - time > frequency * 2 {
            status.present_fps = None;
            status.latency_ms = None;
            return;
        }
        if time <= self.last_qpc {
            return;
        }
        self.last_qpc = time;
        status.present_fps = self.present.update(time, frames);
        status.drops = drops;
        if self.last_pts.is_some_and(|last| pts < last - 0.0001) {
            status.backward_frames += 1;
        }
        self.last_pts = Some(pts);
        let sequence = 1 + (pts * status.input as f64 + 0.001).floor() as i64;
        let age = [sequence, sequence - 1].into_iter().find_map(|seq| {
            let (stored, captured) = times[seq as usize & 1023];
            (stored == seq && captured > 0)
                .then_some((time - captured) as f64 * 1000. / frequency as f64)
        });
        status.latency_ms = age.filter(|age| (0. ..=1500.).contains(age));
        self.late = if age.is_some_and(|ms| ms >= 250.) {
            self.late + 1
        } else {
            0
        };
    }
}

fn render_sample(line: &str) -> Option<(u64, i64, f64, u64)> {
    let mut fields = line.strip_prefix("PANE_RENDER ")?.split_whitespace();
    Some((
        fields.next()?.parse().ok()?,
        fields.next()?.parse().ok()?,
        fields.next()?.parse().ok()?,
        fields.next()?.parse().ok()?,
    ))
}

#[derive(Default)]
struct Rate(std::collections::VecDeque<(i64, u64)>);
impl Rate {
    fn update(&mut self, time: i64, frames: u64) -> Option<f64> {
        let frequency = win::frequency();
        self.0.push_back((time, frames));
        while self.0.len() > 1 && self.0[0].0 < time - frequency * 2 {
            self.0.pop_front();
        }
        let (old_time, old_frames) = self.0[0];
        (time - old_time >= frequency).then(|| {
            frames.saturating_sub(old_frames) as f64 * frequency as f64 / (time - old_time) as f64
        })
    }
}
