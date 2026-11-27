use serde::Serialize;
use std::{
    mem::size_of,
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use windows::{
    Win32::{
        Devices::Display::*,
        Foundation::*,
        Graphics::{
            Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
            Gdi::*,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Performance::*,
            Threading::{
                OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
        UI::{
            Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
    core::{BOOL, PCWSTR, PWSTR, Result, w},
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceId {
    pub handle: usize,
    pub screen: bool,
}

use serde::Deserialize;

#[derive(Serialize)]
pub struct Source {
    pub id: SourceId,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SavedSource {
    label: String,
    device: Option<String>,
    executable: Option<String>,
    class: Option<String>,
}

impl SavedSource {
    pub fn from_id(id: SourceId) -> Option<Self> {
        if id.screen {
            let monitor = monitor_info(id.monitor())?;
            return Some(Self {
                label: String::new(),
                device: Some(String::from_utf16_lossy(wide_name(&monitor.szDevice))),
                executable: None,
                class: None,
            });
        }
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(id.window(), Some(&mut pid));
            if pid == 0 || pid == std::process::id() {
                return None;
            }
            let mut title = [0u16; 512];
            let count = GetWindowTextW(id.window(), &mut title);
            if count <= 0 {
                return None;
            }
            let mut class = [0u16; 256];
            let class_length = GetClassNameW(id.window(), &mut class);
            let executable = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .ok()
                .and_then(|process| {
                    let mut path = vec![0u16; 32768];
                    let mut length = path.len() as u32;
                    let status = QueryFullProcessImageNameW(
                        process,
                        PROCESS_NAME_WIN32,
                        PWSTR(path.as_mut_ptr()),
                        &mut length,
                    );
                    let _ = CloseHandle(process);
                    status
                        .ok()
                        .map(|_| String::from_utf16_lossy(&path[..length as usize]).to_lowercase())
                });
            Some(Self {
                label: String::from_utf16_lossy(&title[..count as usize]),
                device: None,
                executable,
                class: (class_length > 0)
                    .then(|| String::from_utf16_lossy(&class[..class_length as usize])),
            })
        }
    }

    pub fn find(&self, sources: &[Source]) -> Option<SourceId> {
        let mut matches = Vec::new();
        for source in sources
            .iter()
            .filter(|item| item.id.screen == self.device.is_some())
        {
            let Some(candidate) = Self::from_id(source.id) else {
                continue;
            };
            if let Some(device) = &self.device {
                if candidate.device.as_ref() == Some(device) {
                    return Some(source.id);
                }
            } else if self
                .executable
                .as_ref()
                .is_none_or(|value| candidate.executable.as_ref() == Some(value))
                && self
                    .class
                    .as_ref()
                    .is_none_or(|value| candidate.class.as_ref() == Some(value))
            {
                matches.push((source.id, candidate.label == self.label));
            }
        }
        let mut exact = matches.iter().filter(|(_, exact)| *exact);
        if let Some((id, _)) = exact.next() {
            return exact.next().is_none().then_some(*id);
        }
        (self.executable.is_some() && matches.len() == 1).then(|| matches[0].0)
    }
}

impl SourceId {
    pub fn work_area(self) -> Option<RECT> {
        Some(monitor_info(self.monitor())?.monitorInfo.rcWork)
    }

    pub fn monitor_size(self) -> Option<(i32, i32)> {
        let rect = monitor_info(self.monitor())?.monitorInfo.rcMonitor;
        Some((rect.right - rect.left, rect.bottom - rect.top))
    }

    pub fn window(self) -> HWND {
        HWND(self.handle as _)
    }

    pub fn monitor(self) -> HMONITOR {
        if self.screen {
            HMONITOR(self.handle as _)
        } else {
            unsafe { MonitorFromWindow(self.window(), MONITOR_DEFAULTTONEAREST) }
        }
    }

    pub fn bounds(self) -> Option<RECT> {
        if self.screen {
            return Some(monitor_info(self.monitor())?.monitorInfo.rcMonitor);
        }
        let mut rect = RECT::default();
        unsafe {
            DwmGetWindowAttribute(
                self.window(),
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&mut rect as *mut RECT).cast(),
                size_of::<RECT>() as u32,
            )
            .ok()?;
        }
        (rect.right > rect.left && rect.bottom > rect.top).then_some(rect)
    }

    pub fn can_show(self, panel: usize) -> bool {
        if self.screen {
            return true;
        }
        unsafe {
            let foreground = GetForegroundWindow();
            !IsIconic(self.window()).as_bool()
                && IsWindowVisible(self.window()).as_bool()
                && (foreground == self.window() || foreground.0 as usize == panel)
        }
    }

    pub fn return_focus_from(self, panel: usize) {
        unsafe {
            if GetForegroundWindow().0 as usize != panel {
                return;
            }
            let target = if self.screen {
                let previous = HWND(PREVIOUS_FOREGROUND.load(Ordering::Relaxed) as _);
                if !external_window(previous)
                    || MonitorFromWindow(previous, MONITOR_DEFAULTTONULL) != self.monitor()
                {
                    return;
                }
                previous
            } else {
                self.window()
            };
            if external_window(target) {
                let _ = SetForegroundWindow(target);
            }
        }
    }
}

static MAIN_VISIBLE: AtomicBool = AtomicBool::new(false);
static NATIVE_PANEL_GUARD: AtomicBool = AtomicBool::new(false);
static PREVIOUS_FOREGROUND: AtomicUsize = AtomicUsize::new(0);
const RESTORE_FOREGROUND: u32 = WM_APP + 23;

pub fn allow_main_visible(visible: bool) {
    MAIN_VISIBLE.store(visible, Ordering::Release);
}

pub fn guard_panel(hwnd: HWND) -> bool {
    unsafe { SetWindowSubclass(hwnd, Some(panel_proc), 1, 0).as_bool() }
}

const COMPACT_GUARD: usize = 2;
const DRAG_TIMER: usize = 0x5041;
static COMPACT_DRAG: std::sync::Mutex<Option<(POINT, RECT)>> = std::sync::Mutex::new(None);

pub fn guard_compact(hwnd: HWND) -> Result<()> {
    unsafe {
        if !SetWindowSubclass(hwnd, Some(compact_proc), COMPACT_GUARD, 0).as_bool() {
            return Err(windows::core::Error::from_win32());
        }
        let old = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            (old & !(WS_EX_APPWINDOW.0 as isize))
                | (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE).0 as isize,
        );
        SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
    }
}

pub fn show_without_activation(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

pub fn hide_without_activation(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

pub fn resize_compact(hwnd: HWND, scale: f64, expanded: bool) -> Result<()> {
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect)?;
        let info = monitor_info(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST))
            .ok_or_else(windows::core::Error::from_win32)?;
        let area = info.monitorInfo.rcWork;
        let (width, height) = if expanded { (600., 720.) } else { (352., 414.) };
        let width = ((width * scale).round() as i32).min(area.right - area.left);
        let height = ((height * scale).round() as i32).min(area.bottom - area.top);
        SetWindowPos(
            hwnd,
            None,
            rect.left.clamp(area.left, area.right - width),
            rect.top.clamp(area.top, area.bottom - height),
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
}

pub fn place_compact(hwnd: HWND, source: SourceId) {
    if let Some(area) = source.work_area() {
        unsafe {
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                let x = (area.right - (rect.right - rect.left) - 20).max(area.left);
                let y = area.top + 20;
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

pub fn begin_compact_drag(hwnd: HWND) -> Result<()> {
    let mut point = POINT::default();
    let mut rect = RECT::default();
    unsafe {
        if GetAsyncKeyState(i32::from(VK_LBUTTON.0)) >= 0 {
            return Ok(());
        }
        GetCursorPos(&mut point)?;
        GetWindowRect(hwnd, &mut rect)?;
        *COMPACT_DRAG.lock().unwrap() = Some((point, rect));
        if SetTimer(Some(hwnd), DRAG_TIMER, 12, None) == 0 {
            *COMPACT_DRAG.lock().unwrap() = None;
            return Err(windows::core::Error::from_win32());
        }
    }
    Ok(())
}

unsafe extern "system" fn compact_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    extra: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_POINTERACTIVATE => return LRESULT(PA_NOACTIVATE as isize),
            WM_ACTIVATE if wparam.0 & 0xffff != WA_INACTIVE as usize => {
                let previous = HWND(lparam.0 as _);
                if external_window(previous) {
                    let _ = PostMessageW(Some(hwnd), RESTORE_FOREGROUND, WPARAM(0), lparam);
                }
            }
            RESTORE_FOREGROUND => {
                let previous = HWND(lparam.0 as _);
                if GetForegroundWindow() == hwnd && external_window(previous) {
                    let _ = SetForegroundWindow(previous);
                }
                return LRESULT(0);
            }
            WM_TIMER if wparam.0 == DRAG_TIMER => {
                let origin = *COMPACT_DRAG.lock().unwrap();
                if let Some((from, rect)) = origin
                    && GetAsyncKeyState(i32::from(VK_LBUTTON.0)) < 0
                    && IsWindowVisible(hwnd).as_bool()
                {
                    let mut cursor = POINT::default();
                    if GetCursorPos(&mut cursor).is_ok() {
                        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
                        if let Some(info) = monitor_info(monitor) {
                            let area = info.monitorInfo.rcWork;
                            let left = (rect.left + cursor.x - from.x).clamp(
                                area.left,
                                (area.right - (rect.right - rect.left)).max(area.left),
                            );
                            let top = (rect.top + cursor.y - from.y).clamp(
                                area.top,
                                (area.bottom - (rect.bottom - rect.top)).max(area.top),
                            );
                            let _ = SetWindowPos(
                                hwnd,
                                None,
                                left,
                                top,
                                0,
                                0,
                                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                            );
                        }
                    }
                } else {
                    let _ = KillTimer(Some(hwnd), DRAG_TIMER);
                    *COMPACT_DRAG.lock().unwrap() = None;
                }
                return LRESULT(0);
            }
            WM_NCDESTROY => {
                let _ = KillTimer(Some(hwnd), DRAG_TIMER);
                let _ = RemoveWindowSubclass(hwnd, Some(compact_proc), id);
            }
            _ => {}
        }
        let _ = extra;
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

pub fn remember_foreground() {
    let hwnd = unsafe { GetForegroundWindow() };
    if external_window(hwnd) {
        PREVIOUS_FOREGROUND.store(hwnd.0 as usize, Ordering::Relaxed);
    }
}

pub fn block_panel_activation(blocked: bool) {
    NATIVE_PANEL_GUARD.store(blocked, Ordering::Relaxed);
}

fn external_window(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool()
            || IsIconic(hwnd).as_bool()
            || !IsWindowVisible(hwnd).as_bool()
        {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut class = [0u16; 64];
        let len = GetClassNameW(hwnd, &mut class) as usize;
        pid != 0
            && pid != std::process::id()
            && !matches!(
                String::from_utf16_lossy(&class[..len]).as_str(),
                "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW"
            )
    }
}

unsafe extern "system" fn panel_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    let blocked = NATIVE_PANEL_GUARD.load(Ordering::Relaxed);
    unsafe {
        match msg {
            WM_WINDOWPOSCHANGING if !MAIN_VISIBLE.load(Ordering::Acquire) => {
                let pos = &mut *(lparam.0 as *mut WINDOWPOS);
                pos.flags = (pos.flags & !SWP_SHOWWINDOW) | SWP_HIDEWINDOW | SWP_NOACTIVATE;
            }
            WM_MOUSEACTIVATE if blocked => return LRESULT(MA_NOACTIVATE as isize),
            WM_POINTERACTIVATE if blocked => return LRESULT(PA_NOACTIVATE as isize),
            WM_ACTIVATE if wparam.0 & 0xffff != WA_INACTIVE as usize => {
                let previous = HWND(lparam.0 as _);
                if external_window(previous) {
                    PREVIOUS_FOREGROUND.store(previous.0 as usize, Ordering::Relaxed);
                    if blocked {
                        let _ = PostMessageW(Some(hwnd), RESTORE_FOREGROUND, WPARAM(0), lparam);
                    }
                }
            }
            RESTORE_FOREGROUND => {
                let previous = HWND(lparam.0 as _);
                if blocked && GetForegroundWindow() == hwnd && external_window(previous) {
                    let _ = SetForegroundWindow(previous);
                }
                return LRESULT(0);
            }
            WM_NCDESTROY => {
                let _ = RemoveWindowSubclass(hwnd, Some(panel_proc), id);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

fn monitor_info(handle: HMONITOR) -> Option<MONITORINFOEXW> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    unsafe {
        GetMonitorInfoW(handle, &mut info.monitorInfo)
            .as_bool()
            .then_some(info)
    }
}

pub fn list_sources() -> Vec<Source> {
    unsafe extern "system" fn monitor_callback(
        handle: HMONITOR,
        _: HDC,
        _: *mut RECT,
        state: LPARAM,
    ) -> BOOL {
        let sources = unsafe { &mut *(state.0 as *mut Vec<Source>) };
        if let Some(info) = monitor_info(handle) {
            let bounds = info.monitorInfo.rcMonitor;
            sources.push(Source {
                id: SourceId {
                    handle: handle.0 as usize,
                    screen: true,
                },
                label: format!(
                    "Экран {} · {} × {}",
                    sources.len() + 1,
                    bounds.right - bounds.left,
                    bounds.bottom - bounds.top
                ),
            });
        }
        BOOL(1)
    }
    unsafe extern "system" fn window_callback(handle: HWND, state: LPARAM) -> BOOL {
        let mut pid = 0;
        let mut cloaked = 0i32;
        unsafe {
            GetWindowThreadProcessId(handle, Some(&mut pid));
            if pid == std::process::id()
                || !IsWindowVisible(handle).as_bool()
                || IsIconic(handle).as_bool()
                || (DwmGetWindowAttribute(
                    handle,
                    DWMWA_CLOAKED,
                    (&mut cloaked as *mut i32).cast(),
                    4,
                )
                .is_ok()
                    && cloaked != 0)
            {
                return BOOL(1);
            }
            let id = SourceId {
                handle: handle.0 as usize,
                screen: false,
            };
            let mut title = [0u16; 512];
            let count = GetWindowTextW(handle, &mut title);
            if count > 0 && id.bounds().is_some() {
                let sources = &mut *(state.0 as *mut Vec<Source>);
                sources.push(Source {
                    id,
                    label: String::from_utf16_lossy(&title[..count as usize]),
                });
            }
        }
        BOOL(1)
    }

    let mut sources = Vec::new();
    let data = LPARAM((&mut sources as *mut Vec<Source>) as isize);
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(monitor_callback), data);
        let _ = EnumWindows(Some(window_callback), data);
    }
    sources
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub hz: f64,
    pub white: f32,
    pub hdr: bool,
}

pub fn display(source: SourceId) -> Display {
    let fallback = Display {
        hz: 60.,
        white: 1.,
        hdr: false,
    };
    let Some(monitor) = monitor_info(source.monitor()) else {
        return fallback;
    };
    let mut path_count = 0;
    let mut mode_count = 0;
    let flags = QDC_ONLY_ACTIVE_PATHS;
    unsafe {
        if GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) != ERROR_SUCCESS {
            return fallback;
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        if QueryDisplayConfig(
            flags,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        ) != ERROR_SUCCESS
        {
            return fallback;
        }
        for path in &paths[..path_count as usize] {
            let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                    adapterId: path.sourceInfo.adapterId,
                    id: path.sourceInfo.id,
                },
                ..Default::default()
            };
            if DisplayConfigGetDeviceInfo(&mut name.header) != 0
                || wide_name(&name.viewGdiDeviceName) != wide_name(&monitor.szDevice)
            {
                continue;
            }
            let target = path.targetInfo;
            let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    size: size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
                    r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                    adapterId: target.adapterId,
                    id: target.id,
                },
                SDRWhiteLevel: 1000,
            };
            let _ = DisplayConfigGetDeviceInfo(&mut white.header);

            // GET_ADVANCED_COLOR_INFO_2 is available on newer versions of Windows 11.
            #[repr(C)]
            struct AdvancedColor2 {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
                flags: u32,
                encoding: u32,
                bits_per_channel: u32,
                active_color_mode: u32,
            }
            let mut color = AdvancedColor2 {
                header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                    r#type: DISPLAYCONFIG_DEVICE_INFO_TYPE(15),
                    size: size_of::<AdvancedColor2>() as u32,
                    ..white.header
                },
                flags: 0,
                encoding: 0,
                bits_per_channel: 0,
                active_color_mode: 0,
            };
            let hdr = if DisplayConfigGetDeviceInfo(&mut color.header) == 0 {
                color.active_color_mode == 2
            } else {
                let mut legacy = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                        size: size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32,
                        ..white.header
                    },
                    ..Default::default()
                };
                DisplayConfigGetDeviceInfo(&mut legacy.header) == 0
                    && legacy.Anonymous.value & 2 != 0
            };
            let hz = target.refreshRate.Numerator as f64 / target.refreshRate.Denominator as f64;
            return Display {
                hz: if (24. ..=1000.).contains(&hz) {
                    hz
                } else {
                    60.
                },
                white: (white.SDRWhiteLevel as f32 / 1000.).clamp(1., 6.),
                hdr,
            };
        }
    }
    fallback
}

