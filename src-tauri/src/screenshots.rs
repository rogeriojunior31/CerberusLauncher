//! Pictures of a game: F12 while a game Cerberus opened is in front takes one
//! of its window (only the game's picture, never the rest of the screen), kept
//! in `screenshots/<game>/` and shown on the game's page.
//!
//! F12 is watched only while such a game runs (it still reaches the game, so
//! Steam's own F12 keeps working), and the picture is taken only when the
//! window in front belongs to that game's folder. Windows keeps F12 for
//! debuggers, so it can't be registered as a hotkey: its state is read instead.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use cerberus_shared::{Game, Screenshot, Source};
use tauri::{AppHandle, Emitter, Manager};

pub const EVENT: &str = "screenshot-taken";

/// `screenshots` in the app's data folder.
pub fn root(data: &Path) -> PathBuf {
    data.join("screenshots")
}

/// A folder name for the game's id ("recomp:zelda64recomp" -> "recomp_zelda64recomp").
fn folder_name(id: &str) -> String {
    id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect::<String>().trim_matches('.').to_string()
}

pub fn dir_of(data: &Path, id: &str) -> PathBuf {
    root(data).join(folder_name(id))
}

/// The game's pictures, newest first.
pub fn list(data: &Path, id: &str) -> Vec<Screenshot> {
    let Ok(entries) = fs::read_dir(dir_of(data, id)) else { return Vec::new() };
    let mut shots: Vec<Screenshot> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .map(|p| {
            let taken = fs::metadata(&p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
            Screenshot { path: p.to_string_lossy().into_owned(), taken }
        })
        .collect();
    shots.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| b.path.cmp(&a.path)));
    shots
}

/// A picture the launcher took: inside `screenshots`, a .png.
pub fn is_ours(data: &Path, path: &Path) -> bool {
    let root = dunce::canonicalize(root(data)).unwrap_or_else(|_| root(data));
    let path = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    path.starts_with(&root) && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) && path.is_file()
}

/// The running game a program belongs to: the one whose folder holds it.
fn owner<'a>(games: &'a [Game], program: &Path) -> Option<&'a Game> {
    let program = program.to_string_lossy().to_lowercase().replace('/', "\\");
    games.iter().find(|g| {
        let mut dirs = vec![g.install_dir.clone()];
        if let Source::Recomp { exe, .. } | Source::Local { exe } = &g.source {
            dirs.push(Path::new(exe).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
        }
        dirs.iter().filter(|d| d.len() > 3).any(|d| {
            let d = d.to_lowercase().replace('/', "\\");
            let d = d.trim_end_matches('\\');
            program.starts_with(&format!("{d}\\"))
        })
    })
}

/// RGBA pixels as a PNG file.
fn save_png(file: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let out = std::io::BufWriter::new(fs::File::create(file).map_err(|e| e.to_string())?);
    let mut encoder = png::Encoder::new(out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}

/// F12 pressed: the picture of the game in front, if it's one Cerberus opened.
fn take(app: &AppHandle) {
    let Some((window, pid)) = win::foreground() else { return };
    if pid == std::process::id() {
        return;
    }
    let Some(program) = win::program_of(pid) else { return };
    let library = app.state::<crate::library::LocalLibrary>();
    let running: Vec<Game> = app.state::<crate::sessions::Sessions>().running().iter().filter_map(|id| library.get(id).ok().flatten()).collect();
    let Some(game) = owner(&running, &program) else { return };
    let Some((width, height, pixels)) = win::grab(window) else {
        crate::notify::tell(app, "Não foi possível tirar a foto", "o jogo está minimizado ou em tela cheia exclusiva");
        return;
    };
    let data = &app.state::<crate::DataDir>().0;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default();
    let file = dir_of(data, &game.id).join(format!("{now}.png"));
    match save_png(&file, width, height, &pixels) {
        Ok(()) => {
            let _ = app.emit(EVENT, game.id.clone());
            crate::notify::tell(app, "Foto salva", &format!("{} › Fotos", game.name));
        }
        Err(e) => crate::notify::tell(app, "Não foi possível salvar a foto", &e),
    }
}

/// Watches F12 while a game Cerberus opened runs: one picture per press.
pub fn watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut was_down = false;
        loop {
            let playing = !app.state::<crate::sessions::Sessions>().running().is_empty();
            if !playing {
                was_down = false;
                std::thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }
            let down = win::f12_down();
            if down && !was_down {
                take(&app);
            }
            was_down = down;
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
    });
}

