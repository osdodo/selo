pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("nothing is selected")]
    NoSelection,
    #[error("platform: {0}")]
    Platform(String),
    #[error("translate: {0}")]
    Translate(String),
    #[error("ocr: {0}")]
    Ocr(String),
}

/// A screen rectangle in points from [`Region::display`]'s top-left, y down.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Platform display id; `0` means unspecified, treated as the primary display.
    pub display: u64,
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

/// `rect` is physical pixels of the source image, origin top-left.
#[derive(Debug, Clone)]
pub struct TextBlock {
    pub rect: Rect,
    pub text: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub text: String,
    pub via: Acquisition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acquisition {
    Accessibility,
    Clipboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn detect(text: &str) -> Self {
        if text.chars().any(is_cjk) {
            Self::Zh
        } else {
            Self::En
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::En => "en",
        }
    }

    pub fn other(self) -> Self {
        match self {
            Self::Zh => Self::En,
            Self::En => Self::Zh,
        }
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub text: String,
    pub from: Lang,
    pub to: String,
}

impl TranslateRequest {
    pub fn auto(text: impl Into<String>) -> Self {
        Self::with_target(text, None)
    }

    pub fn with_target(text: impl Into<String>, target: Option<String>) -> Self {
        let text = text.into();
        let from = Lang::detect(&text);
        let to = target.unwrap_or_else(|| from.other().code().to_string());
        Self { text, from, to }
    }
}

pub trait SelectionProvider {
    fn selection(&self) -> Result<Selection>;
}

pub trait OcrProvider {
    fn recognize(&self, image: &Image) -> Result<Vec<TextBlock>>;
}

pub trait TranslateProvider {
    fn translate(&self, request: &TranslateRequest) -> Result<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_direction() {
        assert_eq!(Lang::detect("hello world"), Lang::En);
        assert_eq!(Lang::detect("译文原位覆盖原文"), Lang::Zh);
        assert_eq!(Lang::detect("GPUI 是 Zed 的 UI 框架"), Lang::Zh);
        assert_eq!(Lang::detect("use gpui::App;"), Lang::En);
        assert_eq!(Lang::detect(""), Lang::En);
    }

    #[test]
    fn auto_request_flips_direction() {
        let request = TranslateRequest::auto("hello");
        assert_eq!(request.from, Lang::En);
        assert_eq!(request.to, "zh");
    }

    #[test]
    fn target_code_overrides_the_flip() {
        let request = TranslateRequest::with_target("hello", Some("ja".into()));
        assert_eq!(request.from, Lang::En);
        assert_eq!(request.to, "ja");
    }
}
