#[derive(Default)]
struct Activity {
    shortcuts: Vec<Vec<Vec<u16>>>,
    held: Vec<u16>,
    active: bool,
    revision: u64,
    last_tick: u32,
}

impl Activity {
    fn key(&mut self, key: u16, down: bool) {
        let allowed = self.shortcuts.iter().flatten().flatten().any(|&k| k == key);
        if !allowed {
            self.revision = self.revision.wrapping_add(1);
            return;
        }
        if down {
            let repeated = self.held.contains(&key);
            if !repeated {
                self.held.push(key);
            }
            let complete = self.shortcuts.iter().any(|chord| {
                chord
                    .iter()
                    .all(|group| group.iter().any(|k| self.held.contains(k)))
            });
            if complete {
                self.active = true;
            } else if repeated || !matches!(key, 0xa0..=0xa5 | 0x5b | 0x5c) {
                self.revision = self.revision.wrapping_add(1);
            }
        } else {
            if !self.active {
                self.revision = self.revision.wrapping_add(1);
            }
            self.held.retain(|&k| k != key);
            if self.held.is_empty() {
                self.active = false;
            }
        }
    }

    fn token(&self, last_input: u32) -> Option<u64> {
        (self.last_tick == last_input && (self.active || self.held.is_empty()))
            .then_some(self.revision)
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::Activity;
    use crate::settings::AppSettings;
    use handy_keys::Modifiers;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex, OnceLock,
    };
    use std::time::Duration;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG,
        MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL,
    };

    pub(crate) const OWN_INPUT: usize = 0x56545950;
    static STATE: Mutex<Option<Activity>> = Mutex::new(None);
    static STARTED: OnceLock<bool> = OnceLock::new();
    static ALIVE: AtomicBool = AtomicBool::new(false);

    #[repr(C)]
    struct LastInput {
        size: u32,
        time: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetLastInputInfo(info: *mut LastInput) -> i32;
    }

    fn last_input() -> Option<u32> {
        let mut info = LastInput {
            size: std::mem::size_of::<LastInput>() as u32,
            time: 0,
        };
        (unsafe { GetLastInputInfo(&mut info) } != 0).then_some(info.time)
    }

    unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            let event = &*(l.0 as *const KBDLLHOOKSTRUCT);
            if let Ok(mut state) = STATE.lock() {
                if let Some(state) = state.as_mut() {
                    state.last_tick = event.time;
                    if event.dwExtraInfo != OWN_INPUT {
                        state.key(event.vkCode as u16, matches!(w.0, 0x100 | 0x104));
                    }
                }
            }
        }
        CallNextHookEx(None, code, w, l)
    }

    unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 {
            let event = &*(l.0 as *const MSLLHOOKSTRUCT);
            if let Ok(mut state) = STATE.lock() {
                if let Some(state) = state.as_mut() {
                    state.last_tick = event.time;
                    if event.dwExtraInfo != OWN_INPUT {
                        let button = match w.0 {
                            0x201 | 0x202 => Some((1, w.0 == 0x201)),
                            0x204 | 0x205 => Some((2, w.0 == 0x204)),
                            0x207 | 0x208 => Some((4, w.0 == 0x207)),
                            0x20b | 0x20c => {
                                Some((if event.mouseData >> 16 == 1 { 5 } else { 6 }, w.0 == 0x20b))
                            }
                            _ => None,
                        };
                        if let Some((key, down)) = button {
                            state.key(key, down);
                        } else {
                            state.revision = state.revision.wrapping_add(1);
                        }
                    }
                }
            }
        }
        CallNextHookEx(None, code, w, l)
    }

    fn shortcuts(settings: &AppSettings) -> Vec<Vec<Vec<u16>>> {
        settings
            .bindings
            .iter()
            .filter(|(id, _)| id.starts_with("transcribe"))
            .filter_map(|(_, binding)| {
                let binding = &binding.current_binding;
                match binding.as_str() {
                    "MouseX1" => return Some(vec![vec![5]]),
                    "MouseX2" => return Some(vec![vec![6]]),
                    _ => {}
                }
                let hotkey = crate::shortcut::startup_keyboard::keyboard_hotkey(binding)?;
                let mut groups = Vec::new();
                for (both, left, right, l, r) in [
                    (
                        Modifiers::CTRL,
                        Modifiers::CTRL_LEFT,
                        Modifiers::CTRL_RIGHT,
                        0xa2,
                        0xa3,
                    ),
                    (
                        Modifiers::SHIFT,
                        Modifiers::SHIFT_LEFT,
                        Modifiers::SHIFT_RIGHT,
                        0xa0,
                        0xa1,
                    ),
                    (
                        Modifiers::OPT,
                        Modifiers::OPT_LEFT,
                        Modifiers::OPT_RIGHT,
                        0xa4,
                        0xa5,
                    ),
                    (
                        Modifiers::CMD,
                        Modifiers::CMD_LEFT,
                        Modifiers::CMD_RIGHT,
                        0x5b,
                        0x5c,
                    ),
                ] {
                    let required = hotkey.modifiers & both;
                    if !required.is_empty() {
                        groups.push(if required == both {
                            vec![l, r]
                        } else if required == left {
                            vec![l]
                        } else if required == right {
                            vec![r]
                        } else {
                            return None;
                        });
                    }
                }
                if let Some(key) = hotkey.key {
                    groups.push(vec![crate::shortcut::startup_keyboard::virtual_key(key)?]);
                }
                (!groups.is_empty()).then_some(groups)
            })
            .collect()
    }

    pub(crate) fn prepare(settings: &AppSettings) {
        let mut chords = shortcuts(settings);
        chords.sort();
        if let Ok(mut state) = STATE.lock() {
            let state = state.get_or_insert_with(Activity::default);
            if state.shortcuts != chords {
                state.shortcuts = chords;
                state.revision = state.revision.wrapping_add(1);
                state.held.clear();
                state.active = false;
            }
        }
        STARTED.get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel(1);
            if std::thread::Builder::new()
                .name("paste-activity".into())
                .spawn(move || unsafe {
                    let Ok(module) = GetModuleHandleW(None) else {
                        let _ = tx.send(false);
                        return;
                    };
                    let module = Some(HINSTANCE(module.0));
                    let Ok(keyboard) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), module, 0)
                    else {
                        let _ = tx.send(false);
                        return;
                    };
                    let Ok(mouse) = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), module, 0) else {
                        let _ = UnhookWindowsHookEx(keyboard);
                        let _ = tx.send(false);
                        return;
                    };
                    ALIVE.store(true, Ordering::Release);
                    let _ = tx.send(true);
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
                    ALIVE.store(false, Ordering::Release);
                    let _ = UnhookWindowsHookEx(keyboard);
                    let _ = UnhookWindowsHookEx(mouse);
                })
                .is_err()
            {
                return false;
            }
            rx.recv_timeout(Duration::from_secs(1)).unwrap_or(false)
        });
    }

    pub(crate) fn token() -> Option<u64> {
        if !STARTED.get().copied().unwrap_or(false) || !ALIVE.load(Ordering::Acquire) {
            return None;
        }
        let last_input = last_input()?;
        STATE.lock().ok()?.as_ref()?.token(last_input)
    }
}

