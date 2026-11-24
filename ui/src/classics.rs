//! Clássicos: the console games found in the player's ROM folders. Each shows
//! its box art (from the libretro thumbnails, by its No-Intro name), its
//! console, the emulator that opens it on this PC, and whether a recomp also
//! plays it natively. Every classic has its own page; a recomp is one more way
//! to play it, linked from there.

use cerberus_shared::tr;
use cerberus_shared::{ClassicPlayer, RecompEntry, RomView};
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

/// Cerberus has some emulator for the console, installed or not.
fn emulation_exists(console: &str) -> bool {
    EMULATED.contains(&console) || matches!(console, "ps3" | "wiiu")
}

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

/// Who opens this console's classics here, as Play decides it.
pub fn player_for<'a>(players: &'a [ClassicPlayer], console: &str) -> Option<&'a ClassicPlayer> {
    players.iter().find(|p| p.console == console)
}

/// The emulator that will open it, when nothing is missing.
pub fn ready_player(players: &[ClassicPlayer], console: &str) -> Option<String> {
    player_for(players, console).filter(|p| p.problem.is_none()).and_then(|p| p.name.clone())
}

/// A classic's second line: its console and who opens it, or what's missing.
pub fn classic_sub(rom: &RomView, console: &str, players: &[ClassicPlayer]) -> String {
    let name = console_name(console);
    let player = player_for(players, console);
    match (player.and_then(|p| p.name.clone()), player.and_then(|p| p.problem.clone())) {
        (Some(emulator), None) => match rom.last_played {
            Some(t) => format!("{name} · {emulator} · {}", crate::format::ago(t)),
            None => format!("{name} · {emulator}"),
        },
        (Some(_), Some(_)) => tr!("{} · falta o núcleo", name),
        _ => tr!("{} · sem emulador", name),
    }
}

/// The recomp that plays this very file natively, if one is in the catalog.
pub fn recomp_of<'a>(recomps: &'a [RecompEntry], rom: &RomView) -> Option<&'a RecompEntry> {
    recomps.iter().find(|e| e.owned_copy.as_deref() == Some(rom.path.as_str()))
}

/// A classic as a library tile.
pub fn tile(rom: &RomView, players: &[ClassicPlayer], recomps: &[RecompEntry]) -> crate::tiles::TileInfo {
    let console = console_of(rom);
    let badge = recomp_of(recomps, rom).map(|_| tr!("Tem recomp").to_string());
    crate::tiles::classic_tile(display_name(rom), classic_sub(rom, console, players), box_art(rom, console), rom.path.clone(), badge)
}

