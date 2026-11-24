//! A game's shortcut on the desktop, as RecompLauncher and Hydra make. It
//! starts Cerberus with `--play <id>`, so the session is tracked as usual:
//! playtime, save backups, picture filters.

use cerberus_shared::tr;
use std::path::{Path, PathBuf};

/// The `--play` argument's game, from a command line.
pub fn play_request(args: &[String]) -> Option<String> {
    let at = args.iter().position(|a| a == "--play")?;
    args.get(at + 1).filter(|id| !id.is_empty()).cloned()
}

/// A file name Windows accepts, from a game's name.
pub fn file_name(game: &str) -> String {
    let clean: String = game.chars().map(|c| if "<>:\"/\\|?*".contains(c) || c.is_control() { ' ' } else { c }).collect();
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean = clean.trim_matches('.').trim();
    if clean.is_empty() { tr!("Jogo").to_string() } else { clean.to_string() }
}

/// Writes `<desktop>/<name>.lnk` starting Cerberus for this game.
#[cfg(windows)]
pub fn create(desktop: &Path, name: &str, game_id: &str, icon: Option<&Path>) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let link = desktop.join(format!("{}.lnk", file_name(name)));
    let args = format!("--play \"{}\"", game_id.replace('"', ""));
    let icon = icon.filter(|p| p.is_file()).map(Path::to_path_buf).unwrap_or_else(|| exe.clone());
    let (link_path, name) = (link.clone(), name.to_string());
    // COM wants its own thread; the shortcut is written there and waited for.
    std::thread::spawn(move || write_link(&link_path, &exe, &args, &icon, &name))
        .join()
        .map_err(|_| tr!("Não foi possível criar o atalho.").to_string())??;
    Ok(link)
}

#[cfg(windows)]
fn write_link(link: &Path, exe: &Path, args: &str, icon: &Path, name: &str) -> Result<(), String> {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};

    let failed = |e: windows::core::Error| tr!("Não foi possível criar o atalho: {}", e.message());
    unsafe {
        let started = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| {
            let shell: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(failed)?;
            shell.SetPath(&HSTRING::from(exe.as_os_str())).map_err(failed)?;
            shell.SetArguments(&HSTRING::from(args)).map_err(failed)?;
            if let Some(dir) = exe.parent() {
                shell.SetWorkingDirectory(&HSTRING::from(dir.as_os_str())).map_err(failed)?;
            }
            shell.SetIconLocation(&HSTRING::from(icon.as_os_str()), 0).map_err(failed)?;
            shell.SetDescription(&HSTRING::from(tr!("Jogar {} pelo Cerberus", name))).map_err(failed)?;
            let file: IPersistFile = shell.cast().map_err(failed)?;
            file.Save(&HSTRING::from(link.as_os_str()), true).map_err(failed)
        })();
        if started {
            CoUninitialize();
        }
        result
    }
}

#[cfg(not(windows))]
pub fn create(_desktop: &Path, _name: &str, _game_id: &str, _icon: Option<&Path>) -> Result<PathBuf, String> {
    Err(tr!("Atalhos só no Windows por enquanto.").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_play_argument_and_safe_names() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(play_request(&args(&["cerberus.exe", "--play", "recomp:zelda64recomp"])).as_deref(), Some("recomp:zelda64recomp"));
        assert_eq!(play_request(&args(&["cerberus.exe", "--autostart"])), None);
        assert_eq!(play_request(&args(&["cerberus.exe", "--play"])), None);
        assert_eq!(file_name("Zelda: Majora's Mask?"), "Zelda Majora's Mask");
        assert_eq!(file_name("..."), "Jogo");
    }

    #[cfg(windows)]
    #[test]
    fn writes_a_shortcut_windows_can_read() {
        let dir = tempfile::tempdir().unwrap();
        let link = create(dir.path(), "Mario Kart: 64", "recomp:spaghettikart", None).unwrap();
        assert_eq!(link.file_name().unwrap(), "Mario Kart 64.lnk");
        let bytes = std::fs::read(&link).unwrap();
        // A shell link starts with its header size and class id.
        assert_eq!(&bytes[..4], &[0x4C, 0, 0, 0]);
        // The arguments are stored as UTF-16 inside it.
        let wide: Vec<u8> = "recomp:spaghettikart".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert!(bytes.windows(wide.len()).any(|w| w == wide.as_slice()));
    }
}
