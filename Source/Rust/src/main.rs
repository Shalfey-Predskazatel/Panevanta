#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cadence;
mod capture;
mod engine;
mod nr;
mod preview;
mod sound;
mod win;

use capture::{Controller, Settings, Status};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use win::{SavedSource, Source, SourceId};
use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND},
        System::Threading::CreateMutexW,
    },
    core::w,
};

const DEFAULT_HOTKEY: &str = "Shift+Alt+S";
const TRAY_ID: &str = "panevanta-tray";
const COMPACT: &str = "compact";
const TRAY_PANEL: &str = "tray";

fn active_session(phase: &str) -> bool {
    matches!(phase, "starting" | "running")
}

fn hide_panel_window(window: &WebviewWindow) -> bool {
    let Ok(hwnd) = window.hwnd() else {
        return false;
    };
    if window.label() == "main" {
        win::allow_main_visible(false);
        let _ = window.hide();
    } else {
        win::hide_without_activation(HWND(hwnd.0));
    }
    true
}

fn show_compact(app: &AppHandle) -> bool {
    let state = app.state::<Application>();
    if state.tuning.lock().unwrap().is_some() {
        return false;
    }
    let Some(window) = app.get_webview_window(COMPACT) else {
        return false;
    };
    let Ok(hwnd) = window.hwnd() else {
        return false;
    };
    if let Some(source) = *state.source.lock().unwrap()
        && !state.compact_placed.swap(true, Ordering::Relaxed)
    {
        win::place_compact(HWND(hwnd.0), source);
    }
    if let Some(main) = app.get_webview_window("main") {
        hide_panel_window(&main);
    }
    win::show_without_activation(HWND(hwnd.0));
    true
}

fn watch_capture_windows(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<Application>();
        while !state.exiting.load(Ordering::Relaxed) {
            let phase = state.capture.status().phase;
            let active = matches!(phase, "starting" | "running" | "stopping");
            if !active && state.capture_windows.swap(false, Ordering::AcqRel) {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let state = handle.state::<Application>();
                    let phase = state.capture.status().phase;
                    if matches!(phase, "starting" | "running" | "stopping") {
                        return;
                    }
                    if phase != "error"
                        && let Some(compact) = handle.get_webview_window(COMPACT)
                    {
                        hide_panel_window(&compact);
                    }
                    if let Some(main) = handle.get_webview_window("main") {
                        sync_panel_window(&main, phase);
                    }
                });
            }
            std::thread::sleep(Duration::from_millis(125));
        }
    });
}

fn show_main_window(app: &AppHandle) {
    if !app
        .state::<Application>()
        .main_ready
        .load(Ordering::Acquire)
    {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let state = app.state::<Application>();
        let phase = state.capture.status().phase;
        let running = active_session(phase);
        sync_panel_window(&window, phase);
        win::allow_main_visible(true);
        let _ = window.unminimize();
        let _ = window.show();
        if !running {
            let _ = window.set_focus();
        }
        if let Some(compact) = app.get_webview_window(COMPACT) {
            hide_panel_window(&compact);
        }
    }
}

