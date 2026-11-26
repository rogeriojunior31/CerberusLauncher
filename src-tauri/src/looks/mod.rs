//! Looks: picture filters for a game (sharper, livelier colours, an old TV,
//! film), our own shaders applied through ReShade.
//!
//! ReShade is downloaded once from its developer's site, the first time a
//! player picks a look. Nothing is installed in Windows: when the game starts,
//! ReShade goes beside it as `dxgi.dll` (games that draw with Direct3D 10, 11
//! or 12), and it is taken away again when the game closes. Its settings, log
//! and cache stay in Cerberus's own folder (`looks/<game>`), found through
//! `RESHADE_BASE_PATH_OVERRIDE`. A `dxgi.dll` the player already has (their own
//! ReShade, another mod) is never touched: the look is skipped instead.

use cerberus_shared::tr;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use cerberus_shared::Look;
use sha2::{Digest, Sha256};

const SHADERS: &str = include_str!("CerberusLooks.fx");
const SHADER_FILE: &str = "CerberusLooks.fx";
const PROXY: &str = "dxgi.dll";
const TAGS: &str = "https://api.github.com/repos/crosire/reshade/tags";

/// The looks a player can pick: id, name, ReShade technique.
pub const LOOKS: [(&str, &str, &str); 4] = [
    ("sharp", "Nítido", "CerberusSharp"),
    ("vivid", "Cores vivas", "CerberusVivid"),
    ("crt", "CRT", "CerberusCrt"),
    ("film", "Filme", "CerberusFilm"),
];

fn technique(id: &str) -> Option<&'static str> {
    LOOKS.iter().find(|(look, _, _)| *look == id).map(|(_, _, t)| *t)
}

fn reshade_dir(data: &Path) -> PathBuf {
    data.join("reshade")
}

fn game_dir(data: &Path, game_id: &str) -> PathBuf {
    let safe: String = game_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    data.join("looks").join(safe)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// How a game draws, from its program: only Direct3D games can take a look for now.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Dxgi { x64: bool },
    Unsupported(String),
}

/// `hay` already in lowercase; `needle` given in lowercase.
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    memchr::memmem::find(hay, needle).is_some()
}

/// What a program was found to draw with, by its path, size and date: reading
/// it means up to 96 MB, so once per build of the game, not once per visit.
static ROUTES: std::sync::Mutex<Option<std::collections::HashMap<(PathBuf, u64, Option<std::time::SystemTime>), Route>>> = std::sync::Mutex::new(None);

/// 64-bit or not, from the program's header.
fn is_64bit(bytes: &[u8]) -> Option<bool> {
    let pe = u32::from_le_bytes(bytes.get(0x3c..0x40)?.try_into().ok()?) as usize;
    if bytes.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    let machine = u16::from_le_bytes(bytes.get(pe + 4..pe + 6)?.try_into().ok()?);
    Some(machine == 0x8664 || machine == 0xAA64)
}

pub fn route(exe: &Path) -> Route {
    if !cfg!(windows) {
        return Route::Unsupported(tr!("Por enquanto, filtros só funcionam no Windows.").into());
    }
    let key = fs::metadata(exe).ok().map(|m| (exe.to_path_buf(), m.len(), m.modified().ok()));
    if let Some(key) = &key {
        if let Some(known) = ROUTES.lock().ok().and_then(|m| m.as_ref().and_then(|m| m.get(key).cloned())) {
            return known;
        }
    }
    let found = read_route(exe);
    if let (Some(key), Ok(mut cache)) = (key, ROUTES.lock()) {
        cache.get_or_insert_with(Default::default).insert(key, found.clone());
    }
    found
}

fn read_route(exe: &Path) -> Route {
    let mut bytes = Vec::new();
    let Ok(file) = fs::File::open(exe) else {
        return Route::Unsupported(tr!("O programa do jogo não foi encontrado.").into());
    };
    let _ = file.take(96 << 20).read_to_end(&mut bytes);
    if !bytes.starts_with(b"MZ") {
        return Route::Unsupported(tr!("Esse programa não é um jogo de Windows.").into());
    }
    let x64 = is_64bit(&bytes).unwrap_or(true);
    // Names appear in any case ("D3D11.dll"): lowercase once, then fast searches.
    bytes.make_ascii_lowercase();
    if [&b"d3d12.dll"[..], b"d3d11.dll", b"dxgi.dll", b"d3d10.dll"].iter().any(|n| contains(&bytes, n)) {
        return Route::Dxgi { x64 };
    }
    let what = if contains(&bytes, b"vulkan-1.dll") {
        "Vulkan"
    } else if contains(&bytes, b"opengl32.dll") {
        "OpenGL"
    } else if contains(&bytes, b"d3d9.dll") {
        "Direct3D 9"
    } else {
        tr!("um jeito de desenhar que o Cerberus não reconhece")
    };
    Route::Unsupported(tr!("Esse jogo usa {}. Por enquanto os filtros funcionam em jogos com Direct3D 10, 11 ou 12.", what))
}

