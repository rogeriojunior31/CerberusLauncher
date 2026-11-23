//! When a game ends badly: what its exit code usually means, in words the
//! player can act on, and the log it left behind, if any.

use cerberus_shared::tr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Why a game's end looks like a crash, or `None` when it closed normally.
/// Windows' own crash codes always count; any other non-zero code only when
/// the game lasted under a minute (some games return 1 on a normal quit).
pub fn reason(code: i32, seconds: u64) -> Option<String> {
    let status = code as u32;
    let text = match status {
        0 => return None,
        0xC000_0135 => tr!("Falta uma DLL que o jogo precisa. Quase sempre é o Visual C++ 2015-2022 (x64): instale em Configurações › Seu PC.").to_string(),
        0xC000_0139 => tr!("Uma DLL do Windows é antiga demais para o jogo, quase sempre o Visual C++ 2015-2022 (x64). Atualize em Configurações › Seu PC.").to_string(),
        0xC000_007B => tr!("O jogo encontrou uma DLL de 32 bits ou corrompida. Reinstalar o Visual C++ em Configurações › Seu PC costuma resolver.").to_string(),
        0xC000_0005 => tr!("O jogo travou (acesso inválido à memória). Se acontecer sempre no mesmo ponto, atualize o recomp ou o driver de vídeo.").to_string(),
        0xC000_00FD => tr!("O jogo travou (estouro de pilha).").to_string(),
        0xC000_0409 | 0xC000_0602 => tr!("O jogo se encerrou por um erro interno dele.").to_string(),
        0xE06D_7363 => tr!("O jogo parou por um erro interno dele (exceção não tratada).").to_string(),
        0x8000_0003 => tr!("O jogo parou num ponto de depuração; costuma ser uma versão de teste do recomp.").to_string(),
        s if s >= 0xC000_0000 => tr!("O jogo travou (código 0x{}).", format!("{s:08X}")),
        _ if seconds < 60 => tr!("O jogo fechou com o código {} pouco depois de abrir.", code),
        _ => return None,
    };
    Some(text)
}

/// Windows' "a DLL is too old for this program" (0xC0000139).
pub const OLD_DLL: u32 = 0xC000_0139;

/// The runtime DLLs games often carry next to their program.
const RUNTIME_DLLS: [&str; 6] = ["vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll", "concrt140.dll"];

/// The oldest Vulkan loader today's recomps start with.
const VULKAN_NEEDED: [u16; 2] = [1, 2];

pub type Version = [u16; 4];

fn shown(v: Version) -> String {
    format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3])
}

/// What was found on the PC when a game closed with 0xC0000139.
#[derive(Default)]
pub struct OldDllClues {
    /// A runtime DLL in the game's folder older than Windows' own: name, the game's, Windows'.
    pub bundled_older: Vec<(String, Version, Version)>,
    /// The Visual C++ runtime's version, when it is too old for today's recomps.
    pub vc_too_old: Option<String>,
    /// The Vulkan loader's version, when it is older than recomps need.
    pub vulkan_too_old: Option<Version>,
}

/// Which DLL is too old, most likely first: a copy the game brought along,
/// then Visual C++, then the graphics driver's Vulkan. `None` when nothing
/// stands out, and the general explanation applies.
pub fn old_dll_cause(clues: &OldDllClues) -> Option<String> {
    if let Some((name, theirs, windows)) = clues.bundled_older.first() {
        return Some(tr!("A pasta do jogo traz uma cópia antiga de {2} ({0}), mais velha que a do Windows ({1}). Apagar ou renomear essa cópia na pasta do jogo costuma resolver: o Windows usa a dele.", shown(*theirs), shown(*windows), name));
    }
    if let Some(vc) = &clues.vc_too_old {
        return Some(tr!("O Visual C++ deste PC é antigo ({}) para este jogo. Atualize em Configurações › Seu PC.", vc));
    }
    if let Some(v) = clues.vulkan_too_old {
        return Some(tr!("O Vulkan do driver de vídeo é antigo ({}). Atualize o driver da placa de vídeo pelo site da NVIDIA, AMD ou Intel.", shown(v)));
    }
    None
}

