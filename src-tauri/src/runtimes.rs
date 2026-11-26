//! What the PC has that games need, as RecompLauncher's "Your PC" checks:
//! the Visual C++ 2015-2022 runtime (x64), the usual reason a recomp won't
//! start (a missing or old DLL), and DirectX's older parts (D3DX, XInput 1.3,
//! XAudio 2.7) that Windows doesn't bring and older PC ports still load. Both
//! are installed from Microsoft's own links.

use cerberus_shared::tr;
use std::path::Path;
use std::time::Duration;

/// Recomps built with recent compilers need at least this runtime (14.40).
const NEWEST_NEEDED: (u32, u32) = (14, 40);
const VC_URL: &str = "https://aka.ms/vs/17/release/vc_redist.x64.exe";
/// The DirectX End-User Runtime web installer: it fetches only the parts missing.
const DX_URL: &str = "https://download.microsoft.com/download/1/7/1/1718CCC4-6315-4D8E-9543-8E28A4E18C4C/dxwebsetup.exe";
/// DirectX's older parts, one DLL each, as the June 2010 runtime installs them.
const DX_PARTS: [(&str, &str); 4] = [("d3dx9_43.dll", "D3DX9"), ("d3dx11_43.dll", "D3DX11"), ("xinput1_3.dll", "XInput 1.3"), ("XAudio2_7.dll", "XAudio 2.7")];

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

/// DirectX's older parts this PC lacks, by name; empty when it has them all.
#[cfg(windows)]
pub fn dx_missing() -> Vec<String> {
    let system = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(|| r"C:\Windows".into()).join("System32");
    DX_PARTS.iter().filter(|(dll, _)| !system.join(dll).is_file()).map(|(_, name)| (*name).to_string()).collect()
}

#[cfg(not(windows))]
pub fn dx_missing() -> Vec<String> {
    Vec::new()
}

/// Installs or updates Visual C++. Returns the version installed afterwards.
pub async fn install_vc(http: &reqwest::Client, downloads: &Path) -> Result<Option<String>, String> {
    run_microsoft_installer(http, downloads, tr!("Visual C++"), VC_URL, "vc_redist.x64.exe", 5_000_000, &["/install", "/passive", "/norestart"]).await?;
    Ok(vc_version())
}

/// Installs DirectX's older parts. Returns what is still missing afterwards.
pub async fn install_dx(http: &reqwest::Client, downloads: &Path) -> Result<Vec<String>, String> {
    run_microsoft_installer(http, downloads, "DirectX", DX_URL, "dxwebsetup.exe", 200_000, &["/Q"]).await?;
    Ok(dx_missing())
}

/// Downloads one of Microsoft's installers and runs it with `args` (Windows
/// asks the player for permission); `smallest` guards against a cut download.
async fn run_microsoft_installer(http: &reqwest::Client, downloads: &Path, what: &str, url: &str, file: &str, smallest: usize, args: &[&str]) -> Result<(), String> {
    let reply = http
        .get(url)
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| tr!("Não foi possível baixar o {}: {}", what, e))?;
    // Only Microsoft's own servers, wherever the link points.
    let host = reply.url().host_str().unwrap_or_default().to_lowercase();
    if !(host == "microsoft.com" || host.ends_with(".microsoft.com")) {
        return Err(tr!("O link da Microsoft levou a outro site ({}); a instalação foi cancelada.", host));
    }
    let bytes = reply.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() < smallest || !bytes.starts_with(b"MZ") {
        return Err(tr!("O instalador do {} veio incompleto.", what));
    }
    std::fs::create_dir_all(downloads).map_err(|e| e.to_string())?;
    let installer = downloads.join(file);
    std::fs::write(&installer, &bytes).map_err(|e| e.to_string())?;
    let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
    // Windows' permission prompt when the installer asks for administrator rights.
    let code = tauri::async_runtime::spawn_blocking(move || match std::process::Command::new(&installer).args(&args).status() {
        Ok(status) => Ok(status.code()),
        Err(e) if crate::elevated::needs_admin(&e) => {
            let dir = installer.parent().map(Path::to_path_buf).unwrap_or_default();
            crate::elevated::run_and_wait(&installer, &args.join(" "), &dir).map_err(crate::elevated::failure)
        }
        Err(e) => Err(tr!("Não foi possível abrir o instalador: {}", e)),
    })
    .await
    .map_err(|e| e.to_string())??;
    match code {
        // Done; already newer; done but a restart is due.
        Some(0 | 1638 | 3010) => Ok(()),
        Some(1602) => Err(tr!("Instalação cancelada.").into()),
        Some(code) => Err(tr!("O instalador do {} terminou com o código {}.", what, code)),
        None => Err(tr!("O instalador do {} foi interrompido.", what)),
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
    fn checks_directx_by_its_files() {
        // Every name it reports is one it checks.
        assert!(dx_missing().iter().all(|m| DX_PARTS.iter().any(|(_, n)| n == m)));
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
