//! Finds PC games installed through the Xbox app (Game Pass, Microsoft Store).
//!
//! The Xbox app installs into a folder named in a `.GamingRoot` file at the root
//! of each drive ("XboxGames" by default). Each game there has a `Content`
//! folder with `MicrosoftGame.config` (its name, logos and program) and
//! `appxmanifest.xml` (its app id). The game starts through Windows, as the
//! installed package `shell:AppsFolder\<family>!<app>`; the package family is
//! the package name plus a hash of its publisher, computed here as Windows does.

use std::fs;
use std::path::{Path, PathBuf};

use cerberus_shared::{Game, Source};
use sha2::{Digest, Sha256};

/// One game the Xbox app installed.
#[derive(Debug, PartialEq)]
struct Install {
    name: String,
    family: String,
    app: String,
    content: PathBuf,
    #[cfg_attr(not(test), allow(dead_code))]
    program: Option<String>,
    cover: Option<PathBuf>,
}

/// Windows's "publisher id": SHA-256 of the publisher (UTF-16), its first 8
/// bytes in Crockford's base32, 13 letters.
fn publisher_id(publisher: &str) -> String {
    let bytes: Vec<u8> = publisher.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let hash = Sha256::digest(&bytes);
    let mut bits: u128 = 0;
    for b in &hash[..8] {
        bits = (bits << 8) | u128::from(*b);
    }
    // 64 bits plus one zero bit make 65, thirteen groups of five.
    bits <<= 1;
    const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";
    (0..13).rev().map(|i| ALPHABET[((bits >> (i * 5)) & 31) as usize] as char).collect()
}

/// An attribute's value in a small, well-formed XML file the store writes.
fn attribute(xml: &str, element: &str, name: &str) -> Option<String> {
    // The element itself, not one whose name starts the same ("ExecutableList").
    let open = format!("<{element}");
    let start = xml.match_indices(&open).map(|(i, _)| i).find(|&i| {
        xml[i + open.len()..].chars().next().is_some_and(|c| c.is_whitespace() || c == '/' || c == '>')
    })?;
    let tag = &xml[start..];
    let end = tag.find('>')?;
    let tag = &tag[..end];
    let key = format!("{name}=\"");
    let at = tag.find(&key)? + key.len();
    let value = &tag[at..];
    Some(value[..value.find('"')?].to_string())
}

/// The folders the Xbox app installs into, from each drive's `.GamingRoot`.
fn roots() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for letter in b'C'..=b'Z' {
        let drive = PathBuf::from(format!("{}:\\", letter as char));
        let Ok(bytes) = fs::read(drive.join(".GamingRoot")) else { continue };
        if let Some(folder) = gaming_root(&bytes) {
            found.push(drive.join(folder));
        }
    }
    found
}

/// `.GamingRoot`: "RGBX", a count, then the folder in UTF-16, ending in a zero.
fn gaming_root(bytes: &[u8]) -> Option<String> {
    if bytes.get(..4)? != b"RGBX" {
        return None;
    }
    let units: Vec<u16> = bytes.get(8..)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
    let folder = String::from_utf16(&units).ok()?;
    (!folder.is_empty()).then_some(folder)
}

fn read_install(game_dir: &Path) -> Option<Install> {
    let content = game_dir.join("Content");
    let config = fs::read_to_string(content.join("MicrosoftGame.config")).ok()?;
    let package = attribute(&config, "Identity", "Name")?;
    let publisher = attribute(&config, "Identity", "Publisher")?;
    let shown = attribute(&config, "ShellVisuals", "DefaultDisplayName").filter(|n| !n.starts_with("ms-resource:"));
    let name = shown.unwrap_or_else(|| game_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let manifest = fs::read_to_string(content.join("appxmanifest.xml")).unwrap_or_default();
    let app = attribute(&manifest, "Application", "Id").or_else(|| attribute(&config, "Executable", "Id")).unwrap_or_else(|| "Game".into());
    let program = attribute(&config, "Executable", "Name");
    // The logos where the config says (often in an Images or Resources folder), the biggest first.
    let cover = ["Square480x480Logo", "Square150x150Logo", "StoreLogo"]
        .iter()
        .filter_map(|key| attribute(&config, "ShellVisuals", key))
        .map(|relative| content.join(relative.replace('\\', "/")))
        .find(|p| p.is_file());
    Some(Install { name, family: format!("{package}_{}", publisher_id(&publisher)), app, content, program, cover })
}

pub fn installed_games() -> Vec<Game> {
    let mut games = Vec::new();
    for root in roots() {
        let Ok(entries) = fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            if let Some(install) = read_install(&entry.path()) {
                games.push(to_game(install));
            }
        }
    }
    games
}