#[cfg(target_os = "windows")]
pub(crate) use platform::{prepare, token, OWN_INPUT};

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Activity {
        Activity {
            shortcuts: vec![vec![vec![0xa2, 0xa3], vec![0x20]], vec![vec![5]]],
            ..Activity::default()
        }
    }
    #[test]
    fn dictation_shortcuts_do_not_invalidate_paste() {
        let mut s = state();
        for (k, down) in [
            (0xa2, true),
            (0x20, true),
            (0x20, false),
            (0xa2, false),
            (5, true),
            (5, false),
        ] {
            s.key(k, down);
        }
        assert_eq!(s.token(0), Some(0));
    }
    #[test]
    fn ordinary_space_modifiers_arrows_and_typing_invalidate() {
        for keys in [
            vec![(0x20, true), (0x20, false)],
            vec![(0xa2, true), (0xa2, false)],
            vec![(0x25, true)],
            vec![(0x41, true)],
        ] {
            let mut s = state();
            for (k, d) in keys {
                s.key(k, d);
            }
            assert_ne!(s.token(0), Some(0));
        }
    }
    #[test]
    fn unknown_input_or_unfinished_prefix_is_not_safe() {
        let mut s = state();
        assert_eq!(s.token(1), None);
        s.key(0xa2, true);
        assert_eq!(s.token(0), None);
        s.key(0x20, true);
        assert_eq!(s.token(0), Some(0));
    }
    #[test]
    fn caret_click_and_selection_invalidate_even_before_another_shortcut() {
        for key in [1, 2, 4, 0x25, 0x24, 0x41] {
            let mut s = state();
            s.key(key, true);
            s.key(key, false);
            for (k, d) in [(0xa2, true), (0x20, true), (0x20, false), (0xa2, false)] {
                s.key(k, d);
            }
            assert_ne!(s.token(0), Some(0));
        }
    }
}