fn wide_name(value: &[u16]) -> &[u16] {
    &value[..value.iter().position(|&c| c == 0).unwrap_or(value.len())]
}

pub fn qpc() -> i64 {
    let mut time = 0;
    unsafe {
        QueryPerformanceCounter(&mut time).expect("QueryPerformanceCounter");
    }
    time
}

pub fn frequency() -> i64 {
    static FREQUENCY: OnceLock<i64> = OnceLock::new();
    *FREQUENCY.get_or_init(|| {
        let mut hz = 0;
        unsafe {
            QueryPerformanceFrequency(&mut hz).expect("QueryPerformanceFrequency");
        }
        hz
    })
}

pub struct Overlay {
    parent: HWND,
    pub render: HWND,
    visible: bool,
    bounds: RECT,
}

impl Overlay {
    pub fn new(bounds: RECT) -> Result<Self> {
        unsafe extern "system" fn proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            match msg {
                WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
                WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
                _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            }
        }
        unsafe {
            let instance = HINSTANCE(GetModuleHandleW(PCWSTR::null())?.0);
            static REGISTERED: OnceLock<bool> = OnceLock::new();
            let class = w!("Panevanta.Rust.Overlay");
            if !REGISTERED.get_or_init(|| {
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    hInstance: instance,
                    lpszClassName: class,
                    ..Default::default()
                }) != 0
            }) {
                return Err(windows::core::Error::from_win32());
            }
            let width = bounds.right - bounds.left;
            let height = bounds.bottom - bounds.top;
            let parent = CreateWindowExW(
                WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TRANSPARENT
                    | WS_EX_LAYERED
                    | WS_EX_TOPMOST,
                class,
                w!(""),
                WS_POPUP | WS_DISABLED,
                bounds.left,
                bounds.top,
                width,
                height,
                None,
                None,
                Some(instance),
                None,
            )?;
            let configure = (|| {
                SetLayeredWindowAttributes(parent, COLORREF(0), 255, LWA_ALPHA)?;
                SetWindowDisplayAffinity(parent, WDA_EXCLUDEFROMCAPTURE)?;
                CreateWindowExW(
                    WS_EX_TRANSPARENT,
                    class,
                    w!(""),
                    WS_CHILD | WS_VISIBLE | WS_DISABLED,
                    0,
                    0,
                    width,
                    height,
                    Some(parent),
                    None,
                    Some(instance),
                    None,
                )
            })();
            match configure {
                Ok(render) => Ok(Self {
                    parent,
                    render,
                    visible: false,
                    bounds,
                }),
                Err(error) => {
                    let _ = DestroyWindow(parent);
                    Err(error)
                }
            }
        }
    }

    pub fn update(&mut self, show: bool, bounds: Option<RECT>, panel: usize) {
        unsafe {
            if let Some(rect) = bounds.filter(|rect| *rect != self.bounds) {
                self.bounds = rect;
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;
                let _ = SetWindowPos(
                    self.parent,
                    Some(HWND_TOPMOST),
                    rect.left,
                    rect.top,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                let _ = SetWindowPos(
                    self.render,
                    None,
                    0,
                    0,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            if show != self.visible {
                self.visible = show;
                let _ = ShowWindow(self.parent, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
                if show {
                    keep_below_panel(self.parent, HWND(panel as _));
                }
            }
        }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }
}

pub fn keep_below_panel(window: HWND, panel: HWND) {
    unsafe {
        if IsWindowVisible(panel).as_bool()
            && GetWindowLongPtrW(panel, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0
        {
            let _ = SetWindowPos(
                window,
                Some(panel),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.parent);
        }
    }
}

pub fn pump_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

pub fn tray_bounds(x: i32, y: i32) -> RECT {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        info.rcWork
    }
}

pub fn position_tray(hwnd: HWND, area: RECT, x: i32, y: i32) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return;
        }
        let mut left = x - (rect.right - rect.left) / 2;
        let mut top = if y < area.top {
            area.top + 8
        } else {
            y - (rect.bottom - rect.top) - 12
        };
        left = left.clamp(
            area.left,
            (area.right - (rect.right - rect.left)).max(area.left),
        );
        top = top.clamp(
            area.top,
            (area.bottom - (rect.bottom - rect.top)).max(area.top),
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            left,
            top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}
