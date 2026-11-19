//! What the PC has that games need, as RecompLauncher's "Your PC" checks:
//! the Visual C++ 2015-2022 runtime (x64), the usual reason a recomp won't
//! start (a missing or old DLL). It is installed from Microsoft's own link.

use std::path::Path;
use std::time::Duration;

/// Recomps built with recent compilers need at least this runtime (14.40).
const NEWEST_NEEDED: (u32, u32) = (14, 40);
const VC_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";

/// "14.52.37025.00" -> (14, 52).
fn major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.trim_start_matches(['v', 'V']).split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Installed, and recent enough for today's recomps.
pub fn recent_enough(version: &str) -> bool {
    major_minor(version).is_some_and(|v| v >= NEWEST_NEEDED)
}

/// The installed Visual C++ 2015-2022 x64 runtime's version, if any.
#[cfg(windows)]
pub fn vc_version() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let places = [r"SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64", r"SOFTWARE\WOW6432Node\Microsoft\VisualStudio\14.0\VC\Runtimes\x64"];
    places.iter().find_map(|path| {
        let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(path).ok()?;
        let installed: u32 = key.get_value("Installed").ok()?;
        let version: String = key.get_value("Version").ok()?;
        (installed == 1).then(|| version.trim_start_matches('v').to_string())
    })
}

#[cfg(not(windows))]
pub fn vc_version() -> Option<String> {
    None
}

/// Downloads Microsoft's installer and runs it (Windows asks the player for
/// permission). Returns the version installed afterwards.
pub async fn install_vc(http: &reqwest::Client, downloads: &Path) -> Result<Option<String>, String> {
    let reply = http
        .get(VC_URL)
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("Não foi possível baixar o Visual C++: {e}"))?;
    // Only Microsoft's own servers, wherever aka.ms points.
    let host = reply.url().host_str().unwrap_or_default().to_lowercase();
    if !(host == "microsoft.com" || host.ends_with(".microsoft.com")) {
        return Err(format!("O link da Microsoft levou a outro site ({host}); a instalação foi cancelada."));
    }
    let bytes = reply.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() < 5_000_000 || !bytes.starts_with(b"MZ") {
        return Err("O instalador do Visual C++ veio incompleto.".into());
    }
    std::fs::create_dir_all(downloads).map_err(|e| e.to_string())?;
    let installer = downloads.join("vc_redist.x64.exe");
    std::fs::write(&installer, &bytes).map_err(|e| e.to_string())?;
    let status = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new(&installer).args(["/install", "/passive", "/norestart"]).status()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("Não foi possível abrir o instalador: {e}"))?;
    match status.code() {
        // Done; already newer; done but a restart is due.
        Some(0 | 1638 | 3010) => Ok(vc_version()),
        Some(1602) => Err("Instalação cancelada.".into()),
        Some(code) => Err(format!("O instalador do Visual C++ terminou com o código {code}.")),
        None => Err("O instalador do Visual C++ foi interrompido.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_and_compared() {
        assert!(recent_enough("14.52.37025.00"));
        assert!(recent_enough("v14.40.33810.00"));
        assert!(!recent_enough("14.29.30139.00"));
        assert!(!recent_enough(""));
    }

    #[cfg(windows)]
    #[test]
    fn reads_this_pcs_runtime_when_there_is_one() {
        // Most development PCs have it; when present it must parse.
        if let Some(v) = vc_version() {
            assert!(major_minor(&v).is_some(), "unreadable version {v}");
        }
    }
}