/// Starts the game the way the Start menu does.
pub fn launch(family: &str, app: &str) -> std::io::Result<()> {
    std::process::Command::new("explorer.exe").arg(format!("shell:AppsFolder\\{family}!{app}")).spawn().map(|_| ())
}

fn to_game(install: Install) -> Game {
    Game {
        id: format!("xbox:{}", install.family),
        name: install.name,
        source: Source::Xbox { family: install.family, app: install.app },
        install_dir: install.content.to_string_lossy().into_owned(),
        cover_path: install.cover.map(|p| p.to_string_lossy().into_owned()),
        cover_url: None,
        hero_path: None,
        hero_url: None,
        playtime_secs: 0,
        last_played: None,
        last_session_secs: None,
        launch_args: String::new(),
        // Counted like Steam's and Epic's: by a program running from its folder.
        tracking_exe: None,
        game_copy: None,
        copy_verified: None,
        inputs: Default::default(),
        setup_pending: false,
        next_step: Default::default(),
        setting_picks: Default::default(),
        external: false,
        logo_path: None,
        look: None,
        favorite: false,
        hidden: false,
        collections: Vec::new(),
        status: None,
        rating: 0,
        notes: String::new(),
        updates_paused: false,
        part_of: None,
        part_ids: Vec::new(),
        parts: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_package_family_is_computed_as_windows_does() {
        // Chained Beasts, as Windows lists it: FEATHERWEIGHTGAMESPTYLIMI.ChainedBeasts_..._x64__ad46npcgb9ttj
        assert_eq!(publisher_id("CN=DCD889AB-5DD5-489D-BB25-2ED43B3DD87E"), "ad46npcgb9ttj");
        // Microsoft's own apps: ..._8wekyb3d8bbwe.
        assert_eq!(publisher_id("CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US"), "8wekyb3d8bbwe");
    }

    #[test]
    fn reads_a_game_folder() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("Chained Beasts");
        fs::create_dir_all(game.join("Content")).unwrap();
        fs::write(
            game.join("Content").join("MicrosoftGame.config"),
            r#"<Game><Identity Name="FEATHERWEIGHTGAMESPTYLIMI.ChainedBeasts" Publisher="CN=DCD889AB-5DD5-489D-BB25-2ED43B3DD87E" Version="1.0.5.0" />
            <ShellVisuals DefaultDisplayName="Chained Beasts" Square480x480Logo="Square480x480Logo.png" />
            <ExecutableList><Executable Name="ChainedBeasts.exe" Id="Game" /></ExecutableList></Game>"#,
        )
        .unwrap();
        let install = read_install(&game).unwrap();
        assert_eq!(install.name, "Chained Beasts");
        assert_eq!(install.family, "FEATHERWEIGHTGAMESPTYLIMI.ChainedBeasts_ad46npcgb9ttj");
        assert_eq!(install.app, "Game");
        assert_eq!(install.program.as_deref(), Some("ChainedBeasts.exe"));
        // The saves folder beside the games isn't one.
        fs::create_dir_all(dir.path().join("GameSave").join("wgs")).unwrap();
        assert!(read_install(&dir.path().join("GameSave")).is_none());
    }

    #[test]
    fn reads_the_gaming_root() {
        let mut bytes = b"RGBX\x01\x00\x00\x00".to_vec();
        bytes.extend("XboxGames".encode_utf16().flat_map(u16::to_le_bytes));
        bytes.extend([0, 0]);
        assert_eq!(gaming_root(&bytes).as_deref(), Some("XboxGames"));
        assert_eq!(gaming_root(b"nope"), None);
    }
}
