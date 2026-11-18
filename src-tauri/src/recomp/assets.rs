//! Picks, from a release's files, the build for this PC.
//!
//! Developers name their files freely (`Zelda64Recompiled-v1.2.2-Windows.zip`,
//! `SoH-Dewey-Bravo-Win64.zip`, `spaghetti-windows.zip`), so the choice goes by the
//! words in the name: the system and processor it is for, and words that mark a
//! file as something other than the game (debug symbols, a Switch build...).

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub url: String,
    /// Hex SHA-256 GitHub keeps for the upload; older uploads have none.
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Os {
    Windows,
    Linux,
    MacOs,
}

impl Os {
    pub fn current() -> Self {
        match std::env::consts::OS {
            "windows" => Os::Windows,
            "macos" => Os::MacOs,
            _ => Os::Linux,
        }
    }

    fn words(self) -> &'static [&'static str] {
        match self {
            Os::Windows => &["windows", "win", "win64", "win32", "msvc"],
            Os::Linux => &["linux", "appimage", "ubuntu", "deck", "steamdeck"],
            Os::MacOs => &["macos", "mac", "osx", "darwin", "apple", "universal"],
        }
    }
}

const ALL_OS: [Os; 3] = [Os::Windows, Os::Linux, Os::MacOs];
const ARM: &[&str] = &["arm64", "aarch64", "arm", "armhf", "armv7"];
const X64: &[&str] = &["x64", "x86_64", "amd64", "win64", "64bit"];

/// Never the game: debug files, sources, extras.
const NOT_A_BUILD: &[&str] = &["symbols", "pdb", "debug", "dbg", "source", "sources", "src", "cheats"];
/// Builds for a console or a phone, or that need Flatpak itself to run.
const OTHER_PLATFORM: &[&str] = &["switch", "android", "apk", "ios", "ps4", "ps5", "vita", "psp", "3ds", "wiiu", "flatpak"];
/// Something shipped beside the game: a server, an editor, tools.
const BESIDE_THE_GAME: &[&str] = &["server", "dedicated", "editor", "tools", "sdk", "modkit"];

pub fn pick(assets: &[Asset], os: Os, arm: bool) -> Option<&Asset> {
    let mut best: Option<(i32, &Asset)> = None;
    for asset in assets {
        let Some(score) = score(&asset.name, os, arm) else {
            continue;
        };
        // Strictly greater: on a tie the developer's first-listed file wins.
        if best.is_none_or(|(top, _)| score > top) {
            best = Some((score, asset));
        }
    }
    best.map(|(_, asset)| asset)
}

/// The file matching the catalog's pattern for this system
/// (`Zelda64Recompiled-*-Windows.zip`), when one does and can be unpacked.
pub fn by_pattern<'a>(assets: &'a [Asset], pattern: &str) -> Option<&'a Asset> {
    let usable = |name: &str| kind(name).is_some() || is_program(name);
    let fits = |a: &&Asset| super::saves::matches_pattern(pattern, &a.name) && usable(&a.name);
    assets.iter().find(fits)
}

/// A release file that is a program to run as it is (the game itself, or its installer).
pub fn is_program(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".exe") || lower.ends_with(".msi")
}

/// The archive kinds the installer can unpack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Zip,
    SevenZip,
    TarGz,
}

pub fn kind(name: &str) -> Option<Kind> {
    let lower = name.to_lowercase();
    if lower.ends_with(".zip") {
        return Some(Kind::Zip);
    }
    if lower.ends_with(".7z") {
        return Some(Kind::SevenZip);
    }
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return Some(Kind::TarGz);
    }
    None
}

/// `None` when the file cannot be this PC's game.
fn score(name: &str, os: Os, arm: bool) -> Option<i32> {
    let kind = kind(name)?;
    let words = words(name);
    let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
    if has(NOT_A_BUILD) || has(OTHER_PLATFORM) {
        return None;
    }
    let mine = has(os.words());
    // A .tar.gz is how Linux builds ship; on Windows it only counts when its name says Windows.
    if kind == Kind::TarGz && os == Os::Windows && !mine {
        return None;
    }
    let other_os = ALL_OS.iter().any(|other| *other != os && has(other.words()));
    if other_os && !mine {
        return None;
    }
    let other_arch = if arm { has(X64) && !has(ARM) } else { has(ARM) && !has(X64) };
    if other_arch {
        return None;
    }

    let mut score = 0;
    if mine {
        score += 50;
    }
    if has(if arm { ARM } else { X64 }) {
        score += 20;
    }
    if has(BESIDE_THE_GAME) {
        score -= 60;
    }
    // Between equally good builds, the format this system handles most simply.
    score += match (os, kind) {
        (Os::Linux, Kind::TarGz) => 12,
        (_, Kind::Zip) => 10,
        (_, Kind::SevenZip) => 8,
        (_, Kind::TarGz) => 6,
    };
    Some(score)
}

