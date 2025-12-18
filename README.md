# midi-actions

A Rust application that transforms your Akai MPD218 MIDI controller into additional knobs and buttons for desktop automation and OBS control.

## Features

- **Knob Mapping**: Map rotary knobs to linear controls like system volume or microphone levels
- **Pad Mapping**: Assign pads to keyboard shortcuts or shell commands
- **OBS Integration**: Control OBS streaming and scene switching via keyboard shortcuts
- **Virtual Keyboard**: Emulates key presses using Linux evdev
- **Discovery Mode**: Interactive setup to identify control IDs

## Installation

This project uses Nix for dependency management on Linux. For macOS/Windows, use Cargo directly.

### Linux (Nix)

```bash
# Enter the development shell
nix develop

# Build the project
cargo build --release
```

#### NixOS System Installation

For a basic setup, add the flake to your NixOS configuration:

```nix
{
  inputs.midi-actions.url = "github:shift/midi-actions";

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

For a more integrated setup with configuration in Nix, use this custom module wrapper:

```nix
# midi-actions-wrapper.nix
{ config, lib, pkgs, ... }:

let
  cfg = config.services.midi-actions-custom;

  midiActionsPkg = inputs.midi-actions.packages.${pkgs.system}.default;

  configFile = pkgs.writeText "midi-actions-config.toml" ''
    device_name = "${cfg.deviceName}"
    
    [mappings]
    ${lib.concatStringsSep "\n" (lib.mapAttrsToList (id: action: 
      if action.type == "Linear" then 
        "\"${id}\" = { type = \"Linear\", template = \"${action.value}\" }"
      else if action.type == "Key" then
        "\"${id}\" = { type = \"Key\", code = \"${action.value}\" }"
      else
        "\"${id}\" = { type = \"Command\", cmd = \"${action.value}\" }"
    ) cfg.mappings)}
  '';

in {
  options.services.midi-actions-custom = {
    enable = lib.mkEnableOption "Custom MIDI Actions Service";
    deviceName = lib.mkOption { type = lib.types.str; default = "MPD218"; };
    mappings = lib.mkOption {
      type = lib.types.attrsOf (lib.types.submodule {
        options = {
          type = lib.mkOption { type = lib.types.enum [ "Linear" "Key" "Command" ]; };
          value = lib.mkOption { type = lib.types.str; };
        };
      });
      description = "Map MIDI IDs (strings) to actions.";
      example = {
        "3" = { type = "Linear"; value = "pactl set-sink-volume @DEFAULT_SINK@ {}%"; };
        "36" = { type = "Key"; value = "KEY_F13"; };
      };
    };
  };

  config = lib.mkIf cfg.enable {
    hardware.uinput.enable = true;
    services.udev.extraRules = ''
      KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput"
    '';
    users.users.${config.users.users.yourusername.name}.extraGroups = [ "uinput" "input" ];

    systemd.user.services.midi-actions = {
      description = "Midi Actions Daemon";
      wantedBy = [ "graphical-session.target" ];
      serviceConfig = {
        ExecStart = pkgs.writeShellScript "start-midi-actions" ''
          ln -sf ${configFile} ./config.toml
          ${midiActionsPkg}/bin/midi-actions
        '';
      };
    };
  };
}
```

Then configure:

```nix
services.midi-actions-custom = {
  enable = true;
  deviceName = "MPD218";
  mappings = {
    "3" = { type = "Linear"; value = "pactl set-sink-volume @DEFAULT_SINK@ {}%"; };
    "36" = { type = "Key"; value = "KEY_F13"; };
  };
};
```

#### Standalone Installation

```bash
# Install to user profile
nix profile install .#default
```

### macOS / Windows

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/shift/midi-actions.git
cd midi-actions
cargo build --release --features macos  # or windows
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
./midi-actions [--config path/to/config.toml]
```

Options:
- `--config` or `-c`: Path to the configuration file (default: config.toml)

The application will connect to your configured device and execute actions based on the mappings.

## Using Other MIDI Devices

This tool works with any MIDI controller, not just the Akai MPD218. To use a different device:

1. **Connect your device** and ensure it's recognized by your system.

2. **Run setup mode** to discover control IDs:
   ```bash
   ./midi-actions setup
   ```
   Press buttons, knobs, and pads on your device. The tool will output suggested mappings for each control.

3. **Edit `config.toml`**:
   - Change `device_name` to a unique part of your device's name (check the setup output).
   - Add mappings under `[mappings]` using the IDs from setup mode.
   - Example for a different device:
     ```toml
     device_name = "YourDevice"
     [mappings]
     1 = { type = "Linear", template = "pactl set-sink-volume @DEFAULT_SINK@ {}%" }
     64 = { type = "Key", code = "KEY_F13" }
     ```

4. **Run the daemon**:
   ```bash
   ./midi-actions
   ```

Note: Ensure your device sends MIDI messages in the expected format (Note On/Off for pads, Control Change for knobs).

## Configuration

- `device_name`: Partial name of your MIDI device (must match output from setup mode)
- `mappings`: HashMap of MIDI control IDs to actions
  - `Linear`: For knobs, uses a template string with `{}` replaced by percentage (0-100)
  - `Key`: Emulates a keyboard key press (uses evdev Key codes)
  - `Command`: Executes a shell command

## Requirements

- Linux: evdev support, PulseAudio (for volume controls), OBS Studio (for streaming control)
- macOS: OBS Studio (for streaming control) - **Untested** (developer lacks hardware)
- Windows: OBS Studio (for streaming control) - **Untested** (developer lacks hardware)

## License

MIT</content>
<parameter name="filePath">README.md