use i_slint_backend_winit::WinitWindowAccessor;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::ComponentHandle;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, SM_CXICON, SM_CXSCREEN, SM_CXSMICON, SM_CYICON, SM_CYSCREEN,
    SM_CYSMICON, SendMessageW, WM_SETICON,
};

use crate::AppWindow;

/// Slint's `window_handle()` returns nothing with a hand-built backend, so the HWND comes from winit.
pub fn handle(ui: &AppWindow) -> Option<HWND> {
    ui.window()
        .with_winit_window(|winit_window| {
            let handle = winit_window.window_handle().ok()?;

            match handle.as_raw() {
                RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as HWND),
                _ => None,
            }
        })
        .flatten()
}

/// A frameless window has no title bar to drag, so it is re-centred whenever it resizes.
pub fn center(ui: &AppWindow, physical_width: i32, physical_height: i32) {
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if screen_width <= 0 || screen_height <= 0 {
        return;
    }

    let x = (screen_width - physical_width) / 2;
    let y = (screen_height - physical_height) / 2;

    ui.window().set_position(slint::PhysicalPosition::new(x.max(0), y.max(0)));
}

/// winit's window class has no icon, so the one embedded via app.rc is set by hand;
/// otherwise the taskbar and Alt-Tab show a generic placeholder.
pub fn apply_icon(hwnd: HWND) {
    // MAKEINTRESOURCEW(1): the icon id in app.rc.
    let resource = std::ptr::without_provenance::<u16>(1);

    unsafe {
        let module = GetModuleHandleW(std::ptr::null());

        for (slot, width, height) in [(ICON_BIG, SM_CXICON, SM_CYICON), (ICON_SMALL, SM_CXSMICON, SM_CYSMICON)] {
            let icon = LoadImageW(module, resource, IMAGE_ICON, GetSystemMetrics(width), GetSystemMetrics(height), LR_DEFAULTCOLOR);

            if !icon.is_null() {
                SendMessageW(hwnd, WM_SETICON, slot as usize, icon as isize);
            }
        }
    }
}
