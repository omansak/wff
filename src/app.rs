// std
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

// libs
use fff_search::{SharedFilePicker, SharedFrecency};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

// crate
use crate::config::{self, OpenWithRule, Settings};
use crate::i18n::{self, Language, Strings as Translation};
use crate::opener::{self, Failure};
use crate::{AppWindow, OpenRule, ResultRow, Segment, Strings};
use crate::{cli, context_menu, search, window};

pub struct App {
    picker: SharedFilePicker,
    frecency: SharedFrecency,
    rules: Rc<VecModel<OpenRule>>,
    text: RefCell<Translation>,
    language: Cell<Language>,
}

impl App {
    pub fn new(ui: &AppWindow) -> Rc<Self> {
        let settings = config::load();
        let language = settings.language();

        let rules = Rc::new(VecModel::from(
            settings
                .rules
                .into_iter()
                .map(|rule| OpenRule { extension: rule.extension.into(), command: rule.command.into() })
                .collect::<Vec<_>>(),
        ));

        ui.set_rules(ModelRc::from(rules.clone()));

        let app = Rc::new(Self {
            picker: SharedFilePicker::default(),
            frecency: SharedFrecency::default(),
            rules,
            text: RefCell::new(language.load()),
            language: Cell::new(language),
        });

        app.apply_language(ui);
        app
    }

    fn apply_language(&self, ui: &AppWindow) {
        let text = self.text.borrow();
        let strings = ui.global::<Strings>();

        strings.set_search_placeholder(text.search_placeholder.as_str().into());
        strings.set_hint_folder_title(text.hint_folder_title.as_str().into());
        strings.set_hint_folder_note(text.hint_folder_note.as_str().into());
        strings.set_hint_folder_note_selected(text.hint_folder_note_selected.as_str().into());
        strings.set_language_section(text.language_section.as_str().into());
        strings.set_language_note(text.language_note.as_str().into());
        strings.set_language_current(self.language.get().name().into());
        strings.set_open_with_section(text.open_with_section.as_str().into());
        strings.set_open_with_note(text.open_with_note.as_str().into());
        strings.set_rule_add(text.rule_add.as_str().into());
        strings.set_rule_save(text.rule_save.as_str().into());
        strings.set_rule_extension_placeholder(text.rule_extension_placeholder.as_str().into());
        strings.set_rule_command_placeholder(text.rule_command_placeholder.as_str().into());
        strings.set_template_pick(text.template_pick.as_str().into());
        strings.set_windows_section(text.windows_section.as_str().into());
        strings.set_windows_note(text.windows_note.as_str().into());
        strings.set_context_menu_option(text.context_menu_option.as_str().into());
        strings.set_cli_option(text.cli_option.as_str().into());
        strings.set_menu_open(text.menu_open.as_str().into());
        strings.set_menu_reveal(text.menu_reveal.as_str().into());

        let templates: Vec<SharedString> = text.templates.iter().map(|template| template.name.as_str().into()).collect();
        ui.set_templates(ModelRc::from(Rc::new(VecModel::from(templates))));

        let languages: Vec<SharedString> = Language::ALL.iter().map(|language| language.name().into()).collect();
        ui.set_languages(ModelRc::from(Rc::new(VecModel::from(languages))));
    }

    pub fn context_menu_label(&self) -> String {
        self.text.borrow().context_menu_label.clone()
    }

    fn stored_rules(&self) -> Vec<OpenWithRule> {
        self.rules
            .iter()
            .map(|rule| OpenWithRule { extension: rule.extension.to_string(), command: rule.command.to_string() })
            .collect()
    }

    fn persist(&self) -> std::io::Result<()> {
        config::save(&Settings {
            language: self.language.get().code().to_string(),
            rules: self.stored_rules(),
        })
    }

    fn report(&self, ui: &AppWindow, outcome: std::io::Result<()>) {
        let text = self.text.borrow();

        match outcome {
            Ok(()) => ui.set_status(text.status_saved.as_str().into()),
            Err(error) => ui.set_status(i18n::fill(&text.status_save_failed, &[("error", &error.to_string())]).into()),
        }
    }

