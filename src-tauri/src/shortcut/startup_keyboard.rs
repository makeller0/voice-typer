use handy_keys::{Hotkey, Key, Modifiers};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[link(name = "user32")]
extern "system" {
    fn GetAsyncKeyState(key: i32) -> i16;
}

#[derive(Default)]
pub struct KeyboardPressState(Mutex<PressState>);

#[derive(Default)]
struct PressState {
    pressed: bool,
    checked: bool,
    recovering: bool,
}

impl PressState {
    fn accept(&mut self, pressed: bool) -> Option<bool> {
        if self.checked && !self.recovering {
            return None;
        }
        if self.recovering && !self.pressed && pressed {
            self.recovering = false;
        }
        Some(update_pressed(&mut self.pressed, pressed))
    }
}

pub(crate) fn virtual_key(key: Key) -> Option<u16> {
    let name = key.to_string();
    if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() {
        return Some(name.as_bytes()[0].to_ascii_uppercase() as u16);
    }
    if let Some(number) = name.strip_prefix('F').and_then(|s| s.parse::<u16>().ok()) {
        if (1..=24).contains(&number) {
            return Some(0x6f + number);
        }
    }
    let keypad = [
        Key::Keypad0,
        Key::Keypad1,
        Key::Keypad2,
        Key::Keypad3,
        Key::Keypad4,
        Key::Keypad5,
        Key::Keypad6,
        Key::Keypad7,
        Key::Keypad8,
        Key::Keypad9,
    ];
    if let Some(index) = keypad.iter().position(|candidate| *candidate == key) {
        return Some(0x60 + index as u16);
    }
    Some(match key {
        Key::Space => 0x20,
        Key::Return => 0x0d,
        Key::Tab => 0x09,
        Key::Escape => 0x1b,
        Key::Delete => 0x08,
        Key::ForwardDelete => 0x2e,
        Key::Insert => 0x2d,
        Key::Pause => 0x13,
        Key::Home => 0x24,
        Key::End => 0x23,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::LeftArrow => 0x25,
        Key::UpArrow => 0x26,
        Key::RightArrow => 0x27,
        Key::DownArrow => 0x28,
        Key::Minus => 0xbd,
        Key::Equal => 0xbb,
        Key::LeftBracket => 0xdb,
        Key::RightBracket => 0xdd,
        Key::Backslash => 0xdc,
        Key::Semicolon => 0xba,
        Key::Quote => 0xde,
        Key::Comma => 0xbc,
        Key::Period => 0xbe,
        Key::Slash => 0xbf,
        Key::Grave => 0xc0,
        Key::KeypadDecimal => 0x6e,
        Key::KeypadMultiply => 0x6a,
        Key::KeypadPlus => 0x6b,
        Key::KeypadClear => 0x0c,
        Key::KeypadDivide => 0x6f,
        Key::KeypadMinus => 0x6d,
        Key::CapsLock => 0x14,
        Key::ScrollLock => 0x91,
        Key::NumLock => 0x90,
        Key::PrintScreen => 0x2c,
        Key::ContextMenu => 0x5d,
        Key::PlayPause => 0xb3,
        Key::Stop => 0xb2,
        Key::PrevTrack => 0xb1,
        Key::NextTrack => 0xb0,
        _ => return None,
    })
}

pub(crate) fn keyboard_hotkey(binding: &str) -> Option<Hotkey> {
    let hotkey: Hotkey = binding.parse().ok()?;
    if hotkey.modifiers.contains(Modifiers::FN) {
        return None;
    }
    if let Some(key) = hotkey.key {
        virtual_key(key)?;
    }
    Some(hotkey)
}

fn is_binding_held(binding: &str, mut pressed: impl FnMut(u16) -> bool) -> bool {
    let Some(hotkey) = keyboard_hotkey(binding) else {
        return false;
    };
    let mut modifiers = Modifiers::empty();
    for (key, modifier) in [
        (0xa0, Modifiers::SHIFT_LEFT),
        (0xa1, Modifiers::SHIFT_RIGHT),
        (0xa2, Modifiers::CTRL_LEFT),
        (0xa3, Modifiers::CTRL_RIGHT),
        (0xa4, Modifiers::OPT_LEFT),
        (0xa5, Modifiers::OPT_RIGHT),
        (0x5b, Modifiers::CMD_LEFT),
        (0x5c, Modifiers::CMD_RIGHT),
    ] {
        if pressed(key) {
            modifiers |= modifier;
        }
    }
    let modifiers_held = [
        Modifiers::CTRL,
        Modifiers::SHIFT,
        Modifiers::OPT,
        Modifiers::CMD,
    ]
    .into_iter()
    .all(|group| {
        let required = hotkey.modifiers & group;
        if required == group {
            modifiers.intersects(group)
        } else {
            modifiers.contains(required)
        }
    });
    modifiers_held
        && hotkey
            .key
            .is_none_or(|key| virtual_key(key).is_some_and(&mut pressed))
}