fn install_tray(app: &tauri::App) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Panevanta")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state: MouseButtonState::Up,
                position,
                rect,
                ..
            } = event
            {
                let app = tray.app_handle();
                if button == MouseButton::Left {
                    if active_session(app.state::<Application>().capture.status().phase) {
                        show_compact(app);
                    } else {
                        show_main_window(app);
                    }
                } else if button == MouseButton::Right {
                    let origin = rect.position.to_physical::<f64>(1.);
                    let size = rect.size.to_physical::<f64>(1.);
                    let anchor = if size.width > 0. && size.height > 0. {
                        (
                            (origin.x + size.width / 2.) as i32,
                            (origin.y + size.height / 2.) as i32,
                        )
                    } else {
                        (position.x as i32, position.y as i32)
                    };
                    let state = app.state::<Application>();
                    *state.tray_anchor.lock().unwrap() = Some(anchor);
                    if state.tray_ready.load(Ordering::Acquire) {
                        place_tray(app);
                    }
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn place_tray(app: &AppHandle) {
    let Some(window) = app.get_webview_window(TRAY_PANEL) else {
        return;
    };
    let Some((x, y)) = app
        .state::<Application>()
        .tray_anchor
        .lock()
        .unwrap()
        .take()
    else {
        return;
    };
    let area = win::tray_bounds(x, y);
    if let Ok(hwnd) = window.hwnd() {
        win::position_tray(HWND(hwnd.0), area, x, y);
    }
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
fn tray_ready(window: WebviewWindow, app: AppHandle, state: State<'_, Application>) {
    if window.label() == TRAY_PANEL {
        state.tray_ready.store(true, Ordering::Release);
        place_tray(&app);
    }
}

#[tauri::command]
fn tray_action(window: WebviewWindow, app: AppHandle, action: &str) -> Result<(), String> {
    if window.label() != TRAY_PANEL {
        return Err("Меню недоступно.".into());
    }
    let state = app.state::<Application>();
    match action {
        "hide" => {}
        "main" => show_main_window(&app),
        "compact" => {
            if !show_compact(&app) {
                return Err("Панель недоступна во время автоподбора.".into());
            }
        }
        "settings" => {
            if !show_compact(&app) {
                return Err("Панель недоступна во время автоподбора.".into());
            }
            let _ = app.emit_to(COMPACT, "panevanta-expand", ());
        }
        "toggle" => toggle_session(&app, &state, false)?,
        "quit" => app.exit(0),
        _ => return Err("Неизвестная команда.".into()),
    }
    let _ = window.hide();
    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct Preferences {
    settings: Settings,
    hotkey: String,
    theme: String,
    metrics: bool,
    sound: bool,
    motion: bool,
    main_on_top: bool,
    last_multiplier: i32,
    source_window: Option<SavedSource>,
    source_screen: Option<SavedSource>,
    source_mode_screen: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            hotkey: DEFAULT_HOTKEY.into(),
            theme: "system".into(),
            metrics: true,
            sound: true,
            motion: true,
            main_on_top: true,
            last_multiplier: 4,
            source_window: None,
            source_screen: None,
            source_mode_screen: true,
        }
    }
}

struct Application {
    capture: Controller,
    program: PathBuf,
    prefs: Mutex<Preferences>,
    source: Mutex<Option<SourceId>>,
    shortcut: Mutex<Option<Shortcut>>,
    shortcut_changes: tauri::async_runtime::Mutex<()>,
    tuning: Mutex<Option<SourceId>>,
    compact_placed: AtomicBool,
    capture_windows: AtomicBool,
    exiting: AtomicBool,
    main_ready: AtomicBool,
    tray_ready: AtomicBool,
    tray_anchor: Mutex<Option<(i32, i32)>>,
}

impl Application {
    fn toggle(&self, panel: usize, start_only: bool) -> Result<(), String> {
        let source = *self.source.lock().unwrap();
        let settings = self.prefs.lock().unwrap().settings;
        self.capture.toggle(source, settings, panel, start_only)
    }

    fn save(&self, prefs: &Preferences) -> Result<(), String> {
        let dir = self.program.join("settings");
        let scratch = dir.join("panevanta.pending.json");
        let payload = serde_json::to_vec_pretty(prefs).map_err(|error| error.to_string())?;
        fs::create_dir_all(&dir)
            .and_then(|_| fs::write(&scratch, payload))
            .and_then(|_| fs::rename(scratch, dir.join("panevanta.json")))
            .map_err(|error| format!("Не удалось записать настройки. {error}"))
    }
}

fn load_preferences(program: &Path) -> Preferences {
    let mut prefs: Preferences = fs::read(program.join("settings/panevanta.json"))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_else(|| {
            let old = fs::read_to_string(program.join("settings/hotkey.txt"))
                .ok()
                .and_then(|text| {
                    let (mods, key) = text.trim().split_once(',')?;
                    let (mods, key) = (mods.parse::<u32>().ok()?, key.parse::<u32>().ok()?);
                    let key = match key {
                        48..=57 | 65..=90 => char::from_u32(key)?.to_string(),
                        112..=135 => format!("F{}", key - 111),
                        _ => return None,
                    };
                    let mut parts = Vec::new();
                    if mods & 4 != 0 {
                        parts.push("Shift");
                    }
                    if mods & 2 != 0 {
                        parts.push("Ctrl");
                    }
                    if mods & 1 != 0 {
                        parts.push("Alt");
                    }
                    if parts.is_empty() {
                        return None;
                    }
                    parts.push(&key);
                    Some(parts.join("+"))
                });
            Preferences {
                hotkey: old.unwrap_or_else(|| DEFAULT_HOTKEY.into()),
                ..Default::default()
            }
        });
    prefs.settings = prefs.settings.normalized();
    if prefs.settings.multiplier != 1 {
        prefs.last_multiplier = prefs.settings.multiplier;
    } else if ![0, 2, 3, 4, 5, 6].contains(&prefs.last_multiplier) {
        prefs.last_multiplier = 4;
    }
    if !["system", "dark", "light"].contains(&prefs.theme.as_str()) {
        prefs.theme = "system".into();
    }
    if prefs.hotkey.parse::<Shortcut>().is_err() {
        prefs.hotkey = DEFAULT_HOTKEY.into();
    }
    prefs
}

#[tauri::command]
fn sources() -> Vec<Source> {
    win::list_sources()
}

#[tauri::command]
fn restore_source(streamer: bool, state: State<'_, Application>) -> Option<SourceId> {
    let prefs = state.prefs.lock().unwrap();
    let saved = if streamer {
        prefs.source_window.as_ref()
    } else {
        if prefs.source_mode_screen {
            prefs.source_screen.as_ref()
        } else {
            prefs.source_window.as_ref()
        }
    };
    let saved = saved?.clone();
    drop(prefs);
    saved.find(&win::list_sources())
}

#[tauri::command]
fn select_source(
    source: Option<SourceId>,
    remember: Option<bool>,
    state: State<'_, Application>,
) -> Option<String> {
    *state.source.lock().unwrap() = source;
    if remember.unwrap_or(false)
        && let Some(id) = source
        && let Some(saved) = SavedSource::from_id(id)
    {
        let mut prefs = state.prefs.lock().unwrap();
        if id.screen {
            prefs.source_screen = Some(saved);
        } else {
            prefs.source_window = Some(saved);
        }
        if !prefs.settings.streamer {
            prefs.source_mode_screen = id.screen;
        }
        return state.save(&prefs).err();
    }
    None
}

#[tauri::command]
fn preferences(state: State<'_, Application>) -> Preferences {
    state.prefs.lock().unwrap().clone()
}

#[tauri::command]
fn nr_available(state: State<'_, Application>) -> bool {
    nr::runtime(&state.program).is_some()
}

#[tauri::command]
fn status(state: State<'_, Application>) -> Status {
    state.capture.status()
}

#[tauri::command]
fn toggle(
    app: AppHandle,
    state: State<'_, Application>,
    start_only: Option<bool>,
) -> Result<(), String> {
    toggle_session(&app, &state, start_only.unwrap_or(false))
}

fn toggle_session(app: &AppHandle, state: &Application, start_only: bool) -> Result<(), String> {
    if state.tuning.lock().unwrap().is_some() {
        return Err("Сначала завершите автоподбор.".into());
    }
    let main = app
        .get_webview_window("main")
        .ok_or("Панель Panevanta недоступна.")?;
    let main_hwnd = main.hwnd().map_err(|error| error.to_string())?.0 as usize;
    let panel = app
        .get_webview_window(COMPACT)
        .and_then(|window| window.hwnd().ok())
        .map_or(main_hwnd, |hwnd| hwnd.0 as usize);
    let already_active = active_session(state.capture.status().phase);
    state.toggle(panel, start_only)?;
    let phase = state.capture.status().phase;
    sync_panel_window(&main, phase);
    if !already_active && active_session(phase) {
        state.capture_windows.store(true, Ordering::Release);
        if let Some(source) = *state.source.lock().unwrap() {
            source.return_focus_from(main_hwnd);
        }
        show_compact(app);
    }
    Ok(())
}

#[tauri::command]
fn show_full_settings(app: AppHandle) {
    show_main_window(&app);
}

#[tauri::command]
fn drag_compact(window: WebviewWindow) -> Result<(), String> {
    if window.label() != COMPACT {
        return Err("Панель недоступна.".into());
    }
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    win::begin_compact_drag(HWND(hwnd.0)).map_err(|error| error.to_string())
}

#[tauri::command]
fn panel_ready(window: WebviewWindow, state: State<'_, Application>) {
    if window.label() == "main"
        && !state.main_ready.swap(true, Ordering::AcqRel)
        && !active_session(state.capture.status().phase)
    {
        show_main_window(window.app_handle());
    }
}

#[tauri::command]
fn compact_details(window: WebviewWindow, expanded: bool) -> Result<(), String> {
    if window.label() != COMPACT {
        return Err("Панель недоступна.".into());
    }
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    win::resize_compact(HWND(hwnd.0), scale, expanded).map_err(|error| error.to_string())
}

#[tauri::command]
fn hide_compact(window: WebviewWindow) {
    if window.label() == COMPACT {
        hide_panel_window(&window);
    }
}

#[tauri::command]
fn stop(state: State<'_, Application>) {
    state.capture.stop();
}

#[tauri::command]
fn sync_panel_focus(window: WebviewWindow, state: State<'_, Application>) {
    if window.label() == "main" {
        sync_panel_window(&window, state.capture.status().phase);
    }
}

fn sync_panel_window(window: &WebviewWindow, phase: &str) {
    if window.label() == COMPACT {
        return;
    }
    let running = matches!(phase, "starting" | "running" | "stopping");
    win::block_panel_activation(running);
    let _ = window.set_focusable(!running);
    let _ = window.set_content_protected(running);
}

#[tauri::command]
fn begin_tuning(state: State<'_, Application>, source: SourceId) -> Result<(), String> {
    let mut tuning = state.tuning.lock().unwrap();
    if tuning.is_some() || !matches!(state.capture.status().phase, "idle" | "error") {
        return Err("Остановите текущую обработку перед автоподбором.".into());
    }
    if source.screen
        || !win::list_sources()
            .iter()
            .any(|candidate| candidate.id == source)
    {
        return Err("Выберите открытое окно с воспроизводящимся видео или игрой.".into());
    }
    *tuning = Some(source);
    Ok(())
}

#[tauri::command]
fn test_profile(
    window: WebviewWindow,
    state: State<'_, Application>,
    settings: Settings,
) -> Result<(), String> {
    let source = state.tuning.lock().unwrap().ok_or("Автоподбор завершён.")?;
    if !matches!(state.capture.status().phase, "idle" | "error") {
        return Err("Дождитесь остановки предыдущего теста.".into());
    }
    let settings = Settings {
        streamer: false,
        ..settings
    }
    .validated()?;
    let main = window.hwnd().map_err(|error| error.to_string())?.0 as usize;
    state.capture.configure(settings, false);
    state.capture.toggle(Some(source), settings, main, true)?;
    sync_panel_window(&window, state.capture.status().phase);
    source.return_focus_from(main);
    Ok(())
}

#[tauri::command]
fn end_tuning(window: WebviewWindow, state: State<'_, Application>) {
    if state.tuning.lock().unwrap().take().is_some() {
        state.capture.stop();
        let prefs = state.prefs.lock().unwrap();
        state.capture.configure(prefs.settings, prefs.sound);
        sync_panel_window(&window, state.capture.status().phase);
    }
}

#[tauri::command]
fn update_preferences(
    app: AppHandle,
    window: WebviewWindow,
    settings: Option<Settings>,
    theme: Option<String>,
    metrics: Option<bool>,
    sound: Option<bool>,
    motion: Option<bool>,
    main_on_top: Option<bool>,
    state: State<'_, Application>,
) -> Result<Option<String>, String> {
    if state.tuning.lock().unwrap().is_some() {
        return Err("Завершите автоподбор перед изменением настроек.".into());
    }
    let settings = settings.map(Settings::validated).transpose()?;
    if theme
        .as_deref()
        .is_some_and(|value| !["system", "dark", "light"].contains(&value))
    {
        return Err("Неизвестная тема.".into());
    }
    let mut prefs = state.prefs.lock().unwrap();
    let settings = settings.unwrap_or(prefs.settings);
    let candidate = Preferences {
        settings,
        theme: theme.unwrap_or_else(|| prefs.theme.clone()),
        metrics: metrics.unwrap_or(prefs.metrics),
        sound: sound.unwrap_or(prefs.sound),
        motion: motion.unwrap_or(prefs.motion),
        main_on_top: main_on_top.unwrap_or(prefs.main_on_top),
        last_multiplier: if settings.multiplier == 1 {
            prefs.last_multiplier
        } else {
            settings.multiplier
        },
        hotkey: prefs.hotkey.clone(),
        source_window: prefs.source_window.clone(),
        source_screen: prefs.source_screen.clone(),
        source_mode_screen: prefs.source_mode_screen,
    };
    let sound_changed = prefs.sound != candidate.sound;
    *prefs = candidate;
    state.capture.configure(settings, prefs.sound);
    if sound_changed {
        (if prefs.sound {
            sound::Cue::SoundOn
        } else {
            sound::Cue::SoundOff
        })
        .play(true);
    }
    let warning = state.save(&prefs).err();
    let updated = prefs.clone();
    drop(prefs);
    if main_on_top.is_some()
        && let Some(main) = app.get_webview_window("main")
    {
        let _ = main.set_always_on_top(updated.main_on_top);
    }
    sync_preferences(&app, window.label(), &updated);
    Ok(warning)
}

fn sync_preferences(app: &AppHandle, sender: &str, prefs: &Preferences) {
    for label in ["main", COMPACT, TRAY_PANEL] {
        if label != sender {
            let _ = app.emit_to(label, "panevanta-preferences", prefs);
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "camelCase")]
enum QuickEdit {
    Vfg(bool),
    Sr(bool),
    Hdr(bool),
    Nr(bool),
    Height(i32),
    NrResolution(i32),
    NrStyle(u32),
    NrDetail(bool),
    Input(i32),
    Multiplier(i32),
    Quality(i32),
    Scale(f32),
    Peak(i32),
    NrTone(i32),
    NrStructure(i32),
    NrSkin(i32),
    NrPasses(u32),
    NrAutoMask(bool),
    NrUiCorrection(bool),
    Streamer(bool),
}

#[tauri::command]
fn update_quick(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, Application>,
    edit: QuickEdit,
) -> Result<(Preferences, Option<String>), String> {
    if state.tuning.lock().unwrap().is_some() {
        return Err("Завершите автоподбор перед изменением настроек.".into());
    }
    let mut prefs = state.prefs.lock().unwrap();
    let mut settings = prefs.settings;
    match edit {
        QuickEdit::Vfg(on) => {
            if settings.multiplier != 1 {
                prefs.last_multiplier = settings.multiplier;
            }
            settings.multiplier = if on { prefs.last_multiplier } else { 1 };
        }
        QuickEdit::Sr(on) => settings.sr = on,
        QuickEdit::Hdr(on) => settings.hdr = on,
        QuickEdit::Nr(on) => settings.nr = on,
        QuickEdit::Height(height) => settings.height = height,
        QuickEdit::NrResolution(height) => settings.nr_resolution = height,
        QuickEdit::NrStyle(style) => settings.nr_style = style,
        QuickEdit::NrDetail(on) => settings.nr_detail = on,
        QuickEdit::Input(value) => settings.input = value,
        QuickEdit::Multiplier(value) => settings.multiplier = value,
        QuickEdit::Quality(value) => settings.quality = value,
        QuickEdit::Scale(value) => settings.scale = value,
        QuickEdit::Peak(value) => settings.peak = value,
        QuickEdit::NrTone(value) => settings.nr_tone = value,
        QuickEdit::NrStructure(value) => settings.nr_structure = value,
        QuickEdit::NrSkin(value) => settings.nr_skin = value,
        QuickEdit::NrPasses(value) => settings.nr_passes = value,
        QuickEdit::NrAutoMask(on) => settings.nr_auto_mask = on,
        QuickEdit::NrUiCorrection(on) => settings.nr_ui_correction = on,
        QuickEdit::Streamer(on) => settings.streamer = on,
    }
    prefs.settings = settings.validated()?;
    if settings.multiplier != 1 {
        prefs.last_multiplier = settings.multiplier;
    }
    state.capture.configure(prefs.settings, prefs.sound);
    let warning = state.save(&prefs).err();
    let updated = prefs.clone();
    drop(prefs);
    sync_preferences(&app, window.label(), &updated);
    Ok((updated, warning))
}

#[tauri::command]
async fn change_hotkey(
    hotkey: String,
    app: AppHandle,
    state: State<'_, Application>,
) -> Result<Option<String>, String> {
    if hotkey.len() > 64 || !hotkey.contains('+') {
        return Err("Добавьте Shift, Ctrl или Alt.".into());
    }
    let next: Shortcut = hotkey
        .parse()
        .map_err(|_| "Эта клавиша не поддерживается.")?;
    let _change = state.shortcut_changes.lock().await;
    let previous = *state.shortcut.lock().unwrap();
    if previous != Some(next) {
        app.global_shortcut()
            .register(next)
            .map_err(|_| "Эта комбинация занята. Попробуйте другую.")?;
        if let Some(previous) = previous {
            if let Err(error) = app.global_shortcut().unregister(previous) {
                let _ = app.global_shortcut().unregister(next);
                return Err(error.to_string());
            }
        }
        *state.shortcut.lock().unwrap() = Some(next);
    }
    let mut prefs = state.prefs.lock().unwrap();
    prefs.hotkey = hotkey;
    Ok(state.save(&prefs).err())
}

#[tauri::command]
fn feedback(cue: sound::Cue, state: State<'_, Application>) -> bool {
    let enabled = state.prefs.lock().unwrap().sound;
    cue.play(enabled)
}

#[tauri::command]
fn open_log(state: State<'_, Application>) -> Result<(), String> {
    let path = state
        .capture
        .status()
        .log
        .filter(|path| path.is_file())
        .ok_or("Журнал появится после запуска движка.")?;
    Command::new("notepad.exe")
        .arg(path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

struct Instance(HANDLE);
impl Instance {
    fn acquire() -> Option<Self> {
        unsafe {
            let handle = CreateMutexW(None, false, w!("Local\\Veyra.Desktop.Capture")).ok()?;
            let instance = Self(handle);
            (GetLastError() != ERROR_ALREADY_EXISTS).then_some(instance)
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn main() {
    let Some(_instance) = Instance::acquire() else {
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                w!("Panevanta уже запущена."),
                w!("Panevanta"),
                windows::Win32::UI::WindowsAndMessaging::MB_OK,
            );
        }
        return;
    };
    let program = std::env::current_exe()
        .expect("Panevanta executable")
        .parent()
        .unwrap()
        .to_owned();
    let fixed_runtime = program
        .parent()
        .unwrap_or(&program)
        .join("Dependencies/WebView2");
    if std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_none()
        && tauri::webview_version().is_err()
        && fixed_runtime.join("msedgewebview2.exe").is_file()
    {
        // The environment is set before starting the capture worker or Tauri event loop.
        unsafe { std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", fixed_runtime) };
    }
    let prefs = load_preferences(&program);
    let preferred_shortcut = prefs.hotkey.clone();
    let capture = Controller::new(program.clone(), prefs.sound);
    let app = tauri::Builder::default()
        .manage(Application {
            capture,
            program,
            prefs: Mutex::new(prefs),
            source: Mutex::new(None),
            shortcut: Mutex::new(None),
            shortcut_changes: tauri::async_runtime::Mutex::new(()),
            tuning: Mutex::new(None),
            compact_placed: AtomicBool::new(false),
            capture_windows: AtomicBool::new(false),
            exiting: AtomicBool::new(false),
            main_ready: AtomicBool::new(false),
            tray_ready: AtomicBool::new(false),
            tray_anchor: Mutex::new(None),
        })
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }
                    let state = app.state::<Application>();
                    if *state.shortcut.lock().unwrap() != Some(*shortcut) {
                        return;
                    }
                    if let Err(message) = toggle_session(app, &state, false) {
                        let enabled = state.prefs.lock().unwrap().sound;
                        sound::Cue::Error.play(enabled);
                        let _ = app.emit("panevanta-error", message);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            win::remember_foreground();
            let settings = app.state::<Application>().program.join("settings");
            let window_config = &app.config().app.windows[0];
            let mut window = WebviewWindowBuilder::from_config(app, window_config)?;
            if fs::create_dir_all(&settings).is_ok() {
                window = window.data_directory(settings.join("webview"));
            }
            let main_on_top = app.state::<Application>().prefs.lock().unwrap().main_on_top;
            let window = window
                .background_color(tauri::window::Color(22, 16, 30, 255))
                .always_on_top(main_on_top)
                .visible(false)
                .build()?;
            win::guard_panel(HWND(window.hwnd()?.0));
            let mut compact =
                WebviewWindowBuilder::new(app, COMPACT, WebviewUrl::App("index.html".into()))
                    .title("Panevanta — Быстрые настройки")
                    .inner_size(352., 414.)
                    .resizable(false)
                    .decorations(false)
                    .shadow(false)
                    .transparent(true)
                    .skip_taskbar(true)
                    .always_on_top(true)
                    .focused(false)
                    .focusable(false)
                    .visible(false);
            if settings.is_dir() {
                compact = compact.data_directory(settings.join("webview"));
            }
            let compact = compact.build()?;
            if let Err(error) = win::guard_compact(HWND(compact.hwnd()?.0)) {
                eprintln!("Panevanta compact window: {error}");
                let _ = compact.destroy();
            }
            let mut tray_panel =
                WebviewWindowBuilder::new(app, TRAY_PANEL, WebviewUrl::App("index.html".into()))
                    .title("Panevanta — Системный трей")
                    .inner_size(320., 404.)
                    .decorations(false)
                    .transparent(true)
                    .resizable(false)
                    .shadow(false)
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .focused(false)
                    .visible(false);
            if settings.is_dir() {
                tray_panel = tray_panel.data_directory(settings.join("webview"));
            }
            tray_panel.build()?;
            install_tray(app)?;
            watch_capture_windows(app.handle().clone());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<Application>();
                let _change = state.shortcut_changes.lock().await;
                if state.shortcut.lock().unwrap().is_some() {
                    return;
                }
                if let Ok(shortcut) = preferred_shortcut.parse::<Shortcut>() {
                    if handle.global_shortcut().register(shortcut).is_ok() {
                        *state.shortcut.lock().unwrap() = Some(shortcut);
                    } else {
                        let _ = handle.emit(
                            "panevanta-error",
                            "Горячая клавиша занята. Выберите другую комбинацию.",
                        );
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == TRAY_PANEL && matches!(event, WindowEvent::Focused(false)) {
                let _ = window.hide();
            }
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.app_handle().tray_by_id(TRAY_ID).is_some()
            {
                if let Some(window) = window.app_handle().get_webview_window(window.label())
                    && (window.label() == COMPACT || window.label() == "main")
                    && hide_panel_window(&window)
                {
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            sources,
            restore_source,
            select_source,
            preferences,
            nr_available,
            status,
            toggle,
            stop,
            show_full_settings,
            drag_compact,
            panel_ready,
            compact_details,
            hide_compact,
            tray_ready,
            tray_action,
            sync_panel_focus,
            begin_tuning,
            test_profile,
            end_tuning,
            update_preferences,
            update_quick,
            change_hotkey,
            feedback,
            open_log
        ])
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            let message =
                windows::core::HSTRING::from(format!("Не удалось открыть Panevanta.\n\n{error}"));
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    None,
                    &message,
                    w!("Panevanta"),
                    windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
            return;
        }
    };
    app.run(|app, event| {
        if matches!(event, RunEvent::Exit) {
            let state = app.state::<Application>();
            state.exiting.store(true, Ordering::Relaxed);
            state.capture.shutdown();
        }
    });
}