/// The classics whose name has the words searched, A to Z.
pub fn matching(roms: Vec<RomView>, needle: &str) -> Vec<RomView> {
    let mut list: Vec<RomView> = roms.into_iter().filter(|r| needle.is_empty() || {
        let hay = format!("{} {}", display_name(r), console_name(console_of(r))).to_lowercase();
        needle.split_whitespace().all(|w| hay.contains(w))
    }).collect();
    list.sort_by_key(|r| display_name(r).to_lowercase());
    list
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
            library.toast(tr!("Abrindo o jogo..."));
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

/// The classics, as a grid. `embedded`: under the Library's other games, with
/// nothing at all when no classic matches.
#[component]
pub fn ClassicsGrid(needle: String, #[prop(optional)] embedded: bool) -> impl IntoView {
    let library = expect_context::<Library>();
    let grid = move || {
        let Some(all) = library.roms.get() else {
            return (!embedded).then(|| view! { <p class="muted" role="status">{tr!("Procurando seus clássicos…")}</p> }).into_any();
        };
        let players = library.players.get().unwrap_or_default();
        let recomps: Vec<RecompEntry> = library.recomps.get().and_then(Result::ok).unwrap_or_default();
        let list = matching(all, &needle);
        if list.is_empty() {
            if embedded {
                return ().into_any();
            }
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
        let tiles = list.iter().map(|rom| view! { <crate::tiles::Tile info=tile(rom, &players, &recomps) /> }).collect_view();
        let heading = match (embedded, count) {
            (true, 1) => tr!("1 clássico nas suas pastas").to_string(),
            (true, n) => tr!("{} clássicos nas suas pastas", n),
            (false, 1) => tr!("1 clássico").to_string(),
            (false, n) => tr!("{} clássicos", n),
        };
        view! {
            <section class="catalog-results">
                <h2>{heading}</h2>
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
    // Play time may have changed since the list was read.
    library.roms.refetch();
    let content = move || {
        let list = library.roms.get()?;
        let Some(rom) = list.into_iter().find(|r| r.path == wanted.get_value()) else {
            return Some(view! { <div class="page"><p class="muted">{tr!("Esse jogo não está mais nas suas pastas de ROMs.")}</p></div> }.into_any());
        };
        let console = console_of(&rom);
        let title = display_name(&rom);
        let players = library.players.get().unwrap_or_default();
        let player = player_for(&players, console).cloned();
        let emulator = ready_player(&players, console);
        let recomp = library.recomps.get().and_then(Result::ok).and_then(|all| recomp_of(&all, &rom).cloned());
        let art = box_art(&rom, console);
        let (path, console_id) = (rom.path.clone(), console.to_string());
        let play = emulator.clone().map(|name| view! {
            <button type="button" class="btn big main" on:click=move |_| play_original(library, path.clone(), console_id.clone())>
                <crate::icons::Svg icon=crate::icons::Icon::Play size=18 />{tr!("Jogar no {}", name)}
            </button>
        });
        let to_recomp = recomp.map(|e| {
            let id = e.id.clone();
            view! { <button type="button" class="btn big" on:click=move |_| library.go(Route::Game(format!("recomp:{id}")))>{tr!("Ver o recomp: {}", e.name)}</button> }
        });
        // Nothing opens it yet: why, and the way forward, where Play would be.
        let blocked = emulator.is_none().then(|| {
            let why = player.as_ref().and_then(|p| p.problem.clone()).unwrap_or_else(|| tr!("Esse console ainda não tem emulação no Cerberus.").to_string());
            let fixable = player.as_ref().is_some_and(|p| p.name.is_some()) || emulation_exists(console);
            view! {
                <section class="card blocked" role="status">
                    <h3>{tr!("Ainda não dá para jogar aqui")}</h3>
                    <p>{why}</p>
                    {fixable.then(|| view! {
                        <button type="button" class="btn sm main" on:click=move |_| crate::settings_page::open_emulation(library)>{tr!("Abrir Emulação")}</button>
                    })}
                </section>
            }
        });
        let folder = rom.path.clone();
        let open_folder = move |_| {
            let folder = folder.clone();
            leptos::task::spawn_local(async move {
                library.report(crate::api::open_rom_folder(folder).await);
            });
        };
        let facts = [
            (tr!("Console"), console_name(console).to_string()),
            (tr!("Região"), if rom.region.is_empty() { "—".to_string() } else { rom.region.clone() }),
        ]
        .into_iter()
        .chain((rom.discs.len() > 1).then(|| (tr!("Discos"), rom.discs.len().to_string())))
        .map(|(label, value)| view! { <div><dt>{label}</dt><dd>{value}</dd></div> })
        .collect_view();
        let history_id = format!("classic:{}", rom.path);
        let played = match (rom.playtime_secs, rom.last_played) {
            (0, None) => tr!("Você ainda não jogou").to_string(),
            (t, Some(last)) => tr!("Jogou por {} · última vez {}", crate::format::playtime(t), crate::format::ago(last)),
            (t, None) => tr!("Jogou por {}", crate::format::playtime(t)),
        };
        // RetroAchievements for this ROM, found by its hash or its name.
        let (ach_path, ach_console) = (rom.path.clone(), console.to_string());
        let achievements = LocalResource::new(move || {
            let (p, c) = (ach_path.clone(), ach_console.clone());
            async move {
                match crate::api::classic_achievements(&p, &c).await {
                    Ok(Some(d)) => Ok(d),
                    Ok(None) => Err(String::new()),
                    Err(e) => Err(e),
                }
            }
        });
        let on_all = Callback::new(move |_| {
            if let Some(Ok(d)) = achievements.get_untracked() {
                library.go(Route::AchievementGame("retro".into(), d.game.key.clone()));
            }
        });
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
                            <span class="kick">{tr!("Clássico · {}", console_name(console))}</span>
                            <h1>{title.clone()}</h1>
                            <span class="gp-played"><crate::icons::Svg icon=crate::icons::Icon::Clock size=15 />{played}</span>
                        </div>
                        <div class="gp-act">{to_recomp}{play}</div>
                    </section>
                    <div class="ov">
                    <div class="ov-main">
                    {blocked}
                    <crate::game_overview::PlayCard id=history_id.clone() total=rom.playtime_secs last=rom.last_played />
                    <section class="card classic-file">
                        <div class="card-h">
                            <h3>{if rom.discs.len() > 1 { tr!("Arquivos do jogo") } else { tr!("Arquivo do jogo") }}</h3>
                            <button type="button" class="btn sm" on:click=open_folder>{tr!("Abrir pasta")}</button>
                        </div>
                        {if rom.discs.len() > 1 {
                            let swap = match emulator.as_deref() {
                                Some("RetroArch") if matches!(console, "ps1" | "gc") => tr!("O RetroArch abre o disco 1. Quando o jogo pedir outro, troque no menu do RetroArch (F1) › Controle de disco. O Cerberus nunca altera esses arquivos.").to_string(),
                                Some(name) => tr!("O {} abre o disco 1; quando o jogo pedir outro, troque pelo menu dele. O Cerberus nunca altera esses arquivos.", name),
                                None => tr!("O Cerberus nunca altera esses arquivos.").to_string(),
                            };
                            view! {
                                <ol class="disc-list">{rom.discs.iter().map(|d| view! { <li class="path">{d.clone()}</li> }).collect_view()}</ol>
                                <p class="muted small">{swap}</p>
                            }.into_any()
                        } else {
                            view! {
                                <span class="path">{rom.path.clone()}</span>
                                <p class="muted small">{match emulator.as_deref() {
                                    Some(name) => tr!("O {} abre esse arquivo como está; o Cerberus nunca o altera.", name),
                                    None => tr!("O Cerberus nunca altera esse arquivo.").to_string(),
                                }}</p>
                            }.into_any()
                        }}
                    </section>
                    <crate::game::History id=history_id />
                    </div>
                    <aside class="ov-rail">
                        <crate::game_overview::AchievementsCard data=achievements on_all />
                        <section class="card rail-card"><h3>{tr!("Detalhes")}</h3><dl class="facts">{facts}</dl></section>
                    </aside>
                    </div>
                </div>
            }
            .into_any(),
        )
    };
    view! {
        <div class="gp">
            <button type="button" class="btn glass back" on:click=move |_| library.back()>
                <crate::icons::Svg icon=crate::icons::Icon::Back size=18 />{tr!("Voltar")}
            </button>
            {content}
        </div>
    }
}
