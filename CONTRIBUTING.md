# Contributing to Cerberus

**English** · [Português (Brasil)](CONTRIBUTING.pt-BR.md)

Thanks for helping. This guide covers the code layout, the development loop, and how to maintain the recomp catalog.

## Ways to help

- **Report a bug.** [Open an issue](https://github.com/rogeriojunior31/CerberusLauncher/issues/new/choose) and include:
  - what you did and what happened;
  - the game's entry in the game page history (**Gerenciar → Histórico**), when it applies.
- **Request a recomp** with the "Pedir um recomp" form, or the button of the same name in Discover.
- **Send a pull request.** Small, focused PRs are easier to review. The checks below must pass.

The [documentation](docs/README.md) goes deeper: [architecture](docs/development/architecture.md), [building](docs/development/building.md), [the recomp catalog](docs/development/catalog.md), [catalog sources](docs/development/catalog-sources.md), [translations](docs/development/translations.md) and [releasing](docs/development/releasing.md).

## Project layout

```text
shared/                  types used by both sides (Game, SettingsView, ...)
src-tauri/               backend: Tauri 2 app
├── src/
│   ├── recomp/          catalog, releases, install, updates, saves
│   ├── library.rs       the player's library (library.json)
│   ├── sessions.rs      running games, play time, backups at the end
│   ├── emulation.rs     RetroArch and cores for the classics
│   ├── overlay.rs       the menu over a game (Ctrl+F12)
│   └── ...
├── catalog/             recomps.json (own catalog) and candidates.json
└── installer/           NSIS hooks, texts and pictures for the Windows installer
ui/                      interface: Leptos 0.8 compiled to WebAssembly
└── src/tv/              Big Picture Mode
tools/catalog-builder/   keeps the catalog up to date from GitHub
```

## Development

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cargo install tauri-cli --version "^2" --locked

cargo tauri dev                 # run with live reload
cargo test --workspace --locked # what CI runs
cargo build -p cerberus-ui --target wasm32-unknown-unknown --locked
```

Things to keep in mind:

- **Downloads are the player's choice.** Recomps come from their developers' releases and use the player's own copy. User-added Hydra sources can install direct HTTP(S) links when asked; magnet and torrent links remain for reference only.
- **Code from RecompLauncher (GPL) is never copied.** Only ideas and facts are used. Hydra is MIT.
- **User data is precious.** Anything that writes to a game folder, a save or another program's settings needs a backup and a way back.
- **Texts are written in Brazilian Portuguese,** short and plain, and translated to English. Code and comments are in English.

### Translations

Every text the player sees goes through `tr!`, from `shared/src/i18n.rs`. The Portuguese text is its own key, and `shared/locales/en.json` holds the English:

```rust
tr!("Biblioteca")                       // "Library"
tr!("{} jogos", count)                  // "{} games"
tr!("{1} não está mais em {0}.", a, b)  // numbered parts, when the order changes
```

- Add the English for any new text to `en.json`. `cargo test -p cerberus-shared` fails if a text has no translation, if `en.json` keeps a text the code no longer uses, or if a translation drops one of the `{}` parts.
- `const` tables use `tk!("...")`, which marks the text without translating it, and call `cerberus_shared::i18n::t(text)` where it's shown.
- Saved data stays in Portuguese (history steps, backup reasons) and is translated on screen with `i18n::translate`, so a file never mixes languages.
- Keys, file and folder names, URLs and brand names are never translated.
- To add a language: a new `Lang` value, its `locales/<code>.json`, and the language in `bundle.windows.nsis.languages` for the installer.

### Releasing

1. Bump `version` in the root `Cargo.toml` and in `src-tauri/tauri.conf.json`.
2. Add the version's notes at the top of `CHANGELOG.md`.
3. If the installer pictures change, run `pwsh src-tauri/installer/make-images.ps1 -Version x.y`.
4. Run `cargo tauri build`. The installer lands in `target/release/bundle/nsis/`.
5. Tag `vX.Y.Z` and publish a GitHub release with the installer and the changelog notes.

## Maintaining the catalog

Cerberus's own catalog is `src-tauri/catalog/recomps.json`. The app fetches this file from the `main` branch every 6 hours, so whatever is merged reaches every player without a new release.

`catalog-builder` (in `tools/`) maintains the file, and a person reviews every change:

```sh
GITHUB_TOKEN=... cargo run -p catalog-builder -- update     # refresh what comes from GitHub
GITHUB_TOKEN=... cargo run -p catalog-builder -- discover   # look for new projects
```

- `update` only touches what comes from GitHub:
  - the version;
  - each system's file, with its SHA-256;
  - the file name pattern;
  - size, stars and systems.

  It never changes anything written by hand.
- `discover` writes `src-tauri/catalog/candidates.json`. It never adds anything to the catalog on its own.
- The **Catálogo** GitHub Action runs both every day at 09:00 Brasília time. It can also be started by hand from Actions. It opens a pull request with the results.

**To accept a candidate**, copy its entry from `candidates.json` into `recomps.json` and fill in these fields:

| Field | What goes there |
|---|---|
| `name`, `game`, `console`, `origin` | Recomp name, original game, full console name and its short code (`n64`, `x360`...) |
| `title` | The same value for every version of one game (e.g. `majoras-mask`), so they're grouped |
| `needs` | What the player must own: "Sua cópia de ..." |
| `about`, `adds` | One sentence about the project and what this version adds |
| `cover`, `hero`, `icon` | Images: cover (2:3), wide background and icon. Use direct, public links |
| `saves` | Where it saves, like `{local}/Name/saves` or `{install}/Save`. Check it in the project's code |

Tools, libraries and projects without a playable game stay out.

## Catalog sources

Players can add their own JSON lists in **Configurações → Catálogo e fontes**. A list holds games that their developers publish on GitHub (free games, homebrew, ports):

```json
{
  "name": "Free games",
  "games": [
    {
      "id": "srb2",
      "name": "Sonic Robo Blast 2",
      "repo": "STJr/SRB2",
      "assets": { "windows": "SRB2-*-Full.zip" },
      "cover": "https://.../cover.png",
      "about": "One sentence about the game."
    }
  ]
}
```

Only `id`, `name` and `repo` are required. Each game installs from the repository's official release, checked against GitHub's checksum.

Entries in project sources with their own download links (`uris`, `magnet`, `url`, `download`) are ignored. Hydra lists with a `downloads` key can be added by HTTPS URL: entries with direct HTTP(S) links in supported formats join Discover and install when the player asks. Magnet and torrent links remain for reference only.
