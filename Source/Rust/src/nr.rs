use libloading::os::windows::{
    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, Library,
};
use std::{
    ffi::{CStr, c_char, c_void},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::{self, JoinHandle},
};
use windows::{
    Win32::{
        Graphics::Direct3D11::ID3D11Texture2D,
        Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
        },
    },
    core::{Interface, PCWSTR, w},
};

type Create = unsafe extern "C" fn(
    *mut c_void,
    u32,
    u32,
    f32,
    i32,
    i32,
    *const u16,
    *const u16,
    *mut c_char,
    usize,
) -> *mut c_void;
type Process = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_char, usize) -> *mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
type Configure = unsafe extern "C" fn(*mut c_void, &Options);
type ResetHistory = unsafe extern "C" fn(*mut c_void);
type Resize = unsafe extern "C" fn(*mut c_void, u32, u32, *mut c_char, usize) -> i32;

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub struct Options {
    tone: f32,
    structure: f32,
    skin: f32,
    style: u32,
    passes: u32,
    detail: i32,
    auto_mask: i32,
    ui_correction: i32,
}

impl From<crate::capture::Settings> for Options {
    fn from(settings: crate::capture::Settings) -> Self {
        let fraction = |percent: i32| percent as f32 / 100.;
        Self {
            tone: fraction(settings.nr_tone),
            structure: fraction(settings.nr_structure),
            skin: fraction(settings.nr_skin),
            style: settings.nr_style,
            passes: settings.nr_passes,
            detail: settings.nr_detail.into(),
            auto_mask: settings.nr_auto_mask.into(),
            ui_correction: settings.nr_ui_correction.into(),
        }
    }
}

pub fn runtime(program: &Path) -> Option<PathBuf> {
    if !program.join("Panevanta.NR.dll").is_file() {
        return None;
    }
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("DLSS5-NR/runtime"));
    let portable = program.parent().map(|p| p.join("Dependencies/DLSS-NR"));
    [local, portable]
        .into_iter()
        .flatten()
        .filter_map(|folder| {
            let version = file_version(&folder.join("nvngx_dlssnr.dll"))?;
            (version >= ((310 << 16) | 9, 16 << 16)).then_some((version, folder))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, folder)| folder)
}

fn file_version(path: &Path) -> Option<(u32, u32)> {
    let filename: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let filename = PCWSTR(filename.as_ptr());
        let size = GetFileVersionInfoSizeW(filename, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(filename, None, size, data.as_mut_ptr().cast()).ok()?;
        let mut pointer = std::ptr::null_mut();
        let mut length = 0;
        if !VerQueryValueW(data.as_ptr().cast(), w!("\\"), &mut pointer, &mut length).as_bool()
            || pointer.is_null()
            || length < size_of::<VS_FIXEDFILEINFO>() as u32
        {
            return None;
        }
        let info = pointer.cast::<VS_FIXEDFILEINFO>().read_unaligned();
        (info.dwSignature == 0xFEEF04BD).then_some((info.dwFileVersionMS, info.dwFileVersionLS))
    }
}

pub fn dimensions(width: i32, height: i32, limit: (i32, i32)) -> (u32, u32) {
    let ratio = (limit.0 as f64 / width as f64).min(limit.1 as f64 / height as f64);
    let align = |value: i32| ((value as f64 * ratio).round() as u32 & !1).max(32);
    (align(width), align(height))
}

#[cfg(test)]
mod tests {
    use super::dimensions;
    #[test]
    fn fitting_preserves_the_desktop_aspect_ratio() {
        assert_eq!(dimensions(2560, 1600, (852, 480)), (768, 480));
        assert_eq!(dimensions(2560, 1600, (3840, 2160)), (3456, 2160));
        assert_eq!(dimensions(2560, 1600, (2560, 1600)), (2560, 1600));
    }
}

struct Pass {
    _library: Library,
    process: Process,
    destroy: Destroy,
    configure: Configure,
    resize: Resize,
    reset_history: Option<ResetHistory>,
    handle: *mut c_void,
    options: Option<Options>,
}

// Initialization completes on a separate thread before ownership transfers.
// Processing and destruction have a single owner, with D3D11 multithread protection enabled.
unsafe impl Send for Pass {}

type LoadResult = Result<Pass, String>;

#[derive(Default)]
pub struct Live {
    ready: Option<((u32, u32), Pass)>,
    job: Option<((u32, u32), Receiver<LoadResult>, JoinHandle<()>)>,
    failed: Option<((u32, u32), String)>,
}

