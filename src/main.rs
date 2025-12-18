use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use evdev::{uinput::VirtualDeviceBuilder, AttributeSet, Key, InputEvent, EventType};
use midir::{MidiInput, Ignore};
use serde::Deserialize;
use std::{collections::HashMap, process::Command, fs, sync::{Arc, Mutex}};

#[derive(Parser)]
#[command(name = "midi-hub")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Setup,
}

#[derive(Deserialize, Debug, Clone)]
struct Config {
    device_name: String,
    // Keys in TOML are always strings
    mappings: HashMap<String, Action>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum Action {
    Key { code: String },
    Command { cmd: String },
    Linear { template: String },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Setup) => run_setup_mode(),
        None => run_daemon_mode(),
    }
}

// --- SETUP MODE ---
fn run_setup_mode() -> Result<()> {
    let mut midi_in = MidiInput::new("midi-hub-setup")?;
    midi_in.ignore(Ignore::None);

    let ports = midi_in.ports();
    if ports.is_empty() { return Err(anyhow!("No MIDI devices found.")); }

    println!("\n🎹 DISCOVERY MODE");
    let port = &ports[ports.len() - 1]; 
    println!("Listening to '{}'...", midi_in.port_name(port)?);
    println!("(Press Ctrl+C to stop)\n");

    let _conn = midi_in.connect(port, "midir-setup", move |_stamp, msg, _| {
        if msg.len() < 3 { return; }
        
        let msg_type = msg[0] & 0xf0; 
        let id = msg[1];
        let val = msg[2];

        // Debug output
        println!("RAW: [{}, {}, {}] -> Type: {:#x}", msg[0], id, val, msg_type);

        if msg_type == 0xB0 {
             println!("# Knob Detected (ID: {})", id);
             println!("\"{}\" = {{ type = \"Linear\", template = \"pactl set-sink-volume @DEFAULT_SINK@ {{}}%\" }}\n", id);
        } 
        else if msg_type == 0x90 && val > 0 {
             println!("# Button Detected (ID: {})", id);
             println!("\"{}\" = {{ type = \"Key\", code = \"KEY_F13\" }}\n", id);
        }
    }, ()).map_err(|e| anyhow!("Connection failed: {}", e))?;

    loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
}

// --- DAEMON MODE ---
fn run_daemon_mode() -> Result<()> {
    let config_str = fs::read_to_string("config.toml").map_err(|_| anyhow!("config.toml not found!"))?;
    let config: Config = toml::from_str(&config_str)?;

    // 1. Setup Virtual Keyboard
    let mut keys = AttributeSet::<Key>::new();
    for action in config.mappings.values() {
        if let Action::Key { code } = action {
            if let Ok(k) = code.parse::<Key>() { keys.insert(k); }
        }
    }
    let mut v_device = VirtualDeviceBuilder::new()?.name("MIDI Hub").with_keys(&keys)?.build()?;

    // 2. Setup MIDI
    let mut midi_in = MidiInput::new("midi-hub-daemon")?;
    midi_in.ignore(Ignore::None);
    let port = midi_in.ports().into_iter()
        .find(|p| midi_in.port_name(p).unwrap_or_default().contains(&config.device_name))
        .ok_or(anyhow!("Device '{}' not found", config.device_name))?;

    println!("✅ MIDI Hub Running on {}", midi_in.port_name(&port)?);

    let last_knob_vals = Arc::new(Mutex::new(HashMap::new()));

    // 3. Connect
    let _conn = midi_in.connect(&port, "midir-read", move |_, msg, _| {
        if msg.len() < 3 { return; }
        
        let msg_type = msg[0] & 0xf0;
        let id = msg[1];
        let raw_val = msg[2];

        if (msg_type == 0x90 && raw_val > 0) || msg_type == 0xB0 {
            // FIX: Convert the MIDI ID (u8) to String for lookup
            if let Some(action) = config.mappings.get(&id.to_string()) {
                match action {
                    Action::Key { code } => {
                        if let Ok(key) = code.parse::<Key>() {
                            let _ = v_device.emit(&[
                                InputEvent::new(EventType::KEY, key.code(), 1i32),
                                InputEvent::new(EventType::KEY, key.code(), 0i32)
                            ]);
                        }
                    },
                    Action::Command { cmd } => {
                        let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
                    },
                    Action::Linear { template } => {
                        let mut cache = last_knob_vals.lock().unwrap();
                        let percent = (raw_val as f32 / 127.0 * 100.0) as u32;
                        
                        if cache.get(&id) != Some(&percent) {
                            let final_cmd = template.replace("{}", &percent.to_string());
                            let _ = Command::new("sh").arg("-c").arg(final_cmd).spawn();
                            cache.insert(id, percent);
                        }
                    }
                }
            }
        }
    }, ()).map_err(|e| anyhow!("Connection failed: {}", e))?;

    loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
}
