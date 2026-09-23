use selo_core::Result;
use selo_i18n::{Key, t};
use std::path::PathBuf;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::core::{PCWSTR, PWSTR};

const BUFFER: usize = 1024;

pub fn pick_file(title: &str) -> Result<Option<PathBuf>> {
    let filter = encode(&format!(
        "{}\0*.js\0{}\0*.*\0\0",
        t(Key::PluginFileFilter),
        t(Key::AllFilesFilter)
    ));
    let title = encode(&format!("{title}\0"));
    let mut file = [0u16; BUFFER];

    let mut dialog = OPENFILENAMEW {
        lStructSize: core::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file.as_mut_ptr()),
        nMaxFile: file.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };

    // SAFETY: all pointers handed to the dialog point into locals that outlive the call.
    if !unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        return Ok(None);
    }
    let length = file.iter().position(|&unit| unit == 0).unwrap_or(0);
    Ok(Some(PathBuf::from(String::from_utf16_lossy(
        &file[..length],
    ))))
}

fn encode(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
