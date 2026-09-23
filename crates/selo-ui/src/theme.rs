// Windows acrylic ghosts high-contrast text through the blur, so its tint carries more cover.
#[cfg(windows)]
pub const WINDOW: u32 = 0x1b1d24b3;
#[cfg(target_os = "linux")]
pub const WINDOW: u32 = 0x1b1d24ff;
#[cfg(not(any(windows, target_os = "linux")))]
pub const WINDOW: u32 = 0x1b1d2466;
pub const BORDER: u32 = 0xffffff1f;
pub const ACCENT: u32 = 0xe8e8ea;
pub const ACTIVE: u32 = 0xffffff26;
pub const SELECTION: u32 = 0xffffff40;
pub const HOVER: u32 = 0xffffff14;
