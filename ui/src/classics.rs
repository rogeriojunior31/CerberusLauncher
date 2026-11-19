//! Clássicos: the console games found in the player's ROM folders. Each shows
//! its box art (from the libretro thumbnails, by its No-Intro name), its
//! console, and whether a recomp already plays it natively. Emulation itself is
//! still to come; a classic with a recomp opens that recomp.

use cerberus_shared::{RecompEntry, RomView};
use leptos::prelude::*;

use crate::art::Poster;
use crate::{Library, Route};

/// The console a ROM is for: its file type, else the folder it sits in
/// ("D:\ROMS\Playstation\..."), for disc images.
pub fn console_of(rom: &RomView) -> &'static str {
    let by_system = match rom.system.as_str() {
        "nes" => "nes",
        "snes" => "snes",
        "n64" => "n64",
        "gb" => "gb",
        "gbc" => "gbc",
        "gba" => "gba",
        "ds" => "ds",
        "3ds" => "3ds",
        "genesis" => "genesis",
        "switch" => "switch",
        "32x" => "32x",
        "sms" => "sms",
        "gg" => "gg",
        _ => "",
    };
    if !by_system.is_empty() {
        return by_system;
    }
    let path = rom.path.to_lowercase().replace('\\', "/");
    let has = |w: &str| path.split('/').any(|part| part.contains(w));
    if has("xbox 360") || has("x360") {
        "x360"
    } else if has("xbox") {
        "xbox"
    } else if has("playstation 3") || has("ps3") {
        "ps3"
    } else if has("playstation 2") || has("ps2") {
        "ps2"
    } else if has("psp") {
        "psp"
    } else if has("playstation") || has("psx") || has("ps1") {
        "ps1"
    } else if has("gamecube") || has("gcn") {
        "gc"
    } else if has("wii u") || has("wiiu") {
        "wiiu"
    } else if has("wii") {
        "wii"
    } else if has("dreamcast") {
        "dc"
    } else if has("saturn") {
        "saturn"
    } else {
        ""
    }
}

/// Consoles RetroArch plays in Cerberus (cores in Configurações › Emulação).
const EMULATED: &[&str] = &["nes", "snes", "n64", "gb", "gbc", "gba", "genesis", "sms", "gg"];

/// The console RetroArch would play this copy of a game on, if it plays it at all.
pub fn emulated_console(path: &str) -> Option<&'static str> {
    let rom = RomView { path: path.to_string(), title: String::new(), system: system_of(path).to_string(), region: String::new() };
    let console = console_of(&rom);
    EMULATED.contains(&console).then_some(console)
}

/// A cartridge's console from its file type.
fn system_of(path: &str) -> &'static str {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "z64" | "n64" | "v64" => "n64",
        "nes" | "fds" | "unf" => "nes",
        "sfc" | "smc" => "snes",
        "gb" => "gb",
        "gbc" => "gbc",
        "gba" => "gba",
        "md" | "gen" | "smd" => "genesis",
        "sms" => "sms",
        "gg" => "gg",
        _ => "",
    }
}

/// Starts the original game in RetroArch.
pub fn play_original(library: Library, path: String, console: String) {
    leptos::task::spawn_local(async move {
        if library.report(crate::api::play_classic(path, console).await).is_some() {
            library.toast("Abrindo no RetroArch...");
        }
    });
}

pub fn console_name(console: &str) -> &'static str {
    match console {
        "nes" => "NES",
        "snes" => "Super Nintendo",
        "n64" => "Nintendo 64",
        "gb" => "Game Boy",
        "gbc" => "Game Boy Color",
        "gba" => "Game Boy Advance",
        "ds" => "Nintendo DS",
        "3ds" => "Nintendo 3DS",
        "genesis" => "Mega Drive",
        "32x" => "32X",
        "sms" => "Master System",
        "gg" => "Game Gear",
        "switch" => "Nintendo Switch",
        "ps1" => "PlayStation",
        "ps2" => "PlayStation 2",
        "ps3" => "PlayStation 3",
        "psp" => "PSP",
        "xbox" => "Xbox",
        "x360" => "Xbox 360",
        "gc" => "GameCube",
        "wii" => "Wii",
        "wiiu" => "Wii U",
        "dc" => "Dreamcast",
        "saturn" => "Saturn",
        _ => "Console",
    }
}

/// The libretro thumbnails repository for a console.
fn thumbnails_repo(console: &str) -> Option<&'static str> {
    Some(match console {
        "nes" => "Nintendo_-_Nintendo_Entertainment_System",
        "snes" => "Nintendo_-_Super_Nintendo_Entertainment_System",
        "n64" => "Nintendo_-_Nintendo_64",
        "gb" => "Nintendo_-_Game_Boy",
        "gbc" => "Nintendo_-_Game_Boy_Color",
        "gba" => "Nintendo_-_Game_Boy_Advance",
        "ds" => "Nintendo_-_Nintendo_DS",
        "3ds" => "Nintendo_-_Nintendo_3DS",
        "genesis" => "Sega_-_Mega_Drive_-_Genesis",
        "32x" => "Sega_-_32X",
        "sms" => "Sega_-_Master_System_-_Mark_III",
        "gg" => "Sega_-_Game_Gear",
        "ps1" => "Sony_-_PlayStation",
        "ps2" => "Sony_-_PlayStation_2",
        "psp" => "Sony_-_PlayStation_Portable",
        "gc" => "Nintendo_-_GameCube",
        "wii" => "Nintendo_-_Wii",
        "xbox" => "Microsoft_-_Xbox",
        "x360" => "Microsoft_-_Xbox_360",
        "dc" => "Sega_-_Dreamcast",
        "saturn" => "Sega_-_Saturn",
        _ => return None,
    })
}

