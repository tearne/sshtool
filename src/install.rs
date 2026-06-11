//! Windows-only: install the binary into %USERPROFILE%\bin and add it to the
//! user PATH in the registry so `sshtool` works from any terminal.

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW,
};

pub async fn install() -> Result<()> {
    let exe_path = std::env::current_exe()
        .context("failed to determine current executable path")?;

    let install_dir = install_dir()?;
    std::fs::create_dir_all(&install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;

    let dest = install_dir.join("sshtool.exe");
    if same_file(&exe_path, &dest) {
        println!("Already installed at {}", dest.display());
    } else {
        std::fs::copy(&exe_path, &dest)
            .with_context(|| format!("failed to copy to {}", dest.display()))?;
        println!("Installed: {}", dest.display());
    }

    ensure_on_path(&install_dir)?;

    println!("Open a new terminal and run `sshtool` to get started.");
    Ok(())
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    // Canonicalise both paths to resolve symlinks and normalise case/separators.
    // If either path doesn't exist yet, they can't be the same file.
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn install_dir() -> Result<PathBuf> {
    let profile = std::env::var("USERPROFILE")
        .context("USERPROFILE environment variable not set")?;
    Ok(PathBuf::from(profile).join("bin"))
}

fn ensure_on_path(dir: &std::path::Path) -> Result<()> {
    let dir_str = dir.to_string_lossy().to_string();
    let current = read_user_path()?;

    let parts: Vec<&str> = current.split(';').collect();
    if parts.iter().any(|p| p.trim().eq_ignore_ascii_case(&dir_str)) {
        println!("PATH already contains {dir_str}");
        return Ok(());
    }

    let new_path = if current.is_empty() {
        dir_str.clone()
    } else {
        format!("{current};{dir_str}")
    };

    write_user_path(&new_path)?;
    println!("Added {dir_str} to user PATH.");
    Ok(())
}

fn read_user_path() -> Result<String> {
    unsafe {
        let subkey = to_wide("Environment\0");
        let value_name = to_wide("PATH\0");
        let mut hkey = std::ptr::null_mut();

        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut hkey,
        );
        if result != ERROR_SUCCESS {
            return Ok(String::new());
        }

        let mut data_type: u32 = 0;
        let mut size: u32 = 0;
        let result = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut data_type,
            std::ptr::null_mut(),
            &mut size,
        );

        if result != ERROR_SUCCESS {
            RegCloseKey(hkey);
            return Ok(String::new());
        }

        let mut buf = vec![0u16; (size as usize / 2) + 1];
        let result = RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut data_type,
            buf.as_mut_ptr() as *mut u8,
            &mut size,
        );
        RegCloseKey(hkey);

        if result != ERROR_SUCCESS {
            bail!("failed to read PATH (error {result})");
        }

        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(OsString::from_wide(&buf[..len]).to_string_lossy().into_owned())
    }
}

fn write_user_path(value: &str) -> Result<()> {
    unsafe {
        let subkey = to_wide("Environment\0");
        let value_name = to_wide("PATH\0");
        let mut hkey = std::ptr::null_mut();

        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        );
        if result != ERROR_SUCCESS {
            bail!("failed to open HKCU\\Environment for writing (error {result})");
        }

        let wide: Vec<u16> = OsString::from(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let byte_len = (wide.len() * 2) as u32;

        let result = RegSetValueExW(
            hkey,
            value_name.as_ptr(),
            0,
            REG_SZ,
            wide.as_ptr() as *const u8,
            byte_len,
        );
        RegCloseKey(hkey);

        if result != ERROR_SUCCESS {
            bail!("failed to write PATH (error {result})");
        }
        Ok(())
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    OsString::from(s).encode_wide().collect()
}
