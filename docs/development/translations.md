# Translations

Cerberus speaks English and Brazilian Portuguese. This page explains how texts are translated and how to add a language.

## How it works

Texts are written in Brazilian Portuguese, right in the code, and wrapped in `tr!`. The Portuguese text is its own key, and `shared/locales/en.json` maps it to English:

```rust
use cerberus_shared::tr;

tr!("Biblioteca")                        // "Library"
tr!("{} jogos", count)                   // "{} games"
tr!("{1} não está mais em {0}.", a, b)   // numbered parts, when a language needs another order
```

```json
{
  "Biblioteca": "Library",
  "{} jogos": "{} games",
  "{1} não está mais em {0}.": "{1} isn't at {0} anymore."
}
```

- Without parts, `tr!` returns a `&'static str`; with parts, a `String`.
- The UI and the backend share the same module, `shared/src/i18n.rs`, and the same `en.json`.
- A text without a translation falls back to Portuguese.

## Choosing the language

- The choice lives in the settings (`language`: `"pt-BR"`, `"en"`, or none for automatic).
- **Automatic** follows the system: Portuguese for Portuguese speakers, English for everyone else. The backend reads Windows' display language; the interface reads the WebView's.
- The interface also keeps the choice in the browser storage, so the very first frame is drawn in the right language. Changing it reloads the window.
- Numbers and dates follow the language in `ui/src/format.rs`: `7,7 mil` / `7.7K`, `há 3 h` / `3 h ago`, `18/03/2017` / `03/18/2017`.

## Rules

- **Wrap every text the player sees** in `tr!`: labels, buttons, toasts, errors returned to the UI, notifications.
- **Never translate data**: JSON, INI and registry keys, file and folder names, URLs, command-line options, brand and console names.
- **`const` tables** can't call `tr!`. Mark the text with `tk!("...")`, which leaves it as written, and translate where it's shown with `cerberus_shared::i18n::t(text)`.
- **Saved data stays in Portuguese**: history step names and backup reasons are written with `tk!` and translated on screen with `i18n::translate(&text)`. This way a file never mixes languages, and comparisons keep working.
- **Logs** (`eprintln!`) aren't translated.

## Checks

`cargo test -p cerberus-shared` fails when:

- a `tr!`/`tk!` text has no English in `en.json`;
- `en.json` keeps a text the code no longer uses;
- a translation drops or adds a `{}` part.

## Adding a language

1. Add a value to `Lang` in `shared/src/i18n.rs`, with its code, its name in that language, and how `from_code` recognizes it.
2. Create `shared/locales/<code>.json` with the same keys as `en.json`, and load it in `i18n.rs` the way `en.json` is loaded.
3. Extend the tests to check the new file too.
4. Add the language to `bundle.windows.nsis.languages` in `src-tauri/tauri.conf.json`, with a `customLanguageFiles` entry if its texts need adjusting (see `src-tauri/installer/`).
5. Teach `ui/src/format.rs` its number and date formats.

## See also

- [Building](building.md)
- [Architecture](architecture.md)
