use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

use crate::AppWindow;
use crate::window;

/// Undocumented user32 API. The documented `DWMWA_SYSTEMBACKDROP_TYPE` ignores the alpha
/// channel of what Slint draws; this one honours it.
type SetWindowCompositionAttribute = unsafe extern "system" fn(HWND, *mut CompositionAttributeData) -> i32;

const WCA_ACCENT_POLICY: u32 = 19;
const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

/// ABGR order: the tint laid over the blur.
const TINT: u32 = 0xAA4D2F1B;

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    gradient_color: u32,
    animation_id: u32,
}

#[repr(C)]
struct CompositionAttributeData {
    attribute: u32,
    value: *mut std::ffi::c_void,
    size: usize,
}

/// Returns false when the blur could not be applied.
pub fn apply(ui: &AppWindow) -> bool {
    let Some(hwnd) = window::handle(ui) else { return false };

    unsafe {
        set_attribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
        set_attribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, 1);

        let Some(set_composition) = composition_entry_point() else { return false };

        let mut accent = AccentPolicy {
            state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
            flags: 2,
            gradient_color: TINT,
            animation_id: 0,
        };

        let mut data = CompositionAttributeData {
            attribute: WCA_ACCENT_POLICY,
            value: &raw mut accent as *mut std::ffi::c_void,
            size: size_of::<AccentPolicy>(),
        };

        set_composition(hwnd, &raw mut data) != 0
    }
}

unsafe fn composition_entry_point() -> Option<SetWindowCompositionAttribute> {
    unsafe {
        let module = GetModuleHandleA(c"user32.dll".as_ptr() as *const u8);
        if module.is_null() {
            return None;
        }

        let address = GetProcAddress(module, c"SetWindowCompositionAttribute".as_ptr() as *const u8)?;
        Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, SetWindowCompositionAttribute>(address))
    }
}

unsafe fn set_attribute(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, value: i32) {
    unsafe {
        DwmSetWindowAttribute(hwnd, attribute as u32, &raw const value as *const std::ffi::c_void, size_of::<i32>() as u32);
    }
}
