# Classics

Play the ROMs and disc images in your folders through [RetroArch](https://www.retroarch.com), with achievements from RetroAchievements.

## Add your games

1. Go to **Settings → Library and ROMs** and add the folders where you keep ROMs and disc images.
2. Open **Library → Classics**. Each game shows its console, its box art from libretro-thumbnails, and a **Has a recomp** badge when a native PC version exists.

A classic's page shows play time, last played, console, region and the game's files.

## RetroArch

Go to **Settings → Emulation**:

- **Install** downloads the official RetroArch build from the libretro website, about 200 MB, into a folder of Cerberus's own.
- **Use mine** points Cerberus to a RetroArch you already have.

### Cores

Each console uses one RetroArch core, downloaded from the libretro website when you install it:

| Consoles | Core |
|---|---|
| NES | fceumm |
| Super Nintendo | snes9x |
| Nintendo 64 | mupen64plus_next |
| Game Boy, Game Boy Color | gambatte |
| Game Boy Advance | mgba |
| Mega Drive, Master System, Game Gear | genesis_plus_gx |
| PlayStation | pcsx_rearmed |
| PSP | ppsspp |
| GameCube, Wii | dolphin |
| PlayStation 2 | pcsx2 |

### BIOS

The BIOS is the console's built-in program. **Cerberus never downloads BIOS files.** Point it to the folder where you keep yours with **Add BIOS from a folder**.

- **PlayStation 2** needs the BIOS from your own console. These are 4 MB files like `SCPH-70012.bin`.
- **PlayStation** runs without one, but is more compatible with it. It's a 512 KB file like `scph5501.bin`.

## Standalone emulators

Besides RetroArch, **Settings → Emulation → Emulators** has six standalone emulators, each installed from its own project's official release (the ones on GitHub come with their SHA-256 checked) into Cerberus's folder:

| Emulator | Plays |
|---|---|
| DuckStation | PlayStation |
| PCSX2 | PlayStation 2 |
| RPCS3 | PlayStation 3 |
| PPSSPP | PSP |
| Dolphin | GameCube and Wii |
| Cemu | Wii U |

- **Install** downloads it; **I have it** uses the one already on your PC (Cerberus also finds them in the usual install folders on its own). **Open** starts the emulator by itself, to set its controls, video or BIOS folder. **Remove** deletes the copy Cerberus installed.
- They never come with BIOS or firmware: those come from your own console.
- **Who opens each console** picks, per console, RetroArch or an installed emulator. By default, RetroArch opens what it has a core for; PlayStation 3 and Wii U need their emulator.
- **Use the emulators' folders** (on by default): the game folders your emulators already list (DuckStation's and PCSX2's game list, Dolphin's ISO folders, PPSSPP's last folder, Cemu's game paths, RPCS3's games) join your classics. Cerberus only reads them.
- Play time is counted the same way. RetroAchievements notices on the spot are for RetroArch only.

## Multi-disc games

Files named like `Game (Disc 1)`, `Game (Disc 2)` in the same folder show up as **one game**. If you have your own `.m3u` list, Cerberus uses it.

For PlayStation and GameCube, Cerberus writes a disc list in its own folder (never in your ROM folder) and opens RetroArch with it. When the game asks for the next disc, swap it in RetroArch's menu (**F1 → Disc Control**).

## Play the original of a recomp

Recomp pages show **Original in RetroArch** when your copy of the original game is in your ROM folders, so you can compare.

## RetroAchievements

1. Go to **Settings → RetroAchievements** and connect with your username and your **Web API key**. You'll find the key on retroachievements.org, under Settings → Keys. It stays on this PC.
2. To earn achievements while you play, go to **Settings → Emulation → Achievements in RetroArch** and type your password once.

Cerberus trades your password for an access token on the RetroAchievements website and saves **only the token** in RetroArch's settings. The password is never stored. RetroArch's settings file is backed up before it's changed.

When you unlock an achievement, Cerberus shows a Windows notification and adds it to the game's history.

## See also

- [Easy Setup](easy-setup.md)
- [Settings](settings.md#emulation)