    pub fn start_indexing(&self, ui: &AppWindow, folder: &str) {
        let text = self.text.borrow();

        ui.set_results(ModelRc::default());
        ui.set_folder(folder.into());
        ui.set_status(text.status_indexing.as_str().into());

        if let Err(error) = search::start_indexing(&self.picker, &self.frecency, folder) {
            ui.set_status(i18n::fill(&text.status_folder_failed, &[("error", &error.to_string())]).into());
            return;
        }

        let picker = self.picker.clone();
        let ui_handle = ui.as_weak();
        let message = text.status_indexed.clone();

        std::thread::spawn(move || {
            let count = search::wait_for_scan(&picker);

            ui_handle
                .upgrade_in_event_loop(move |ui| ui.set_status(i18n::fill(&message, &[("count", &count.to_string())]).into()))
                .ok();
        });
    }
}

pub fn wire(ui: &AppWindow, app: &Rc<App>) {
    wire_folder(ui, app);
    wire_search(ui, app);
    wire_results(ui, app);
    wire_rules(ui, app);
    wire_language(ui, app);
    wire_integrations(ui, app);
    wire_window(ui);
}

fn wire_folder(ui: &AppWindow, app: &Rc<App>) {
    let app = app.clone();
    let handle = ui.as_weak();

    ui.on_pick_folder(move || {
        let Some(folder) = rfd::FileDialog::new().pick_folder() else { return };
        let Some(ui) = handle.upgrade() else { return };

        app.start_indexing(&ui, &folder.display().to_string());
    });
}

fn wire_search(ui: &AppWindow, app: &Rc<App>) {
    let app = app.clone();
    let handle = ui.as_weak();

    // `running` cancels the search in flight; `latest` lets a late result be discarded.
    let mut running: Option<Arc<AtomicBool>> = None;
    let latest = Arc::new(AtomicU64::new(0));

    ui.on_search(move |typed| {
        if let Some(previous) = running.take() {
            previous.store(true, Ordering::Relaxed);
        }

        let Some(ui) = handle.upgrade() else { return };
        let ticket = latest.fetch_add(1, Ordering::Relaxed) + 1;
        let query = typed.trim().to_string();
        let text = app.text.borrow();

        if query.chars().count() < search::MIN_QUERY_LEN {
            ui.set_results(ModelRc::default());
            ui.set_status(i18n::fill(&text.status_min_query, &[("count", &search::MIN_QUERY_LEN.to_string())]).into());
            return;
        }

        let cancelled = Arc::new(AtomicBool::new(false));
        running = Some(cancelled.clone());

        let picker = app.picker.clone();
        let ui_handle = ui.as_weak();
        let latest = latest.clone();
        let missing_folder = text.status_pick_folder_first.clone();
        let summary = text.status_results.clone();

        std::thread::spawn(move || {
            let started = Instant::now();

            let Ok(guard) = picker.read() else { return };
            let Some(picker) = guard.as_ref() else {
                ui_handle.upgrade_in_event_loop(move |ui| ui.set_status(missing_folder.into())).ok();
                return;
            };

            let outcome = search::run(picker, &query, &cancelled);
            let elapsed = started.elapsed();

            if cancelled.load(Ordering::Relaxed) {
                return;
            }

            ui_handle
                .upgrade_in_event_loop(move |ui| {
                    // A newer search has started since; drop this result.
                    if latest.load(Ordering::Relaxed) != ticket {
                        return;
                    }

                    let rows: Vec<ResultRow> = outcome.rows.into_iter().map(result_row).collect();

                    ui.set_results(ModelRc::from(Rc::new(VecModel::from(rows))));
                    ui.set_status(
                        i18n::fill(
                            &summary,
                            &[
                                ("names", &outcome.name_hits.to_string()),
                                ("contents", &outcome.content_hits.to_string()),
                                ("elapsed", &format!("{elapsed:?}")),
                            ],
                        )
                        .into(),
                    );
                })
                .ok();
        });
    });
}

