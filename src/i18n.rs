use serde::Deserialize;
use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    English,
    Turkish,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::English, Language::Turkish];

    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Turkish => "tr",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Turkish => "Türkçe",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|language| language.code() == code)
    }

    // TODO: Change in future but this changes depends portable one-file binary distribution of the app.
    pub fn load(self) -> Strings {
        let json = match self {
            Language::English => include_str!("resources/lang.en.json"),
            Language::Turkish => include_str!("resources/lang.tr.json"),
        };

        serde_json::from_str(json).unwrap_or_else(|error| panic!("bundled {} translation is malformed: {error}", self.code()))
    }
}

pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut text = template.to_string();

    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }

    text
}

pub fn system_default() -> Language {
    // LOCALE_NAME_MAX_LENGTH
    let mut buffer = [0u16; 85];
    let written = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };

    if written == 0 {
        return Language::English;
    }

    // `written` includes the terminating null.
    let locale = String::from_utf16_lossy(&buffer[..written as usize - 1]);
    let code = locale.split('-').next().unwrap_or_default().to_lowercase();

    Language::from_code(&code).unwrap_or(Language::English)
}

#[derive(Clone, Deserialize)]
pub struct Template {
    pub name: String,
    pub command: String,
}

#[derive(Clone, Deserialize)]
pub struct Strings {
    pub search_placeholder: String,
    pub hint_folder_title: String,
    pub hint_folder_note: String,
    pub hint_folder_note_selected: String,
    pub language_section: String,
    pub language_note: String,
    pub open_with_section: String,
    pub open_with_note: String,
    pub rule_add: String,
    pub rule_save: String,
    pub rule_extension_placeholder: String,
    pub rule_command_placeholder: String,
    pub template_pick: String,
    pub windows_section: String,
    pub windows_note: String,
    pub context_menu_option: String,
    pub cli_option: String,
    pub menu_open: String,
    pub menu_reveal: String,
    pub context_menu_label: String,
    pub status_indexing: String,
    pub status_indexed: String,
    pub status_folder_failed: String,
    pub status_min_query: String,
    pub status_pick_folder_first: String,
    pub status_results: String,
    pub status_saving: String,
    pub status_saved: String,
    pub status_save_failed: String,
    pub status_command_ran: String,
    pub status_no_rule: String,
    pub status_rule_empty: String,
    pub status_command_failed: String,
    pub status_context_menu_on: String,
    pub status_context_menu_off: String,
    pub status_cli_on: String,
    pub status_cli_off: String,
    pub status_registry_failed: String,
    pub status_revealed: String,
    pub status_missing: String,
    pub templates: Vec<Template>,
}
