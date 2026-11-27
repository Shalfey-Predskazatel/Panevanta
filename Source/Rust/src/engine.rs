use libloading::os::windows::{
    LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, Library,
};
use std::{
    ffi::{CStr, c_char, c_void},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::null_mut,
};
use windows::{
    Graphics::DirectX::Direct3D11::{IDirect3DDevice, IDirect3DSurface},
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct3D11::{ID3D11Device, ID3D11Texture2D},
            Dxgi::IDXGIDevice,
        },
        System::WinRT::Direct3D11::{
            CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
        },
    },
    core::{HRESULT, Interface},
};

#[repr(C)]
pub struct Parameters {
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub factor: i32,
    pub quality: i32,
    pub sr: i32,
    pub hdr: i32,
    pub display_hdr: i32,
    pub peak: i32,
    pub white: f32,
    pub scale: f32,
}

type CreateDevice = unsafe extern "C" fn(*mut *mut c_void) -> HRESULT;
type Create = unsafe extern "C" fn(
    *mut c_void,
    HWND,
    &Parameters,
    *const u16,
    *const u16,
    *mut c_char,
    usize,
) -> *mut c_void;
type Submit = unsafe extern "C" fn(*mut c_void, *mut c_void, i64, i64, *mut c_char, usize) -> i32;
type Destroy = unsafe extern "C" fn(*mut c_void);

pub struct EngineApi {
    _library: Library,
    device: CreateDevice,
    create: Create,
    submit: Submit,
    destroy: Destroy,
    sdk: PathBuf,
}

impl EngineApi {
    pub fn load(program: &Path) -> Result<Self, String> {
        let default_sdk = program
            .join("..")
            .join("Dependencies")
            .join("NVIDIA-VideoFX");
        let sdk = if default_sdk.join("bin/NVVideoEffects.dll").is_file() {
            default_sdk
        } else {
            std::env::var_os("NV_VIDEO_EFFECTS_PATH")
                .map(PathBuf::from)
                .unwrap_or(default_sdk)
        };
        if !sdk.join("bin/NVVideoEffects.dll").is_file() {
            return Err("Не найден NVIDIA Video Effects SDK. Распакуйте папки Program и Dependencies рядом.".into());
        }
        let dll = program.join("Panevanta.Native.dll");
        unsafe {
            let library = Library::load_with_flags(
                &dll,
                LOAD_LIBRARY_SEARCH_DEFAULT_DIRS | LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
            )
            .map_err(|error| format!("Не удалось загрузить {}. {error}", dll.display()))?;
            fn symbol<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, String> {
                unsafe {
                    lib.get::<T>(name)
                        .map(|value| *value)
                        .map_err(|error| error.to_string())
                }
            }
            Ok(Self {
                device: symbol(&library, b"PaneCreateCaptureDevice\0")?,
                create: symbol(&library, b"PaneCreate\0")?,
                submit: symbol(&library, b"PaneSubmit\0")?,
                destroy: symbol(&library, b"PaneDestroy\0")?,
                _library: library,
                sdk,
            })
        }
    }

    pub fn capture_device(&self) -> windows::core::Result<IDirect3DDevice> {
        unsafe {
            let mut raw = null_mut();
            (self.device)(&mut raw).ok()?;
            let device = ID3D11Device::from_raw(raw);
            let dxgi: IDXGIDevice = device.cast()?;
            CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()
        }
    }

    pub fn open(
        &self,
        texture: &ID3D11Texture2D,
        hwnd: HWND,
        settings: &Parameters,
        log: &Path,
    ) -> Result<Engine<'_>, String> {
        let sdk = wide(&self.sdk);
        let log = wide(log);
        let mut error = [0; 2048];
        let handle = unsafe {
            (self.create)(
                texture.as_raw(),
                hwnd,
                settings,
                sdk.as_ptr(),
                log.as_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if handle.is_null() {
            return Err(engine_error(&error));
        }
        Ok(Engine { handle, api: self })
    }
}

pub struct Engine<'a> {
    handle: *mut c_void,
    api: &'a EngineApi,
}

impl Engine<'_> {
    pub fn submit(
        &self,
        texture: Option<&ID3D11Texture2D>,
        sequence: i64,
        time: i64,
    ) -> Result<(), String> {
        let mut error = [0; 2048];
        let raw = texture.map_or(null_mut(), Interface::as_raw);
        let ok = unsafe {
            (self.api.submit)(
                self.handle,
                raw,
                sequence,
                time,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if ok != 0 {
            Ok(())
        } else {
            Err(engine_error(&error))
        }
    }
}

impl Drop for Engine<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.destroy)(self.handle) }
    }
}

pub fn texture(surface: &IDirect3DSurface) -> windows::core::Result<ID3D11Texture2D> {
    let dxgi: IDirect3DDxgiInterfaceAccess = surface.cast()?;
    unsafe { dxgi.GetInterface() }
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn engine_error(error: &[c_char]) -> String {
    let message = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
    if message.is_empty() {
        "Ошибка движка Panevanta.".into()
    } else {
        message.into_owned()
    }
}
