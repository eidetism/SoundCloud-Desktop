//! Secrets (proxy password, OAuth token) in the Windows Credential Manager.
//!
//! Stored as generic credentials with `CRED_PERSIST_LOCAL_MACHINE`, so they
//! never roam with the profile. Values never touch settings files or logs.

use std::ptr;

use windows_sys::Win32::Foundation::{ERROR_INVALID_PARAMETER, ERROR_NOT_FOUND, GetLastError};
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};

use crate::app::APP_IDENTIFIER;

#[derive(Clone, Copy)]
pub enum Secret {
    ProxyPassword,
    OAuthToken,
}

impl Secret {
    fn name(self) -> &'static str {
        match self {
            Secret::ProxyPassword => "proxy-password",
            Secret::OAuthToken => "oauth-token",
        }
    }

    fn target(self) -> String {
        format!("{APP_IDENTIFIER}/{}", self.name())
    }
}

fn failed(secret: Secret, action: &str, code: u32) -> String {
    format!(
        "credential manager: {action} {} failed (windows error {code})",
        secret.name()
    )
}

pub fn load(secret: Secret) -> Result<Option<String>, String> {
    let target = wide(&secret.target());
    let mut found: *mut CREDENTIALW = ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut found) } == 0 {
        return match unsafe { GetLastError() } {
            ERROR_NOT_FOUND => Ok(None),
            code => Err(failed(secret, "read", code)),
        };
    }
    let bytes = unsafe {
        let credential = &*found;
        let bytes = if credential.CredentialBlob.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
            .to_vec()
        };
        CredFree(found as *const core::ffi::c_void);
        bytes
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("credential manager: {} is not UTF-8", secret.name()))
}

pub fn store(secret: Secret, value: &str) -> Result<(), String> {
    let mut target = wide(&secret.target());
    let mut user = wide(secret.name());
    let mut blob = value.as_bytes().to_vec();
    let size =
        u32::try_from(blob.len()).map_err(|_| failed(secret, "write", ERROR_INVALID_PARAMETER))?;
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        UserName: user.as_mut_ptr(),
        CredentialBlobSize: size,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    let written = unsafe { CredWriteW(&credential, 0) } != 0;
    blob.fill(0);
    if !written {
        return Err(failed(secret, "write", unsafe { GetLastError() }));
    }
    Ok(())
}

pub fn delete(secret: Secret) -> Result<(), String> {
    let target = wide(&secret.target());
    if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        return match unsafe { GetLastError() } {
            ERROR_NOT_FOUND => Ok(()),
            code => Err(failed(secret, "delete", code)),
        };
    }
    Ok(())
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
