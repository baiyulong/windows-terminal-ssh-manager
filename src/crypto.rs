/// Windows DPAPI password encryption.
///
/// `encrypt_password` / `decrypt_password` bind the encrypted blob to the
/// current Windows user account via CryptProtectData / CryptUnprotectData.
/// No master password is required; only the same OS user can decrypt.
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as B64, Engine};

pub fn encrypt_password(plaintext: &str) -> Result<String> {
    let blob = dpapi_protect(plaintext.as_bytes())?;
    Ok(B64.encode(blob))
}

pub fn decrypt_password(ciphertext: &str) -> Result<String> {
    let blob = B64.decode(ciphertext)?;
    let plain = dpapi_unprotect(&blob)?;
    Ok(String::from_utf8(plain)?)
}

// ── Windows DPAPI (Crypt32.dll + Kernel32.dll) ────────────────────────────────

#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

#[cfg(windows)]
#[link(name = "Crypt32")]
extern "system" {
    fn CryptProtectData(
        p_data_in: *const DataBlob,
        sz_data_descr: *const u16,
        p_optional_entropy: *const DataBlob,
        pv_reserved: *const std::ffi::c_void,
        p_prompt_struct: *const std::ffi::c_void,
        dw_flags: u32,
        p_data_out: *mut DataBlob,
    ) -> i32;

    fn CryptUnprotectData(
        p_data_in: *const DataBlob,
        pp_sz_data_descr: *mut *mut u16,
        p_optional_entropy: *const DataBlob,
        pv_reserved: *const std::ffi::c_void,
        p_prompt_struct: *const std::ffi::c_void,
        dw_flags: u32,
        p_data_out: *mut DataBlob,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(h_mem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn GetLastError() -> u32;
}

#[cfg(windows)]
fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>> {
    use std::{ffi::c_void, ptr};

    let input = DataBlob { cb_data: data.len() as u32, pb_data: data.as_ptr() as *mut u8 };
    let mut output = DataBlob { cb_data: 0, pb_data: ptr::null_mut() };

    let ok = unsafe {
        CryptProtectData(&input, ptr::null(), ptr::null(), ptr::null(), ptr::null(), 0, &mut output)
    };
    if ok == 0 {
        anyhow::bail!("CryptProtectData failed (error {})", unsafe { GetLastError() });
    }

    let result = unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize) }.to_vec();
    unsafe { LocalFree(output.pb_data as *mut c_void) };
    Ok(result)
}

#[cfg(windows)]
fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>> {
    use std::{ffi::c_void, ptr};

    let input = DataBlob { cb_data: data.len() as u32, pb_data: data.as_ptr() as *mut u8 };
    let mut output = DataBlob { cb_data: 0, pb_data: ptr::null_mut() };

    let ok = unsafe {
        CryptUnprotectData(
            &input, ptr::null_mut(), ptr::null(), ptr::null(), ptr::null(), 0, &mut output,
        )
    };
    if ok == 0 {
        anyhow::bail!(
            "CryptUnprotectData failed — the data may belong to a different user or machine"
        );
    }

    let result = unsafe { std::slice::from_raw_parts(output.pb_data, output.cb_data as usize) }.to_vec();
    unsafe { LocalFree(output.pb_data as *mut c_void) };
    Ok(result)
}

#[cfg(not(windows))]
fn dpapi_protect(_data: &[u8]) -> Result<Vec<u8>> {
    anyhow::bail!("DPAPI encryption is only available on Windows")
}

#[cfg(not(windows))]
fn dpapi_unprotect(_data: &[u8]) -> Result<Vec<u8>> {
    anyhow::bail!("DPAPI decryption is only available on Windows")
}
