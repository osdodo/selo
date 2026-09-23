use super::clipboard;
use selo_core::{Acquisition, Error, Result, Selection, SelectionProvider};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationTextPattern, UIA_TextPatternId,
};

pub struct PlatformSelection;

impl SelectionProvider for PlatformSelection {
    fn selection(&self) -> Result<Selection> {
        if let Some(text) = uia_selected_text() {
            return Ok(Selection {
                text,
                via: Acquisition::Accessibility,
            });
        }
        match clipboard::copy_selection() {
            Some(text) => Ok(Selection {
                text,
                via: Acquisition::Clipboard,
            }),
            None => Err(Error::NoSelection),
        }
    }
}

fn uia_selected_text() -> Option<String> {
    // SAFETY: every COM interface is a live object owned by this scope; failures short-circuit
    // to the clipboard fallback.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let element = automation.GetFocusedElement().ok()?;
        let pattern: IUIAutomationTextPattern =
            element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let ranges = pattern.GetSelection().ok()?;
        let count = ranges.Length().ok()?;
        let mut text = String::new();
        for index in 0..count {
            let range = ranges.GetElement(index).ok()?;
            text.push_str(&range.GetText(-1).ok()?.to_string());
        }
        (!text.trim().is_empty()).then_some(text)
    }
}
