//! Line icons, drawn on a 24px grid.

use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Home,
    Library,
    List,
    Discover,
    Download,
    Saves,
    Plus,
    Play,
    Pause,
    Back,
    Left,
    Right,
    Minimize,
    Maximize,
    Close,
    Search,
    Folder,
    External,
    Trash,
    Edit,
    Check,
    Clock,
    Settings,
    Tv,
    Info,
    Puzzle,
    Trophy,
    Gamepad,
    Bell,
    Heart,
    HeartFill,
}

impl Icon {
    fn path(self) -> &'static str {
        match self {
            Icon::Home => "M4 10.5 12 4l8 6.5V20h-5.5v-5.5h-5V20H4z",
            Icon::List => "M4 6h2m4 0h10M4 12h2m4 0h10M4 18h2m4 0h10",
            Icon::Library => "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
            Icon::Discover => "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM15.5 8.5l-2 5-5 2 2-5z",
            Icon::Download => "M12 4v11M7.5 10.5 12 15l4.5-4.5M5 20h14",
            Icon::Saves => "M5 4h11l3 3v13H5zM8 4v5h7V4M8 20v-6h8v6",
            Icon::Plus => "M12 5v14M5 12h14",
            Icon::Pause => "M9 5v14m6-14v14",
            Icon::Play => "M8 5.5v13l11-6.5z",
            Icon::Back | Icon::Left => "M15 18l-6-6 6-6",
            Icon::Right => "M9 18l6-6-6-6",
            Icon::Minimize => "M6 12h12",
            Icon::Maximize => "M6.5 6.5h11v11h-11z",
            Icon::Close => "M6.5 6.5l11 11M17.5 6.5l-11 11",
            Icon::Search => "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zM20 20l-4-4",
            Icon::Folder => "M3.5 6.5h6l2 2h9v10h-17z",
            Icon::External => "M14 4h6v6M20 4l-9 9M18 14v6H4V6h6",
            Icon::Trash => "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13",
            Icon::Edit => "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
            Icon::Check => "M5 12.5l4.5 4.5L19 7.5",
            Icon::Heart | Icon::HeartFill => "M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z",
            Icon::Bell => "M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15zM10 20.5a2 2 0 0 0 4 0",
            Icon::Clock => "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2",
            Icon::Settings => "M4 7h9M17 7h3M15 5v4M4 17h3M11 17h9M9 15v4",
            Icon::Tv => "M3.5 5h17v12h-17zM8.5 20.5h7M10.5 8.5l4 2.5-4 2.5z",
            Icon::Info => "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v6M12 7.5v.01",
            Icon::Puzzle => "M10 4h4v2.5a1.5 1.5 0 1 0 3 0V4h3v6h-2.5a1.5 1.5 0 1 0 0 3H20v7h-6v-2.5a1.5 1.5 0 1 0-3 0V20H4v-6h2.5a1.5 1.5 0 1 0 0-3H4V4h6z",
            Icon::Trophy => "M8 4h8v5a4 4 0 0 1-8 0zM8 6H4.5v1.5A3.5 3.5 0 0 0 8 11M16 6h3.5v1.5A3.5 3.5 0 0 1 16 11M12 13v4M8.5 20h7M10 17h4v3h-4z",
            Icon::Gamepad => "M7 8h10a4 4 0 0 1 4 4v1.5a3.5 3.5 0 0 1-6.3 2.1L13.5 14h-3l-1.2 1.6A3.5 3.5 0 0 1 3 13.5V12a4 4 0 0 1 4-4zM8 10.5v3M6.5 12h3M15.5 11.5v.01M17.5 13v.01",
        }
    }
}

#[component]
pub fn Svg(icon: Icon, #[prop(default = 20)] size: u32) -> impl IntoView {
    // Play and a favorite's heart read better solid; everything else is drawn as lines.
    let filled = matches!(icon, Icon::Play | Icon::HeartFill);
    view! {
        <svg
            width=size
            height=size
            viewBox="0 0 24 24"
            fill=if filled { "currentColor" } else { "none" }
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d=icon.path() />
        </svg>
    }
}