/// ReShade is downloaded already.
pub fn ready(data: &Path) -> bool {
    reshade_dir(data).join("ReShade64.dll").is_file() && reshade_dir(data).join("ReShade32.dll").is_file()
}

/// The ReShade DLLs from its setup program, which carries them in a zip
/// starting on a 512-byte boundary.
fn dlls_from_setup(setup: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let bad = || tr!("O instalador do ReShade veio diferente do esperado.").to_string();
    if !setup.starts_with(b"MZ") {
        return Err(bad());
    }
    let start = (0..setup.len())
        .step_by(512)
        .find(|&i| setup[i..].starts_with(b"PK\x03\x04") && setup[i + 4..(i + 30).min(setup.len())].iter().any(|&b| b != 0))
        .ok_or_else(bad)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(&setup[start..])).map_err(|_| bad())?;
    let mut read = |name: &str| -> Result<Vec<u8>, String> {
        let mut file = zip.by_name(name).map_err(|_| bad())?;
        let mut out = Vec::new();
        file.read_to_end(&mut out).map_err(|_| bad())?;
        if !out.starts_with(b"MZ") {
            return Err(bad());
        }
        Ok(out)
    };
    Ok((read("ReShade64.dll")?, read("ReShade32.dll")?))
}

/// The newest ReShade version, `6.8.0`, from its repository's tags.
async fn latest(http: &reqwest::Client) -> Result<String, String> {
    let tags: serde_json::Value = http
        .get(TAGS)
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| tr!("Não foi possível ver a versão do ReShade: {}", e))?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    tags.as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t.get("name")?.as_str()?.strip_prefix('v').map(str::to_string))
        .find(|v| v.split('.').count() == 3 && v.split('.').all(|p| p.parse::<u32>().is_ok()))
        .ok_or_else(|| tr!("Não foi possível ver a versão do ReShade.").to_string())
}

/// Downloads ReShade from its developer's site, once.
pub async fn ensure(http: &reqwest::Client, data: &Path) -> Result<(), String> {
    if ready(data) {
        return Ok(());
    }
    let version = latest(http).await?;
    let url = format!("https://reshade.me/downloads/ReShade_Setup_{version}.exe");
    let reply = http
        .get(&url)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| tr!("Não foi possível baixar o ReShade: {}", e.without_url()))?;
    let setup = crate::net::read_limited(reply, 256 << 20).await.map_err(|e| tr!("Não foi possível baixar o ReShade: {}", e))?;
    let (x64, x86) = dlls_from_setup(&setup)?;
    let dir = reshade_dir(data);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Each DLL whole or not at all, and the version noted last: an interrupted
    // download is never taken as ready (a cut DLL would crash the game).
    crate::state_file::write_bytes(&dir.join("ReShade64.dll"), &x64)?;
    crate::state_file::write_bytes(&dir.join("ReShade32.dll"), &x86)?;
    crate::state_file::write_bytes(&dir.join("version.txt"), version.as_bytes())
}

/// What a game start with a look needs: the environment for ReShade. The file
/// put beside the game is noted in the game's looks folder, to take away later.
#[derive(Debug, Default)]
pub struct Prepared {
    pub env: Vec<(String, String)>,
}