fn physically_held(binding: &str) -> bool {
    is_binding_held(binding, |key| unsafe { GetAsyncKeyState(key as i32) < 0 })
}

pub fn handle_event(app: &AppHandle, id: &str, binding: &str, pressed: bool) -> bool {
    if id != "transcribe" || keyboard_hotkey(binding).is_none() {
        return false;
    }
    let Some(state) = app.try_state::<KeyboardPressState>() else {
        return false;
    };
    let Ok(mut previous) = state.0.lock() else {
        return false;
    };
    let Some(dispatch) = previous.accept(pressed) else {
        return false;
    };
    if dispatch {
        super::handler::dispatch_shortcut_event(app, id, binding, pressed);
    }
    true
}

fn update_pressed(previous: &mut bool, pressed: bool) -> bool {
    let changed = *previous != pressed;
    *previous = pressed;
    changed
}

pub fn recover_held_transcribe(app: &AppHandle) {
    let settings = crate::settings::get_settings(app);
    let binding = settings
        .bindings
        .get("transcribe")
        .map(|binding| binding.current_binding.clone())
        .unwrap_or_default();
    let held = !matches!(
        settings.keyboard_implementation,
        crate::settings::KeyboardImplementation::None
    ) && physically_held(&binding);
    if held {
        log::debug!("Recovering held startup keyboard shortcut: {binding}");
        super::handler::handle_shortcut_event(app, "transcribe", &binding, true);
    }
    let Some(state) = app.try_state::<KeyboardPressState>() else {
        return;
    };
    let Ok(mut state) = state.0.lock() else {
        return;
    };
    state.checked = true;
    state.recovering = held;
    drop(state);
    if !held {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        // handy-keys only emits Released for presses it observed itself.
        loop {
            let Some(state) = app.try_state::<KeyboardPressState>() else {
                return;
            };
            let Ok(mut state) = state.0.lock() else {
                return;
            };
            if !state.recovering || !state.pressed {
                return;
            }
            if !physically_held(&binding) {
                state.pressed = false;
                super::handler::dispatch_shortcut_event(&app, "transcribe", &binding, false);
                return;
            }
            drop(state);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(binding: &str, keys: &[u16]) -> bool {
        is_binding_held(binding, |key| keys.contains(&key))
    }

    #[test]
    fn requires_all_keys_and_matches_either_modifier_side() {
        assert!(held("ctrl+space", &[0xa2, 0x20]));
        assert!(held("ctrl+space", &[0xa3, 0x20]));
        assert!(!held("ctrl+space", &[0xa2]));
        assert!(!held("ctrl+space", &[0x20]));
        assert!(held("ctrl+space", &[0xa2, 0xa0, 0x20]));
        assert!(held("ctrl_right+space", &[0xa3, 0x20]));
        assert!(!held("ctrl_right+space", &[0xa2, 0x20]));
    }

    #[test]
    fn recovered_and_native_events_dispatch_each_edge_only_once() {
        let mut previous = false;
        assert!(!update_pressed(&mut previous, false));
        assert!(update_pressed(&mut previous, true));
        assert!(!update_pressed(&mut previous, true));
        assert!(update_pressed(&mut previous, false));
        let mut state = PressState::default();
        assert_eq!(state.accept(true), Some(true));
        state.checked = true;
        state.recovering = true;
        assert_eq!(state.accept(true), Some(false));
        assert_eq!(state.accept(false), Some(true));
        assert_eq!(state.accept(false), Some(false));
        assert_eq!(state.accept(true), Some(true));
        assert_eq!(state.accept(false), None);
        assert!(!update_pressed(&mut previous, false));
        assert!(update_pressed(&mut previous, true));
        assert!(update_pressed(&mut previous, false));
    }

    #[test]
    fn supports_keyboard_keys_and_rejects_empty_invalid_and_mouse_bindings() {
        assert!(held("F24", &[0x87]));
        assert!(held("ctrl+K", &[0xa2, 0x4b]));
        assert!(held("shift", &[0xa1]));
        for binding in ["", "not-a-key", "MouseX1", "ctrl+MouseLeft", "fn+space"] {
            assert!(!held(binding, &[0xa2, 0x20, 0x01, 0x05]));
        }
    }
}
