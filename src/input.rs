// Port of Input.h / Input.cpp

use crate::game_header::GH_MOUSE_SMOOTH;

pub struct Input {
    //Keyboard
    pub key: [bool; 256],
    pub key_press: [bool; 256],

    //Mouse
    pub mouse_button: [bool; 3],
    pub mouse_button_press: [bool; 3],
    pub mouse_dx: f32,
    pub mouse_dy: f32,
    pub mouse_ddx: f32,
    pub mouse_ddy: f32,
    // EXT: the joystick slots CodeParade left as `//TODO:` (Input.h:23-24), now filled in by
    // src/ext/gamepad.rs. Analog, in -1..1. `Player::Move` normalises only when the combined
    // magnitude exceeds 1 (Player.cpp:92-96), so these simply add to the keyboard vector.
    pub pad_move_f: f32,
    pub pad_move_l: f32,
    // EXT: right-stick look, already scaled to radians per fixed step.
    pub pad_look_x: f32,
    pub pad_look_y: f32,
    // EXT: raw state of the pad's rotate modifier (R1 / `Button::RightTrigger`), refreshed every
    // poll by src/ext/gamepad.rs and read by src/ext/rotate.rs. Level, not edge: it mirrors
    // `mouse_button[2]` / `key[b'R']`, the keyboard-side modifiers.
    pub pad_rotate_mod: bool,

    //Bindings
    //TODO:

    //Calibration
    //TODO:
}

impl Input {
    // Input::Input() { memset(this, 0, sizeof(Input)); }   (Input.cpp:6-8)
    pub fn new() -> Input {
        Input {
            key: [false; 256],
            key_press: [false; 256],
            mouse_button: [false; 3],
            mouse_button_press: [false; 3],
            mouse_dx: 0.0,
            mouse_dy: 0.0,
            mouse_ddx: 0.0,
            mouse_ddy: 0.0,
            // EXT: gamepad axes.
            pad_move_f: 0.0,
            pad_move_l: 0.0,
            pad_look_x: 0.0,
            pad_look_y: 0.0,
            // EXT: rotate modifier.
            pad_rotate_mod: false,
        }
    }

    // Input::EndFrame()   (Input.cpp:10-17)
    pub fn end_frame(&mut self) {
        // PORT: array fill instead of memset (was: memset(key_press, 0, sizeof(key_press)), Input.cpp:11)
        self.key_press = [false; 256];
        // PORT: array fill instead of memset (was: memset(mouse_button_press, 0, sizeof(mouse_button_press)), Input.cpp:12)
        self.mouse_button_press = [false; 3];
        self.mouse_dx = self.mouse_dx * GH_MOUSE_SMOOTH + self.mouse_ddx * (1.0 - GH_MOUSE_SMOOTH);
        self.mouse_dy = self.mouse_dy * GH_MOUSE_SMOOTH + self.mouse_ddy * (1.0 - GH_MOUSE_SMOOTH);
        self.mouse_ddx = 0.0;
        self.mouse_ddy = 0.0;
    }

    // PORT: replaces Input::UpdateRaw(const tagRAWINPUT*) mouse-move branch, which is Win32
    // raw-input only; the platform layer feeds relative motion here instead
    // (was: if (raw->data.mouse.usFlags == MOUSE_MOVE_RELATIVE) { mouse_ddx += raw->data.mouse.lLastX;
    //       mouse_ddy += raw->data.mouse.lLastY; }, Input.cpp:24-27)
    pub fn add_mouse_motion(&mut self, dx: f32, dy: f32) {
        self.mouse_ddx += dx;
        self.mouse_ddy += dy;
    }

    // PORT: replaces Input::UpdateRaw's RI_MOUSE_*_BUTTON_DOWN/UP handling; the platform layer
    // reports one button index at a time
    // (was: if (raw->data.mouse.usButtonFlags & RI_MOUSE_LEFT_BUTTON_DOWN) { mouse_button[0] = true;
    //       mouse_button_press[0] = true; } ... , Input.cpp:28-42)
    pub fn set_mouse_button(&mut self, idx: usize, down: bool) {
        if down {
            self.mouse_button[idx] = true;
            self.mouse_button_press[idx] = true;
        } else {
            self.mouse_button[idx] = false;
        }
    }
}

impl Default for Input {
    fn default() -> Input {
        Input::new()
    }
}

// PORT: replaces the Win32 WndProc's 'input.key[wParam & 0xFF]' virtual-key indexing
// (was: case WM_KEYDOWN: input.key[wParam & 0xFF] = true; ..., Engine.cpp:295-313).
// winit KeyCodes are mapped into the same ASCII slots the C++ code reads
// ('W','A','S','D','1'..'7',' ').
pub fn key_index(k: winit::keyboard::KeyCode) -> Option<usize> {
    use winit::keyboard::KeyCode;
    let c: u8 = match k {
        KeyCode::KeyW => b'W',
        KeyCode::KeyA => b'A',
        KeyCode::KeyS => b'S',
        KeyCode::KeyD => b'D',
        KeyCode::Digit1 => b'1',
        KeyCode::Digit2 => b'2',
        KeyCode::Digit3 => b'3',
        KeyCode::Digit4 => b'4',
        KeyCode::Digit5 => b'5',
        KeyCode::Digit6 => b'6',
        KeyCode::Digit7 => b'7',
        // EXT: the port stops at 7 because the C++ has seven scenes (Engine.cpp:90-104).
        // Extension scenes continue the run: 8, 9, 0, then '-' and '='.
        KeyCode::Digit8 => b'8',
        KeyCode::Digit9 => b'9',
        KeyCode::Digit0 => b'0',
        KeyCode::Minus => b'-',
        KeyCode::Equal => b'=',
        KeyCode::BracketLeft => b'[',
        KeyCode::BracketRight => b']',
        KeyCode::Backslash => b'\\',
        KeyCode::Semicolon => b';',
        // EXT: grab / release the held object.
        KeyCode::KeyE => b'E',
        // EXT: mute toggle.
        KeyCode::KeyM => b'M',
        // EXT: rotate-modifier on keyboard (hold R + mouse).
        KeyCode::KeyR => b'R',
        // EXT: menu navigation, in the Win32 virtual-key slots the C++ layout implies
        // (VK_UP/DOWN/LEFT/RIGHT = 38/40/37/39, VK_RETURN = 13, VK_BACK = 8, VK_ESCAPE = 27).
        KeyCode::ArrowUp => 38,
        KeyCode::ArrowDown => 40,
        KeyCode::ArrowLeft => 37,
        KeyCode::ArrowRight => 39,
        KeyCode::Enter => 13,
        KeyCode::Backspace => 8,
        KeyCode::Space => b' ',
        _ => return None,
    };
    Some(c as usize)
}
