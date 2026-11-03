use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

/// One entry appears on the folder icon, the other inside an open folder's empty space.
const KEYS: [&str; 2] = [r"Software\Classes\Directory\shell\WffSearch", r"Software\Classes\Directory\Background\shell\WffSearch"];

/// Pre-rename key names. Left behind, they show a second entry pointing at a missing executable.
const LEGACY_KEYS: [&str; 2] = [r"Software\Classes\Directory\shell\FffSearch", r"Software\Classes\Directory\Background\shell\FffSearch"];

pub fn installed() -> bool {
    RegKey::predef(HKEY_CURRENT_USER).open_subkey(KEYS[0]).is_ok()
}

/// The entry stores the executable path and the menu label, so it goes stale when the program
/// moves or the language changes. It is rewritten only then, or when a legacy entry is found.
pub fn refresh(label: &str) {
    let has_legacy = RegKey::predef(HKEY_CURRENT_USER).open_subkey(LEGACY_KEYS[0]).is_ok();

    if has_legacy || (installed() && !registration_matches(label)) {
        install(label).ok();
    }
}

fn registration_matches(label: &str) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let expected = command_line(&exe.display().to_string());
    let user = RegKey::predef(HKEY_CURRENT_USER);

    KEYS.iter().all(|key| {
        let stored_label: String = user.open_subkey(key).and_then(|menu| menu.get_value("")).unwrap_or_default();
        let stored_command: String = user.open_subkey(format!(r"{key}\command")).and_then(|command| command.get_value("")).unwrap_or_default();

        stored_label == label && stored_command == expected
    })
}

pub fn install(label: &str) -> std::io::Result<()> {
    let exe = std::env::current_exe()?.display().to_string();
    let user = RegKey::predef(HKEY_CURRENT_USER);

    drop_keys(&LEGACY_KEYS)?;

    for key in KEYS {
        let (menu, _) = user.create_subkey(key)?;
        menu.set_value("", &label)?;
        menu.set_value("Icon", &exe)?;

        let (command, _) = user.create_subkey(format!(r"{key}\command"))?;
        command.set_value("", &command_line(&exe))?;
    }

    Ok(())
}

pub fn remove() -> std::io::Result<()> {
    drop_keys(&LEGACY_KEYS)?;
    drop_keys(&KEYS)
}

/// %V holds the folder the user right-clicked.
fn command_line(exe: &str) -> String {
    format!("\"{exe}\" \"%V\"")
}

fn drop_keys(keys: &[&str]) -> std::io::Result<()> {
    let user = RegKey::predef(HKEY_CURRENT_USER);

    for key in keys {
        match user.delete_subkey_all(key) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    Ok(())
}