/// The words of a name, lower case: `Zelda64Recompiled-v1.2.2-Linux-X64.zip`
/// gives zelda64recompiled, v1, 2, 2, linux, x64, zip.
pub fn words(name: &str) -> Vec<String> {
    let parts = name.split(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    let parts = parts.flat_map(|part| part.split('_').chain(std::iter::once(part)));
    // "x86_64" is one word to us, but "Some_Game" is two; keep both readings.
    let words = parts.filter(|w| !w.is_empty()).map(str::to_lowercase);
    words.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets(names: &[&str]) -> Vec<Asset> {
        let to_asset = |name: &&str| Asset { name: name.to_string(), size: 1, url: String::new(), sha256: None };
        names.iter().map(to_asset).collect()
    }

    fn pick_name(names: &[&str], os: Os) -> Option<String> {
        pick(&assets(names), os, false).map(|a| a.name.clone())
    }

    #[test]
    fn picks_windows_from_a_zelda_release() {
        let names = [
            "Zelda64Recompiled-v1.2.2-Linux-ARM64.zip",
            "Zelda64Recompiled-v1.2.2-Linux-Flatpak-X64.zip",
            "Zelda64Recompiled-v1.2.2-Linux-X64.zip",
            "Zelda64Recompiled-v1.2.2-macOS.zip",
            "Zelda64Recompiled-v1.2.2-Windows.zip",
        ];
        assert_eq!(pick_name(&names, Os::Windows).as_deref(), Some("Zelda64Recompiled-v1.2.2-Windows.zip"));
        assert_eq!(pick_name(&names, Os::Linux).as_deref(), Some("Zelda64Recompiled-v1.2.2-Linux-X64.zip"));
    }

    #[test]
    fn skips_cheats_and_debug_symbols() {
        let names = [
            "SF64Recomp-Dev-Cheats.zip",
            "Goemon64Recompiled-PDB-RelWithDebInfo.zip",
            "Starfox64Recompiled-v1.0.3-Windows-RelWithDebInfo.zip",
        ];
        assert_eq!(pick_name(&names, Os::Windows).as_deref(), Some("Starfox64Recompiled-v1.0.3-Windows-RelWithDebInfo.zip"));
    }

    #[test]
    fn skips_console_builds() {
        let names = ["Starship-Barnard-Alfa-Switch.zip", "Starship-Barnard-Alfa-Windows.zip"];
        assert_eq!(pick_name(&names, Os::Windows).as_deref(), Some("Starship-Barnard-Alfa-Windows.zip"));
    }

    #[test]
    fn understands_win64() {
        let names = ["SoH-Dewey-Bravo-Mac.zip", "SoH-Dewey-Bravo-Win64.zip", "SoH-Dewey-Bravo.appimage"];
        assert_eq!(pick_name(&names, Os::Windows).as_deref(), Some("SoH-Dewey-Bravo-Win64.zip"));
    }

    #[test]
    fn keeps_intel_and_arm_macs_apart() {
        let names = ["spaghetti-mac-arm64.zip", "spaghetti-mac-intel-x64.zip", "spaghetti-windows.zip"];
        let arm = pick(&assets(&names), Os::MacOs, true).map(|a| a.name.clone());
        assert_eq!(arm.as_deref(), Some("spaghetti-mac-arm64.zip"));
        let intel = pick(&assets(&names), Os::MacOs, false).map(|a| a.name.clone());
        assert_eq!(intel.as_deref(), Some("spaghetti-mac-intel-x64.zip"));
    }

    #[test]
    fn has_nothing_when_no_build_fits() {
        let names = ["BanjoRecompiled-v1.0.2-Linux-X64.tar.gz", "BanjoRecompiled-v1.0.2-Linux-Flatpak-X64.zip"];
        assert_eq!(pick_name(&names, Os::Windows), None);
    }

    #[test]
    fn linux_takes_a_tarball() {
        let names = [
            "BanjoRecompiled-v1.0.2-Linux-ARM64.tar.gz",
            "BanjoRecompiled-v1.0.2-Linux-Flatpak-X64.zip",
            "BanjoRecompiled-v1.0.2-Linux-X64.tar.gz",
            "BanjoRecompiled-v1.0.2-Windows.zip",
        ];
        assert_eq!(pick_name(&names, Os::Linux).as_deref(), Some("BanjoRecompiled-v1.0.2-Linux-X64.tar.gz"));
    }

    #[test]
    fn takes_a_7z_when_it_is_the_only_windows_build() {
        let names = ["Game-v1-Linux.tar.gz", "Game-v1-Windows.7z"];
        assert_eq!(pick_name(&names, Os::Windows).as_deref(), Some("Game-v1-Windows.7z"));
    }

    #[test]
    fn prefers_a_zip_to_a_7z_of_the_same_build() {
        let names = ["Game-v1-Windows.7z", "Game-v1-Windows.zip"];
        assert_eq!(pick_name(&names, Os::Windows).as_deref(), Some("Game-v1-Windows.zip"));
    }

    #[test]
    fn windows_ignores_tarballs_not_named_for_it() {
        assert_eq!(pick_name(&["game-x86_64.tar.gz"], Os::Windows), None);
        assert_eq!(pick_name(&["tmc-windows-x86_64.tar.gz"], Os::Windows).as_deref(), Some("tmc-windows-x86_64.tar.gz"));
    }
}
