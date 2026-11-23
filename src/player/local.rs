use anyhow::{Context, Result};
use libloading::os::windows::Library;
use std::{ffi::c_void, ptr, sync::OnceLock};
use windows::{
    Win32::Media::MediaFoundation::IMFMediaEngineExtension,
    core::{HRESULT, Interface},
};

type Create = unsafe extern "C" fn(
    *const u16,
    *const u16,
    i32,
    i32,
    Option<super::mel::Convert>,
    *mut *mut c_void,
) -> HRESULT;
type Clock = unsafe extern "C" fn(*mut c_void, f64, i32) -> i32;
type Close = unsafe extern "C" fn(*mut c_void);

struct Api {
    create: Create,
    probe: unsafe extern "C" fn(*const u16, super::mel::Convert, *mut i32) -> HRESULT,
    mel_error: unsafe extern "C" fn(*mut c_void) -> i32,
    clock: Clock,
    close: Close,
    changed: unsafe extern "C" fn(*mut c_void) -> i32,
    _library: Library,
}

fn api() -> Result<&'static Api> {
    // Media Foundation may release its last COM callback after a player is closed.
    // Keep the module mapped for the lifetime of the process.
    static API: OnceLock<Api> = OnceLock::new();
    if let Some(api) = API.get() {
        return Ok(api);
    }
    let path = std::env::current_exe()?
        .with_file_name("media")
        .join("rustyfin-media.dll");
    unsafe {
        // LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32
        let library = Library::load_with_flags(&path, 0x100 | 0x800).context(
            "Opening the local media backend. Run scripts/Get-NativeMedia.ps1 for this build.",
        )?;
        let loaded = Api {
            create: *library
                .get(b"rf_create_v2\0")
                .context("Update the local media backend with scripts/Get-NativeMedia.ps1.")?,
            mel_error: *library.get(b"rf_mel_error\0")?,
            probe: *library
                .get(b"rf_probe_layer\0")
                .context("Update the local media backend with scripts/Get-NativeMedia.ps1.")?,
            clock: *library.get(b"rf_clock\0")?,
            close: *library.get(b"rf_close\0")?,
            changed: *library.get(b"rf_endpoint_changed\0")?,
            _library: library,
        };
        let _ = API.set(loaded);
    }
    Ok(API.get().expect("media module initialized"))
}

pub(super) fn profile7_route(url: &str, mel_to_81: bool) -> Result<crate::jellyfin::Route> {
    use crate::jellyfin::Route;
    use windows::Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    };
    let api = api()?;
    let wide: Vec<_> = url.encode_utf16().chain(Some(0)).collect();
    let apartment = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    if let Err(e) = &apartment
        && e.code() != RPC_E_CHANGED_MODE
    {
        return Err(e.clone().into());
    }
    let result = (|| {
        let _mf = super::native::Foundation::start()?;
        let mut code = 0;
        unsafe { (api.probe)(wide.as_ptr(), super::mel::packet, &mut code).ok() }
            .context("Reading the Profile 7 enhancement layer")?;
        match super::mel::LayerError::from_code(code) {
            Some(e) if e.is_fel() => Ok(Route::FelHdr10),
            Some(e) => Err(e.into()),
            None if mel_to_81 => Ok(Route::Mel81),
            None => Ok(Route::MelHdr10),
        }
    })();
    if apartment.is_ok() {
        unsafe { RoUninitialize() };
    }
    result
}

pub(super) struct LocalSource {
    pub mel: bool,
    pub extension: IMFMediaEngineExtension,
    api: &'static Api,
}

impl LocalSource {
    pub fn open(
        url: &str,
        profile: &str,
        index: Option<i32>,
        passthrough: bool,
        mel: bool,
    ) -> Result<Self> {
        let api = api()?;
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut ptr = ptr::null_mut();
        unsafe {
            (api.create)(
                wide(url).as_ptr(),
                wide(profile).as_ptr(),
                index.unwrap_or(-1),
                passthrough.into(),
                mel.then_some(super::mel::packet),
                &mut ptr,
            )
            .ok()?;
            Ok(Self {
                mel,
                extension: IMFMediaEngineExtension::from_raw(ptr),
                api,
            })
        }
    }
    pub fn mel_error(&self) -> Option<super::mel::LayerError> {
        super::mel::LayerError::from_code(unsafe { (self.api.mel_error)(self.extension.as_raw()) })
    }
    pub fn clock(&self, position: f64, active: bool) -> i32 {
        unsafe { (self.api.clock)(self.extension.as_raw(), position, active.into()) }
    }
    pub fn endpoint_changed(&self) -> bool {
        unsafe { (self.api.changed)(self.extension.as_raw()) != 0 }
    }
    pub fn close(&self) {
        unsafe { (self.api.close)(self.extension.as_raw()) }
    }
}
