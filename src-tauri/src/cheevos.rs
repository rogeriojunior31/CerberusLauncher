//! RetroAchievements in the emulators, shown only by Cerberus.
//!
//! Before a classic opens through Cerberus, the emulator gets the player's
//! RetroAchievements account (the connect token Cerberus already keeps, never
//! the password) and, for this session only, its own achievement pop-ups and
//! sounds are turned off and its log is written to a file. Cerberus reads that
//! log as the game runs and shows its own notice. When the game closes, every
//! setting changed for the session goes back to what the player had (and if
//! Cerberus closed first, it's put back the next time it opens).
//!
//! The setting names come from each emulator's own source: RetroArch's
//! `cheevos_visibility_*`, DuckStation's `[Cheevos]` and PCSX2's `[Achievements]`.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How an emulator writes an unlock in its log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    /// rcheevos in RetroArch: `Awarding achievement 1234: Title`.
    RetroArch,
    /// `Achievement 1234 (Title) for game 99 unlocked`.
    DuckStation,
    /// `Achievements: Achievement Title (1234) for game 99 unlocked`.
    Pcsx2,
}

/// An achievement unlocked, by its RetroAchievements id and title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unlock {
    pub id: u64,
    pub title: String,
    /// The game's RetroAchievements id, when the log says it.
    pub game: Option<u64>,
}

/// The number after " for game " in DuckStation's and PCSX2's lines.
fn game_after(text: &str) -> Option<u64> {
    let rest = &text[text.find(" for game ")? + " for game ".len()..];
    rest.split_whitespace().next()?.parse().ok()
}

/// RetroAchievements "awards" its own warnings (101000001 "Warning: Unsupported
/// Core", and the ones after it): they aren't the game's achievements.
fn real(id: u64, title: &str) -> bool {
    id < 101_000_000 && !title.trim_start().starts_with("Warning:") && !title.trim().is_empty()
}

/// An unlock from one line of an emulator's log.
pub fn parse(format: Format, line: &str) -> Option<Unlock> {
    let unlock = match format {
        Format::RetroArch => {
            let at = line.find("chievement ")?;
            let before = line[..at].to_lowercase();
            if !(before.ends_with("awarding a") || before.ends_with("awarded a")) || before.contains("error") {
                return None;
            }
            let (id, text) = line[at + "chievement ".len()..].split_once(':')?;
            let text = text.trim();
            // "Title (description)": the title alone.
            let title = match text.rfind(" (") {
                Some(i) if text.ends_with(')') => &text[..i],
                _ => text,
            };
            Unlock { id: id.trim().parse().ok()?, title: title.to_string(), game: None }
        }
        Format::DuckStation => {
            let rest = &line[line.find("Achievement ")? + "Achievement ".len()..];
            let end = rest.find(" for game ")?;
            if !rest[end..].contains(" unlocked") {
                return None;
            }
            let (id, title) = rest[..end].split_once(" (")?;
            Unlock { id: id.trim().parse().ok()?, title: title.strip_suffix(')')?.to_string(), game: game_after(&rest[end..]) }
        }
        Format::Pcsx2 => {
            let rest = &line[line.find("Achievements: Achievement ")? + "Achievements: Achievement ".len()..];
            let end = rest.find(" for game ")?;
            if !rest[end..].contains(" unlocked") {
                return None;
            }
            let (title, id) = rest[..end].rsplit_once(" (")?;
            Unlock { id: id.strip_suffix(')')?.trim().parse().ok()?, title: title.to_string(), game: game_after(&rest[end..]) }
        }
    };
    real(unlock.id, &unlock.title).then_some(unlock)
}

// ---------- editing settings files, line by line ----------

/// One setting changed for a session: the line the player had (none: there
/// wasn't one) and the one Cerberus wrote, to undo it afterwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edit {
    pub file: PathBuf,
    /// The `[Section]` of an INI file; none for RetroArch's flat `retroarch.cfg`.
    pub section: Option<String>,
    pub key: String,
    pub original: Option<String>,
    pub ours: String,
}

fn key_of(line: &str) -> Option<&str> {
    let (k, _) = line.split_once('=')?;
    Some(k.trim())
}

fn line_for(section: Option<&str>, key: &str, value: &str) -> String {
    match section {
        None => format!("{key} = \"{value}\""),
        Some(_) => format!("{key} = {value}"),
    }
}