/// Looks at the game's folder and the PC for the clues above.
pub fn old_dll_clues(program_dir: &Path) -> OldDllClues {
    let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");
    let bundled_older = RUNTIME_DLLS
        .iter()
        .filter_map(|dll| {
            let theirs = file_version(&program_dir.join(dll))?;
            let windows = file_version(&system.join(dll))?;
            (theirs < windows).then(|| ((*dll).to_string(), theirs, windows))
        })
        .collect();
    let vc_too_old = crate::runtimes::vc_version().filter(|v| !crate::runtimes::recent_enough(v));
    let vulkan_too_old = file_version(&system.join("vulkan-1.dll")).filter(|v| v[..2] < VULKAN_NEEDED[..]);
    OldDllClues { bundled_older, vc_too_old, vulkan_too_old }
}

/// A program file's version, as Windows shows it in the file's properties.
#[cfg(windows)]
pub fn file_version(path: &Path) -> Option<Version> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW};
    if !path.is_file() {
        return None;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: every pointer handed over is valid for the call; the buffer is
    // the size Windows asked for, and the fixed info is checked to fit in it.
    unsafe {
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), std::ptr::null_mut());
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut info: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let root = [u16::from(b'\\'), 0];
        if VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), &mut info, &mut len) == 0 || info.is_null() || (len as usize) < size_of::<VS_FIXEDFILEINFO>() {
            return None;
        }
        let fixed = std::ptr::read_unaligned(info.cast::<VS_FIXEDFILEINFO>());
        Some([(fixed.dwFileVersionMS >> 16) as u16, fixed.dwFileVersionMS as u16, (fixed.dwFileVersionLS >> 16) as u16, fixed.dwFileVersionLS as u16])
    }
}

#[cfg(not(windows))]
pub fn file_version(_path: &Path) -> Option<Version> {
    None
}

/// The newest log file the game wrote during the session, near its program.
pub fn newest_log(dir: &Path, since: SystemTime) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    let mut look = |folder: &Path| {
        let Ok(entries) = fs::read_dir(folder) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_log = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("log") || x.eq_ignore_ascii_case("txt"))
                && path.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy().to_lowercase();
                    n.ends_with(".log") || n.contains("log")
                });
            let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else { continue };
            if is_log && modified >= since && best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, path));
            }
        }
    };
    look(dir);
    for sub in ["logs", "log", "Logs", "game-output"] {
        look(&dir.join(sub));
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn crash_codes_are_explained_and_normal_ends_are_not() {
        assert_eq!(reason(0, 5), None);
        assert!(reason(0xC000_0135_u32 as i32, 3600).unwrap().contains("Visual C++"));
        assert!(reason(0xC000_0005_u32 as i32, 3600).unwrap().contains("travou"));
        assert!(reason(0xC0AB_CDEF_u32 as i32, 3600).unwrap().contains("0xC0ABCDEF"));
        // A plain code counts only right after opening.
        assert!(reason(1, 10).is_some());
        assert_eq!(reason(1, 600), None);
    }

    #[test]
    fn names_the_old_dll_most_likely_to_blame() {
        assert_eq!(old_dll_cause(&OldDllClues::default()), None);
        let mut clues = OldDllClues { vc_too_old: Some("14.29".into()), vulkan_too_old: Some([1, 1, 0, 0]), ..Default::default() };
        assert!(old_dll_cause(&clues).unwrap().contains("Visual C++"));
        clues.bundled_older.push(("msvcp140.dll".into(), [14, 16, 0, 0], [14, 44, 0, 0]));
        let why = old_dll_cause(&clues).unwrap();
        assert!(why.contains("msvcp140.dll") && why.contains("14.16.0.0"), "{why}");
        clues = OldDllClues { vulkan_too_old: Some([1, 1, 70, 0]), ..Default::default() };
        assert!(old_dll_cause(&clues).unwrap().contains("driver"));
    }

    #[cfg(windows)]
    #[test]
    fn reads_a_program_files_version() {
        let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
        let v = file_version(&system.join("kernel32.dll")).expect("kernel32 has a version");
        assert!(v[0] >= 6, "{v:?}");
        assert_eq!(file_version(&system.join("no-such-file.dll")), None);
    }

    #[test]
    fn finds_the_log_written_during_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let before = SystemTime::now() - Duration::from_secs(5);
        std::fs::write(dir.path().join("readme.txt"), b"x").unwrap();
        std::fs::create_dir_all(dir.path().join("logs")).unwrap();
        std::fs::write(dir.path().join("logs").join("game.log"), b"boom").unwrap();
        assert_eq!(newest_log(dir.path(), before), Some(dir.path().join("logs").join("game.log")));
        // Nothing written since: no log to point at.
        assert_eq!(newest_log(dir.path(), SystemTime::now() + Duration::from_secs(60)), None);
    }
}
