# eden-cheats-manager

A CLI tool that automates the tedious parts of managing cheats on the
[Eden](https://github.com/eden-emulator) Nintendo Switch emulator for Android.

**Before:** hunt through log files by hand, create 3 levels of nested folders
with exact hex names, fight Android's scoped storage file picker.

**After:** `eden-cheats-manager wizard` — done in 30 seconds.

## What it does

| Step | Manual | eden-cheats-manager |
|------|--------|-------------|
| Find your Build ID | Scroll through logs, guess which `name=main` line is right | `eden-cheats-manager extract --adb` |
| Create folder structure | File manager → create 3 nested folders one by one | Automatic |
| Name the `.txt` file | Must be exactly the 16-char Build ID — easy to typo | Automatic |
| Paste hex codes | Create file, paste, save, hope the format is right | Paste once in your editor |
| Push to phone | Android file picker, navigate scoped storage | `eden-cheats-manager push` via ADB (WiFi or USB) |

## Installation

### Prerequisites

- **Python 3.9+** (stdlib only, no pip packages needed)
- **ADB** (`android-tools`) — for push and extract-over-WiFi
- The [Eden emulator](https://github.com/eden-emulator) on your Android device
- USB debugging enabled on your phone (Settings → Developer Options)

### Quick install

```bash
# Arch 
sudo pacman -Syu android-tools

# Debian / Ubuntu
sudo apt install adb

# macOS
brew install android-platform-tools
```

```bash
# Install eden-cheats-manager itself
mkdir -p ~/.local/bin
curl -o ~/.local/bin/eden-cheats-manager https://raw.githubusercontent.com/ChrisA95G/eden-cheats-manager/main/eden-cheats-manager
chmod +x ~/.local/bin/eden-cheats-manager

# Add to PATH (add this line to your .bashrc / .zshrc / config.fish)
export PATH="$HOME/.local/bin:$PATH"
# Fish users:
fish_add_path ~/.local/bin
```

> **Tip:** `eden-cheats-manager` is a mouthful. Add this alias to your shell config:
> ```bash
> alias ecm='eden-cheats-manager'
> ```

### Connect your phone

**USB (first time):**
```bash
# Plug in via USB-C, then:
adb devices
# Tap "Always allow" on the phone prompt
```

**WiFi (after initial USB):**
```bash
adb tcpip 5555
adb connect <device_ip>:5555   # your phone's IP
adb devices                    # should show:  <device_ip>:5555  device
```

---

## Quick start

### The wizard (recommended)

```bash
eden-cheats-manager wizard
```

Walks you through every step interactively:
1. Enter Title ID (long-press game in Eden → Info → Program ID)
2. Pull Build ID from phone log, parse a local log, or enter manually
3. Name your cheat
4. Paste or edit hex codes
5. Push to device (optional)

### Step-by-step

```bash
# 1. Find your Build ID
eden-cheats-manager extract --adb              # from phone (WiFi/USB)
eden-cheats-manager extract --log eden.log     # from a local log file

# 2. Create the cheat
eden-cheats-manager create \
  -t 0100FF500E34A000 \                # Title ID
  -b 92C78BB3DCBBC3F7 \                # Build ID (from step 1)
  -n "Infinite HP" \                    # name shown in Eden
  -c "[Infinite HP]
04000000 00C88A70 3B9AC9FF"            # hex codes

# Or read codes from a file
eden-cheats-manager create -t ... -b ... -n "Speed 2x" -f cheat.txt
# Or pipe from clipboard
wl-paste | eden-cheats-manager create -t ... -b ... -n "Max Money" --stdin

# 3. Push to your phone
eden-cheats-manager push -t 0100FF500E34A000 -n "Infinite HP"

# 4. In Eden: long-press game → Add-ons → Install → Mods and cheats
#    Navigate to the cheat folder → select it → done
```

### List what you have

```bash
eden-cheats-manager list                       # local cheats
eden-cheats-manager list -t 0100FF500E34A000   # cheats for one game
eden-cheats-manager list --adb                 # cheats on connected device
```

---

## How it works

### The folder structure

Eden expects cheats at:

```
/Android/data/dev.eden.eden_emulator/files/load/
  └── <TitleID>/           ← 16-char hex (long-press → Info in Eden)
       └── <CheatName>/    ← anything you want ("Infinite HP")
            └── cheats/     ← must literally be "cheats"
                 └── <BuildID>.txt  ← 16-char hex, game version specific
```

Example:

```
/.../load/0100FF500E34A000/EPX4/cheats/92C78BB3DCBBC3F7.txt
```

### Where does the Build ID come from?

Every time a game launches, Eden writes a line like this to its log:

```
[  56.453463] Loader <Info> core/file_sys/patch_manager.cpp:456:HasNSOPatch:
  Querying NSO patch existence for build_id=92C78BB3DCBBC3F7A3CAF601D7B85F7A36C20907, name=main
```

`eden-cheats-manager extract` finds the `name=main` line and takes the **first 16 characters**
of the hash — that's your Build ID. Different game versions have different Build IDs,
so you need cheats matching your exact version.

### Why not just use Cheatslips' download button?

Cheatslips' downloads are packaged for Atmosphere CFW on real Switch hardware
(`atmosphere/contents/...`). Eden uses a completely different folder structure and
requires **one cheat per folder**. eden-cheats-manager bridges that gap.

---

## Command reference

### `extract` — Find Build IDs

```
eden-cheats-manager extract --adb          # pull log from phone
eden-cheats-manager extract --log PATH     # parse local log file
```

Outputs a table of all unique Build IDs found in the log, showing both the
16-char version (use this) and the full hash.

### `create` — Build cheat folder

```
eden-cheats-manager create -t TITLE_ID -b BUILD_ID -n NAME (-c CODE | -f FILE | --stdin)
```

| Flag | Description |
|------|-------------|
| `-t, --title-id` | Game Title ID (16 hex chars) |
| `-b, --build-id` | Build ID (16 hex chars, from `extract`) |
| `-n, --name` | Display name in Eden |
| `-c, --code` | Hex codes as a string |
| `-f, --file` | Read hex codes from a file |
| `--stdin` | Read hex codes from stdin |
| `-o, --output` | Custom output directory |
| `--no-push` | Don't show push hint after creation |

If no code source is given, opens `$EDITOR` (or nano) for you to type/paste.

### `push` — Send to device via ADB

```
eden-cheats-manager push -t TITLE_ID [-n NAME]
```

Pushes one cheat or all cheats for a game. Requires ADB device connected.

### `list` — Show installed cheats

```
eden-cheats-manager list [-t TITLE_ID] [--adb] [-l PATH]
```

### `wizard` — Interactive walkthrough

```
eden-cheats-manager wizard
```

Guides you through Title ID → Build ID → name → codes → push. Best way to start.

---

## Troubleshooting

### "Could not pull log" / no Build IDs found
- Make sure you've launched the game at least once in Eden (to generate a log entry).
- Check the **old** log too: `eden-cheats-manager extract --adb` checks both
  `eden_log.txt` and `eden_log.txt.old.txt` automatically.
- If scoped storage blocks ADB from reading the log, pull it manually:
  On your phone, use a file manager to copy
  `Android/data/dev.eden.eden_emulator/files/log/eden_log.txt` to `Downloads/`,
  then: `eden-cheats-manager extract --log ~/Downloads/eden_log.txt`

### Push fails with "Operation not permitted"
Android's scoped storage prevents `adb push` from creating directories inside
app-private storage. eden-cheats-manager works around this by creating the directory
via `adb shell mkdir -p` first. If you still hit this, make sure you're running
the latest version of the script.

### Cheat doesn't appear in Eden
- **Build ID mismatch** — #1 cause. Triple-check it matches your game version.
  Each game update changes the Build ID.
- Make sure you selected the **cheat folder** (the one containing `cheats/`),
  not the `cheats/` subdirectory itself, in Eden's file picker.
- File must be named exactly `<BuildID>.txt` (uppercase hex).

### Cheat shows but doesn't work
- The hex codes may be for a different game version. Even minor updates change
  memory addresses.
- Some cheats only work with specific emulator settings (CPU accuracy, etc.).
- Test on PC with Ryujinx first if you have access to one — it'll confirm
  whether the codes are valid before you spend time on Android.

---

## Files and storage

| What | Where |
|------|-------|
| Script | `~/.local/bin/eden-cheats-manager` |
| Local cheat storage | `~/.local/share/eden-cheats-manager/` |
| Device cheat storage | `/Android/data/dev.eden.eden_emulator/files/load/` |
| Eden logs (on device) | `/Android/data/dev.eden.eden_emulator/files/log/` |

---

## License

MIT © 2026

See [LICENSE](LICENSE) for full text.