/// The range of lines that belong to `[section]` (after its header), or the whole file.
fn section_range(lines: &[String], section: Option<&str>) -> Option<(usize, usize)> {
    let Some(name) = section else { return Some((0, lines.len())) };
    let start = lines.iter().position(|l| l.trim().strip_prefix('[').and_then(|r| r.strip_suffix(']')).is_some_and(|n| n.trim().eq_ignore_ascii_case(name)))? + 1;
    let end = lines[start..].iter().position(|l| l.trim().starts_with('[')).map_or(lines.len(), |i| start + i);
    Some((start, end))
}

/// Sets one value; returns the new text and the line that was there before.
pub fn set_value(text: &str, section: Option<&str>, key: &str, value: &str) -> (String, Option<String>) {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let new_line = line_for(section, key, value);
    let original = match section_range(&lines, section) {
        Some((start, end)) => match (start..end).find(|&i| key_of(&lines[i]).is_some_and(|k| k.eq_ignore_ascii_case(key))) {
            Some(i) => Some(std::mem::replace(&mut lines[i], new_line)),
            None => {
                // After the section's last setting, so blank lines stay between sections.
                let at = (start..end).rev().find(|&i| !lines[i].trim().is_empty()).map_or(start, |i| i + 1);
                lines.insert(at, new_line);
                None
            }
        },
        None => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push(format!("[{}]", section.unwrap_or_default()));
            lines.push(new_line);
            None
        }
    };
    (lines.join("\n") + "\n", original)
}

/// Undoes one edit, unless the player changed that setting in the meantime.
fn undo(text: &str, edit: &Edit) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let Some((start, end)) = section_range(&lines, edit.section.as_deref()) else { return text.to_string() };
    let Some(i) = (start..end).find(|&i| key_of(&lines[i]).is_some_and(|k| k.eq_ignore_ascii_case(&edit.key))) else { return text.to_string() };
    if lines[i].trim() != edit.ours.trim() {
        return text.to_string();
    }
    match &edit.original {
        Some(line) => lines[i] = line.clone(),
        None => {
            lines.remove(i);
        }
    }
    lines.join("\n") + "\n"
}

/// Writes values into a settings file; returns the edits that undo them.
fn apply(file: &Path, section: Option<&str>, values: &[(&str, &str)]) -> Vec<Edit> {
    let mut text = fs::read_to_string(file).unwrap_or_default();
    let mut edits = Vec::new();
    for (key, value) in values {
        let (next, original) = set_value(&text, section, key, value);
        text = next;
        let ours = line_for(section, key, value);
        if original.as_deref().map(str::trim) != Some(ours.as_str()) {
            edits.push(Edit { file: file.to_path_buf(), section: section.map(str::to_string), key: key.to_string(), original, ours });
        }
    }
    if !edits.is_empty() {
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(file, text);
    }
    edits
}

/// Values written for good (the account), not undone after the session.
fn keep(file: &Path, section: Option<&str>, values: &[(&str, &str)]) {
    let mut text = fs::read_to_string(file).unwrap_or_default();
    let before = text.clone();
    for (key, value) in values {
        text = set_value(&text, section, key, value).0;
    }
    if text != before {
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(file, text);
    }
}

fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let (start, end) = section_range(&lines, Some(section))?;
    lines[start..end].iter().find(|l| key_of(l).is_some_and(|k| k.eq_ignore_ascii_case(key))).and_then(|l| l.split_once('=')).map(|(_, v)| v.trim().to_string())
}

// ---------- a session ----------

/// The account to sign the emulator in with.
pub struct Account<'a> {
    pub user: &'a str,
    pub token: &'a str,
}

/// What a session changed and where its log is.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Session {
    pub edits: Vec<Edit>,
    pub log: Option<PathBuf>,
    /// Where the log ended when the game opened: only what comes after is new.
    pub from: u64,
    pub format: Option<Format>,
}

fn pending_file(data: &Path) -> PathBuf {
    data.join("emulators").join("session-settings.json")
}

fn documents() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Documents"))
}

fn portable(exe: &Path) -> bool {
    exe.parent().is_some_and(|d| d.join("portable.txt").is_file() || d.join("portable.ini").is_file())
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default()
}

