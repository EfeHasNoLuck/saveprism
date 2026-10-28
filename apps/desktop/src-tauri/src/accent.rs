//! The Windows accent colour for the "Windows accent" app-colour choice.
//!
//! Windows keeps the accent under `HKCU\Software\Microsoft\Windows\DWM`. Both values are a DWORD
//! in `0xAABBGGRR` order (alpha first, then blue, green and red), so the low three bytes are
//! already `RRGGBB`. `AccentColor` is the real accent; `ColorizationColor` is the older value and
//! is only a fallback. Nothing is written and no other key is read.

use std::fmt::Write as _;

/// Decodes a DWM colour DWORD (`0xAABBGGRR`) to a lower-case `#rrggbb` string. The alpha byte is
/// ignored. Kept free of registry access so it can be tested anywhere.
pub fn decode_dwm_color(value: u32) -> String {
    let mut text = String::with_capacity(7);
    text.push('#');
    // Writing to a String cannot fail.
    let _ = write!(
        text,
        "{:02x}{:02x}{:02x}",
        value & 0xff,
        (value >> 8) & 0xff,
        (value >> 16) & 0xff
    );
    text
}

/// The Windows accent colour, or `None` when it cannot be read. Tries `AccentColor` first and
/// falls back to `ColorizationColor`; zero means "not set" and is skipped.
#[cfg(windows)]
pub fn read_accent_color() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"Software\Microsoft\Windows\DWM", KEY_READ)
        .ok()?;
    for name in ["AccentColor", "ColorizationColor"] {
        if let Ok(value) = key.get_value::<u32, _>(name)
            && value != 0
        {
            return Some(decode_dwm_color(value));
        }
    }
    None
}

/// Other platforms have no Windows accent.
#[cfg(not(windows))]
pub fn read_accent_color() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::decode_dwm_color;

    #[test]
    fn decodes_accent_dword_to_rrggbb() {
        assert_eq!(decode_dwm_color(0xffd6_7f4a), "#4a7fd6");
        assert_eq!(decode_dwm_color(0x0012_3456), "#563412");
    }

    #[test]
    fn keeps_zero_black_and_alpha_is_ignored() {
        assert_eq!(decode_dwm_color(0), "#000000");
        assert_eq!(decode_dwm_color(0xffff_ffff), "#ffffff");
        assert_eq!(decode_dwm_color(0x12ab_cdef), "#efcdab");
    }
}
