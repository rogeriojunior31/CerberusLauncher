//! The Downloads list, kept on disk (`downloads.json`): what was paused or
//! didn't finish comes back after a restart, instead of living only in the
//! window. One download runs at a time; the others wait their turn ("Queued").

use cerberus_shared::{InstallProgress, InstallStep};
use std::collections::{BTreeMap, HashMap};
use std::mem::{discriminant, Discriminant};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One download (with its unpacking) at a time: several at once split the
/// bandwidth and the disk, and all finish later.
pub static ONE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// Recomps waiting their turn, in order (the player can put one first), and
/// the ones the player paused or cancelled meanwhile.
static QUEUED: Mutex<Vec<String>> = Mutex::new(Vec::new());
static WITHDRAWN: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);
/// The last kind of step written per recomp: the file changes when the state
/// does, not on every percent.
static WRITTEN: Mutex<Option<HashMap<String, Discriminant<InstallStep>>>> = Mutex::new(None);

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct Log {
    downloads: BTreeMap<String, InstallStep>,
}

fn file(data: &Path) -> PathBuf {
    data.join("downloads.json")
}

/// Keeps a step of a recomp's download (a finished one leaves the list).
pub fn note(data: &Path, id: &str, step: &InstallStep) {
    let kind = discriminant(step);
    {
        let mut written = WRITTEN.lock().unwrap_or_else(|p| p.into_inner());
        let written = written.get_or_insert_with(HashMap::new);
        if written.get(id) == Some(&kind) && matches!(step, InstallStep::Downloading { .. }) {
            return;
        }
        written.insert(id.to_string(), kind);
    }
    let _ = crate::state_file::update(&file(data), |log: &mut Log| match step {
        InstallStep::Finished => {
            log.downloads.remove(id);
        }
        step => {
            log.downloads.insert(id.to_string(), step.clone());
        }
    });
}

/// The recomp leaves the list (cancelled, or the player dismissed it).
pub fn forget(data: &Path, id: &str) {
    WRITTEN.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).remove(id);
    let _ = crate::state_file::update(&file(data), |log: &mut Log| {
        log.downloads.remove(id);
    });
}

/// What the list had when Cerberus last ran: paused and unfinished downloads.
/// One that was running when the app closed shows as paused (its bytes wait
/// for the next Install).
pub fn waiting(data: &Path) -> Vec<InstallProgress> {
    let log: Log = crate::state_file::load_or_default(&file(data));
    log.downloads
        .into_iter()
        .map(|(recomp_id, step)| {
            let step = match step {
                InstallStep::Downloading { done, total } => InstallStep::Paused { done, total },
                InstallStep::Unpacking | InstallStep::Queued => InstallStep::Paused { done: 0, total: 0 },
                other => other,
            };
            InstallProgress { recomp_id, step }
        })
        .filter(|p| !matches!(p.step, InstallStep::Finished))
        .collect()
}

/// Marks a recomp as waiting its turn, at the end of the line.
pub fn queue(id: &str) {
    let mut queued = QUEUED.lock().unwrap_or_else(|p| p.into_inner());
    if !queued.iter().any(|q| q == id) {
        queued.push(id.to_string());
    }
}

/// Puts a waiting recomp first in line; false when it isn't waiting.
pub fn put_first(id: &str) -> bool {
    let mut queued = QUEUED.lock().unwrap_or_else(|p| p.into_inner());
    let Some(at) = queued.iter().position(|q| q == id) else { return false };
    let it = queued.remove(at);
    queued.insert(0, it);
    true
}

/// Whether `id` goes next: first in line, or paused/cancelled while waiting
/// (it then leaves without a turn).
fn next_is(id: &str) -> bool {
    let withdrawn = WITHDRAWN.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).contains_key(id);
    withdrawn || QUEUED.lock().unwrap_or_else(|p| p.into_inner()).first().is_none_or(|q| q == id)
}

/// The slot at once, if it's free and this recomp is first in line (a new
/// download never jumps ahead of ones already waiting).
pub fn try_turn(id: &str) -> Option<tokio::sync::SemaphorePermit<'static>> {
    if next_is(id) { ONE.try_acquire().ok() } else { None }
}

/// Waits for this recomp's turn: the download slot free and it first in line.
/// `None` when it was paused or cancelled while waiting (it takes no slot).
pub async fn wait_turn(id: &str) -> Option<tokio::sync::SemaphorePermit<'static>> {
    loop {
        if next_is(id) {
            if WITHDRAWN.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).contains_key(id) {
                return None;
            }
            if let Ok(permit) = ONE.try_acquire() {
                return Some(permit);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// Its turn came: whether the player paused (`Some(false)`) or cancelled
/// (`Some(true)`) it while it waited.
pub fn turn(id: &str) -> Option<bool> {
    QUEUED.lock().unwrap_or_else(|p| p.into_inner()).retain(|q| q != id);
    WITHDRAWN.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).remove(id)
}

/// Pause or cancel for a recomp still waiting its turn (an active one is
/// stopped by its own halt).
pub fn withdraw(id: &str, cancel: bool) {
    let waiting = QUEUED.lock().unwrap_or_else(|p| p.into_inner()).iter().any(|q| q == id);
    if waiting {
        WITHDRAWN.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).insert(id.to_string(), cancel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line is one for the app: its tests take turns.
    static LINE: Mutex<()> = Mutex::new(());

    #[test]
    fn paused_and_unfinished_downloads_come_back_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        note(dir.path(), "a", &InstallStep::Downloading { done: 0, total: 100 });
        note(dir.path(), "a", &InstallStep::Downloading { done: 50, total: 100 });
        note(dir.path(), "b", &InstallStep::Paused { done: 10, total: 90 });
        note(dir.path(), "c", &InstallStep::Failed { message: "no room".into() });
        note(dir.path(), "d", &InstallStep::Downloading { done: 0, total: 5 });
        note(dir.path(), "d", &InstallStep::Finished);
        let back = waiting(dir.path());
        let of = |id: &str| back.iter().find(|p| p.recomp_id == id).map(|p| p.step.clone());
        // Running when the app closed: paused, from the last state written.
        assert_eq!(of("a"), Some(InstallStep::Paused { done: 0, total: 100 }));
        assert_eq!(of("b"), Some(InstallStep::Paused { done: 10, total: 90 }));
        assert_eq!(of("c"), Some(InstallStep::Failed { message: "no room".into() }));
        assert_eq!(of("d"), None, "a finished one leaves the list");
        forget(dir.path(), "c");
        assert_eq!(waiting(dir.path()).len(), 2);
    }

    #[test]
    fn the_line_keeps_its_order_and_one_can_be_put_first() {
        let _line = LINE.lock().unwrap_or_else(|p| p.into_inner());
        for id in ["l1", "l2", "l3"] {
            queue(id);
        }
        queue("l1");
        assert!(next_is("l1") && !next_is("l3"), "first come, first served; queued twice is once");
        assert!(put_first("l3"));
        assert!(next_is("l3") && !next_is("l1"));
        assert!(!put_first("not-waiting"));
        for id in ["l1", "l2", "l3"] {
            turn(id);
        }
    }

    #[test]
    fn a_download_paused_while_queued_is_told_when_its_turn_comes() {
        let _line = LINE.lock().unwrap_or_else(|p| p.into_inner());
        queue("q1");
        withdraw("q1", false);
        withdraw("not-queued", true);
        assert_eq!(turn("q1"), Some(false));
        assert_eq!(turn("not-queued"), None, "only a queued one is withdrawn here");
    }
}