/// The game's name from its file: "Monster Hunter Generations (USA).3ds" ->
/// "Monster Hunter Generations (USA)", the name libretro's art goes by.
fn file_name(rom: &RomView) -> String {
    let base = rom.path.rsplit(['\\', '/']).next().unwrap_or(&rom.path);
    base.rsplit_once('.').map_or(base, |(stem, _)| stem).to_string()
}

/// The name shown: without the region and revision tags.
pub fn display_name(rom: &RomView) -> String {
    let name = file_name(rom);
    let cut = name.find(" (").unwrap_or(name.len());
    name[..cut].trim().to_string()
}

fn box_art(rom: &RomView, console: &str) -> Vec<String> {
    let Some(repo) = thumbnails_repo(console) else { return Vec::new() };
    // libretro replaces these characters in file names with "_".
    let name: String = file_name(rom).chars().map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c }).collect();
    let encoded: String = name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect();
    vec![format!("https://raw.githubusercontent.com/libretro-thumbnails/{repo}/master/Named_Boxarts/{encoded}.png")]
}

#[component]
pub fn ClassicsGrid(needle: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let roms = LocalResource::new(move || async move { crate::api::get_settings().await.map(|s| s.roms).unwrap_or_default() });
    let grid = move || {
        let Some(list) = roms.get() else {
            return view! { <p class="muted" role="status">"Procurando seus clássicos…"</p> }.into_any();
        };
        let recomps: Vec<RecompEntry> = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let mut list: Vec<RomView> = list.into_iter().filter(|r| needle.is_empty() || display_name(r).to_lowercase().contains(&needle)).collect();
        list.sort_by_key(|r| display_name(r).to_lowercase());
        if list.is_empty() {
            return view! {
                <div class="empty">
                    <h2>"Nenhum clássico encontrado"</h2>
                    <p class="muted">"Adicione as pastas onde você guarda ROMs e imagens de disco em Configurações › Biblioteca e ROMs."</p>
                    <button type="button" class="btn" on:click=move |_| library.go(Route::Settings)>"Abrir Configurações"</button>
                </div>
            }
            .into_any();
        }
        let count = list.len();
        let tiles = list
            .into_iter()
            .map(|rom| {
                let console = console_of(&rom);
                let recomp = recomps.iter().find(|e| e.owned_copy.as_deref() == Some(rom.path.as_str())).cloned();
                let title = display_name(&rom);
                let (hover, alt) = (title.clone(), title.clone());
                let emulated = EMULATED.contains(&console);
                let sub = match &recomp {
                    Some(e) if emulated => format!("{} · recomp ou RetroArch", console_name(console)),
                    Some(e) => format!("{} · tem recomp: {}", console_name(console), e.name),
                    None if emulated => format!("{} · RetroArch", console_name(console)),
                    None => console_name(console).to_string(),
                };
                let badge = recomp.as_ref().map(|_| view! { <span class="chip">"Tem recomp"</span> });
                let open = recomp.map(|e| e.id);
                let (path, console_id) = (rom.path.clone(), console.to_string());
                // A classic with a recomp opens the recomp's page, which offers both ways to play.
                let on_open = move |_| match &open {
                    Some(id) => library.go(Route::Game(format!("recomp:{id}"))),
                    None if emulated => play_original(library, path.clone(), console_id.clone()),
                    None => library.toast("Esse console ainda não tem emulação no Cerberus. Por enquanto: NES, Super Nintendo, Nintendo 64, Game Boy, GBA e Mega Drive."),
                };
                view! {
                    <div class="tile">
                        <button type="button" class="tile-b" on:click=on_open title=hover>
                            <div class="poster-wrap">
                                <Poster sources=box_art(&rom, console) title=alt />
                                {badge}
                            </div>
                            <span class="t">{title}</span>
                            <span class="s">{sub}</span>
                        </button>
                    </div>
                }
            })
            .collect_view();
        view! {
            <section class="catalog-results">
                <h2>{if count == 1 { "1 clássico".to_string() } else { format!("{count} clássicos") }}</h2>
                <div class="grid">{tiles}</div>
            </section>
        }
        .into_any()
    };
    view! { {grid} }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom(path: &str, system: &str) -> RomView {
        RomView { path: path.into(), title: String::new(), system: system.into(), region: String::new() }
    }

    #[test]
    fn consoles_come_from_the_file_or_its_folder() {
        assert_eq!(console_of(&rom("D:/ROMS/x/Mario.z64", "n64")), "n64");
        assert_eq!(console_of(&rom("D:\\ROMS\\Xbox 360\\FIFA Street.iso", "")), "x360");
        assert_eq!(console_of(&rom("D:\\ROMS\\Playstation\\Crash\\Crash Bandicoot (USA).cue", "")), "ps1");
        assert_eq!(display_name(&rom("D:/x/Monster Hunter Generations (USA).3ds", "3ds")), "Monster Hunter Generations");
    }
}
