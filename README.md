# Eden Cheats Manager

EDC lets you browse and manage Switch cheats on Android, Windows and Linux. It can install cheats directly into Eden or, on Android, prepare files for an emulator that imports them.

[Downloads](https://github.com/ChrisA95G/eden-cheats-manager/releases) ·
[Setup](#quick-start) ·
[Report a bug](https://github.com/ChrisA95G/eden-cheats-manager/issues/new)

## Features

- Scan NSP, XCI, and NSZ/XCZ games and updates into one library.
- Browse cheats by game and version.
- Use the included cheat catalog, add your own codes, or fetch cheats from Cheatslips.
- Manage cheat files in your chosen install folder.
- Adapts to desktop and handheld screens.

## Downloads

Choose the file for your platform from the [releases page](https://github.com/ChrisA95G/eden-cheats-manager/releases).

| Platform | Download |
| --- | --- |
| Android | `eden-cheats-manager.apk` |
| Windows x86-64 | `x64-setup.exe` or `.msi` |
| Linux x86-64 | `.AppImage` |

On Linux, make the AppImage executable before opening it.

## What you need

- Access to your emulator’s `load` folder for direct installs, or a writable Android folder for imports.
- Your game files and any updates you use.
- Your `prod.keys` file to read the game files.

A Cheatslips API token is only needed to fetch online cheats. Add it in Settings
if you want to use that source.

## Quick start

1. On Windows or Linux, select your Eden installation’s `load` folder. On Android,
   tap **Choose folder**, select a directory, and confirm with **Use this folder**.
   Choose **Install directly** for the emulator’s existing `load` folder or
   **Prepare for import** to keep files in a separate folder. EDC uses that
   folder’s `load` subfolder, creating it if needed.
2. In **Settings → Game library**, select `prod.keys` and the folder containing
   your game files, then save.
3. Select a game and choose the version you play.
4. Choose a cheat and select **Install**. For a direct install, enable the cheat
   in your emulator. For an import, open the emulator’s importer and select
   `load/<title ID>` for that game. The import guide is available in
   **Settings → Cheat install folder**.

## Help and license

Found a problem? [Open an issue](https://github.com/ChrisA95G/eden-cheats-manager/issues/new).

Code licensed under [MIT](LICENSE). See [third-party notices](THIRD_PARTY_NOTICES.md).
