# Building

How to build, run and test Cerberus from source.

## Requirements

- Windows 10 or 11.
- [Rust](https://rustup.rs) 1.100 or newer, with the WebAssembly target.
- [Trunk](https://trunkrs.dev) to build the interface.
- The [Tauri CLI](https://tauri.app) 2.x.
- Microsoft WebView2 (built into Windows 11).

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cargo install tauri-cli --version "^2" --locked
```

## Run

```sh
cargo tauri dev
```

This starts Trunk on `http://localhost:1420` and the app on top of it. Changes to `ui/` reload the window; changes to `src-tauri/` or `shared/` rebuild and restart the app.

> The development build uses the same data folder as an installed Cerberus (`%APPDATA%\app.cerberus.launcher`). Back it up before testing anything that changes it.

## Test

The same checks that run on every pull request:

```sh
cargo test --workspace --locked
cargo build -p cerberus-ui --target wasm32-unknown-unknown --locked
```

- `cerberus-launcher`: the backend tests. A few tests that reach real servers are `#[ignore]`d; run them with `cargo test -- --ignored`.
- `cerberus-shared`: includes the translation checks. See [Translations](translations.md).
- `catalog-builder`: the catalog tool's tests.

Tests that need another language use `cerberus_shared::i18n::with_lang`, which changes the language for the current thread only, since tests run in parallel.

## Build the installer

```sh
cargo tauri build
```

The installer lands in `target/release/bundle/nsis/`. Its pictures, texts and hooks are in `src-tauri/installer/`. See [Releasing](releasing.md).

## Project layout

```text
shared/                  types and translations used by both sides
src-tauri/               backend (Tauri 2)
├── src/                 see Architecture
├── catalog/             recomps.json and candidates.json
└── installer/           NSIS hooks, texts and pictures
ui/                      interface (Leptos → WebAssembly)
tools/catalog-builder/   keeps the catalog up to date
docs/                    this documentation
```

## See also

- [Architecture](architecture.md)
- [CONTRIBUTING.md](../../CONTRIBUTING.md)
