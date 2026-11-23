//! Clássicos: the console games found in the player's ROM folders. Each shows
//! its box art (from the libretro thumbnails, by its No-Intro name), its
//! console, and whether a recomp already plays it natively. A classic with a
//! recomp opens that recomp's page (which plays the original too); the others
//! have their own page, played in RetroArch.

use cerberus_shared::tr;
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
const EMULATED: &[&str] = &["nes", "snes", "n64", "gb", "gbc", "gba", "genesis", "sms", "gg", "ps1", "ps2", "psp", "gc", "wii"];

/// RetroArch plays this console in Cerberus.
pub fn is_emulated(console: &str) -> bool {
    EMULATED.contains(&console)
}

/// The console RetroArch would play this copy of a game on, if it plays it at all.
pub fn emulated_console(path: &str) -> Option<&'static str> {
    let rom = RomView { path: path.to_string(), title: String::new(), system: system_of(path).to_string(), region: String::new(), playtime_secs: 0, last_played: None, discs: Vec::new() };
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
            library.toast(tr!("Abrindo no RetroArch..."));
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
        _ => tr!("Console"),
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

pub fn box_art(rom: &RomView, console: &str) -> Vec<String> {
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
    vec![format!("https://raw.githubusercontent.com/libretro-thumbnails/{}/master/Named_Boxarts/{}.png", repo, encoded)]
}

