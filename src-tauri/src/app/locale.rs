//! System UI language. The app speaks Russian on a Russian Windows UI and
//! English everywhere else.

const LANG_RUSSIAN: u16 = 0x19;
const PRIMARY_LANGUAGE_MASK: u16 = 0x3ff;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

pub fn is_russian() -> bool {
    let language = unsafe { GetUserDefaultUILanguage() };
    language & PRIMARY_LANGUAGE_MASK == LANG_RUSSIAN
}

/// Pick the Russian or English variant for the system UI language.
pub fn pick<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if is_russian() { ru } else { en }
}
