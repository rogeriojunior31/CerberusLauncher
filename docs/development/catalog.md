# Recomp catalog

Where the list of recomps comes from, its format, and how it's kept up to date.

## Two lists

| List | Where | Notes |
|---|---|---|
| **Cerberus's own catalog** | `src-tauri/catalog/recomps.json` in this repository | Curated by hand, 117 games. Built into the app and fetched again from `main` every 6 hours |
| **recomplauncher.com** | `https://recomplauncher.com/pc-catalog.json` | The public list RecompLauncher uses, almost 1,900 entries. On by default; the player can turn it off in **Settings → Catalog and sources** |

Both are merged, together with any [catalog sources](catalog-sources.md) the player added. Because the own catalog is read from `main`, a merged change reaches every player without a new release.

## Entry format

An entry in `recomps.json`:

```json
{
  "id": "zelda64recomp",
  "title": "majoras-mask",
  "name": "Zelda 64: Recompiled",
  "game": "The Legend of Zelda: Majora's Mask",
  "console": "Nintendo 64",
  "origin": "n64",
  "repo": "Zelda64Recomp/Zelda64Recomp",
  "icon": "https://.../512.png",
  "cover": "https://.../boxart.png",
  "hero": "https://.../snap.png",
  "about": "One sentence about the project.",
  "adds": "What this version brings",
  "needs": "Sua ROM de Majora's Mask (Nintendo 64).",
  "saves": ["{local}/Zelda64Recompiled/saves"],
  "tag": "v1.2.2",
  "released": "2025-08-27",
  "systems": ["windows", "linux", "macos"],
  "assets": { "windows": "Zelda64Recompiled-*-Windows.zip" },
  "files": { "windows": { "name": "Zelda64Recompiled-v1.2.2-Windows.zip", "sha256": "d02b95f1..." } },
  "size_text": "28 MB",
  "stars": 7744
}
```

| Field | Meaning | Filled by |
|---|---|---|
| `id` | Unique id; the installed game becomes `recomp:<id>` | Hand |
| `title` | The same for every version of one game, so they're grouped | Hand |
| `name`, `game`, `console`, `origin` | Recomp name, original game, console name, console code (`n64`, `x360`, `ps2`...) | Hand |
| `repo` | `owner/project` on GitHub, GitLab or a Forgejo server | Hand |
| `icon`, `cover`, `hero` | Icon, 2:3 cover, wide background. Direct, public links | Hand |
| `about`, `adds`, `needs` | One sentence each: the project, what it adds, what the player must own | Hand |
| `saves` | Save locations as templates: `{install}/...`, `{local}/...` (`%LOCALAPPDATA%`), `{appdata}/...` (`%APPDATA%`) | Hand |
| `start` | The program to start, when the release has several | Hand |
| `kind` | How the release ships: an archive (default), `installer` or `single-exe` | Hand |
| `handoff`, `copy_accepts`, `copy_contains` | How the player's copy is handed over, and what it must be (file, folder, folder or ISO) and contain | Hand |
| `player_note` | What the player should know before the first start | Hand |
| `tag`, `released`, `systems`, `assets`, `files`, `size_text`, `stars` | Latest release, its file per system with SHA-256, size and stars | `catalog-builder` |

Texts in the own catalog are written in Brazilian Portuguese. The app shows `needs` and tags in English to English-speaking players, translating known patterns such as "Sua cópia de …" → "Your own copy of …".

## Keeping it up to date

`tools/catalog-builder` maintains the file, and a person reviews every change:

```sh
GITHUB_TOKEN=... cargo run -p catalog-builder -- update     # refresh what GitHub knows
GITHUB_TOKEN=... cargo run -p catalog-builder -- discover   # look for new projects
```

- **`update`** refreshes only what comes from the host: version, files and checksums, name pattern, size, stars and systems. It never changes fields written by hand.
- **`discover`** searches GitHub for recomp projects and writes them to `src-tauri/catalog/candidates.json`. It never adds anything to the catalog by itself.
- The **Catálogo** GitHub Action runs both every day at 12:00 UTC (or by hand) and opens a pull request with the results.

A `GITHUB_TOKEN` raises GitHub's limit from 60 to 5,000 requests per hour.

## Accepting a candidate

1. Copy the entry from `candidates.json` into `recomps.json`.
2. Fill in the hand-written fields above. Check `saves` in the project's code, since a wrong path means no backups.
3. Leave out tools, libraries and projects without a playable game.
4. Open a pull request.

Requests come in through the **Request a recomp** button, which opens the `pedir-recomp` issue form.

## See also

- [Catalog sources](catalog-sources.md)
- [Recomps](../guide/recomps.md)
