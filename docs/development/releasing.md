# Releasing

How to cut a new version of Cerberus.

## Steps

1. **Version.** Bump `version` in the root `Cargo.toml` (shared by every crate) and in `src-tauri/tauri.conf.json`. Use semantic versioning: `0.11.0`.
2. **Notes.** Add the version's notes at the top of `CHANGELOG.md`, grouped by area, written for players.
3. **Installer pictures.** If the icon changed, or to show the new version on the side picture, run:
   ```powershell
   pwsh src-tauri/installer/make-images.ps1 -Version 0.11
   ```
4. **Checks.** Run `cargo test --workspace --locked` and build the interface for WebAssembly. Both must pass.
5. **Merge** the release branch into `main` through a pull request.
6. **Build** from `main`, so the binary matches the tag:
   ```sh
   cargo tauri build
   ```
7. **Tag.** Tag the merge commit and push the tag:
   ```sh
   git tag -a v0.11.0 -m "Cerberus Launcher 0.11.0"
   git push origin v0.11.0
   ```
8. **Publish.**
   - Create a GitHub release for the tag.
   - Name the installer `Cerberus-Launcher_<version>_x64-setup.exe`, with no spaces, since GitHub replaces them with dots.
   - Paste the changelog section as the notes and add the installer's SHA-256.

## The installer

The Windows installer is built by Tauri's NSIS bundler (`bundle` in `src-tauri/tauri.conf.json`):

| Setting | Value |
|---|---|
| Install mode | Current user only, no administrator rights |
| Languages | English and Brazilian Portuguese, picked by Windows' language |
| WebView2 | Bootstrapper embedded, installs it when missing |
| Pictures | `src-tauri/installer/sidebar.bmp`, `header.bmp` |
| Texts | `src-tauri/installer/English.nsh`, `PortugueseBR.nsh` (based on Tauri's, with clearer wording) |
| Hooks | `src-tauri/installer/hooks.nsh`: uninstalling removes the "Start with Windows" entry and the installer's registry keys, but keeps them during updates |

## Not done yet

- **Code signing.** Without it, Windows SmartScreen warns on first run.
- **In-app updates.** Players download new versions by hand for now.

## See also

- [Building](building.md)
- [Installation](../getting-started/installation.md)
