# midi-actions

A Rust application that transforms your Akai MPD218 MIDI controller into additional knobs and buttons for desktop automation and OBS control.

## Features

- **Knob Mapping**: Map rotary knobs to linear controls like system volume or microphone levels
- **Pad Mapping**: Assign pads to keyboard shortcuts or shell commands
- **OBS Integration**: Control OBS streaming and scene switching via keyboard shortcuts
- **Virtual Keyboard**: Emulates key presses using Linux evdev
- **Discovery Mode**: Interactive setup to identify control IDs

## Installation

This project uses Nix for dependency management. Ensure you have Nix installed.

### Development

```bash
# Enter the development shell
nix develop

# Build the project
cargo build --release
```

### NixOS System Installation

Add the flake to your NixOS configuration:

```nix
{
  inputs.midi-actions.url = "path/to/your/flake"; # or github repo

  outputs = { nixpkgs, midi-actions, ... }: {
    nixosConfigurations.yourhost = nixpkgs.lib.nixosSystem {
      modules = [
        midi-actions.nixosModules.default
        {
          services.midi-actions.enable = true;
          services.midi-actions.user = "yourusername";
        }
        # ... other modules
      ];
    };
  };
}
```

This sets up the necessary permissions for uinput access.

### Standalone Installation

```bash
# Install to user profile
nix profile install .#default
```

## Usage

### 1. Setup Mode

First, connect your Akai MPD218 and run discovery mode to identify control IDs:

```bash
./target/release/midi-actions setup
```

Press knobs and pads to see their IDs and suggested mappings. Use Ctrl+C to exit.

### 2. Configuration

Edit `config.toml` to define your mappings. Example:

```toml
device_name = "MPD218"

[mappings]
# Knob 1 -> Master Volume
3 = { type = "Linear", template = "pactl set-sink-volume @DEFAULT_SINK@ {}%" }

# Knob 2 -> Microphone Volume
9 = { type = "Linear", template = "pactl set-source-volume @DEFAULT_SOURCE@ {}%" }

# Pad 1 -> OBS Start Streaming (mapped to F13)
36 = { type = "Key", code = "KEY_F13" }

# Pad 2 -> Launch Firefox
37 = { type = "Command", cmd = "firefox" }
```

### 3. Daemon Mode

Run the daemon to start listening for MIDI events:

```bash
./target/release/midi-actions
```

The application will connect to your configured device and execute actions based on the mappings.

## Configuration

- `device_name`: Partial name of your MIDI device (must match output from setup mode)
- `mappings`: HashMap of MIDI control IDs to actions
  - `Linear`: For knobs, uses a template string with `{}` replaced by percentage (0-100)
  - `Key`: Emulates a keyboard key press (uses evdev Key codes)
  - `Command`: Executes a shell command

## Requirements

- Linux with evdev support
- PulseAudio (for volume controls)
- OBS Studio (for streaming control)

## License

MIT</content>
<parameter name="filePath">README.md