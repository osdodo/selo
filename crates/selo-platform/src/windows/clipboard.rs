use std::mem::size_of;
use std::thread::sleep;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::Console::{GetConsoleWindow, SetConsoleCtrlHandler};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_C, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT,
    VK_LWIN, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

const COPY_DEADLINE: Duration = Duration::from_millis(600);
const MODIFIER_RELEASE: Duration = Duration::from_millis(500);
const FORMAT: u32 = CF_UNICODETEXT.0 as u32;

// Both sides listed: the generic VK_MENU/VK_CONTROL don't tell GetAsyncKeyState which key is down.
const HELD_MODIFIERS: [VIRTUAL_KEY; 8] = [
    VK_LMENU,
    VK_RMENU,
    VK_LCONTROL,
    VK_RCONTROL,
    VK_LSHIFT,
    VK_RSHIFT,
    VK_LWIN,
    VK_RWIN,
];

pub fn text() -> Option<String> {
    // SAFETY: the clipboard is opened and closed around every access; the handle is only
    // dereferenced while owned by this process.
    unsafe {
        OpenClipboard(None).ok()?;
        let text = GetClipboardData(FORMAT)
            .ok()
            .and_then(|handle| hglobal_string(handle));
        let _ = CloseClipboard();
        text.filter(|value| !value.is_empty())
    }
}

pub fn set_text(text: &str) -> bool {
    // SAFETY: opened and closed around the write; `SetClipboardData` takes ownership of the
    // moved memory on success.
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let stored = alloc_utf16(text)
            .is_some_and(|memory| SetClipboardData(FORMAT, Some(HANDLE(memory.0))).is_ok());
        let _ = CloseClipboard();
        stored
    }
}

// ponytail: only CF_UNICODETEXT is snapshotted, so an image/rich-text clipboard is lost on the
// fallback path.
pub fn copy_selection() -> Option<String> {
    if !foreground_is_foreign() {
        return None;
    }
    let before_sequence = unsafe { GetClipboardSequenceNumber() };
    let before_text = text();

    let _ignore_ctrl_c = IgnoreCtrlC::new();
    wait_for_modifiers_up();
    press_copy();

    let deadline = Instant::now() + COPY_DEADLINE;
    while Instant::now() < deadline && unsafe { GetClipboardSequenceNumber() } == before_sequence {
        sleep(Duration::from_millis(20));
    }
    let copied = (unsafe { GetClipboardSequenceNumber() } != before_sequence)
        .then(text)
        .flatten();

    restore(before_text);
    copied
}

// A synthesized Ctrl+C on our own windows would paste or kill the process, so require a foreign
// foreground.
fn foreground_is_foreign() -> bool {
    // SAFETY: side-effect free window/process queries.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || hwnd == GetConsoleWindow() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid != GetCurrentProcessId()
    }
}

// Disables our own Ctrl+C handling while the synthesized copy is in flight; restored on drop.
struct IgnoreCtrlC;

impl IgnoreCtrlC {
    fn new() -> Self {
        // SAFETY: a NULL handler with `add = true` just sets the ignore flag.
        unsafe {
            let _ = SetConsoleCtrlHandler(None, true);
        }
        Self
    }
}

impl Drop for IgnoreCtrlC {
    fn drop(&mut self) {
        // SAFETY: a NULL handler with `add = false` restores normal handling.
        unsafe {
            let _ = SetConsoleCtrlHandler(None, false);
        }
    }
}

fn restore(value: Option<String>) {
    // SAFETY: opened and closed around the write; `SetClipboardData` takes ownership of the
    // moved memory on success.
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        let _ = EmptyClipboard();
        if let Some(value) = value
            && let Some(memory) = alloc_utf16(&value)
        {
            let _ = SetClipboardData(FORMAT, Some(HANDLE(memory.0)));
        }
        let _ = CloseClipboard();
    }
}

unsafe fn hglobal_string(handle: HANDLE) -> Option<String> {
    // SAFETY: the caller holds the clipboard open, so the handle is a valid global memory block
    // holding a NUL-terminated UTF-16 string.
    unsafe {
        let pointer = GlobalLock(HGLOBAL(handle.0)).cast::<u16>();
        if pointer.is_null() {
            return None;
        }
        let mut length = 0usize;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        let slice = std::slice::from_raw_parts(pointer, length);
        let text = String::from_utf16_lossy(slice);
        let _ = GlobalUnlock(HGLOBAL(handle.0));
        Some(text)
    }
}

fn alloc_utf16(value: &str) -> Option<HGLOBAL> {
    let units: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `GMEM_MOVEABLE` block large enough for the string; the pointer is written only
    // within those bounds and unlocked before returning.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, units.len() * size_of::<u16>()).ok()?;
        let pointer = GlobalLock(memory).cast::<u16>();
        if pointer.is_null() {
            return None;
        }
        std::ptr::copy_nonoverlapping(units.as_ptr(), pointer, units.len());
        let _ = GlobalUnlock(memory);
        Some(memory)
    }
}

// The hotkey chord is usually still held, which would turn Ctrl+C into Ctrl+Alt+C; wait briefly.
fn wait_for_modifiers_up() {
    let deadline = Instant::now() + MODIFIER_RELEASE;
    while Instant::now() < deadline {
        let held = HELD_MODIFIERS
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(key.0 as i32) } < 0);
        if !held {
            return;
        }
        sleep(Duration::from_millis(10));
    }
}

fn press_copy() {
    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VK_C, false),
        key_input(VK_C, true),
        key_input(VK_CONTROL, true),
    ];
    // SAFETY: `inputs` is a valid slice and its element size matches the declared struct size.
    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}

fn key_input(key: VIRTUAL_KEY, release: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if release {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