impl Live {
    pub fn frame(
        &mut self,
        program: &Path,
        source: &ID3D11Texture2D,
        size: (u32, u32),
        display: crate::win::Display,
        enabled: bool,
        options: Options,
    ) -> Result<Option<ID3D11Texture2D>, String> {
        if let Some((_, receiver, _)) = &self.job {
            let result = match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => {
                    Some(Err("Подготовка DLSS 5 NR прервана.".into()))
                }
                Err(TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                let (size, _, worker) = self.job.take().expect("NR job with delivered result");
                let _ = worker.join();
                match result {
                    Ok(pass) => self.ready = Some((size, pass)),
                    Err(message) => self.failed = Some((size, message)),
                }
            }
        }
        if !enabled {
            if let Some((_, pass)) = &self.ready
                && let Some(reset) = pass.reset_history
            {
                unsafe { reset(pass.handle) };
            }
            self.failed = None;
            return Ok(None);
        }
        if let Some((previous, error)) = &self.failed {
            if *previous == size {
                return Err(error.clone());
            }
            self.failed = None;
        }
        if let Some((ready, pass)) = &mut self.ready
            && *ready == size
        {
            return match pass.process(source, options) {
                Ok(texture) => Ok(Some(texture)),
                Err(error) => {
                    self.ready = None;
                    self.failed = Some((size, error.clone()));
                    Err(error)
                }
            };
        }
        if self.job.is_none() {
            let previous = self.ready.take();
            let program = program.to_owned();
            let source = source.clone();
            let (sender, receiver) = mpsc::sync_channel(1);
            let worker = thread::Builder::new()
                .name("Panevanta NR load".into())
                .spawn(move || {
                    let result = match previous {
                        Some((_, pass)) => pass.with_size(size),
                        None => Pass::new(&program, &source, size, display.white, display.hdr),
                    };
                    let _ = sender.send(result);
                })
                .map_err(|error| error.to_string())?;
            self.job = Some((size, receiver, worker));
        }
        Ok(None)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        if let Some((_, receiver, worker)) = self.job.take() {
            drop(receiver);
            let _ = worker.join();
        }
    }
}

impl Pass {
    pub fn new(
        program: &Path,
        source: &ID3D11Texture2D,
        size: (u32, u32),
        white: f32,
        hdr: bool,
    ) -> Result<Self, String> {
        let runtime =
            runtime(program).ok_or("Установите NVIDIA-подписанную библиотеку nvngx_dlssnr.dll.")?;
        let cache = program.join("settings/DLSS-NR");
        std::fs::create_dir_all(&cache).map_err(|e| format!("DLSS NR: {e}"))?;
        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let mut message = [0; 512];
        unsafe {
            let library = Library::load_with_flags(
                program.join("Panevanta.NR.dll"),
                LOAD_LIBRARY_SEARCH_DEFAULT_DIRS | LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
            )
            .map_err(|e| format!("Panevanta.NR.dll: {e}"))?;
            let create = *library
                .get::<Create>(b"PaneNrCreate\0")
                .map_err(|e| e.to_string())?;
            let process = *library
                .get::<Process>(b"PaneNrProcess\0")
                .map_err(|e| e.to_string())?;
            let destroy = *library
                .get::<Destroy>(b"PaneNrDestroy\0")
                .map_err(|e| e.to_string())?;
            let configure = *library
                .get::<Configure>(b"PaneNrConfigureV2\0")
                .map_err(|_| "Обновите Panevanta.NR.dll из портативного комплекта.".to_owned())?;
            let reset_history = library
                .get::<ResetHistory>(b"PaneNrResetHistory\0")
                .ok()
                .map(|symbol| *symbol);
            let resize = *library
                .get::<Resize>(b"PaneNrResize\0")
                .map_err(|e| e.to_string())?;
            let handle = create(
                source.as_raw(),
                size.0,
                size.1,
                white,
                hdr.into(),
                1,
                wide(&runtime).as_ptr(),
                wide(&cache).as_ptr(),
                message.as_mut_ptr(),
                message.len(),
            );
            if handle.is_null() {
                return Err(error(&message));
            }
            Ok(Self {
                _library: library,
                process,
                destroy,
                configure,
                resize,
                reset_history,
                handle,
                options: None,
            })
        }
    }

    fn with_size(self, size: (u32, u32)) -> Result<Self, String> {
        let mut message = [0; 512];
        let success = unsafe {
            (self.resize)(
                self.handle,
                size.0,
                size.1,
                message.as_mut_ptr(),
                message.len(),
            )
        };
        if success == 0 {
            Err(error(&message))
        } else {
            Ok(self)
        }
    }

    fn process(
        &mut self,
        source: &ID3D11Texture2D,
        options: Options,
    ) -> Result<ID3D11Texture2D, String> {
        let mut message = [0; 512];
        unsafe {
            if self.options != Some(options) {
                (self.configure)(self.handle, &options);
                self.options = Some(options);
            }
            let frame = (self.process)(
                self.handle,
                source.as_raw(),
                message.as_mut_ptr(),
                message.len(),
            );
            if frame.is_null() {
                Err(error(&message))
            } else {
                Ok(ID3D11Texture2D::from_raw(frame))
            }
        }
    }
}

impl Drop for Pass {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.handle) };
    }
}

fn error(message: &[c_char]) -> String {
    let message = unsafe { CStr::from_ptr(message.as_ptr()) }.to_string_lossy();
    if message.is_empty() {
        "Ошибка DLSS 5 NR.".into()
    } else {
        message.into_owned()
    }
}
