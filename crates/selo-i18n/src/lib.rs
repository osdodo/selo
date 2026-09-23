use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Locale {
    Zh,
    ZhHant,
    #[default]
    En,
    Ja,
    Ko,
    Fr,
    De,
    Es,
    Ru,
    Pt,
    It,
}

impl Locale {
    pub const ALL: [Locale; 11] = [
        Locale::Zh,
        Locale::ZhHant,
        Locale::En,
        Locale::Ja,
        Locale::Ko,
        Locale::Fr,
        Locale::De,
        Locale::Es,
        Locale::Ru,
        Locale::Pt,
        Locale::It,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Zh => "zh",
            Self::ZhHant => "zh-Hant",
            Self::En => "en",
            Self::Ja => "ja",
            Self::Ko => "ko",
            Self::Fr => "fr",
            Self::De => "de",
            Self::Es => "es",
            Self::Ru => "ru",
            Self::Pt => "pt",
            Self::It => "it",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Zh => "中文（简体）",
            Self::ZhHant => "中文（繁體）",
            Self::En => "English",
            Self::Ja => "日本語",
            Self::Ko => "한국어",
            Self::Fr => "Français",
            Self::De => "Deutsch",
            Self::Es => "Español",
            Self::Ru => "Русский",
            Self::Pt => "Português",
            Self::It => "Italiano",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "zh" => Self::Zh,
            "zh-Hant" => Self::ZhHant,
            "en" => Self::En,
            "ja" => Self::Ja,
            "ko" => Self::Ko,
            "fr" => Self::Fr,
            "de" => Self::De,
            "es" => Self::Es,
            "ru" => Self::Ru,
            "pt" => Self::Pt,
            "it" => Self::It,
            _ => Self::default(),
        }
    }

    /// Maps a system locale tag (`zh-Hans-CN`, `en-US`, `zh_TW`, …) to a locale.
    pub fn from_tag(tag: &str) -> Self {
        let tag = tag.split(['.', '@']).next().unwrap_or_default();
        let mut parts = tag.split(['-', '_']);
        let language = parts.next().unwrap_or_default().to_ascii_lowercase();
        let rest: Vec<String> = parts.map(str::to_ascii_lowercase).collect();
        let has = |needle: &str| rest.iter().any(|part| part == needle);
        match language.as_str() {
            "zh" if has("hant") || has("tw") || has("hk") || has("mo") => Self::ZhHant,
            "zh" => Self::Zh,
            "ja" => Self::Ja,
            "ko" => Self::Ko,
            "fr" => Self::Fr,
            "de" => Self::De,
            "es" => Self::Es,
            "ru" => Self::Ru,
            "pt" => Self::Pt,
            "it" => Self::It,
            _ => Self::default(),
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Locale::En as u8);

pub fn current() -> Locale {
    Locale::ALL
        .get(CURRENT.load(Ordering::Relaxed) as usize)
        .copied()
        .unwrap_or_default()
}

pub fn set(locale: Locale) {
    CURRENT.store(locale as u8, Ordering::Relaxed);
}

#[derive(Clone, Copy)]
pub enum Key {
    SettingsTitle,
    General,
    Translation,
    Capture,
    Services,
    AddBuiltinService,
    AddExternalPlugin,
    Back,
    Edit,
    Delete,
    ClickToRecord,
    ClickToFill,
    On,
    Off,
    Translating,
    Copy,
    Copied,
    Compare,
    Pin,
    Pinned,
    SystemDefault,
    TranslationEngine,
    OcrEngine,
    TargetLanguage,
    UiLanguage,
    Auto,
    LaunchAtLogin,
    TranslateHotkey,
    CaptureHotkey,
    PickPluginFile,
    TraySettings,
    TrayQuit,
    HotkeyTranslateDesc,
    HotkeyCaptureDesc,
    PluginFileFilter,
    AllFilesFilter,
}

mod de;
mod en;
mod es;
mod fr;
mod it;
mod ja;
mod ko;
mod pt;
mod ru;
mod zh;
mod zh_hant;

pub fn t(key: Key) -> &'static str {
    translate(current(), key)
}

pub fn translate(locale: Locale, key: Key) -> &'static str {
    let table: fn(Key) -> &'static str = match locale {
        Locale::Zh => zh::tr,
        Locale::ZhHant => zh_hant::tr,
        Locale::En => en::tr,
        Locale::Ja => ja::tr,
        Locale::Ko => ko::tr,
        Locale::Fr => fr::tr,
        Locale::De => de::tr,
        Locale::Es => es::tr,
        Locale::Ru => ru::tr,
        Locale::Pt => pt::tr,
        Locale::It => it::tr,
    };
    table(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_translates_its_own_way() {
        assert_eq!(translate(Locale::Zh, Key::Copy), "复制");
        assert_eq!(translate(Locale::ZhHant, Key::Copy), "複製");
        assert_eq!(translate(Locale::En, Key::Copy), "Copy");
        assert_eq!(translate(Locale::Ja, Key::Copy), "コピー");
        assert_eq!(translate(Locale::Ko, Key::Copy), "복사");
        assert_eq!(translate(Locale::Fr, Key::Copy), "Copier");
        assert_eq!(translate(Locale::De, Key::Copy), "Kopieren");
        assert_eq!(translate(Locale::Es, Key::Copy), "Copiar");
        assert_eq!(translate(Locale::Ru, Key::Copy), "Копировать");
        assert_eq!(translate(Locale::Pt, Key::Copy), "Copiar");
        assert_eq!(translate(Locale::It, Key::Copy), "Copia");
    }

    #[test]
    fn codes_round_trip_and_unknown_falls_back_to_english() {
        for locale in Locale::ALL {
            assert_eq!(Locale::from_code(locale.code()), locale);
        }
        assert_eq!(Locale::from_code("sv"), Locale::En);
    }

    #[test]
    fn system_tags_map_to_the_right_locale() {
        assert_eq!(Locale::from_tag("en-US"), Locale::En);
        assert_eq!(Locale::from_tag("zh-Hans-CN"), Locale::Zh);
        assert_eq!(Locale::from_tag("zh-Hant-TW"), Locale::ZhHant);
        assert_eq!(Locale::from_tag("zh_TW.UTF-8"), Locale::ZhHant);
        assert_eq!(Locale::from_tag("ja-JP"), Locale::Ja);
        assert_eq!(Locale::from_tag("ko-KR"), Locale::Ko);
        assert_eq!(Locale::from_tag("pt-BR"), Locale::Pt);
        assert_eq!(Locale::from_tag("C"), Locale::En);
    }

    #[test]
    fn all_is_discriminant_ordered() {
        for (index, locale) in Locale::ALL.into_iter().enumerate() {
            assert_eq!(locale as usize, index);
        }
    }
}
