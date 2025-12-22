use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use enigo::{Enigo, Key, KeyboardControllable};
#[cfg(target_os = "linux")]
use evdev::{
    uinput::VirtualDeviceBuilder, AttributeSet, EventType as EvdevEventType, InputEvent,
    Key as EvdevKey,
};
use midir::{Ignore, MidiInput};
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    process::Command,
    sync::{Arc, Mutex, RwLock},
};

#[derive(Parser)]
#[command(name = "midi-actions")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the configuration file (default: config.toml)
    #[arg(short, long)]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    Setup,
}

#[derive(Deserialize, Debug, Clone)]
struct MidiConfig {
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
    Relative { 
        inc_cmd: String, 
        dec_cmd: String 
    },
}

const NOTE_ON: u8 = 0x90;
const CONTROL_CHANGE: u8 = 0xB0;

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Setup) => run_setup_mode(),
        None => run_daemon_mode(cli.config.as_deref()),
    }
}

// --- SETUP MODE ---
fn run_setup_mode() -> Result<()> {
    let mut midi_in = MidiInput::new("midi-actions-setup")?;
    midi_in.ignore(Ignore::None);

    let ports = midi_in.ports();
    if ports.is_empty() {
        return Err(anyhow!("No MIDI devices found."));
    }

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

    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

// --- DAEMON MODE ---
fn run_daemon_mode(config_path: Option<&str>) -> Result<()> {
    let config_path = config_path.unwrap_or("config.toml");

    // Load initial config
    let config_str =
        fs::read_to_string(&config_path).map_err(|_| anyhow!("{} not found!", config_path))?;
    let config: MidiConfig = toml::from_str(&config_str)?;

    // Create runtime mappings with u8 keys
    let runtime_mappings: Arc<RwLock<HashMap<u8, Action>>> = Arc::new(RwLock::new(
        config
            .mappings
            .into_iter()
            .filter_map(|(k, v)| k.parse::<u8>().ok().map(|id| (id, v)))
            .collect(),
    ));

    #[cfg(target_os = "linux")]
    // 1. Setup Virtual Keyboard
    let mut keys = AttributeSet::<EvdevKey>::new();
    for action in runtime_mappings.read().unwrap().values() {
        if let Action::Key { code } = action {
            if let Ok(k) = code.parse::<EvdevKey>() {
                keys.insert(k);
            }
        }
    }
    #[cfg(target_os = "linux")]
    let mut v_device = VirtualDeviceBuilder::new()?
        .name("midi-actions")
        .with_keys(&keys)?
        .build()?;

    // TODO: Setup PulseAudio context for native volume control

    // 2. Setup MIDI
    let mut midi_in = MidiInput::new("midi-actions-daemon")?;
    midi_in.ignore(Ignore::None);
    let port = midi_in
        .ports()
        .into_iter()
        .find(|p| {
            midi_in
                .port_name(p)
                .unwrap_or_default()
                .contains(&config.device_name)
        })
        .ok_or(anyhow!("Device '{}' not found", config.device_name))?;

    println!("✅ midi-actions Running on {}", midi_in.port_name(&port)?);

    let last_knob_vals = Arc::new(Mutex::new(HashMap::new()));
    let last_knob_directions = Arc::new(Mutex::new(HashMap::new()));

    // 3. Connect
    let _conn = midi_in
        .connect(
            &port,
            "midir-read",
            move |_, msg, _| {
                if msg.len() < 3 {
                    return;
                }

                let msg_type = msg[0] & 0xf0;
                let id = msg[1];
                let raw_val = msg[2];

                if (msg_type == NOTE_ON && raw_val > 0) || msg_type == CONTROL_CHANGE {
                    if let Some(action) = runtime_mappings.read().unwrap().get(&id) {
                        match action {
                            Action::Key { code } => {
                                #[cfg(target_os = "linux")]
                                {
                                    if let Ok(key) = code.parse::<EvdevKey>() {
                                        if let Err(e) = v_device.emit(&[
                                            InputEvent::new(EvdevEventType::KEY, key.code(), 1i32),
                                            InputEvent::new(EvdevEventType::KEY, key.code(), 0i32),
                                        ]) {
                                            eprintln!("Failed to emit key: {}", e);
                                        }
                                    }
                                }
                                #[cfg(any(target_os = "macos", target_os = "windows"))]
                                {
                                    if let Some(key) = string_to_enigo_key(code) {
                                        let mut enigo = Enigo::new();
                                        if let Err(e) = enigo.key_click(key) {
                                            eprintln!("Failed to simulate key: {}", e);
                                        }
                                    }
                                }
                            }
                            Action::Command { cmd } => {
                                if let Err(e) = Command::new("sh").arg("-c").arg(cmd).spawn() {
                                    eprintln!("Failed to spawn command: {}", e);
                                }
                            }
                            Action::Linear { template } => {
                                let mut cache = last_knob_vals.lock().unwrap();
                                let percent = (raw_val as f32 / 127.0 * 100.0) as u32;

                                if cache.get(&id) != Some(&percent) {
                                    let final_cmd = template.replace("{}", &percent.to_string());
                                    if let Err(e) =
                                        Command::new("sh").arg("-c").arg(final_cmd).spawn()
                                    {
                                        eprintln!("Failed to spawn volume command: {}", e);
                                    }
                                    cache.insert(id, percent);
                                }
                            }
                            Action::Relative { inc_cmd, dec_cmd } => {
                                let mut cache = last_knob_vals.lock().unwrap();
                                let mut directions = last_knob_directions.lock().unwrap();
                                
                                // Get previous value, default to current if not found
                                let prev_val = *cache.get(&id).unwrap_or(&raw_val);
                                cache.insert(id, raw_val);
                                
                                // Determine direction based on value change
                                if raw_val > prev_val {
                                    // Knob turned right/increased
                                    if let Err(e) = Command::new("sh").arg("-c").arg(inc_cmd).spawn() {
                                        eprintln!("Failed to spawn relative increment command: {}", e);
                                    }
                                } else if raw_val < prev_val {
                                    // Knob turned left/decreased
                                    if let Err(e) = Command::new("sh").arg("-c").arg(dec_cmd).spawn() {
                                        eprintln!("Failed to spawn relative decrement command: {}", e);
                                    }
                                }
                                // If raw_val == prev_val, no action needed
                            }
                        }
                    }
                }
            },
            (),
        )
        .map_err(|e| anyhow!("Connection failed: {}", e))?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn string_to_enigo_key(s: &str) -> Option<Key> {
    match s {
        "KEY_F13" => Some(Key::F13),
        "KEY_F14" => Some(Key::F14),
        // Add more as needed
        _ => None,
    }
}