#[component]
pub fn ClassicsGrid(needle: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let roms = LocalResource::new(move || async move { crate::api::get_settings().await.map(|s| s.roms).unwrap_or_default() });
    let grid = move || {
        let Some(list) = roms.get() else {
            return view! { <p class="muted" role="status">{tr!("Procurando seus clássicos…")}</p> }.into_any();
        };
        let recomps: Vec<RecompEntry> = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let mut list: Vec<RomView> = list.into_iter().filter(|r| needle.is_empty() || display_name(r).to_lowercase().contains(&needle)).collect();
        list.sort_by_key(|r| display_name(r).to_lowercase());
        if list.is_empty() {
            return view! {
                <div class="empty">
                    <h2>{tr!("Nenhum clássico encontrado")}</h2>
                    <p class="muted">{tr!("Adicione as pastas onde você guarda ROMs e imagens de disco em Configurações › Biblioteca e ROMs.")}</p>
                    <button type="button" class="btn" on:click=move |_| library.go(Route::Settings)>{tr!("Abrir Configurações")}</button>
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
                    Some(e) if emulated => tr!("{} · recomp ou RetroArch", console_name(console)),
                    Some(e) => format!("{} · tem recomp: {}", console_name(console), e.name),
                    None if emulated => match rom.last_played {
                        Some(t) => format!("{} · jogado {} · {}", console_name(console), crate::format::ago(t), crate::format::playtime(rom.playtime_secs)),
                        None => format!("{} · RetroArch", console_name(console)),
                    },
                    None => console_name(console).to_string(),
                };
                let badge = recomp.as_ref().map(|_| view! { <span class="chip">{tr!("Tem recomp")}</span> });
                let open = recomp.map(|e| e.id);
                let path = rom.path.clone();
                // A classic with a recomp opens the recomp's page, which offers both ways to play.
                let on_open = move |_| match &open {
                    Some(id) => library.go(Route::Game(format!("recomp:{id}"))),
                    None => library.go(Route::Classic(path.clone())),
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
                <h2>{if count == 1 { tr!("1 clássico").to_string() } else { tr!("{} clássicos", count) }}</h2>
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
        RomView { path: path.into(), title: String::new(), system: system.into(), region: String::new(), playtime_secs: 0, last_played: None, discs: Vec::new() }
    }

    #[test]
    fn consoles_come_from_the_file_or_its_folder() {
        assert_eq!(console_of(&rom("D:/ROMS/x/Mario.z64", "n64")), "n64");
        assert_eq!(console_of(&rom("D:\\ROMS\\Xbox 360\\FIFA Street.iso", "")), "x360");
        assert_eq!(console_of(&rom("D:\\ROMS\\Playstation\\Crash\\Crash Bandicoot (USA).cue", "")), "ps1");
        assert_eq!(display_name(&rom("D:/x/Monster Hunter Generations (USA).3ds", "3ds")), "Monster Hunter Generations");
    }
}

/// A classic's own page: its box art, how much it was played, and Play in RetroArch.
#[component]
pub fn ClassicPage(path: String) -> impl IntoView {
    let library = expect_context::<Library>();
    let wanted = StoredValue::new(path);
    let roms = LocalResource::new(move || async move { crate::api::get_settings().await.map(|s| s.roms).unwrap_or_default() });
    let content = move || {
        let list = roms.get()?;
        let Some(rom) = list.into_iter().find(|r| r.path == wanted.get_value()) else {
            return Some(view! { <div class="page"><p class="muted">"Esse jogo não está mais nas suas pastas de ROMs."</p></div> }.into_any());
        };
        let console = console_of(&rom);
        let title = display_name(&rom);
        let emulated = is_emulated(console);
        let recomp = library.recomps.get().and_then(Result::ok).and_then(|all| all.into_iter().find(|e| e.owned_copy.as_deref() == Some(rom.path.as_str())));
        let art = box_art(&rom, console);
        let (path, console_id) = (rom.path.clone(), console.to_string());
        let play = if emulated {
            view! {
                <button type="button" class="btn big main" on:click=move |_| play_original(library, path.clone(), console_id.clone())>
                    <crate::icons::Svg icon=crate::icons::Icon::Play size=18 />"Jogar no RetroArch"
                </button>
            }
            .into_any()
        } else {
            view! { <p class="muted small">"Esse console ainda não tem emulação no Cerberus."</p> }.into_any()
        };
        let to_recomp = recomp.map(|e| {
            let id = e.id.clone();
            view! { <button type="button" class="btn big" on:click=move |_| library.go(Route::Game(format!("recomp:{id}")))>{format!("Ver o recomp: {}", e.name)}</button> }
        });
        let folder = rom.path.clone();
        let open_folder = move |_| {
            let folder = folder.clone();
            leptos::task::spawn_local(async move {
                library.report(crate::api::open_rom_folder(folder).await);
            });
        };
        let stats = [
            ("Tempo de jogo", if rom.playtime_secs > 0 { crate::format::playtime(rom.playtime_secs) } else { "Ainda não jogado".to_string() }),
            ("Última vez", rom.last_played.map(crate::format::ago).unwrap_or_else(|| "Nunca".to_string())),
            ("Console", console_name(console).to_string()),
            ("Região", if rom.region.is_empty() { "—".to_string() } else { rom.region.clone() }),
        ]
        .into_iter()
        .chain((rom.discs.len() > 1).then(|| ("Discos", rom.discs.len().to_string())))
        .into_iter()
        .map(|(label, value)| view! { <div class="stat"><span>{label}</span><b>{value}</b></div> })
        .collect_view();
        let history_id = format!("classic:{}", rom.path);
        Some(
            view! {
                <div class="gp-hero">
                    <crate::art::Backdrop sources=art.clone() soft=true />
                    <div class="gp-shade"></div>
                </div>
                <div class="page gp-page">
                    <section class="gp-head">
                        <div class="gp-cover"><Poster sources=art title=title.clone() /></div>
                        <div class="gp-info">
                            <span class="kick">{format!("Clássico · {}", console_name(console))}</span>
                            <h1>{title.clone()}</h1>
                        </div>
                        <div class="gp-act">{to_recomp}{play}</div>
                    </section>
                    <div class="stats">{stats}</div>
                    <section class="card classic-file">
                        <div class="card-h">
                            <h3>{if rom.discs.len() > 1 { "Arquivos do jogo" } else { "Arquivo do jogo" }}</h3>
                            <button type="button" class="btn sm" on:click=open_folder>"Abrir pasta"</button>
                        </div>
                        {if rom.discs.len() > 1 {
                            let swap = if matches!(console, "ps1" | "gc") {
                                "O RetroArch abre o disco 1. Quando o jogo pedir outro, troque no menu do RetroArch (F1) › Controle de disco. O Cerberus nunca altera esses arquivos."
                            } else {
                                "O RetroArch abre o disco 1; esse console ainda não troca de disco pelo Cerberus. O Cerberus nunca altera esses arquivos."
                            };
                            view! {
                                <ol class="disc-list">{rom.discs.iter().map(|d| view! { <li class="path">{d.clone()}</li> }).collect_view()}</ol>
                                <p class="muted small">{swap}</p>
                            }.into_any()
                        } else {
                            view! {
                                <span class="path">{rom.path.clone()}</span>
                                <p class="muted small">"O RetroArch abre esse arquivo como está; o Cerberus nunca o altera."</p>
                            }.into_any()
                        }}
                    </section>
                    <crate::game::History id=history_id />
                </div>
            }
            .into_any(),
        )
    };
    view! {
        <div class="gp">
            <button type="button" class="btn glass back" on:click=move |_| library.back()>
                <crate::icons::Svg icon=crate::icons::Icon::Back size=18 />"Voltar"
            </button>
            {content}
        </div>
    }
}
