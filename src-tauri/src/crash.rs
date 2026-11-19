//! When a game ends badly: what its exit code usually means, in words the
//! player can act on, and the log it left behind, if any.

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
        0xC000_0135 => "Falta uma DLL que o jogo precisa. Quase sempre é o Visual C++ 2015-2022 (x64) ou o DirectX.".to_string(),
        0xC000_0139 => "Uma DLL do Windows é antiga demais para o jogo, quase sempre o Visual C++ 2015-2022 (x64). Instale a versão mais nova, do site da Microsoft.".to_string(),
        0xC000_007B => "O jogo encontrou uma DLL de 32 bits ou corrompida. Reinstalar o Visual C++ 2015-2022 (x64) costuma resolver.".to_string(),
        0xC000_0005 => "O jogo travou (acesso inválido à memória). Se acontecer sempre no mesmo ponto, atualize o recomp ou o driver de vídeo.".to_string(),
        0xC000_00FD => "O jogo travou (estouro de pilha).".to_string(),
        0xC000_0409 | 0xC000_0602 => "O jogo se encerrou por um erro interno dele.".to_string(),
        0xE06D_7363 => "O jogo parou por um erro interno dele (exceção não tratada).".to_string(),
        0x8000_0003 => "O jogo parou num ponto de depuração; costuma ser uma versão de teste do recomp.".to_string(),
        s if s >= 0xC000_0000 => format!("O jogo travou (código 0x{s:08X})."),
        _ if seconds < 60 => format!("O jogo fechou com o código {code} pouco depois de abrir."),
        _ => return None,
    };
    Some(text)
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
