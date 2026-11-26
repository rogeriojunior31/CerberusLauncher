//! Track the processes from one click on Play, including descendants of short-lived launchers.
use cerberus_shared::tr;
use super::Observation;
use std::ffi::OsString;
use std::io;
use std::mem::{offset_of, size_of, size_of_val};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::{ChildExt, CommandExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::ptr::{null, null_mut};
use std::time::Duration;
use crate::awake_time::AwakeInstant as Instant;

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, FILETIME, WAIT_OBJECT_0};
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Threading::*;

/// A job a program runs in that ends with Cerberus: closed, crashed or killed,
/// the job's last handle goes and Windows ends every process in it (the
/// program and whatever it started). For the setup step, which must not go on
/// alone and meet a second one started on the next Play.
pub(super) struct Tied(OwnedHandle);

impl Tied {
    /// Ends the program and everything it started.
    pub(super) fn end(&self) {
        unsafe {
            TerminateJobObject(self.0.as_raw_handle(), 1);
        }
    }
}

/// Starts `command` (with `flags`) inside a job tied to Cerberus. Without a
/// job (Windows refused one), the program still starts, untied.
pub(super) fn spawn_tied(command: &mut Command, flags: u32) -> io::Result<(Child, Option<Tied>)> {
    let handle = unsafe { CreateJobObjectW(null(), null()) };
    if handle.is_null() {
        return command.creation_flags(flags).spawn().map(|c| (c, None));
    }
    let job = unsafe { OwnedHandle::from_raw_handle(handle) };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let set = unsafe {
        SetInformationJobObject(job.as_raw_handle(), JobObjectExtendedLimitInformation, (&raw const limits).cast(), size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32)
    };
    if set == 0 {
        return command.creation_flags(flags).spawn().map(|c| (c, None));
    }
    // Enrolled before its first instruction, so nothing it starts escapes the job.
    let mut child = command.creation_flags(flags | CREATE_SUSPENDED).spawn()?;
    if unsafe { AssignProcessToJobObject(job.as_raw_handle(), child.as_raw_handle()) } == 0
        || unsafe { ResumeThread(child.main_thread_handle().as_raw_handle()) } == u32::MAX
    {
        let error = io::Error::last_os_error();
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    Ok((child, Some(Tied(job))))
}

pub(super) struct GameProcess {
    job: OwnedHandle,
    launcher: Child,
    runtime: Option<PathBuf>,
    started: Instant,
}

// Common installations such as ReStreet ship FooLauncher.exe beside Foo.exe or Binaries/Foo.exe.
// Only select this mode when the corresponding playable file exists.
pub(super) fn runtime_for(exe: &Path) -> Option<PathBuf> {
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
        .then(|| PathBuf::from(runtime))
}

pub(super) fn spawn_game(mut command: Command, selected: Option<&Path>) -> io::Result<GameProcess> {
    let runtime = match selected {
        Some(path) => Some(dunce::canonicalize(path).map_err(|e|
            io::Error::new(e.kind(), tr!("Confira o executável acompanhado em Gerenciar ({0}): {1}", path.display(), e)))?),
        None => runtime_for(Path::new(command.get_program())),
    };
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
    pub(super) fn observe(mut self, mut report: impl FnMut(Observation)) -> io::Result<()> {
        let mut buffer = vec![0usize; 32];
        let mut launcher_code = None;
        let mut closed_first = false;
        if self.runtime.is_none() {
            report(Observation::Started(self.started));
            let watch = WindowWatch::start(self.job.as_raw_handle() as usize);
            launcher_code = self.launcher.wait()?.code();
            closed_first = watch.closed_first();
        }
        let launcher_pid = self.launcher.id();
        loop {
            let members = self.members(&mut buffer)?;
            // A crash reporter or updater the game left running isn't the game:
            // once only those remain, the session is over.
            if members.iter().all(|&pid| pid as u32 != launcher_pid && image_name(pid as u32).is_some_and(|n| is_helper(&n))) {
                break;
            }
            if let Some(runtime) = &self.runtime {
                if let Some(game) = members.iter().find_map(|&pid| self.playable(pid as u32, runtime)) {
                    let (begun, wall) = (Instant::now(), std::time::SystemTime::now());
                    report(Observation::Started(begun));
                    let watch = WindowWatch::start(self.job.as_raw_handle() as usize);
                    let (ran, code) = runtime_duration(game)?;
                    // The process's own times are wall time: the PC sleeping meanwhile
                    // (wall clock moved, the awake one didn't) comes off.
                    let played = crate::awake_time::without_sleep(ran, wall.elapsed().unwrap_or_default(), begun.elapsed());
                    if watch.closed_first() {
                        report(Observation::WindowsClosedFirst);
                    }
                    report(Observation::Exited(code));
                    report(Observation::Closed(played));
                    continue;
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if self.runtime.is_none() {
            if let Some(code) = launcher_code {
                if closed_first {
                    report(Observation::WindowsClosedFirst);
                }
                report(Observation::Exited(code));
            }
            report(Observation::Closed(self.started.elapsed()));
        }
        Ok(())
    }

    fn members<'a>(&self, buffer: &'a mut Vec<usize>) -> io::Result<&'a [usize]> {
        job_members(self.job.as_raw_handle() as usize, buffer)
    }

    fn playable(&self, pid: u32, target: &Path) -> Option<OwnedHandle> {
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
        let matches = if target.is_absolute() {
            path.as_os_str().eq_ignore_ascii_case(target.as_os_str())
                || (path.file_name()?.eq_ignore_ascii_case(target.file_name()?)
                    && dunce::canonicalize(&path).is_ok_and(|canonical| canonical == target))
        } else {
            path.file_name()?.eq_ignore_ascii_case(target.as_os_str())
        };
        matches.then_some(process)
    }
}

/// The file name of a running process's program.
fn image_name(pid: u32) -> Option<String> {
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return None;
    }
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut path = vec![0u16; 32768];
    let mut len = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process.as_raw_handle(), 0, path.as_mut_ptr(), &mut len) } == 0 {
        return None;
    }
    let path = PathBuf::from(OsString::from_wide(&path[..len as usize]));
    Some(path.file_name()?.to_string_lossy().into_owned())
}

