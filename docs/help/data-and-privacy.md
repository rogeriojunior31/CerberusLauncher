# Data and privacy

What Cerberus keeps on your PC, and every online service it talks to.

## No account, no tracking

Cerberus has no account of its own, no analytics and no telemetry. It only goes online to do what you ask it to (install, update, look up art) and to keep the catalog fresh.

## What's stored, and where

| Folder | Contents |
|---|---|
| `%LOCALAPPDATA%\Cerberus Launcher` | The program |
| `%APPDATA%\app.cerberus.launcher` | Everything else, listed below |

Inside the data folder:

| Item | Contents |
|---|---|
| `settings.json` | Your settings |
| `library.json` | Games you added, recomps, play time |
| `trail.jsonl` | Each game's history (up to 600 entries) |
| `favorites.json`, `collections.json` | Favorites, collections and hidden games |
| `game-notes.json` | Your status, rating and notes for each game |
| `sessions.jsonl` | Each play session (when it ended and how long), for the play-time chart |
| `classics.json` | Play time of your classics |
| `art.json`, `covers/`, `cache/` | Custom art and downloaded images |
| `cache/achievements/` | Achievement answers from Steam and RetroAchievements, so the Achievements page opens at once |
| `store-playtime.json` | Play time of Steam and Epic games |
| `recomps/` | Recomps installed in the default folder |
| `downloads/` | Downloads in progress |
| `saves/` | Save backups |
| `screenshots/` | Pictures taken with F12 |
| `emulators/` | RetroArch, cores and disc lists, when installed by Cerberus |
| `reshade/` | ReShade, when you use visual filters |

## Keys and passwords

- **SteamGridDB, Steam Web API and RetroAchievements keys** stay on this PC, in `settings.json`. They are only sent to the service they belong to.
- **Your RetroAchievements password is never stored.** Cerberus trades it once for an access token, saves only the token in RetroArch's settings, and backs up that file first.
- **A copy of your data** (Settings → Storage → Copy my data) never includes keys, tokens or passwords, so it can be shared or synced without them.

## Files outside the data folder

Cerberus changes files outside its own folder only when you ask, and backs them up first when they matter:

- A recomp's saves, when you restore a backup. Current saves are backed up first.
- Steam's `shortcuts.vdf`, when you add a game to Steam. A copy is kept.
- RetroArch's settings, when you connect achievements. A copy is kept.
- Hedge Mod Manager's `ModsDB.ini`, when you install a mod for Unleashed or Marathon. A copy is kept.
- A game's DLSS DLL, when you swap it. The original is kept.
- ReShade files in a game folder, only while the game runs.

## Online services

| Service | When | What for |
|---|---|---|
| `raw.githubusercontent.com` | Every 6 hours | Cerberus's own recomp catalog |
| `api.github.com`, `github.com` | At start and every 6 hours; the download only when you click Update | Newer versions of Cerberus itself |
| `recomplauncher.com` | Every 6 hours, unless turned off | The public recomp catalog |
| GitHub, GitLab, Forgejo servers | When you install or update | Releases of each recomp |
| `store.steampowered.com`, `api.steampowered.com`, Steam CDN | When you add a game or open a page | Names and covers |
| `www.steamgriddb.com` | Only with your key | Community art |
| `thunderstore.io`, `gamebanana.com` | When you browse mods | Mods |
| `buildbot.libretro.com` | When you install RetroArch or a core | RetroArch and cores |
| `api.steampowered.com` | Only with your Steam Web API key, when the Achievements page updates | Your games, achievements and play time |
| `api.steampowered.com` (no key), Steam CDN | When you open a game's achievements | How rare each achievement is, and icons. Kept for a week |
| `api.steampowered.com` (no key) | When you open Novidades | The latest announcements of your 12 most recently played Steam games (only their app ids are sent). Kept for 6 hours in `cache/news/` |
| `pt.wikipedia.org` or `en.wikipedia.org` | When you open a recomp's page | The original game's name, to find its article. Kept for a month in `cache/wiki/` |
| `retroachievements.org` | When you connect, open Achievements or play with achievements | Profile and achievements |
| `reshade.me`, `api.github.com` | First visual filter | ReShade |
| `aka.ms`, `download.microsoft.com` | Only when you ask | Visual C++ and DirectX installers |
| `raw.githubusercontent.com/libretro-thumbnails` | When classics are listed | Box art |

## See also

- [Installation](../getting-started/installation.md#uninstall)
- [Saves](../guide/saves.md)
