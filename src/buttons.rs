use std::collections::BTreeSet;

use enigo::Key;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnesButton {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L,
    R,
    Start,
    Select,
}

impl SnesButton {
    pub const ALL: [SnesButton; 12] = [
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::A,
        Self::B,
        Self::X,
        Self::Y,
        Self::L,
        Self::R,
        Self::Start,
        Self::Select,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
            Self::A => "a",
            Self::B => "b",
            Self::X => "x",
            Self::Y => "y",
            Self::L => "l",
            Self::R => "r",
            Self::Start => "start",
            Self::Select => "select",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::A => "A",
            Self::B => "B",
            Self::X => "X",
            Self::Y => "Y",
            Self::L => "L",
            Self::R => "R",
            Self::Start => "Start",
            Self::Select => "Select",
        }
    }

    pub fn short_label(self) -> String {
        self.display_name().to_uppercase()
    }
}

/// macOS virtual key codes, matching the Swift profile JSON.
pub fn default_key_code(button: SnesButton) -> u16 {
    match button {
        SnesButton::Up => 126,
        SnesButton::Down => 125,
        SnesButton::Left => 123,
        SnesButton::Right => 124,
        SnesButton::A => 0,
        SnesButton::B => 6,
        SnesButton::X => 1,
        SnesButton::Y => 7,
        SnesButton::L => 12,
        SnesButton::R => 13,
        SnesButton::Start => 36,
        SnesButton::Select => 60,
    }
}

pub fn default_key_map() -> std::collections::BTreeMap<String, u16> {
    SnesButton::ALL
        .into_iter()
        .map(|button| (button.as_str().to_string(), default_key_code(button)))
        .collect()
}

pub fn key_name(code: u16) -> String {
    let name = match code {
        0 => "A",
        1 => "S",
        2 => "D",
        3 => "F",
        4 => "H",
        5 => "G",
        6 => "Z",
        7 => "X",
        8 => "C",
        9 => "V",
        11 => "B",
        12 => "Q",
        13 => "W",
        14 => "E",
        15 => "R",
        16 => "Y",
        17 => "T",
        18 => "1",
        19 => "2",
        20 => "3",
        21 => "4",
        22 => "6",
        23 => "5",
        24 => "=",
        25 => "9",
        26 => "7",
        27 => "-",
        28 => "8",
        29 => "0",
        30 => "]",
        31 => "O",
        32 => "U",
        33 => "[",
        34 => "I",
        35 => "P",
        36 => "Return",
        37 => "L",
        38 => "J",
        39 => "'",
        40 => "K",
        41 => ";",
        42 => "\\",
        43 => ",",
        44 => "/",
        45 => "N",
        46 => "M",
        47 => ".",
        48 => "Tab",
        49 => "Space",
        50 => "`",
        51 => "Delete",
        53 => "Esc",
        54 => "Right Cmd",
        55 => "Left Cmd",
        56 => "Left Shift",
        58 => "Left Option",
        59 => "Left Ctrl",
        60 => "Right Shift",
        61 => "Right Option",
        62 => "Right Ctrl",
        123 => "←",
        124 => "→",
        125 => "↓",
        126 => "↑",
        _ => return format!("Key {code}"),
    };
    name.to_string()
}

pub fn enigo_key(code: u16) -> Option<Key> {
    // Profiles store macOS virtual key codes, which are the same numbers Core
    // Graphics posts. Mapping those through `Key::Unicode` makes enigo recover a
    // key code by walking the active keyboard layout (`TISCopyCurrentKeyboardInputSource`).
    // That lookup runs on the audio thread, and Text Input Source calls are only
    // safe on the main thread — when the current source is briefly null the
    // process aborts inside `CFRelease`. Post the code itself instead.
    #[cfg(target_os = "macos")]
    {
        return Some(Key::Other(u32::from(code)));
    }

    #[cfg(not(target_os = "macos"))]
    {
        Some(match code {
            36 => Key::Return,
            48 => Key::Tab,
            49 => Key::Space,
            51 => Key::Backspace,
            53 => Key::Escape,
            123 => Key::LeftArrow,
            124 => Key::RightArrow,
            125 => Key::DownArrow,
            126 => Key::UpArrow,
            56 => Key::LShift,
            60 => Key::RShift,
            59 => Key::LControl,
            62 => Key::RControl,
            58 | 61 => Key::Alt,
            54 | 55 => Key::Meta,
            other => Key::Unicode(mac_char(other)?),
        })
    }
}

#[cfg(not(target_os = "macos"))]
fn mac_char(code: u16) -> Option<char> {
    Some(match code {
        0 => 'a',
        1 => 's',
        2 => 'd',
        3 => 'f',
        4 => 'h',
        5 => 'g',
        6 => 'z',
        7 => 'x',
        8 => 'c',
        9 => 'v',
        11 => 'b',
        12 => 'q',
        13 => 'w',
        14 => 'e',
        15 => 'r',
        16 => 'y',
        17 => 't',
        18 => '1',
        19 => '2',
        20 => '3',
        21 => '4',
        22 => '6',
        23 => '5',
        24 => '=',
        25 => '9',
        26 => '7',
        27 => '-',
        28 => '8',
        29 => '0',
        30 => ']',
        31 => 'o',
        32 => 'u',
        33 => '[',
        34 => 'i',
        35 => 'p',
        37 => 'l',
        38 => 'j',
        39 => '\'',
        40 => 'k',
        41 => ';',
        42 => '\\',
        43 => ',',
        44 => '/',
        45 => 'n',
        46 => 'm',
        47 => '.',
        50 => '`',
        _ => return None,
    })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn macos_posts_virtual_key_codes_without_a_layout_lookup() {
        assert_eq!(enigo_key(0), Some(Key::Other(0)));
        assert_eq!(enigo_key(6), Some(Key::Other(6)));
        assert_eq!(enigo_key(7), Some(Key::Other(7)));
        assert_eq!(enigo_key(124), Some(Key::Other(124)));
        assert_eq!(enigo_key(60), Some(Key::Other(60)));
    }
}

pub fn buttons_label(buttons: &BTreeSet<SnesButton>) -> String {
    buttons
        .iter()
        .map(|button| button.display_name())
        .collect::<Vec<_>>()
        .join("+")
}
