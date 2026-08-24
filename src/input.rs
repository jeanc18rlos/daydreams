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
    // EXT: raw state of the pad's sprint button (L3 / `Button::LeftThumb`), refreshed every
    // poll by src/ext/gamepad.rs. A level like `pad_rotate_mod`; the press *edge* that flips
    // toggle mode travels separately, as `PadEvents::sprint`.
    pub pad_sprint: bool,
    // EXT: raw state of the pad's jump button (Cross / `Button::South`), refreshed every poll by
    // src/ext/gamepad.rs and read by `Player::update_player` beside the keyboard's Space. A level
    // like `pad_rotate_mod`, and for a second reason as well as the first: `end_frame` runs inside
    // the engine's fixed-step loop, so an edge slot is clear for every step of a frame but the
    // first (src/ext/jump.rs).
    pub pad_jump: bool,
    // EXT: mouse wheel since the last `end_frame`, in notches, positive away from the player.
    // The inventory's slot selector reads it (src/ext/inventory.rs). An edge like `key_press`,
    // and cleared with them: the wheel says "one notch happened", not "the wheel is at 3".
    pub wheel: f32,
    // EXT: this frame's resolved sprint multipliers -- hold, toggle, the forward-only rule and
    // the eased speed cap already folded in by `ext::sprint::Sprint::resolve`, which the engine
    // runs once per rendered frame before the fixed-step loop. `Player::update_player` reads
    // only this, never the raw keys, so the ported movement code does not have to know which
    // input produced it. The title backdrop swaps in a blank `Input`, where it is the walk.
    pub sprint: crate::ext::sprint::Factors,
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
            // EXT: sprint -- raw pad level and the engine-resolved level.
            pad_sprint: false,
            // EXT: jump.
            pad_jump: false,
            // EXT: the wheel's edge.
            wheel: 0.0,
            sprint: crate::ext::sprint::Factors::WALK,
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
        // EXT: the wheel is an edge, cleared with the key and button edges above. This runs
        // inside the 500 Hz loop, so `Engine::run_frame` latches the notch before the loop,
        // exactly as it latches the E press.
        self.wheel = 0.0;
    }

    // EXT: one wheel event from the platform layer. winit reports a mouse's wheel in lines and
    // a trackpad's two-finger scroll in pixels; both arrive here as notches, so nothing
    // downstream has to know which device it was. Horizontal scroll is dropped: nothing reads
    // it, and a trackpad's diagonal flick would otherwise change the selected slot sideways.
    //
    // `scale` is the window's scale factor. `PixelDelta` carries PHYSICAL pixels, so on a retina
    // panel the same flick of the same fingers arrives twice as large as it does on an external
    // 1x monitor; dividing by the scale is what makes the gesture, rather than the display, decide
    // how many slots go by.
    pub fn add_mouse_wheel(&mut self, delta: winit::event::MouseScrollDelta, scale: f32) {
        /// Trackpad pixels per notch, at the window's logical-pixel scale.
        const PIXELS_PER_NOTCH: f32 = 40.0;
        self.wheel += match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => y,
            winit::event::MouseScrollDelta::PixelDelta(p) => {
                p.y as f32 / (PIXELS_PER_NOTCH * scale.max(0.1))
            }
        };
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
        KeyCode::Quote => b'\'',
        // EXT: the two interiors, on past the quote key.
        KeyCode::Comma => b',',
        KeyCode::Period => b'.',
        // EXT: grab / release the held object.
        KeyCode::KeyE => b'E',
        // EXT: mute toggle.
        KeyCode::KeyM => b'M',
        // EXT: the inventory -- stow / take out, and put down (src/ext/inventory.rs).
        KeyCode::KeyF => b'F',
        KeyCode::KeyG => b'G',
        // EXT: rotate-modifier on keyboard (hold R + mouse).
        KeyCode::KeyR => b'R',
        // EXT: sprint (hold). Both Shifts land in the Win32 VK_SHIFT slot (16), the slot the
        // C++ WndProc's `wParam & 0xFF` indexing would have given them (Engine.cpp:295-313).
        KeyCode::ShiftLeft | KeyCode::ShiftRight => 16,
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

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    /// Every key the game reads, and the slot it reads it from: the ASCII slots the C++
    /// indexes with `wParam & 0xFF`, the Win32 virtual-key numbers for the rest.
    #[test]
    fn every_key_the_game_reads_lands_in_its_slot() {
        let table: [(KeyCode, usize); 40] = [
            (KeyCode::KeyW, b'W' as usize),
            (KeyCode::KeyA, b'A' as usize),
            (KeyCode::KeyS, b'S' as usize),
            (KeyCode::KeyD, b'D' as usize),
            (KeyCode::Digit1, b'1' as usize),
            (KeyCode::Digit2, b'2' as usize),
            (KeyCode::Digit3, b'3' as usize),
            (KeyCode::Digit4, b'4' as usize),
            (KeyCode::Digit5, b'5' as usize),
            (KeyCode::Digit6, b'6' as usize),
            (KeyCode::Digit7, b'7' as usize),
            (KeyCode::Digit8, b'8' as usize),
            (KeyCode::Digit9, b'9' as usize),
            (KeyCode::Digit0, b'0' as usize),
            (KeyCode::Minus, b'-' as usize),
            (KeyCode::Equal, b'=' as usize),
            (KeyCode::BracketLeft, b'[' as usize),
            (KeyCode::BracketRight, b']' as usize),
            (KeyCode::Backslash, b'\\' as usize),
            (KeyCode::Semicolon, b';' as usize),
            (KeyCode::Quote, b'\'' as usize),
            (KeyCode::Comma, b',' as usize),
            (KeyCode::Period, b'.' as usize),
            (KeyCode::KeyE, b'E' as usize),
            (KeyCode::KeyM, b'M' as usize),
            (KeyCode::KeyF, b'F' as usize),
            (KeyCode::KeyG, b'G' as usize),
            (KeyCode::KeyR, b'R' as usize),
            (KeyCode::ShiftLeft, crate::ext::sprint::KEY_SPRINT),
            (KeyCode::ShiftRight, crate::ext::sprint::KEY_SPRINT),
            (KeyCode::ArrowUp, 38),
            (KeyCode::ArrowDown, 40),
            (KeyCode::ArrowLeft, 37),
            (KeyCode::ArrowRight, 39),
            (KeyCode::Enter, 13),
            (KeyCode::Backspace, 8),
            (KeyCode::Space, b' ' as usize),
            // The scene keys in the registry's own terms.
            (KeyCode::Digit1, crate::ext::scenes::SCENES[0].key as usize),
            (KeyCode::Quote, crate::ext::scenes::SCENES[crate::ext::scenes::INTRO].key as usize),
            (KeyCode::Semicolon, crate::ext::scenes::SCENES[15].key as usize),
        ];
        for (code, slot) in table {
            assert_eq!(key_index(code), Some(slot), "{code:?}");
            assert!(slot < 256);
        }
        // Every registered scene key is reached by exactly one physical key above.
        for entry in crate::ext::scenes::SCENES {
            let hits = table.iter().filter(|(_, s)| *s == entry.key as usize).count();
            assert!(hits >= 1, "scene key {:?} is not typeable", entry.key as char);
        }
    }

    #[test]
    fn keys_the_game_does_not_read_map_nowhere() {
        // Escape is deliberately absent: main.rs writes slot 27 itself so the menu can see
        // the release too. The rest would be silent slots nothing reads.
        for code in [
            KeyCode::Escape,
            KeyCode::KeyQ,
            KeyCode::KeyZ,
            KeyCode::Tab,
            KeyCode::ControlLeft,
            KeyCode::AltLeft,
            KeyCode::F1,
            KeyCode::Slash,
            KeyCode::Backquote,
            KeyCode::Numpad1,
        ] {
            assert_eq!(key_index(code), None, "{code:?}");
        }
    }

    #[test]
    fn end_frame_clears_presses_and_smooths_the_mouse() {
        let mut i = Input::new();
        i.key[b'W' as usize] = true;
        i.key_press[b'W' as usize] = true;
        i.set_mouse_button(0, true);
        i.add_mouse_motion(10.0, -4.0);
        i.end_frame();
        // Levels survive, edges do not.
        assert!(i.key[b'W' as usize] && !i.key_press[b'W' as usize]);
        assert!(i.mouse_button[0] && !i.mouse_button_press[0]);
        // The smoothed delta takes (1 - GH_MOUSE_SMOOTH) of the raw motion and the raw
        // accumulator is emptied.
        let k = 1.0 - GH_MOUSE_SMOOTH;
        assert!((i.mouse_dx - 10.0 * k).abs() < 1e-6 && (i.mouse_dy + 4.0 * k).abs() < 1e-6);
        assert!(i.mouse_ddx == 0.0 && i.mouse_ddy == 0.0);
        i.set_mouse_button(0, false);
        assert!(!i.mouse_button[0]);
    }

    /// EXT: both of winit's wheel shapes arrive as notches, they accumulate within a frame,
    /// and `end_frame` clears them like every other edge.
    #[test]
    fn the_wheel_accumulates_in_notches_and_is_an_edge() {
        use winit::dpi::PhysicalPosition;
        use winit::event::MouseScrollDelta;
        let mut i = Input::new();
        assert_eq!(i.wheel, 0.0);
        i.add_mouse_wheel(MouseScrollDelta::LineDelta(0.0, 1.0), 1.0);
        i.add_mouse_wheel(MouseScrollDelta::LineDelta(0.0, -3.0), 1.0);
        assert!((i.wheel + 2.0).abs() < 1e-6, "a mouse's lines are notches, whatever the display");
        // A trackpad's pixels: 40 logical ones are one notch, and its horizontal half is dropped.
        i.wheel = 0.0;
        i.add_mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(500.0, 20.0)), 1.0);
        assert!((i.wheel - 0.5).abs() < 1e-6);
        // The same gesture on a retina panel arrives twice as large and must spend the same.
        i.wheel = 0.0;
        i.add_mouse_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(500.0, 40.0)), 2.0);
        assert!((i.wheel - 0.5).abs() < 1e-6, "physical pixels, divided by the scale factor");
        i.end_frame();
        assert_eq!(i.wheel, 0.0, "an edge, not a level");
    }
}
