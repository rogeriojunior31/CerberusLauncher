//! Which file of a release is the build for each system, and the name pattern
//! the launcher matches it with on later releases.

/// A release file as GitHub lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseFile {
    pub name: String,
    pub size: u64,
    /// Lower-case hex SHA-256; uploads from before GitHub kept checksums have none.
    pub sha256: Option<String>,
}

pub const SYSTEMS: [&str; 3] = ["windows", "linux", "macos"];

fn words(name: &str) -> Vec<String> {
    let lower = name.to_lowercase();
    let parts = lower.split(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    parts.flat_map(|p| p.split('_').chain(std::iter::once(p)).map(str::to_string).collect::<Vec<_>>()).filter(|w| !w.is_empty()).collect()
}

fn system_words(system: &str) -> &'static [&'static str] {
    match system {
        "windows" => &["windows", "win", "win64", "win32", "msvc"],
        "linux" => &["linux", "appimage", "ubuntu", "deck", "steamdeck"],
        _ => &["macos", "mac", "osx", "darwin", "apple", "universal"],
    }
}

/// Never the game: debug files, sources, checksums, cheats, console builds, Flatpak bundles.
const NOT_A_BUILD: &[&str] = &[
    "symbols", "pdb", "debug", "dbg", "source", "sources", "src", "cheats", "switch", "android", "apk", "ios", "flatpak",
];
const ARM: &[&str] = &["arm64", "aarch64", "arm", "armhf", "armv7"];
const X64: &[&str] = &["x64", "x86_64", "amd64", "win64", "64bit"];

/// The kinds of file the launcher installs, per system.
fn installable(name: &str, system: &str) -> bool {
    let lower = name.to_lowercase();
    let archive = lower.ends_with(".zip") || lower.ends_with(".7z");
    let tarball = lower.ends_with(".tar.gz") || lower.ends_with(".tgz");
    let program = lower.ends_with(".exe") || lower.ends_with(".msi");
    match system {
        "windows" => archive || program,
        "linux" => archive || tarball || lower.ends_with(".appimage"),
        _ => archive,
    }
}

/// The best file of a release for one system, if it has one. x64 builds only:
/// the catalog lists what most players can run.
pub fn pick<'a>(files: &'a [ReleaseFile], system: &str) -> Option<&'a ReleaseFile> {
    let mut best: Option<(i32, &ReleaseFile)> = None;
    for file in files {
        if !installable(&file.name, system) {
            continue;
        }
        let w = words(&file.name);
        let has = |list: &[&str]| w.iter().any(|x| list.contains(&x.as_str()));
        if has(NOT_A_BUILD) || (has(ARM) && !has(X64)) {
            continue;
        }
        let mine = has(system_words(system));
        let other = SYSTEMS.iter().any(|s| *s != system && has(system_words(s)));
        if other && !mine {
            continue;
        }
        // A file that names no system is only taken for Windows, where most such builds are.
        if !mine && system != "windows" {
            continue;
        }
        let lower = file.name.to_lowercase();
        let mut score = if mine { 50 } else { 0 };
        if has(X64) {
            score += 20;
        }
        if lower.ends_with(".zip") {
            score += 10;
        }
        if lower.contains("setup") || lower.contains("installer") {
            score -= 15;
        }
        if best.is_none_or(|(top, _)| score > top) {
            best = Some((score, file));
        }
    }
    best.map(|(_, file)| file)
}

/// The file name with its version taken out, so it matches the next release:
/// `Zelda64Recompiled-v1.2.2-Windows.zip` with tag `v1.2.2` gives
/// `Zelda64Recompiled-*-Windows.zip`. A name without the version stays as it is.
pub fn pattern(file_name: &str, tag: &str) -> String {
    let bare = tag.trim_start_matches(['v', 'V']);
    for version in [tag, bare] {
        if version.len() >= 2 && file_name.contains(version) {
            return file_name.replacen(version, "*", 1);
        }
    }
    file_name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<ReleaseFile> {
        names.iter().map(|n| ReleaseFile { name: n.to_string(), size: 1, sha256: None }).collect()
    }

    #[test]
    fn picks_each_systems_build() {
        let list = files(&[
            "Zelda64Recompiled-v1.2.2-Linux-ARM64.zip",
            "Zelda64Recompiled-v1.2.2-Linux-Flatpak-X64.zip",
            "Zelda64Recompiled-v1.2.2-Linux-X64.zip",
            "Zelda64Recompiled-v1.2.2-macOS.zip",
            "Zelda64Recompiled-v1.2.2-Windows.zip",
        ]);
        assert_eq!(pick(&list, "windows").unwrap().name, "Zelda64Recompiled-v1.2.2-Windows.zip");
        assert_eq!(pick(&list, "linux").unwrap().name, "Zelda64Recompiled-v1.2.2-Linux-X64.zip");
        assert_eq!(pick(&list, "macos").unwrap().name, "Zelda64Recompiled-v1.2.2-macOS.zip");
    }

    #[test]
    fn leaves_out_extras_and_console_builds() {
        let list = files(&["Game-PDB-RelWithDebInfo.zip", "Game-Switch.zip", "Game-Windows-RelWithDebInfo.zip"]);
        assert_eq!(pick(&list, "windows").unwrap().name, "Game-Windows-RelWithDebInfo.zip");
    }

    #[test]
    fn a_lone_program_is_a_windows_build() {
        let list = files(&["PGR2Launcher.exe"]);
        assert_eq!(pick(&list, "windows").unwrap().name, "PGR2Launcher.exe");
        assert!(pick(&list, "linux").is_none());
    }

    #[test]
    fn takes_the_version_out_of_the_name() {
        assert_eq!(pattern("Zelda64Recompiled-v1.2.2-Windows.zip", "v1.2.2"), "Zelda64Recompiled-*-Windows.zip");
        assert_eq!(pattern("Starfox64Recompiled-v1.0.3-Windows.zip", "1.0.3"), "Starfox64Recompiled-v*-Windows.zip");
        assert_eq!(pattern("spaghetti-windows.zip", "1.0.0"), "spaghetti-windows.zip");
    }
}
