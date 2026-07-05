use std::{thread, time::Duration};

pub const CAPTURE_HOTKEY: &str = "F8";

pub fn capture_item_text() -> Result<String, String> {
    send_copy_shortcut()?;
    thread::sleep(Duration::from_millis(160));
    read_clipboard_text()
}

fn read_clipboard_text() -> Result<String, String> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| format!("Clipboard is unavailable: {error}"))?;
    let text = clipboard
        .get_text()
        .map_err(|error| format!("Clipboard does not contain item text: {error}"))?;

    if text.trim().is_empty() {
        return Err("Clipboard is empty. Hover an item in Path of Exile 2 and try again.".to_string());
    }

    Ok(text)
}

#[cfg(target_os = "windows")]
fn send_copy_shortcut() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        VK_CONTROL,
    };

    const VK_C: VIRTUAL_KEY = VIRTUAL_KEY(0x43);

    fn keyboard_input(key: VIRTUAL_KEY, key_up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: key,
                    wScan: 0,
                    dwFlags: if key_up { KEYEVENTF_KEYUP } else { Default::default() },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    let inputs = [
        keyboard_input(VK_CONTROL, false),
        keyboard_input(VK_C, false),
        keyboard_input(VK_C, true),
        keyboard_input(VK_CONTROL, true),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };

    if sent != inputs.len() as u32 {
        return Err("Windows did not accept the copy shortcut input sequence.".to_string());
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn send_copy_shortcut() -> Result<(), String> {
    Err("Automatic item copy is only implemented on Windows. Paste item text manually.".to_string())
}
