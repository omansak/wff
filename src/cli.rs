use std::path::{Path, PathBuf};

use windows_sys::Win32::UI::WindowsAndMessaging::{HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, RegType};
use winreg::types::{FromRegValue, ToRegValue};

/// The name typed on the command line: `wff .`
pub const NAME: &str = "wff";

fn directory() -> std::io::Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|base| PathBuf::from(base).join(NAME).join("bin"))
        .ok_or_else(|| std::io::Error::other("LOCALAPPDATA is not set"))
}

fn script(dir: &Path) -> PathBuf {
    dir.join(format!("{NAME}.cmd"))
}

pub fn installed() -> bool {
    directory().is_ok_and(|dir| script(&dir).exists())
}

/// Re-registers only when the stored path no longer matches the running executable.
pub fn refresh() {
    if installed() && !registration_matches() {
        install().ok();
    }
}

fn registration_matches() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let registered: String = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(app_paths_key())
        .and_then(|key| key.get_value(""))
        .unwrap_or_default();

    registered == exe.display().to_string()
}

pub fn install() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let dir = directory()?;

    std::fs::create_dir_all(&dir)?;

    // A trailing backslash (drive roots like `D:\`) would escape the closing quote, so it is doubled.
    let batch = format!(
        "@echo off\r\n\
         setlocal\r\n\
         set \"TARGET=%~1\"\r\n\
         if not defined TARGET set \"TARGET=%CD%\"\r\n\
         if \"%TARGET:~-1%\"==\"{slash}\" set \"TARGET=%TARGET%{slash}\"\r\n\
         start \"\" \"{exe}\" \"%TARGET%\"\r\n\
         endlocal\r\n",
        slash = '\\',
        exe = exe.display()
    );

    std::fs::write(script(&dir), batch)?;

    add_to_user_path(&dir)?;
    register_app_path(&exe)
}

pub fn remove() -> std::io::Result<()> {
    let dir = directory()?;

    ignore_missing(std::fs::remove_file(script(&dir)))?;
    unregister_app_path()?;
    remove_from_user_path(&dir)
}

fn ignore_missing(outcome: std::io::Result<()>) -> std::io::Result<()> {
    match outcome {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn app_paths_key() -> String {
    format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{NAME}.exe")
}

fn register_app_path(exe: &Path) -> std::io::Result<()> {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(app_paths_key())?;

    key.set_value("", &exe.display().to_string())?;
    key.set_value("Path", &exe.parent().unwrap_or(exe).display().to_string())
}

fn unregister_app_path() -> std::io::Result<()> {
    ignore_missing(RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(app_paths_key()))
}

fn add_to_user_path(dir: &Path) -> std::io::Result<()> {
    let entry = dir.display().to_string();
    let environment = user_environment()?;
    let (current, value_type) = read_user_path(&environment)?;

    if current.split(';').any(|part| part.trim().eq_ignore_ascii_case(&entry)) {
        return Ok(());
    }

    let updated = if current.trim().is_empty() { entry } else { format!("{};{entry}", current.trim_end_matches(';')) };

    write_user_path(&environment, &updated, value_type)
}

fn remove_from_user_path(dir: &Path) -> std::io::Result<()> {
    let entry = dir.display().to_string();
    let environment = user_environment()?;
    let (current, value_type) = read_user_path(&environment)?;

    let kept: Vec<&str> = current
        .split(';')
        .filter(|part| !part.trim().is_empty() && !part.trim().eq_ignore_ascii_case(&entry))
        .collect();

    let updated = kept.join(";");
    if updated == current {
        return Ok(());
    }

    write_user_path(&environment, &updated, value_type)
}

fn user_environment() -> std::io::Result<RegKey> {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)
}

/// PATH is usually REG_EXPAND_SZ; written back as REG_SZ, entries like `%USERPROFILE%` would stop
/// expanding, so the type is preserved. Only a missing value counts as empty: any other read
/// error must not end up replacing the whole PATH.
fn read_user_path(environment: &RegKey) -> std::io::Result<(String, RegType)> {
    match environment.get_raw_value("Path") {
        Ok(raw) => {
            let value_type = raw.vtype.clone();
            let text = String::from_reg_value(&raw).map_err(|_| std::io::Error::other("the PATH value is not text"))?;
            Ok((text, value_type))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((String::new(), RegType::REG_EXPAND_SZ)),
        Err(error) => Err(error),
    }
}

fn write_user_path(environment: &RegKey, value: &str, value_type: RegType) -> std::io::Result<()> {
    let mut raw = value.to_reg_value();
    raw.vtype = value_type;

    environment.set_raw_value("Path", &raw)?;
    broadcast_environment_change();
    Ok(())
}

fn broadcast_environment_change() {
    let name: Vec<u16> = "Environment\0".encode_utf16().collect();

    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, name.as_ptr() as isize, SMTO_ABORTIFHUNG, 1000, std::ptr::null_mut());
    }
}