/// Whether the emulator may be signed in with this account: nobody signed in
/// yet, or the same player (another account the player chose stays).
fn may_sign_in(signed_in: Option<String>, user: &str) -> bool {
    signed_in.filter(|u| !u.is_empty()).is_none_or(|u| u.eq_ignore_ascii_case(user))
}

/// Gets an emulator ready: signed in, its own pop-ups off for now, its log on.
/// `emulator`: "retroarch" or a standalone id.
pub fn prepare(data: &Path, emulator: &str, exe: &Path, account: Option<Account>, retroarch_log: Option<&Path>) -> Session {
    // Anything a session that never finished left behind goes back first.
    restore_pending(data);
    let dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut session = Session::default();
    match emulator {
        "retroarch" => {
            let cfg = dir.join("retroarch.cfg");
            // The account is written by `emulation::write_token` (it keeps a backup).
            session.edits = apply(
                &cfg,
                None,
                &[
                    ("cheevos_visibility_unlock", "false"),
                    ("cheevos_visibility_mastery", "false"),
                    ("cheevos_visibility_account", "false"),
                    ("cheevos_visibility_summary", "0"),
                    ("cheevos_visibility_progress_tracker", "false"),
                    ("cheevos_visibility_lboard_start", "false"),
                    ("cheevos_visibility_lboard_submit", "false"),
                    ("cheevos_visibility_lboard_cancel", "false"),
                    ("cheevos_visibility_lboard_trackers", "false"),
                    ("cheevos_unlock_sound_enable", "false"),
                ],
            );
            session.log = retroarch_log.map(Path::to_path_buf);
            session.format = Some(Format::RetroArch);
        }
        "duckstation" => {
            let root = if portable(exe) { Some(dir.clone()) } else { documents().map(|d| d.join("DuckStation")) };
            let Some(root) = root else { return session };
            let ini = root.join("settings.ini");
            if let Some(a) = account {
                let text = fs::read_to_string(&ini).unwrap_or_default();
                if may_sign_in(ini_value(&text, "Cheevos", "Username"), a.user) {
                    keep(&ini, Some("Cheevos"), &[("Enabled", "true"), ("Username", a.user), ("Token", a.token), ("LoginTimestamp", &now().to_string())]);
                }
            }
            session.edits = apply(&ini, Some("Cheevos"), &[("Notifications", "false"), ("LeaderboardNotifications", "false"), ("SoundEffects", "false")]);
            session.edits.extend(apply(&ini, Some("Logging"), &[("LogToFile", "true")]));
            session.log = Some(root.join("duckstation.log"));
            session.format = Some(Format::DuckStation);
        }
        "pcsx2" => {
            let root = if portable(exe) { Some(dir.clone()) } else { documents().map(|d| d.join("PCSX2")) };
            let Some(root) = root else { return session };
            let ini = root.join("inis").join("PCSX2.ini");
            if let Some(a) = account {
                let text = fs::read_to_string(&ini).unwrap_or_default();
                if may_sign_in(ini_value(&text, "Achievements", "Username"), a.user) {
                    keep(&ini, Some("Achievements"), &[("Enabled", "true"), ("Username", a.user), ("LoginTimestamp", &now().to_string())]);
                    // PCSX2 keeps the token apart, in secrets.ini.
                    keep(&root.join("inis").join("secrets.ini"), Some("Achievements"), &[("Token", a.token)]);
                }
            }
            session.edits = apply(&ini, Some("Achievements"), &[("Notifications", "false"), ("LeaderboardNotifications", "false"), ("SoundEffects", "false"), ("UnlockSound", "false")]);
            session.log = Some(root.join("logs").join("emulog.txt"));
            session.format = Some(Format::Pcsx2);
        }
        _ => {}
    }
    session.from = session.log.as_ref().and_then(|l| fs::metadata(l).ok()).map_or(0, |m| m.len());
    if !session.edits.is_empty() {
        if let Some(dir) = pending_file(data).parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(pending_file(data), serde_json::to_string(&session.edits).unwrap_or_default());
    }
    session
}

/// Puts back what a session changed.
pub fn finish(data: &Path, session: &Session) {
    undo_all(&session.edits);
    let _ = fs::remove_file(pending_file(data));
}

