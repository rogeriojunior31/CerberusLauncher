//! Track the processes from one click on Play, including descendants of short-lived launchers.
use std::ffi::OsString;
use std::io;
use std::mem::{offset_of, size_of, size_of_val};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::{ChildExt, CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, FILETIME, WAIT_OBJECT_0};
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Threading::*;

pub(super) struct GameProcess {
    job: OwnedHandle,
    launcher: Child,
    runtime: Option<String>,
    started: Instant,
}

// Common installations such as ReStreet ship FooLauncher.exe beside Foo.exe or Binaries/Foo.exe.
// Only select this mode when the corresponding playable file exists.
fn runtime_for(exe: &Path) -> Option<String> {
    let stem = exe.file_stem()?.to_str()?.to_ascii_lowercase();
    let title = stem.strip_suffix("launcher")?.trim_end_matches([' ', '-', '_']);
    if title.is_empty() {
        return None;
    }
    let runtime = format!("{title}.exe");
    let parent = exe.parent()?;
    ["", "Binaries", "bin"]
        .iter()
        .any(|dir| parent.join(dir).join(&runtime).is_file())
        .then_some(runtime)
}

pub(super) fn spawn_game(mut command: Command) -> io::Result<GameProcess> {
    let runtime = runtime_for(Path::new(command.get_program()));
    // An unnamed job without termination limits leaves the game alive if Cerberus closes.
    let handle = unsafe { CreateJobObjectW(null(), null()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = unsafe { OwnedHandle::from_raw_handle(handle) };
    let started = Instant::now();
    let mut launcher = command.creation_flags(CREATE_SUSPENDED).spawn()?;
    // Enroll before the launcher's first instruction, including when it immediately hands off.
    if unsafe { AssignProcessToJobObject(job.as_raw_handle(), launcher.as_raw_handle()) } == 0
        || unsafe { ResumeThread(launcher.main_thread_handle().as_raw_handle()) } == u32::MAX
    {
        let error = io::Error::last_os_error();
        let _ = launcher.kill();
        let _ = launcher.wait();
        return Err(error);
    }
    Ok(GameProcess {
        job,
        launcher,
        runtime,
        started,
    })
}

impl GameProcess {
    pub(super) fn observe(mut self, mut played: impl FnMut(Duration)) -> io::Result<()> {
        let mut buffer = vec![0usize; 32];
        if self.runtime.is_none() {
            self.launcher.wait()?;
        }
        loop {
            let members = self.members(&mut buffer)?;
            if members.is_empty() {
                break;
            }
            if let Some(runtime) = &self.runtime {
                if let Some(game) = members.iter().find_map(|&pid| self.playable(pid as u32, runtime)) {
                    played(runtime_duration(game)?);
                    continue;
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if self.runtime.is_none() {
            played(self.started.elapsed());
        }
        Ok(())
    }

    fn members<'a>(&self, buffer: &'a mut Vec<usize>) -> io::Result<&'a [usize]> {
        loop {
            // Win32 writes a fixed header and a variable array of pointer-sized process IDs.
            let ok = unsafe {
                QueryInformationJobObject(
                    self.job.as_raw_handle(),
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    size_of_val(buffer.as_slice()).try_into().map_err(io::Error::other)?,
                    null_mut(),
                )
            };
            if ok != 0 {
                let header = unsafe { &*buffer.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };
                let start = offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList) / size_of::<usize>();
                return Ok(&buffer[start..start + header.NumberOfProcessIdsInList as usize]);
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_MORE_DATA as i32) {
                return Err(error);
            }
            buffer.resize(buffer.len() * 2, 0);
        }
    }

    fn playable(&self, pid: u32, name: &str) -> Option<OwnedHandle> {
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, pid) };
        if raw.is_null() {
            return None;
        }
        let process = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut member = 0;
        // The original PID may have exited and been reused between the query and OpenProcess.
        if unsafe { IsProcessInJob(raw, self.job.as_raw_handle(), &mut member) } == 0 || member == 0 {
            return None;
        }
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        if unsafe { QueryFullProcessImageNameW(raw, 0, path.as_mut_ptr(), &mut len) } == 0 {
            return None;
        }
        let path = PathBuf::from(OsString::from_wide(&path[..len as usize]));
        path.file_name()?.eq_ignore_ascii_case(name).then_some(process)
    }
}

fn runtime_duration(process: OwnedHandle) -> io::Result<Duration> {
    let raw = process.as_raw_handle();
    if unsafe { WaitForSingleObject(raw, INFINITE) } != WAIT_OBJECT_0 {
        return Err(io::Error::last_os_error());
    }
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    if unsafe { GetProcessTimes(raw, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let ticks = |time: FILETIME| (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    let elapsed = ticks(exited).saturating_sub(ticks(created));
    Ok(Duration::from_secs(elapsed / 10_000_000) + Duration::from_nanos(elapsed % 10_000_000 * 100))
}
