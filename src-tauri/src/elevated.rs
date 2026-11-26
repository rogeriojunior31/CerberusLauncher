//! Installers that ask for administrator rights. Starting one the plain way
//! fails with error 740; it is started again the way Explorer does ("runas"),
//! so Windows shows its permission prompt, and its end is waited for.

use cerberus_shared::tr;
use std::path::Path;

/// Windows' "this program needs administrator rights".
pub const NEEDS_ADMIN: i32 = 740;
/// The player said no to Windows' permission prompt.
pub const DECLINED: i32 = 1223;

/// Whether starting a program failed only because it asks for administrator rights.
pub fn needs_admin(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(NEEDS_ADMIN)
}

/// What the player reads when starting through the permission prompt failed.
pub fn failure(code: i32) -> String {
    match code {
        DECLINED => tr!("O instalador pede permissão de administrador e ela foi recusada. Abra de novo e aceite o pedido do Windows para instalar.").into(),
        code => tr!("Não foi possível abrir o instalador como administrador (código {}).", code),
    }
}

/// Starts `program` through Windows' permission prompt and waits for it to
/// end: its exit code, or the Windows error that kept it from starting.
#[cfg(windows)]
pub fn run_and_wait(program: &Path, args: &str, dir: &Path) -> Result<Option<i32>, i32> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    let wide = |s: &std::ffi::OsStr| s.encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (verb, file, params, folder) = (wide("runas".as_ref()), wide(program.as_os_str()), wide(args.as_ref()), wide(dir.as_os_str()));
    // SAFETY: every pointer is to a buffer that lives until the call returns;
    // the process handle is closed once its exit code is read.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = if args.is_empty() { std::ptr::null() } else { params.as_ptr() };
        info.lpDirectory = folder.as_ptr();
        info.nShow = 1;
        if ShellExecuteExW(&mut info) == 0 {
            return Err(GetLastError() as i32);
        }
        if info.hProcess.is_null() {
            return Ok(None);
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0u32;
        let read = GetExitCodeProcess(info.hProcess, &mut code) != 0;
        CloseHandle(info.hProcess);
        Ok(read.then_some(code as i32))
    }
}

#[cfg(not(windows))]
pub fn run_and_wait(_: &Path, _: &str, _: &Path) -> Result<Option<i32>, i32> {
    Err(NEEDS_ADMIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_error_740_asks_for_the_permission_prompt() {
        assert!(needs_admin(&std::io::Error::from_raw_os_error(740)));
        assert!(!needs_admin(&std::io::Error::from_raw_os_error(2)));
        assert!(failure(DECLINED).contains("recusada"));
        assert!(failure(5).contains('5'));
    }
}