/// Programs that come along with a game but aren't it: crash reporters,
/// updaters, uninstallers, Windows' error reporting.
pub(super) fn is_helper(file_name: &str) -> bool {
    let name = file_name.to_ascii_lowercase();
    let stem = name.strip_suffix(".exe").unwrap_or(&name);
    const PARTS: [&str; 9] = ["crashhandler", "crash_handler", "crashpad", "crashreport", "crashsender", "bugsplat", "updater", "uninstall", "werfault"];
    PARTS.iter().any(|p| stem.contains(p)) || stem.starts_with("unins0") || stem == "sentry-cli"
}

/// How long the game ran, and its exit code.
fn runtime_duration(process: OwnedHandle) -> io::Result<(Duration, i32)> {
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
    let mut code = 0u32;
    unsafe { GetExitCodeProcess(raw, &mut code) };
    Ok((Duration::from_secs(elapsed / 10_000_000) + Duration::from_nanos(elapsed % 10_000_000 * 100), code as i32))
}


/// The processes in the job (its raw handle, kept as a number to cross threads).
fn job_members(job: usize, buffer: &mut Vec<usize>) -> io::Result<&[usize]> {
    loop {
        // Win32 writes a fixed header and a variable array of pointer-sized process IDs.
        let ok = unsafe {
            QueryInformationJobObject(
                job as windows_sys::Win32::Foundation::HANDLE,
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

/// Whether one of these processes shows a window.
fn shows_a_window(pids: &[u32]) -> bool {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};
    struct Search<'a> {
        pids: &'a [u32],
        found: bool,
    }
    unsafe extern "system" fn each(window: HWND, data: LPARAM) -> i32 {
        let search = unsafe { &mut *(data as *mut Search) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if search.pids.contains(&pid) && unsafe { IsWindowVisible(window) } != 0 {
            search.found = true;
            return 0;
        }
        1
    }
    let mut search = Search { pids, found: false };
    unsafe { EnumWindows(Some(each), &mut search as *mut Search as LPARAM) };
    search.found
}

/// Watches, while the game runs, the last moment it showed a window. A game
/// that crashes while playing still has its window when it goes (Windows keeps
/// it, frozen, while it reports the error); one that crashes while shutting
/// down closed its window first.
struct WindowWatch {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    last_seen: std::sync::Arc<std::sync::Mutex<Option<Instant>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WindowWatch {
    fn start(job: usize) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let last_seen = std::sync::Arc::new(std::sync::Mutex::new(None));
        let (halt, seen) = (stop.clone(), last_seen.clone());
        let thread = std::thread::spawn(move || {
            let mut buffer = vec![0usize; 32];
            while !halt.load(std::sync::atomic::Ordering::Relaxed) {
                let pids: Vec<u32> = job_members(job, &mut buffer).map(|m| m.iter().map(|&p| p as u32).collect()).unwrap_or_default();
                if !pids.is_empty() && shows_a_window(&pids) {
                    if let Ok(mut at) = seen.lock() {
                        *at = Some(Instant::now());
                    }
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        });
        WindowWatch { stop, last_seen, thread: Some(thread) }
    }

    /// Called right after the game ended: its windows were gone well before.
    fn closed_first(mut self) -> bool {
        let ended = Instant::now();
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let last = self.last_seen.lock().ok().and_then(|at| *at);
        last.is_some_and(|at| ended.saturating_duration_since(at) >= Duration::from_millis(600))
    }
}