#[cfg(windows)]
mod win {
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDC, GetDIBits, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F12};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, GetForegroundWindow, GetWindowThreadProcessId, IsIconic};

    /// The window in front and its process.
    pub fn foreground() -> Option<(HWND, u32)> {
        // SAFETY: plain Win32 queries on a window handle Windows gave us.
        unsafe {
            let window = GetForegroundWindow();
            if window.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, &mut pid);
            (pid != 0).then_some((window, pid))
        }
    }

    /// The program file a process runs.
    pub fn program_of(pid: u32) -> Option<PathBuf> {
        // SAFETY: the handle is checked and closed; the buffer's size is passed.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut buffer = vec![0u16; 1024];
            let mut size = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size);
            CloseHandle(process);
            (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..size as usize])))
        }
    }

    /// The window's inside (no title bar) as it is on screen, in RGBA.
    pub fn grab(window: HWND) -> Option<(u32, u32, Vec<u8>)> {
        // SAFETY: every GDI object made here is released before returning; the
        // pixel buffer is sized for the bitmap GetDIBits fills.
        unsafe {
            if IsIconic(window) != 0 {
                return None;
            }
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if GetClientRect(window, &mut rect) == 0 {
                return None;
            }
            let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
            if width <= 0 || height <= 0 || width > 16384 || height > 16384 {
                return None;
            }
            let mut corner = POINT { x: 0, y: 0 };
            ClientToScreen(window, &mut corner);
            let screen = GetDC(std::ptr::null_mut());
            let memory = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            let old = SelectObject(memory, bitmap);
            let copied = BitBlt(memory, 0, 0, width, height, screen, corner.x, corner.y, SRCCOPY | CAPTUREBLT) != 0;
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            let lines = if copied { GetDIBits(memory, bitmap, 0, height as u32, pixels.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS) } else { 0 };
            SelectObject(memory, old);
            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(std::ptr::null_mut(), screen);
            if lines != height {
                return None;
            }
            // Windows gives blue, green, red; the PNG wants red first, and opaque.
            for px in pixels.chunks_exact_mut(4) {
                px.swap(0, 2);
                px[3] = 255;
            }
            // An all-black picture is a game Windows can't see (exclusive full screen).
            if pixels.chunks_exact(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0) {
                return None;
            }
            Some((width as u32, height as u32, pixels))
        }
    }

    /// Whether F12 is held down now.
    pub fn f12_down() -> bool {
        // SAFETY: reads the key's state; no memory is passed.
        unsafe { (GetAsyncKeyState(VK_F12 as i32) as u16 & 0x8000) != 0 }
    }
}

#[cfg(not(windows))]
mod win {
    use std::path::PathBuf;
    pub fn foreground() -> Option<((), u32)> {
        None
    }
    pub fn program_of(_: u32) -> Option<PathBuf> {
        None
    }
    pub fn grab(_: ()) -> Option<(u32, u32, Vec<u8>)> {
        None
    }
    pub fn f12_down() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: &str, dir: &str, exe: &str) -> Game {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": id, "install_dir": dir, "cover_path": null, "cover_url": null,
            "source": {"kind": "local", "exe": exe}
        }))
        .unwrap()
    }

    #[test]
    fn only_the_running_games_own_window_is_taken() {
        let games = vec![game("local:a", r"D:\Games\Zelda", r"D:\Games\Zelda\bin\zelda.exe"), game("local:b", "", r"E:\Other\b.exe")];
        assert_eq!(owner(&games, Path::new(r"d:\games\zelda\bin\zelda.exe")).map(|g| g.id.as_str()), Some("local:a"));
        assert_eq!(owner(&games, Path::new(r"E:\Other\b.exe")).map(|g| g.id.as_str()), Some("local:b"));
        assert!(owner(&games, Path::new(r"D:\Games\Zelda2\x.exe")).is_none(), "a folder beside isn't inside");
        assert!(owner(&games, Path::new(r"C:\Windows\explorer.exe")).is_none());
    }

    #[test]
    fn pictures_are_saved_and_listed_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path();
        assert_eq!(folder_name("recomp:zelda64recomp"), "recomp_zelda64recomp");
        let first = dir_of(data, "recomp:x").join("1.png");
        save_png(&first, 2, 1, &[255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let second = dir_of(data, "recomp:x").join("2.png");
        save_png(&second, 1, 1, &[0, 0, 255, 255]).unwrap();
        let shots = list(data, "recomp:x");
        assert_eq!(shots.len(), 2);
        assert!(shots[0].path.ends_with("2.png") || shots[0].taken == shots[1].taken);
        assert!(is_ours(data, &first));
        assert!(!is_ours(data, &data.join("elsewhere.png")));
        assert_eq!(&fs::read(&first).unwrap()[1..4], b"PNG");
    }
}
