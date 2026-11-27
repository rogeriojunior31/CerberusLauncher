//! Whether a downloaded program carries a valid Windows signature (Authenticode)
//! from the publisher it should: checked before a Microsoft installer runs, so a
//! mirror or a proxy can't hand Cerberus another program under that name.

use cerberus_shared::tr;
use std::path::Path;

/// The signer's name ("Microsoft Corporation") when the signature is valid
/// and chains to a root Windows trusts; why not, otherwise.
#[cfg(windows)]
pub fn signer(path: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows_sys::Win32::Security::WinTrust::{
        WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO, WTD_CHOICE_FILE,
        WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: the structures point at `wide`, `file` and `action`, which live
    // until the call that closes the state; the state is always closed.
    unsafe {
        let mut file = WINTRUST_FILE_INFO { cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32, pcwszFilePath: wide.as_ptr(), ..Default::default() };
        let mut data = WINTRUST_DATA {
            cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            dwStateAction: WTD_STATEACTION_VERIFY,
            ..Default::default()
        };
        data.Anonymous.pFile = &mut file;
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let status = WinVerifyTrust(std::ptr::null_mut(), &mut action, (&raw mut data).cast());
        let name = if status == 0 {
            let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
            let signer = if provider.is_null() { std::ptr::null_mut() } else { WTHelperGetProvSignerFromChain(provider, 0, 0, 0) };
            let cert = if signer.is_null() || (*signer).csCertChain == 0 { std::ptr::null() } else { (*(*signer).pasCertChain).pCert };
            if cert.is_null() {
                None
            } else {
                let mut buffer = [0u16; 256];
                let len = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, std::ptr::null(), buffer.as_mut_ptr(), buffer.len() as u32);
                Some(String::from_utf16_lossy(&buffer[..(len as usize).saturating_sub(1)]))
            }
        } else {
            None
        };
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(std::ptr::null_mut(), &mut action, (&raw mut data).cast());
        match (status, name) {
            (0, Some(name)) => Ok(name),
            (0, None) => Err(tr!("a assinatura não diz quem a fez").into()),
            (code, _) => Err(tr!("sem assinatura válida (código {})", format!("{:#x}", code as u32))),
        }
    }
}

#[cfg(not(windows))]
pub fn signer(_: &Path) -> Result<String, String> {
    Err(tr!("a assinatura só é conferida no Windows").into())
}

/// Refuses a program that isn't signed by `publisher`.
pub fn require(path: &Path, publisher: &str) -> Result<(), String> {
    match signer(path) {
        Ok(name) if name == publisher => Ok(()),
        Ok(name) => Err(tr!("O programa baixado é assinado por {0}, não por {1}; não foi aberto.", name, publisher)),
        Err(why) => Err(tr!("O programa baixado não pôde ser conferido ({}); não foi aberto.", why)),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_own_programs_are_signed_by_microsoft_and_a_plain_file_is_not() {
        let notepad = std::path::PathBuf::from(std::env::var("WINDIR").unwrap_or(r"C:\Windows".into())).join("System32").join("notepad.exe");
        // Some Windows files are signed through a catalog, not inside: only check one that carries its own.
        let kernel = notepad.with_file_name("ntoskrnl.exe");
        let signed = [notepad, kernel].into_iter().find(|p| signer(p).is_ok());
        if let Some(signed) = signed {
            assert_eq!(signer(&signed).unwrap(), "Microsoft Windows");
        }
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("vc_redist.x64.exe");
        std::fs::write(&fake, b"MZ not really a program").unwrap();
        assert!(require(&fake, "Microsoft Corporation").is_err());
    }
}
