# Eden Cheats Manager

A desktop app for managing cheat codes on the [Eden](https://github.com/eden-emulator) Nintendo Switch emulator. Supports both Android devices (via ADB) and PC installs.

**Before:** hunt through log files by hand, create 3 levels of nested folders with exact hex names, push files over ADB one at a time.

**After:** open the app, pick a game, click Install.

## Features

- Browse your installed Switch library (Android or PC)
- Search a bundled offline cheat database (sourced from cheatslips.com)
- **Fetch from Cheatslips API** — pull the latest cheats for any game on demand using your own API key (3 requests/day on the free tier)
- Auto-detect Build IDs from Eden's log files
- **Scan Build ID** — launch the game via ADB and capture the Build ID automatically
- Install and delete individual cheats to your device or PC load directory
- Create and save your own custom cheats
- Clear API-fetched cheats per game to start fresh
- USB and wireless ADB device management

## Installation

Download the latest release for your platform from the [Releases](../../releases) page.

### Build from source

**Prerequisites:** [Rust](https://rustup.rs), [Node.js](https://nodejs.org), [ADB](https://developer.android.com/tools/adb) (for Android mode)

```bash
git clone https://github.com/ChrisA95G/eden-cheats-manager
cd eden-cheats-manager/gui
npm install
npm run tauri build
```

## Quick start

1. **First launch** — the setup wizard asks for your target mode:
   - **Android** — point to your ADB binary and connect a device
   - **PC** — point to your Eden load directory

2. **Scan your library** — click *Scan Games* in the sidebar. Your installed titles appear grouped by base game, update, and DLC.

3. **Pick a game** — the right panel loads available cheats from the local database and detects your Build ID from Eden's logs automatically.

4. **Install a cheat** — expand the matching Build ID row and click *Install* next to any cheat section.

### Fetching cheats from Cheatslips

The bundled database covers many games but is static. To get the latest cheats:

1. Sign up at [cheatslips.com](https://www.cheatslips.com) and get your API token
2. Paste it in **Settings → Cheatslips API Token**
3. Select a game and click **↓ API** in the Available Cheats header

Fetched cheats are cached in your local database — subsequent opens load instantly without using a request. The free tier allows **3 requests per day**.

To remove API-fetched cheats for a game (e.g. to re-fetch after a bad result), click **✕ API** next to the fetch button.

### Custom cheats

If you have cheat codes that aren't in the database, click **+ Custom** in the Available Cheats header. Enter the Build ID (pre-filled if detected) and paste your cheat content in Eden's format:

```
[Cheat Name]
04000000 00C88A70 3B9AC9FF
```

Custom cheats are saved to the local database and appear alongside bundled cheats.

### Scan Build ID (Android)

If the Build ID isn't detected from existing logs, use **Scan Build ID**. The app launches the game via ADB, waits for Eden to write the Build ID to its log, then force-stops the game. Make sure the device screen is unlocked.

## How cheats work

### Folder structure

Eden loads cheats from:

```
<load_dir>/
  └── <TitleID>/        ← 16-char hex  (long-press game in Eden → Info → Program ID)
       └── <CheatName>/ ← any name you choose
            └── cheats/
                 └── <BuildID>.txt   ← 16-char hex, version-specific
```

On Android the load directory is:
```
/Android/data/dev.eden.eden_emulator/files/load/
```

### What is a Build ID?

When a game launches, Eden writes a line like this to its log:

```
Querying NSO patch existence for build_id=92C78BB3DCBBC3F7A3CAF601D7B85F7A36C20907, name=main
```

The **first 16 characters** (`92C78BB3DCBBC3F7`) are the Build ID. Cheats are tied to a specific game version — a game update changes the Build ID and existing cheats stop working until you find codes for the new version.

## Troubleshooting

**Build ID not detected**
Launch the game in Eden at least once to generate a log entry, then use *Detect Build IDs* or *Scan Build ID*.

**Scan Build ID fails**
- Screen must be unlocked
- Make sure the game is listed in one of Eden's configured ROM directories
- If the game is on an SD card, Eden must have SAF access to that directory

**Cheat doesn't appear in Eden**
- Confirm the Build ID matches your exact game version — updates change it
- In Eden: long-press game → Add-ons → Install → navigate to the cheat folder (the folder *containing* `cheats/`, not `cheats/` itself)

**Cheat shows but doesn't work**
- The codes may target a different game version — memory addresses change with updates
- Check whether the cheat requires specific emulator settings (CPU accuracy, etc.)

**↓ API fetch returns far more cheats than expected**
This is normal — Cheatslips stores one entry per contributor per build ID, and some contributors bundle many individual cheats into a single submission. Use **✕ API** to clear and re-fetch if needed.

## File locations

| What | Where |
|------|-------|
| App data (cached DBs, settings) | Platform app data directory |
| Android cheat storage | `/Android/data/dev.eden.eden_emulator/files/load/` |
| Eden logs (Android) | `/Android/data/dev.eden.eden_emulator/files/log/` |

## License

MIT © 2026 — see [LICENSE](LICENSE)
