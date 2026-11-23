use crate::jellyfin::Account;
use anyhow::Result;
use serde::{Serialize, de::DeserializeOwned};
use windows::{
    Win32::{Foundation::ERROR_NOT_FOUND, Security::Credentials::*},
    core::{PCWSTR, PWSTR, w},
};

// Keep the original Credential Manager target names so upgrades retain accounts and service keys.
const ACTIVE: &str = "RustFin/Jellyfin";
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn profile(a: &Account) -> String {
    format!(
        "RustFin/Profile/{}",
        crate::preferences::item_key(&format!("{}\0{}", a.server, a.user))
    )
}

pub fn read_secret<T: DeserializeOwned>(name: &str) -> Result<Option<T>> {
    let name = wide(name);
    let mut raw = std::ptr::null_mut();
    unsafe {
        if let Err(e) = CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut raw) {
            if e.code() == ERROR_NOT_FOUND.to_hresult() {
                return Ok(None);
            }
            return Err(e.into());
        }
        let result = decode(raw);
        CredFree(raw.cast());
        result
    }
}
unsafe fn decode<T: DeserializeOwned>(raw: *const CREDENTIALW) -> Result<Option<T>> {
    let c = unsafe { &*raw };
    if c.CredentialBlobSize == 0 || c.CredentialBlob.is_null() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(unsafe {
        std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
    })?))
}
pub fn write_secret<T: Serialize>(name: &str, value: &T) -> Result<()> {
    let mut name = wide(name);
    let mut data = serde_json::to_vec(value)?;
    let record = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlob: data.as_mut_ptr(),
        CredentialBlobSize: data.len().try_into()?,
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    unsafe {
        CredWriteW(&record, 0)?;
    }
    Ok(())
}
pub fn remove_secret(name: &str) -> Result<()> {
    let name = wide(name);
    if let Err(e) = unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) }
        && e.code() != ERROR_NOT_FOUND.to_hresult()
    {
        return Err(e.into());
    }
    Ok(())
}
pub fn profiles() -> Result<Vec<Account>> {
    let mut n = 0;
    let mut list = std::ptr::null_mut();
    unsafe {
        if let Err(e) = CredEnumerateW(w!("RustFin/Profile/*"), None, &mut n, &mut list) {
            if e.code() == ERROR_NOT_FOUND.to_hresult() {
                return Ok(vec![]);
            }
            return Err(e.into());
        }
        let records = std::slice::from_raw_parts(list, n as usize)
            .iter()
            .map(|&p| decode(p))
            .collect::<Result<Vec<_>>>();
        CredFree(list.cast());
        Ok(records?.into_iter().flatten().collect())
    }
}
pub fn load() -> Result<Option<Account>> {
    read_secret(ACTIVE)
}
pub fn save(a: &Account) -> Result<()> {
    if let Some(previous) = load()? {
        write_secret(&profile(&previous), &previous)?;
    }
    write_secret(&profile(a), a)?;
    write_secret(ACTIVE, a)
}
pub fn clear() -> Result<()> {
    if let Some(a) = load()? {
        remove_secret(&profile(&a))?;
    }
    remove_secret(ACTIVE)
}
