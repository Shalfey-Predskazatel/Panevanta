use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::{BLACK_BRUSH, GetStockObject, HBRUSH},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::{HSTRING, PCWSTR, Result, w},
};

const SHOW: u32 = WM_APP + 1;
const ASPECT: u32 = WM_APP + 2;
const DESTROY: u32 = WM_APP + 3;
const STYLE: WINDOW_STYLE = WS_OVERLAPPEDWINDOW;
pub const TITLE: &str = "Panevanta — Вывод для стрима";

pub struct Preview {
    parent: usize,
    render: usize,
    closed: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

struct WindowState {
    render: HWND,
    source: (i32, i32),
    closed: Arc<AtomicBool>,
    panel: HWND,
}

impl Preview {
    pub fn new(
        work_area: RECT,
        source: (i32, i32),
        panel: usize,
    ) -> std::result::Result<Self, String> {
        let closed = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&closed);
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("Panevanta stream window".into())
            .spawn(move || {
                let mut state = WindowState {
                    render: HWND::default(),
                    source,
                    closed: signal,
                    panel: HWND(panel as _),
                };
                let result = create_window(&mut state, work_area);
                match result {
                    Ok(parent) => {
                        if sender
                            .send(Ok((parent.0 as usize, state.render.0 as usize)))
                            .is_ok()
                        {
                            unsafe {
                                let mut msg = MSG::default();
                                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                                    let _ = TranslateMessage(&msg);
                                    DispatchMessageW(&msg);
                                }
                            }
                        }
                        unsafe {
                            if IsWindow(Some(parent)).as_bool() {
                                let _ = DestroyWindow(parent);
                            }
                        }
                        state.closed.store(true, Ordering::Release);
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        match receiver.recv() {
            Ok(Ok((parent, render))) => Ok(Self {
                parent,
                render,
                closed,
                thread: Some(thread),
            }),
            result => {
                let _ = thread.join();
                Err(match result {
                    Ok(Err(message)) => message,
                    _ => "Поток окна завершился при запуске.".into(),
                })
            }
        }
    }

    fn parent(&self) -> HWND {
        HWND(self.parent as _)
    }
    pub fn render(&self) -> HWND {
        HWND(self.render as _)
    }
    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    pub fn visible(&self) -> bool {
        unsafe { IsWindowVisible(self.parent()).as_bool() }
    }
    pub fn drawable(&self) -> bool {
        unsafe { self.visible() && !IsIconic(self.parent()).as_bool() }
    }

    pub fn show(&self) {
        unsafe {
            let _ = PostMessageW(Some(self.parent()), SHOW, WPARAM(0), LPARAM(0));
        }
    }

    pub fn set_aspect(&self, width: i32, height: i32) {
        unsafe {
            let _ = PostMessageW(
                Some(self.parent()),
                ASPECT,
                WPARAM(width as usize),
                LPARAM(height as isize),
            );
        }
    }

    pub fn monitor_source(&self) -> crate::win::SourceId {
        crate::win::SourceId {
            handle: self.parent,
            screen: false,
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        unsafe {
            let parent = Some(self.parent());
            let _ = PostMessageW(parent, WM_CANCELMODE, WPARAM(0), LPARAM(0));
            let _ = PostMessageW(parent, DESTROY, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn create_window(state: &mut WindowState, area: RECT) -> Result<HWND> {
    unsafe {
        let instance = HINSTANCE(GetModuleHandleW(PCWSTR::null())?.0);
        const MAIN: PCWSTR = w!("Panevanta.StreamOutput");
        const VIDEO: PCWSTR = w!("Panevanta.StreamVideo");
        static REGISTERED: OnceLock<bool> = OnceLock::new();
        let black = HBRUSH(GetStockObject(BLACK_BRUSH).0);
        if !REGISTERED.get_or_init(|| {
            let window = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                lpszClassName: MAIN,
                hbrBackground: black,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                ..Default::default()
            };
            let video = WNDCLASSW {
                lpszClassName: VIDEO,
                lpfnWndProc: Some(video_proc),
                ..window
            };
            RegisterClassW(&window) != 0 && RegisterClassW(&video) != 0
        }) {
            return Err(windows::core::Error::from_win32());
        }

        let max_w = ((area.right - area.left) * 3 / 5).clamp(160, 1280);
        let max_h = ((area.bottom - area.top) * 7 / 10).max(120);
        let (width, height) = fit(state.source, (max_w, max_h));
        let mut client = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let style_ex = WS_EX_APPWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE;
        let _ = AdjustWindowRectEx(&mut client, STYLE, false, style_ex);
        let outer_w = client.right - client.left;
        let outer_h = client.bottom - client.top;
        let title = HSTRING::from(TITLE);
        let parent = CreateWindowExW(
            style_ex,
            MAIN,
            &title,
            STYLE,
            area.right - outer_w - 24,
            area.top + ((area.bottom - area.top - outer_h) / 3).max(0),
            outer_w,
            outer_h,
            None,
            None,
            Some(instance),
            None,
        )?;
        let child = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            VIDEO,
            w!(""),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            width,
            height,
            Some(parent),
            None,
            Some(instance),
            None,
        );
        match child {
            Ok(render) => {
                state.render = render;
                SetWindowLongPtrW(parent, GWLP_USERDATA, state as *mut WindowState as isize);
                layout(parent, state.render, state.source);
                Ok(parent)
            }
            Err(err) => {
                let _ = DestroyWindow(parent);
                Err(err)
            }
        }
    }
}

unsafe extern "system" fn video_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
        match msg {
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_POINTERACTIVATE => return LRESULT(PA_NOACTIVATE as isize),
            WM_CLOSE => {
                if let Some(state) = state.as_ref() {
                    state.closed.store(true, Ordering::Release);
                }
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            WM_SIZE => {
                if let Some(state) = state.as_ref() {
                    layout(hwnd, state.render, state.source);
                }
            }
            SHOW => {
                if let Some(state) = state.as_ref()
                    && !state.closed.load(Ordering::Acquire)
                {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    crate::win::keep_below_panel(hwnd, state.panel);
                }
            }
            ASPECT => {
                if let Some(state) = state.as_mut() {
                    state.source = (wparam.0 as i32, lparam.0 as i32);
                    layout(hwnd, state.render, state.source);
                }
            }
            DESTROY => {
                let _ = DestroyWindow(hwnd);
            }
            WM_DESTROY => {
                PostQuitMessage(0);
            }
            _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
    LRESULT(0)
}

fn layout(hwnd: HWND, render: HWND, source: (i32, i32)) {
    let mut rect = RECT::default();
    unsafe {
        if GetClientRect(hwnd, &mut rect).is_err() || rect.right <= 0 || rect.bottom <= 0 {
            return;
        }
        let (width, height) = fit(source, (rect.right, rect.bottom));
        let _ = SetWindowPos(
            render,
            None,
            (rect.right - width) / 2,
            (rect.bottom - height) / 2,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn fit((w, h): (i32, i32), (max_w, max_h): (i32, i32)) -> (i32, i32) {
    let width_for_height = i64::from(w) * i64::from(max_h) / i64::from(h);
    if width_for_height <= i64::from(max_w) {
        (width_for_height.max(1) as i32, max_h)
    } else {
        (
            max_w,
            (i64::from(h) * i64::from(max_w) / i64::from(w)).max(1) as i32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::fit;
    #[test]
    fn fits_widescreen_and_portrait_without_stretching() {
        assert_eq!(fit((1920, 1080), (1000, 1000)), (1000, 562));
        assert_eq!(fit((1080, 1920), (1000, 1000)), (562, 1000));
        assert_eq!(fit((1920, 1080), (1280, 720)), (1280, 720));
    }
}
