# Managing a game

The **Manage** tab on a game's page gathers everything that changes how a game is installed or opened.

## Launch

| Option | What it does |
|---|---|
| **Launch program** | Shows which program Cerberus starts |
| **Tracked process** | Picks which program counts as playing, for games that open a launcher first |
| **Launch options** | Extra command-line options passed to the game. Use quotes for values with spaces |
| **History** | The game's recent events (sessions, installs, backups, errors) and its last failure |

## Files

| Option | What it does |
|---|---|
| **Open game folder** | Opens the install folder |
| **Move to another folder or drive** | Moves the whole recomp folder, with saves, mods and settings, and opens it from there |
| **Repair installation** | Downloads the current version again over the installed one. Saves, settings and mods stay, and saves are backed up first |
| **Uninstall** / **Remove from library** | Uninstall deletes the folder of a recomp Cerberus installed. Remove from library leaves the files alone. Save backups are kept either way |

## Shortcuts and Steam

- **Create a desktop shortcut** makes a shortcut that opens the game through Cerberus, so play time, backups and filters keep working.
- **Add to Steam** puts a non-Steam game in your Steam library, with its cover and background, opening through Cerberus. That's useful for Steam Deck and Steam's Big Picture. To add every game at once, use **Settings → Steam → Add all**.
  - Close Steam first, because it rewrites its game list when it closes.
  - Cerberus backs up Steam's `shortcuts.vdf` before changing it.
  - **Remove from Steam** undoes it.

## Art

- **Customize art** picks a cover, background and logo from SteamGridDB or a file of your own.
- **Edit name and cover** renames a game you added by hand.

## DLSS

When a game ships NVIDIA DLSS files, the **DLSS** card lists them with their version:

- `nvngx_dlss.dll` for Super Resolution.
- `nvngx_dlssg.dll` for Frame Generation.
- `nvngx_dlssd.dll` for Ray Reconstruction.

**Swap version…** replaces one with a DLL of the same name that you already have, from another game or downloaded yourself. **Cerberus downloads nothing.** It checks that the file is an NVIDIA DLL with a version, and keeps the game's own file first. **Restore the original** puts it back.

- A game update or a Steam file check may bring the game's own file back. When that happens, Cerberus drops its kept copy.
- Avoid swapping DLSS in online games with anti-cheat.

## Linking to the catalog

A game you added by hand that's actually a recomp can be linked to its catalog entry. Cerberus suggests it when the program name matches. Once linked, it gets the catalog's settings, mods, save backups and art, and its play time carries over.

## See also

- [Library](library.md)
- [Saves](saves.md)