fn wire_results(ui: &AppWindow, app: &Rc<App>) {
    ui.on_open_result({
        let app = app.clone();
        let handle = ui.as_weak();

        move |path: SharedString, line: i32| {
            let Some(ui) = handle.upgrade() else { return };
            let text = app.text.borrow();

            match opener::open(&app.stored_rules(), &path, line) {
                Ok(command) => ui.set_status(i18n::fill(&text.status_command_ran, &[("command", &command)]).into()),
                Err(Failure::NoRule(extension)) => ui.set_status(i18n::fill(&text.status_no_rule, &[("extension", &extension)]).into()),
                Err(Failure::EmptyCommand) => ui.set_status(text.status_rule_empty.as_str().into()),
                Err(Failure::Spawn(error)) => ui.set_status(i18n::fill(&text.status_command_failed, &[("error", &error)]).into()),
                Err(Failure::Missing(missing)) => ui.set_status(i18n::fill(&text.status_missing, &[("path", &missing)]).into()),
            }
        }
    });

    ui.on_reveal_result({
        let app = app.clone();
        let handle = ui.as_weak();

        move |path: SharedString| {
            let Some(ui) = handle.upgrade() else { return };
            let text = app.text.borrow();

            match opener::reveal(&path) {
                Ok(()) => ui.set_status(text.status_revealed.as_str().into()),
                Err(Failure::Missing(missing)) => ui.set_status(i18n::fill(&text.status_missing, &[("path", &missing)]).into()),
                Err(Failure::Spawn(error)) => ui.set_status(i18n::fill(&text.status_command_failed, &[("error", &error)]).into()),
                Err(Failure::NoRule(_) | Failure::EmptyCommand) => {}
            }
        }
    });
}

fn wire_rules(ui: &AppWindow, app: &Rc<App>) {
    ui.on_add_rule({
        let app = app.clone();
        move || app.rules.push(OpenRule { extension: SharedString::new(), command: SharedString::new() })
    });

    ui.on_remove_rule({
        let app = app.clone();
        move |index| {
            let index = index as usize;
            if index < app.rules.row_count() {
                app.rules.remove(index);
            }
        }
    });

    ui.on_edit_extension({
        let app = app.clone();
        move |index, value| update_rule(&app.rules, index, |rule| rule.extension = value)
    });

    ui.on_edit_command({
        let app = app.clone();
        move |index, value| update_rule(&app.rules, index, |rule| rule.command = value)
    });

    ui.on_pick_template({
        let app = app.clone();
        move |index, choice| {
            let command = {
                let text = app.text.borrow();
                let Some(template) = text.templates.get(choice as usize) else { return };
                if template.command.is_empty() {
                    return;
                }
                template.command.clone()
            };

            update_rule(&app.rules, index, |rule| rule.command = command.into());
        }
    });

    ui.on_save_rules({
        let app = app.clone();
        let handle = ui.as_weak();

        move || {
            let Some(ui) = handle.upgrade() else { return };
            let outcome = app.persist();
            app.report(&ui, outcome);
        }
    });
}

fn wire_language(ui: &AppWindow, app: &Rc<App>) {
    let app = app.clone();
    let handle = ui.as_weak();

    ui.on_pick_language(move |choice| {
        let Some(language) = Language::ALL.get(choice as usize).copied() else { return };
        let Some(ui) = handle.upgrade() else { return };

        if language == app.language.get() {
            return;
        }

        app.language.set(language);
        *app.text.borrow_mut() = language.load();
        app.apply_language(&ui);

        let outcome = app.persist();
        app.report(&ui, outcome);
    });
}

