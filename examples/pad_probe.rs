//! EXT dev probe: what does gilrs actually see?
//!
//! `src/ext/gamepad.rs` prints one line per connected pad at startup and then goes quiet, which
//! is no help at all when the answer is "none" and a controller is plainly paired. This dumps
//! the whole picture -- backend errors, every gamepad gilrs knows about whether or not it counts
//! as connected, and then a live stream of events and axis values.
//!
//! Run: cargo run --release --example pad_probe

fn main() {
    let mut gilrs = match gilrs::Gilrs::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Gilrs::new() failed: {e}");
            eprintln!(
                "On macOS this is usually the IOHIDManager being refused: check \
                 System Settings > Privacy & Security > Input Monitoring for the terminal."
            );
            std::process::exit(1);
        }
    };

    // `gamepads()` yields only CONNECTED pads; the id sweep below also shows remembered ones,
    // which is how you tell "paired but asleep" from "gilrs cannot see it at all".
    println!("connected gamepads: {}", gilrs.gamepads().count());
    for (id, pad) in gilrs.gamepads() {
        println!(
            "  [{id}] {:?}  uuid={:?}  power={:?}  mapped={}",
            pad.name(),
            uuid_hex(pad.uuid()),
            pad.power_info(),
            pad.mapping_source() != gilrs::MappingSource::None,
        );
    }

    println!("\nlistening for events -- press buttons, move sticks (ctrl-c to stop)");
    let mut ticks = 0u32;
    let mut described = false;
    loop {
        while let Some(gilrs::Event { id, event, .. }) = gilrs.next_event() {
            println!("  event [{id}] {event:?}");
        }
        // A pad usually only shows up after its Connected event has been drained, so the
        // interesting details have to be printed here rather than before the loop.
        if !described && gilrs.gamepads().count() > 0 {
            described = true;
            for (id, pad) in gilrs.gamepads() {
                println!(
                    "\n  [{id}] {:?} os={:?} mapping={:?} power={:?}",
                    pad.name(),
                    pad.os_name(),
                    pad.mapping_source(),
                    pad.power_info()
                );
                // Whether the SDL database actually bound each control. An unmapped pad reports
                // None here and every is_pressed(Button::X) below is dead.
                for b in [
                    gilrs::Button::South, gilrs::Button::East, gilrs::Button::North,
                    gilrs::Button::West, gilrs::Button::LeftTrigger, gilrs::Button::RightTrigger,
                    gilrs::Button::LeftTrigger2, gilrs::Button::RightTrigger2,
                    gilrs::Button::Select, gilrs::Button::Start, gilrs::Button::Mode,
                    gilrs::Button::DPadUp, gilrs::Button::DPadDown,
                    gilrs::Button::DPadLeft, gilrs::Button::DPadRight,
                ] {
                    println!("      button {b:?} -> {:?}", pad.button_code(b));
                }
                for a in [
                    gilrs::Axis::LeftStickX, gilrs::Axis::LeftStickY,
                    gilrs::Axis::RightStickX, gilrs::Axis::RightStickY,
                ] {
                    println!("      axis   {a:?} -> {:?}", pad.axis_code(a));
                }
                println!();
            }
        }
        // Poll the cached state too: a pad that raises no events but reports live axes means the
        // event queue is the broken half, not the connection.
        for (id, pad) in gilrs.gamepads() {
            let axes = [
                gilrs::Axis::LeftStickX,
                gilrs::Axis::LeftStickY,
                gilrs::Axis::RightStickX,
                gilrs::Axis::RightStickY,
            ];
            let vals: Vec<String> = axes
                .iter()
                .map(|a| format!("{:+.2}", pad.value(*a)))
                .collect();
            let held: Vec<String> = ALL_BUTTONS
                .iter()
                .filter(|b| pad.is_pressed(**b))
                .map(|b| format!("{b:?}"))
                .collect();
            if ticks % 60 == 0 || !held.is_empty() {
                println!(
                    "  state [{id}] {} axes={} held=[{}]",
                    pad.name(),
                    vals.join(" "),
                    held.join(" ")
                );
            }
        }
        ticks = ticks.wrapping_add(1);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}

const ALL_BUTTONS: [gilrs::Button; 15] = [
    gilrs::Button::South, gilrs::Button::East, gilrs::Button::North, gilrs::Button::West,
    gilrs::Button::LeftTrigger, gilrs::Button::RightTrigger,
    gilrs::Button::LeftTrigger2, gilrs::Button::RightTrigger2,
    gilrs::Button::Select, gilrs::Button::Start, gilrs::Button::Mode,
    gilrs::Button::DPadUp, gilrs::Button::DPadDown,
    gilrs::Button::DPadLeft, gilrs::Button::DPadRight,
];

fn uuid_hex(u: [u8; 16]) -> String {
    u.iter().map(|b| format!("{b:02x}")).collect()
}
