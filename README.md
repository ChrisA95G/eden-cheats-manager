# Eden Cheats Manager

Manage cheats for Eden on desktop and Android. Scan your NSP/XCI game library,
choose a base game or update, and install cheats for its detected Build ID.

**0.2.0-rc.1 is in preparation.** This branch documents the rewritten app, not
the older 0.1.x releases. RC builds are for testing; back up your settings and
cheats first.

[Downloads](https://github.com/ChrisA95G/eden-cheats-manager/releases) ·
[Setup](#quick-start) ·
[Report a bug](https://github.com/ChrisA95G/eden-cheats-manager/issues/new/choose)

## What it does

- Builds your library from scanned **NSP/XCI base games and updates**.
- Extracts package Build IDs locally and lets you choose a version within each game.
- Shows whether the game has a corresponding entry in Eden's `load` directory.
- Browses the bundled cheat catalog, supports custom codes, and optionally fetches
  more entries from Cheatslips.
- Installs individual cheat files, lists existing files, and removes them separately
  from catalog entries.
- Adapts to desktop and handheld screens.
## Downloads

Use the assets attached to the release you intend to test.

| Platform | Asset to choose |
| --- | --- |
| Linux x86-64 | `.AppImage` or `.deb` |
| Android | `eden-cheats-manager.apk` |
| Windows x86-64 | `x64-setup.exe` or `.msi` |

Tested on Linux and Android (AYN Thor); Windows testing is still pending.

## Before you start

You need:

- Eden installed on the same computer or Android device.
- Access to Eden's `load` directory for presence checks and cheat files.
- Optionally, a Cheatslips API token for online fetching.

ECM does not supply games or keys. Compressed NSZ/XCZ files are not supported.

## Quick start

### Desktop

1. Complete setup with Eden's `load` directory.
2. Select `prod.keys` and your game-package folder during setup or in **Settings →
   Game library**, then save.
3. Let the library scan finish. Select a game, then choose **Game version**.
4. Expand the matching cheat group and select **Install** beside a cheat.
5. Enable and verify the cheat in Eden. ECM installs files; it does not enable them.

### Android

1. Open Eden once, then open ECM.
2. In the system picker, choose the **Eden** provider, open **load**, and grant access.
3. In ECM Settings, select `prod.keys` and your package-library folder using the
   document pickers. Use local storage that supports seekable file access.
4. Open a scanned game, choose its base game or update, and install a matching cheat.

## Understand the two libraries

**The package folder tells ECM which games and versions to show.**
**Eden's `load` directory tells ECM where cheat files belong and supplies its
presence signal.** They are separate locations with separate purposes.

“Present in Eden” means ECM found a corresponding `load/<TitleID>` entry. It does
not prove a package is installed, that Eden is running a particular update, or
that a cheat is enabled. A fresh game may not have a load entry yet; a stale entry
may remain after removal. Installation is disabled when no matching entry is found.

Choosing **Game version** filters cheats by the selected package's Build ID. It
does not switch Eden's version. **All builds** is for browsing and does not establish
compatibility. A matching Build ID is necessary evidence, not a guarantee a code works.

## Online and custom cheats

The bundled catalog is available without an API token. **Connect source** opens
Settings so you can add a Cheatslips token; **Fetch online** then requests cheats
for the selected title. Downloaded entries are kept locally. Request limits depend
on Cheatslips; repeatedly fetching can use your allowance.

**Custom cheat** saves your Build ID and named code sections to the local catalog.
Install those sections separately. The **Installed** tab manages files in Eden;
deleting a catalog entry does not delete its installed files, and clearing downloaded
entries preserves custom entries and installed files.

## Help and project information

- [Report a bug](https://github.com/ChrisA95G/eden-cheats-manager/issues/new)
- [Third-party notices](THIRD_PARTY_NOTICES.md)

MIT © 2026 — see [LICENSE](LICENSE). The code licence does not automatically cover
third-party databases, artwork or cheat content. This is an independent project,
not an official Eden or Nintendo product.
