use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::config::OpenWithRule;

const EXECUTABLE: [&str; 2] = ["exe", "com"];
const SCRIPT: [&str; 2] = ["cmd", "bat"];

pub enum Failure {
    NoRule(String),
    EmptyCommand,
    Spawn(String),
    Missing(String),
}

enum Program {
    /// Started directly.
    Executable(PathBuf),
    /// A .cmd or .bat file; std runs it through cmd.exe.
    Script(PathBuf),
    /// A document, an App Paths name or an unknown name: the shell decides.
    Shell(String),
}

pub fn open(rules: &[OpenWithRule], path: &str, line: i32) -> Result<String, Failure> {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();

    let rule = rules
        .iter()
        .find(|rule| !extension.is_empty() && normalize(&rule.extension) == extension)
        .or_else(|| rules.iter().find(|rule| rule.extension.trim() == "*"))
        .ok_or_else(|| Failure::NoRule(extension.clone()))?;

    let command = rule.command.trim().to_string();
    if command.is_empty() {
        return Err(Failure::EmptyCommand);
    }

    let mut parts: Vec<String> = tokenize(&command)
        .into_iter()
        .map(|token| {
            token
                .replace("{path}", path)
                .replace("{line}", &line.max(1).to_string())
                .replace("{col}", "1")
        })
        .collect();

    // `start "" <file>` from older configs; the shell branch below does the same job.
    if parts.first().is_some_and(|first| first.eq_ignore_ascii_case("start")) {
        parts.remove(0);
        if parts.first().is_some_and(String::is_empty) {
            parts.remove(0);
        }
    }

    let Some((program, arguments)) = parts.split_first() else {
        return Err(Failure::EmptyCommand);
    };

    let outcome = match resolve(program) {
        Program::Executable(exe) => std::process::Command::new(exe).args(arguments).spawn().map(drop),
        // CREATE_NO_WINDOW keeps cmd.exe from flashing a console.
        Program::Script(script) => std::process::Command::new(script).args(arguments).creation_flags(CREATE_NO_WINDOW).spawn().map(drop),
        Program::Shell(file) => shell_open(&file, arguments),
    };

    outcome.map(|()| command).map_err(|error| Failure::Spawn(error.to_string()))
}

/// Opens Explorer with the file selected. `/select,<path>` has to reach Explorer as one
/// argument, hence `raw_arg`.
pub fn reveal(path: &str) -> Result<(), Failure> {
    // Also normalises separators: Explorer ignores paths written with forward slashes.
    let full = std::fs::canonicalize(path).map_err(|_| Failure::Missing(path.to_string()))?;
    let text = full.display().to_string();
    let target = text.strip_prefix(r"\\?\").unwrap_or(&text);

    std::process::Command::new("explorer")
        .raw_arg(format!("/select,\"{target}\""))
        .spawn()
        .map(drop)
        .map_err(|error| Failure::Spawn(error.to_string()))
}

fn normalize(extension: &str) -> String {
    extension.trim().trim_start_matches('.').to_lowercase()
}

fn resolve(program: &str) -> Program {
    let Some(found) = locate(program) else { return Program::Shell(program.to_string()) };
    let extension = found.extension().and_then(|value| value.to_str()).unwrap_or_default().to_lowercase();

    if EXECUTABLE.contains(&extension.as_str()) {
        Program::Executable(found)
    } else if SCRIPT.contains(&extension.as_str()) {
        Program::Script(found)
    } else {
        Program::Shell(program.to_string())
    }
}

/// Finds the file behind a program name the way the command line does: a name without an
/// extension gets the executable extensions appended, and each PATH directory is tried in turn.
fn locate(program: &str) -> Option<PathBuf> {
    let candidates: Vec<String> = if Path::new(program).extension().is_some() {
        vec![program.to_string()]
    } else {
        EXECUTABLE.iter().chain(&SCRIPT).map(|extension| format!("{program}.{extension}")).collect()
    };

    if program.contains(['\\', '/']) {
        return candidates.iter().map(PathBuf::from).find(|path| path.is_file());
    }

    let path = std::env::var_os("PATH")?;

    std::env::split_paths(&path).find_map(|dir| candidates.iter().map(|name| dir.join(name)).find(|full| full.is_file()))
}

fn shell_open(file: &str, arguments: &[String]) -> std::io::Result<()> {
    let file = wide(file);
    let parameters = wide(&quote(arguments));

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            file.as_ptr(),
            if arguments.is_empty() { std::ptr::null() } else { parameters.as_ptr() },
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    // Values up to 32 are error codes.
    if result as isize > 32 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn quote(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| {
            if !argument.is_empty() && !argument.contains([' ', '\t', '"']) {
                argument.clone()
            } else {
                format!("\"{}\"", argument.replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut inside_quotes = false;
    let mut has_token = false;

    for character in command.chars() {
        match character {
            '"' => {
                inside_quotes = !inside_quotes;
                has_token = true;
            }
            character if character.is_whitespace() && !inside_quotes => {
                if has_token {
                    tokens.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            character => {
                current.push(character);
                has_token = true;
            }
        }
    }

    if has_token {
        tokens.push(current);
    }

    tokens
}