/// Registry work runs on a worker thread so the window does not freeze meanwhile.
fn wire_integrations(ui: &AppWindow, app: &Rc<App>) {
    ui.on_toggle_context_menu({
        let app = app.clone();
        let handle = ui.as_weak();

        move |enabled| {
            let Some(ui) = handle.upgrade() else { return };
            let text = app.text.borrow();
            let label = text.context_menu_label.clone();
            let on = text.status_context_menu_on.clone();
            let off = text.status_context_menu_off.clone();
            let failed = text.status_registry_failed.clone();

            ui.set_busy(true);
            ui.set_status(text.status_saving.as_str().into());

            let ui_handle = ui.as_weak();

            std::thread::spawn(move || {
                let outcome = if enabled { context_menu::install(&label) } else { context_menu::remove() };

                ui_handle
                    .upgrade_in_event_loop(move |ui| {
                        ui.set_busy(false);
                        ui.set_context_menu_enabled(context_menu::installed());
                        ui.set_status(integration_message(enabled, outcome, &on, &off, &failed).into());
                    })
                    .ok();
            });
        }
    });

    ui.on_toggle_cli({
        let app = app.clone();
        let handle = ui.as_weak();

        move |enabled| {
            let Some(ui) = handle.upgrade() else { return };
            let text = app.text.borrow();
            let on = text.status_cli_on.clone();
            let off = text.status_cli_off.clone();
            let failed = text.status_registry_failed.clone();

            ui.set_busy(true);
            ui.set_status(text.status_saving.as_str().into());

            let ui_handle = ui.as_weak();

            std::thread::spawn(move || {
                let outcome = if enabled { cli::install() } else { cli::remove() };

                ui_handle
                    .upgrade_in_event_loop(move |ui| {
                        ui.set_busy(false);
                        ui.set_cli_enabled(cli::installed());
                        ui.set_status(integration_message(enabled, outcome, &on, &off, &failed).into());
                    })
                    .ok();
            });
        }
    });
}

fn integration_message(enabled: bool, outcome: std::io::Result<()>, on: &str, off: &str, failed: &str) -> String {
    match outcome {
        Ok(()) if enabled => on.to_string(),
        Ok(()) => off.to_string(),
        Err(error) => i18n::fill(failed, &[("error", &error.to_string())]),
    }
}

fn wire_window(ui: &AppWindow) {
    ui.on_dismiss(|| {
        slint::quit_event_loop().ok();
    });

    ui.on_recenter({
        let handle = ui.as_weak();

        move |width, height| {
            let Some(ui) = handle.upgrade() else { return };

            // Fires before the window has actually resized, hence the explicit size.
            let scale = ui.window().scale_factor();
            window::center(&ui, (width * scale) as i32, (height * scale) as i32);
        }
    });
}

fn result_row(row: search::Row) -> ResultRow {
    ResultRow {
        header: segments(&row.header, &row.header_spans),
        body: segments(&row.body, &row.body_spans),
        has_body: row.has_body,
        path: row.path.into(),
        line: row.line,
    }
}

fn segments(text: &str, spans: &[(u32, u32)]) -> ModelRc<Segment> {
    let mut parts: Vec<Segment> = Vec::new();
    let mut cursor = 0usize;

    for &(start, end) in spans {
        let (start, end) = (start as usize, end as usize);

        // Skip malformed or overlapping spans.
        let usable = start >= cursor && start < end && end <= text.len() && text.is_char_boundary(start) && text.is_char_boundary(end);
        if !usable {
            continue;
        }

        if start > cursor {
            parts.push(Segment { text: text[cursor..start].into(), hit: false });
        }

        parts.push(Segment { text: text[start..end].into(), hit: true });
        cursor = end;
    }

    if cursor < text.len() {
        parts.push(Segment { text: text[cursor..].into(), hit: false });
    }

    ModelRc::from(Rc::new(VecModel::from(parts)))
}

fn update_rule(rules: &VecModel<OpenRule>, index: i32, change: impl FnOnce(&mut OpenRule)) {
    let Some(mut rule) = rules.row_data(index as usize) else { return };

    change(&mut rule);
    rules.set_row_data(index as usize, rule);
}
