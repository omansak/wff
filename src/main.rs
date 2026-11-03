#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod app;
mod backdrop;
mod cli;
mod config;
mod context_menu;
mod i18n;
mod opener;
mod search;
mod window;

use app::App;
use i_slint_backend_winit::WinitWindowAccessor;
use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = i_slint_backend_winit::Backend::builder()
        .with_window_attributes_hook(|attributes| attributes.with_transparent(true))
        .build()?;

    slint::platform::set_platform(Box::new(backend)).map_err(|error| format!("could not install the Slint backend: {error}"))?;

    let ui = AppWindow::new()?;
    let app = App::new(&ui);
    app::wire(&ui, &app);

    ui.set_context_menu_enabled(context_menu::installed());
    ui.set_cli_enabled(cli::installed());
    cli::refresh();
    context_menu::refresh(&app.context_menu_label());

    if let Some(folder) = folder_argument() {
        app.start_indexing(&ui, &resolve_folder(&folder));
    }

    ui.show()?;

    // The winit window exists only once the event loop is running.
    slint::spawn_local({
        let ui_handle = ui.as_weak();

        async move {
            let Some(ui) = ui_handle.upgrade() else { return };
            if ui.window().winit_window().await.is_err() {
                return;
            }

            ui.set_ui_scale(ui.window().scale_factor());

            if let Some(hwnd) = window::handle(&ui) {
                window::apply_icon(hwnd);
            }

            if backdrop::apply(&ui) {
                ui.set_backdrop_ready(true);
            }

            let size = ui.window().size();
            window::center(&ui, size.width as i32, size.height as i32);
        }
    })?;

    slint::run_event_loop()?;
    Ok(())
}

/// An unquoted path with spaces arrives split into several arguments.
fn folder_argument() -> Option<String> {
    let joined = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let folder = joined.trim().to_string();

    if folder.is_empty() { None } else { Some(folder) }
}

fn resolve_folder(input: &str) -> String {
    let Ok(path) = std::fs::canonicalize(input) else { return input.to_string() };
    let text = path.display().to_string();

    // canonicalize adds a `\\?\` prefix on Windows that confuses some programs.
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}
