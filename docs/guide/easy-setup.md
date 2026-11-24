# Easy Setup

Recomps need data from **your own copy** of the original game. Easy Setup finds that copy and hands it to the recomp the way it expects. Everything happens on your PC; nothing is uploaded.

## How it finds your copy

Cerberus scans the [ROM folders](settings.md#library-and-roms) you added:

- **Nintendo 64 ROMs** are recognized by their header. Cerberus also checks the region and revision, and warns you when the recomp needs a different one (for example, the USA version).
- **Everything else** is matched by name, including disc images.

When a match is found, the game page shows **We found your copy**. Click **Use this copy**, or **Choose another...** to pick a file or folder yourself.

## How the copy is handed over

Each recomp says in the catalog how it wants the game. Cerberus supports these ways:

| The recomp… | Cerberus… |
|---|---|
| Asks for the file itself on first start | Opens it and lets the recomp ask |
| Takes the file as a command-line option | Starts it with that option pointing to your copy |
| Reads the file from an environment variable | Sets that variable when it starts the game |
| Reads files from a folder | Copies your file there, or extracts the game's files into it |

| Has its own setup program that unpacks the disc | Runs it once, without a window, before the first Play (and again after you pick another copy) |

## The developer's own file: `recomplauncher.json`

Many recomps ship a `recomplauncher.json` beside their program, in the open [RecompLauncher SDK](https://github.com/elripalda/RecompLauncher-SDK) format. When it's there, it wins over the catalog:

- **Which files the game takes.** The copy goes in the big box. Other discs, DLC, an update or keys show as rows under it, each with its own **Choose...**. Optional ones are listed as extras.
- **Checking your copy.** The game's serial (PlayStation, PS2, PSP, GameCube, Wii, N64, PS3/PS4 title IDs), a file inside the disc or folder, the file's name and checksum. The wrong game is refused with what was found, so a Budokai Tenkaichi 3 disc isn't taken for Tenkaichi 4.
- **Verified copies.** When the developer lists known good dumps, a copy of the same size is read whole and, when it matches, the card shows **Matches the verified disc (USA)**.
- **Preparation.** The game's own setup step runs once before the first Play, with a progress bar on the card. It runs again after a new release, a new copy or a changed option. Ticks for a vendor's SDK show under **During setup**; one with a license starts off, and ticking it accepts that license.
- **Settings, saves and logs.** Its settings files are kept up (the path to your copy, keys it needs), its saves are backed up, and its log goes with a crash report.
- **Exit codes.** When the game closes with a code it explains, you see its message instead of a crash notice.
- **Game folders.** Buttons in the **Settings** tab open its folders, or empty one (click twice).
- **What the PC needs.** Listed under **This version**. An older Windows than it asks is said when you press Play; the game still starts.
- **Mods and picture filters.** Where its mods come from (Thunderstore or GameBanana), and whether picture filters work with it.

A project on its own Forgejo or Gitea server can keep the file in its repo instead; Cerberus fetches it once when installing.

Not read yet: several games in one release (`games`), and the shared keys and NVIDIA DLSS files a launcher would fetch itself.

## Disc images

When a recomp needs the files inside a disc, Cerberus extracts them itself, without any extra program. It reads:

| Format | Typical use |
|---|---|
| ISO 9660 (`.iso`) | PlayStation 2, PSP, most CDs and DVDs |
| XDVDFS | Xbox and Xbox 360 discs |
| CHD (`.chd`) | Compressed CD and DVD images (the data track) |
| CSO (`.cso`) | Compressed PSP and PS2 images |

When a recomp wants the image file itself, a CHD or CSO is converted to an `.iso` in the game's folder. **Your original file is never changed.**

Not supported yet: RVZ, ZSO (LZ4) and "child" CHDs that depend on a parent file.

## Folders instead of files

Some recomps need a folder with the game's files, such as Switch titles with `exefs` and `romfs`. Easy Setup then asks for a folder and checks that it contains what the recomp needs.

## See also

- [Recomps](recomps.md)
- [Classics](classics.md)