fn undo_all(edits: &[Edit]) {
    let mut files: Vec<&PathBuf> = edits.iter().map(|e| &e.file).collect();
    files.dedup();
    for file in files {
        let Ok(mut text) = fs::read_to_string(file) else { continue };
        for edit in edits.iter().filter(|e| &e.file == file) {
            text = undo(&text, edit);
        }
        let _ = fs::write(file, text);
    }
}

/// A session Cerberus couldn't finish (it closed, or the PC turned off): its changes go back.
pub fn restore_pending(data: &Path) {
    let file = pending_file(data);
    if let Some(edits) = fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Vec<Edit>>(&t).ok()) {
        undo_all(&edits);
    }
    let _ = fs::remove_file(file);
}

/// Reads what the log gained since `from`; returns the new lines and where it ends now.
/// A log that got shorter was started over: it's read from the start.
pub fn read_new(log: &Path, from: u64) -> (Vec<String>, u64) {
    let Ok(mut file) = fs::File::open(log) else { return (Vec::new(), from) };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = if len < from { 0 } else { from };
    if len == start || file.seek(SeekFrom::Start(start)).is_err() {
        return (Vec::new(), len);
    }
    let mut bytes = Vec::new();
    if file.take(len - start).read_to_end(&mut bytes).is_err() {
        return (Vec::new(), start);
    }
    // Only whole lines; a line still being written waits for the next read.
    let whole = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let text = String::from_utf8_lossy(&bytes[..whole]);
    (text.lines().map(str::to_string).collect(), start + whole as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlocks_are_read_from_each_emulators_log() {
        let ra = parse(Format::RetroArch, "[INFO] [RCHEEVOS] Awarding achievement 12345: First Blood (Win a race)").unwrap();
        assert_eq!((ra.id, ra.title.as_str()), (12345, "First Blood"));
        let ds = parse(Format::DuckStation, "[  12.345] I(Achievements): Achievement 78910 (Wumpa Collector) for game 11 unlocked").unwrap();
        assert_eq!((ds.id, ds.title.as_str(), ds.game), (78910, "Wumpa Collector", Some(11)));
        let p2 = parse(Format::Pcsx2, "Achievements: Achievement Kamehameha (Wave) (4242) for game 77 unlocked").unwrap();
        assert_eq!((p2.id, p2.title.as_str(), p2.game), (4242, "Kamehameha (Wave)", Some(77)));
        // Seen in a real session: RetroAchievements' own warning.
        assert!(parse(Format::RetroArch, "[INFO] [RCHEEVOS] Awarding achievement 101000001: Warning: Unsupported Core").is_none());
        assert!(parse(Format::DuckStation, "Achievement 5 (x) for game 1 primed").is_none());
        assert!(parse(Format::RetroArch, "[RCHEEVOS]: Error awarding achievement 12: Speedy, retrying").is_none());
    }

    #[test]
    fn settings_change_for_the_session_and_go_back_after() {
        let dir = tempfile::tempdir().unwrap();
        let ini = dir.path().join("settings.ini");
        fs::write(&ini, "[Main]\nFoo = 1\n\n[Cheevos]\nEnabled = true\nNotifications = true\n\n[Logging]\nLogLevel = Info\n").unwrap();
        let edits = apply(&ini, Some("Cheevos"), &[("Notifications", "false"), ("SoundEffects", "false")]);
        let during = fs::read_to_string(&ini).unwrap();
        assert!(during.contains("Notifications = false") && during.contains("SoundEffects = false"));
        assert!(during.find("SoundEffects").unwrap() < during.find("[Logging]").unwrap(), "inside its section");
        assert_eq!(edits.len(), 2);
        undo_all(&edits);
        let after = fs::read_to_string(&ini).unwrap();
        assert!(after.contains("Notifications = true") && !after.contains("SoundEffects"), "{after}");
        assert!(after.contains("Foo = 1") && after.contains("LogLevel = Info"));
    }

    #[test]
    fn a_setting_the_player_changed_during_the_game_stays() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("retroarch.cfg");
        fs::write(&cfg, "video_fullscreen = \"true\"\ncheevos_visibility_unlock = \"true\"\n").unwrap();
        let edits = apply(&cfg, None, &[("cheevos_visibility_unlock", "false"), ("cheevos_unlock_sound_enable", "false")]);
        assert!(fs::read_to_string(&cfg).unwrap().contains("cheevos_visibility_unlock = \"false\""));
        // The player turned the sound on in RetroArch's menu while playing.
        let text = fs::read_to_string(&cfg).unwrap().replace("cheevos_unlock_sound_enable = \"false\"", "cheevos_unlock_sound_enable = \"true\"");
        fs::write(&cfg, text).unwrap();
        undo_all(&edits);
        let after = fs::read_to_string(&cfg).unwrap();
        assert!(after.contains("cheevos_visibility_unlock = \"true\"") && after.contains("cheevos_unlock_sound_enable = \"true\"") && after.contains("video_fullscreen"));
    }

    #[test]
    fn a_session_left_behind_is_undone_on_the_next_start() {
        let data = tempfile::tempdir().unwrap();
        let exe_dir = data.path().join("duck");
        fs::create_dir_all(&exe_dir).unwrap();
        fs::write(exe_dir.join("portable.txt"), "").unwrap();
        let exe = exe_dir.join("duckstation-qt-x64-ReleaseLTCG.exe");
        let s = prepare(data.path(), "duckstation", &exe, Some(Account { user: "Player", token: "TOKEN" }), None);
        let ini = fs::read_to_string(exe_dir.join("settings.ini")).unwrap();
        assert!(ini.contains("Username = Player") && ini.contains("Notifications = false") && ini.contains("LogToFile = true"));
        assert_eq!(s.log.as_deref(), Some(exe_dir.join("duckstation.log").as_path()));
        // Cerberus closed before the game did.
        restore_pending(data.path());
        let ini = fs::read_to_string(exe_dir.join("settings.ini")).unwrap();
        assert!(!ini.contains("Notifications = false") && !ini.contains("LogToFile"), "{ini}");
        assert!(ini.contains("Username = Player"), "the account stays");
    }

    /// With copies of the player's real settings (the originals aren't touched):
    /// a session turns the pop-ups off, and finishing gives back the same file.
    /// `cargo test -p cerberus-launcher real_emulator_settings -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_emulator_settings() {
        let emulators = PathBuf::from(std::env::var("APPDATA").unwrap()).join("app.cerberus.launcher").join("emulators");
        let cases = [("retroarch", "RetroArch", "retroarch.cfg", "retroarch.exe", "cheevos_visibility_unlock = \"false\""), ("duckstation", "duckstation", "settings.ini", "duckstation-qt-x64-ReleaseLTCG.exe", "Notifications = false")];
        for (id, folder, file, exe, off) in cases {
            let Some(real) = fs::read_dir(emulators.join(folder)).ok().and_then(|_| {
                let direct = emulators.join(folder).join(file);
                direct.is_file().then_some(direct)
            }) else {
                println!("{id}: no {file} yet (never opened), skipped");
                continue;
            };
            let data = tempfile::tempdir().unwrap();
            let dir = data.path().join(folder);
            fs::create_dir_all(&dir).unwrap();
            fs::copy(&real, dir.join(file)).unwrap();
            fs::write(dir.join("portable.txt"), "").unwrap();
            let before = fs::read_to_string(dir.join(file)).unwrap();
            let session = prepare(data.path(), id, &dir.join(exe), None, None);
            let during = fs::read_to_string(dir.join(file)).unwrap();
            assert!(during.contains(off), "{id}: pop-ups not turned off");
            finish(data.path(), &session);
            let after = fs::read_to_string(dir.join(file)).unwrap();
            assert_eq!(after.replace("
", "
").trim_end(), before.replace("
", "
").trim_end(), "{id}: not the same after the session");
            println!("{id}: {} settings changed for the session, all back after ({} lines)", session.edits.len(), after.lines().count());
        }
    }

    #[test]
    fn another_account_the_player_chose_is_kept() {
        assert!(may_sign_in(None, "Player") && may_sign_in(Some(String::new()), "Player") && may_sign_in(Some("player".into()), "Player"));
        assert!(!may_sign_in(Some("Someone".into()), "Player"));
    }

    #[test]
    fn the_log_is_read_a_whole_line_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("emu.log");
        fs::write(&log, "old line\n").unwrap();
        let from = fs::metadata(&log).unwrap().len();
        fs::write(&log, "old line\nnew one\nhalf").unwrap();
        let (lines, at) = read_new(&log, from);
        assert_eq!(lines, ["new one"]);
        fs::write(&log, "fresh\n").unwrap();
        assert_eq!(read_new(&log, at).0, ["fresh"], "a log started over is read from the start");
    }
}