/// ReShade's settings for one game: where our shaders are and which look is on.
fn write_settings(data: &Path, base: &Path, look: &Look, technique: &str, lines: Option<u32>) -> Result<(), String> {
    let shaders = reshade_dir(data).join("shaders");
    fs::create_dir_all(&shaders).map_err(|e| e.to_string())?;
    let shader = shaders.join(SHADER_FILE);
    if fs::read_to_string(&shader).ok().as_deref() != Some(SHADERS) {
        fs::write(&shader, SHADERS).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(base).map_err(|e| e.to_string())?;
    // ReShade splits values at commas; a doubled comma is a comma.
    let path = |p: &Path| p.display().to_string().replace(',', ",,");
    let preset = base.join("Cerberus.ini");
    let strength = f32::from(look.strength.min(100)) / 100.0;
    let lines = lines.filter(|n| (1..=1200).contains(n)).unwrap_or(0);
    fs::write(&preset, format!("Techniques={technique}@{SHADER_FILE}\nTechniqueSorting={technique}@{SHADER_FILE}\n\n[{SHADER_FILE}]\nStrength={strength:.2}\nLines={lines}\n"))
        .map_err(|e| e.to_string())?;
    let ini = format!(
        "[GENERAL]\nEffectSearchPaths={}\nTextureSearchPaths={}\nPresetPath={}\nIntermediateCachePath={}\nNoReloadOnInit=0\n\n[OVERLAY]\nTutorialProgress=4\n\n[INPUT]\nKeyScreenshot=0,0,0,0\n",
        path(&shaders),
        path(&shaders),
        path(&preset),
        path(&base.join("cache"))
    );
    fs::write(base.join("ReShade.ini"), ini).map_err(|e| e.to_string())
}

fn note_file(data: &Path, game_id: &str) -> PathBuf {
    game_dir(data, game_id).join("placed.txt")
}

/// Gets a game start ready for its look: ReShade beside the program, unless a
/// `dxgi.dll` that isn't ours is there already. `lines`: the game's own line
/// count for the CRT look, when its developer gives one.
pub fn prepare(data: &Path, game_id: &str, exe: &Path, look: &Look, lines: Option<u32>) -> Result<Prepared, String> {
    let technique = technique(&look.id).ok_or(tr!("Esse filtro não existe mais."))?;
    let Route::Dxgi { x64 } = route(exe) else {
        return Err(tr!("Esse jogo não aceita filtros.").into());
    };
    let dll = fs::read(reshade_dir(data).join(if x64 { "ReShade64.dll" } else { "ReShade32.dll" }))
        .map_err(|_| tr!("O ReShade ainda não foi baixado. Escolha o filtro de novo na aba Ajustes.").to_string())?;
    let base = game_dir(data, game_id);
    write_settings(data, &base, look, technique, lines)?;
    let target = exe.parent().ok_or(tr!("O jogo não está numa pasta."))?.join(PROXY);
    let note = note_file(data, game_id);
    if target.exists() && !placed_by_us(&note, &target) {
        return Err(tr!("A pasta do jogo já tem um {0} (talvez um ReShade seu ou outro mod), então o filtro ficou desligado para não mexer nele.", PROXY));
    }
    fs::write(&note, format!("{}\n{}\n", target.display(), sha256(&dll))).map_err(|e| e.to_string())?;
    fs::write(&target, &dll).map_err(|e| tr!("Não foi possível colocar o filtro na pasta do jogo: {}", e))?;
    Ok(Prepared { env: vec![("RESHADE_BASE_PATH_OVERRIDE".into(), base.display().to_string())] })
}

/// The file is the one we put there, unchanged since.
fn placed_by_us(note: &Path, file: &Path) -> bool {
    let Ok(text) = fs::read_to_string(note) else { return false };
    let mut lines = text.lines();
    let (Some(path), Some(sum)) = (lines.next(), lines.next()) else { return false };
    Path::new(path) == file && fs::read(file).is_ok_and(|bytes| sha256(&bytes) == sum)
}

/// Takes ReShade away from beside a game once it has closed. Only our own,
/// unchanged copy is ever deleted.
pub fn take_back(data: &Path, game_id: &str) {
    let note = note_file(data, game_id);
    let Ok(text) = fs::read_to_string(&note) else { return };
    let Some(path) = text.lines().next().map(PathBuf::from) else { return };
    if placed_by_us(&note, &path) {
        // A game closing lets go of it a moment later: a few tries over some
        // seconds; still held, the next start (with or without a filter) or
        // the next app start tries again.
        let mut removed = false;
        for attempt in 0..10 {
            if fs::remove_file(&path).is_ok() || !path.exists() {
                removed = true;
                break;
            }
            if attempt < 9 {
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
        if !removed {
            return;
        }
    }
    let _ = fs::remove_file(&note);
}

/// After a crash or a forced close: every copy still beside a game goes.
pub fn take_back_all(data: &Path) {
    let Ok(games) = fs::read_dir(data.join("looks")) else { return };
    for game in games.flatten() {
        let note = game.path().join("placed.txt");
        let Ok(text) = fs::read_to_string(&note) else { continue };
        if let Some(path) = text.lines().next().map(PathBuf::from) {
            if placed_by_us(&note, &path) && fs::remove_file(&path).is_err() {
                continue;
            }
        }
        let _ = fs::remove_file(&note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn a_filter_held_for_a_moment_is_still_taken_back() {
        use std::os::windows::fs::OpenOptionsExt;
        let data = tempfile::tempdir().unwrap();
        let game = tempfile::tempdir().unwrap();
        let dll = game.path().join(PROXY);
        fs::write(&dll, b"reshade").unwrap();
        let note = note_file(data.path(), "g");
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, format!("{}
{}
", dll.display(), sha256(b"reshade"))).unwrap();
        // A game still closing: readable, not deletable, for 600 ms.
        let held = fs::OpenOptions::new().read(true).share_mode(1).open(&dll).unwrap();
        let letting_go = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(600));
            drop(held);
        });
        take_back(data.path(), "g");
        letting_go.join().unwrap();
        assert!(!dll.exists(), "taken back once the game let go");
        assert!(!note.exists());
    }

    /// A tiny 64-bit program that names Direct3D 11.
    fn program(dir: &Path, names: &[u8]) -> PathBuf {
        let mut b = vec![0u8; 0x200];
        b[0] = b'M';
        b[1] = b'Z';
        b[0x3c] = 0x80;
        b[0x80..0x84].copy_from_slice(b"PE\0\0");
        b[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        b.extend_from_slice(names);
        // One file per program: `route` remembers a program by path, size and
        // date, and two fakes of one size written in the same instant would look alike.
        let tag: String = String::from_utf8_lossy(names).chars().filter(char::is_ascii_alphanumeric).collect();
        let exe = dir.join(format!("Game-{tag}.exe"));
        fs::write(&exe, b).unwrap();
        exe
    }

    fn fake_reshade(data: &Path) {
        let dir = reshade_dir(data);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ReShade64.dll"), b"MZ reshade 64").unwrap();
        fs::write(dir.join("ReShade32.dll"), b"MZ reshade 32").unwrap();
    }

    #[test]
    fn only_direct3d_games_take_a_look() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(route(&program(dir.path(), b"d3d11.dll")), Route::Dxgi { x64: true });
        assert!(matches!(route(&program(dir.path(), b"vulkan-1.dll")), Route::Unsupported(r) if r.contains("Vulkan")));
        assert!(matches!(route(&program(dir.path(), b"OPENGL32.dll")), Route::Unsupported(r) if r.contains("OpenGL")));
    }

    #[test]
    fn reshade_goes_beside_the_game_and_only_ours_is_taken_back() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let game = dir.path().join("game");
        fs::create_dir_all(&game).unwrap();
        fake_reshade(&data);
        let exe = program(&game, b"d3d11.dll");
        let look = Look { id: "crt".into(), strength: 60 };

        let run = prepare(&data, "recomp:x", &exe, &look, Some(448)).unwrap();
        assert_eq!(fs::read(game.join(PROXY)).unwrap(), b"MZ reshade 64");
        let base = PathBuf::from(&run.env[0].1);
        assert!(fs::read_to_string(base.join("Cerberus.ini")).unwrap().contains("Techniques=CerberusCrt@CerberusLooks.fx"));
        assert!(fs::read_to_string(base.join("Cerberus.ini")).unwrap().contains("Strength=0.60\nLines=448"));
        take_back(&data, "recomp:x");
        assert!(!game.join(PROXY).exists());
        assert_eq!(fs::read_dir(&game).unwrap().count(), 1, "only the game's own program is left");

        // The player's own dxgi.dll: never replaced, never deleted.
        fs::write(game.join(PROXY), b"MZ their own").unwrap();
        assert!(prepare(&data, "recomp:x", &exe, &look, None).unwrap_err().contains("já tem um dxgi.dll"));
        take_back(&data, "recomp:x");
        take_back_all(&data);
        assert_eq!(fs::read(game.join(PROXY)).unwrap(), b"MZ their own");
    }

    #[test]
    fn a_copy_left_by_a_crash_goes_on_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        let game = dir.path().join("game");
        fs::create_dir_all(&game).unwrap();
        fake_reshade(&data);
        let exe = program(&game, b"dxgi.dll");
        prepare(&data, "local:y", &exe, &Look { id: "vivid".into(), strength: 100 }, None).unwrap();
        take_back_all(&data);
        assert!(!game.join(PROXY).exists());
    }

    #[test]
    fn the_dlls_come_out_of_the_setup_program() {
        let mut zip_bytes = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut zip_bytes);
            let options = zip::write::SimpleFileOptions::default();
            for name in ["ReShade64.dll", "ReShade32.dll"] {
                zip.start_file(name, options).unwrap();
                std::io::Write::write_all(&mut zip, b"MZ dll").unwrap();
            }
            zip.finish().unwrap();
        }
        let mut setup = b"MZ setup program".to_vec();
        setup.resize(1024, 0);
        setup.extend_from_slice(zip_bytes.get_ref());
        let (x64, x86) = dlls_from_setup(&setup).unwrap();
        assert_eq!((x64.as_slice(), x86.as_slice()), (&b"MZ dll"[..], &b"MZ dll"[..]));
        assert!(dlls_from_setup(b"not a program").is_err());
    }
}
