# Library

Every game Cerberus knows about, in one place.

![The library](../screenshots/library.jpg)

## Where games come from

| Source | How they get in |
|---|---|
| **Steam** | Found on their own in every Steam library folder on the PC. |
| **Epic** | Found on their own from the Epic Games Launcher's install records. |
| **Recomps** | Installed from [Discover](recomps.md), or brought in from a folder you already have. |
| **Added** | Any `.exe` you add with **+** (Add game) at the bottom of the side menu. Cerberus looks up the name and cover on Steam. |
| **Classics** | ROMs and disc images found in your ROM folders. See [Classics](classics.md). |

Steam and Epic games still start through their own launchers. Cerberus opens them and counts play time while it's running, even when it's minimized to the tray. For Steam games, the play time Steam itself counts is shown too.

With your Steam Web API key connected (see [Achievements](achievements.md)), the games on your Steam account that aren't on this PC show up under **Not installed**. **Install** opens Steam's own install window, and the game turns into **Play** once Steam finishes.

## Tabs and filters

- **All**, **Favorites**, **Collections**, **Recomps**, **PC** and **Classics** split the library by kind.
- **Hidden** shows up only when something is hidden.
- The source chips (**All sources**, **Steam**, **Epic**, **Added**) filter the PC games, with how many each one has.
- **Installed** / **Not installed** separates what's on this PC from the rest of your Steam account.
- The status chips (**Playing**, **Want to play**, **Beaten**...) show up once you mark a game.
- Search by name, sort by **A to Z**, **Recently played**, **Most played**, **Your rating** or **Achievements** (closest to 100% first), and switch between grid and list view.
- **Play something random** picks one of the installed games on screen, with **Pick another** and **Play**.

## Your status, rating and notes

On a game's page, the **Your game** card keeps your own word on it:

- **Status:** Playing, Want to play, Beaten, Completed, On hold or Dropped. Click it again to clear it.
- **Rating:** 1 to 5 stars.
- **Notes:** where you stopped, what's left, tips. Saved when you click out of the box.

They stay on this PC in `game-notes.json` and work for every kind of game.

## Favorites, collections and hidden games

Right-click any cover for a quick menu:

- **Play**, or whatever the next step is (install, update, set up).
- **Game page**.
- **Favorite**, which also puts the game on Home's Favorites row and at the top of **Ready to play** in the side menu.
- **Collections**: tick the collections this game belongs to, or create a new one on the spot.
- **Hide from library**: the game moves to the Hidden tab and leaves Home, the sidebar and Big Picture Mode. **Show in library** brings it back.

In the **Collections** tab you can create, rename and delete collections. Deleting a collection never deletes its games.

## Art

- Covers, backgrounds and logos come from the stores, the recomp catalog and [libretro-thumbnails](https://github.com/libretro-thumbnails) for classics.
- **Manage → Customize art** picks community art from [SteamGridDB](https://www.steamgriddb.com) or an image of your own, with crop and zoom. SteamGridDB needs a free API key, added in **Settings → Game art**.
- **Manage → Edit name and cover** renames a game you added by hand.

## The game page

Clicking a game opens its page. It has the stats (play time, last session, versions, stars), what the recomp needs, and these tabs:

| Tab | For |
|---|---|
| **Overview** | Stats, description, setup notes and Easy Setup |
| **Saves** | [Save backups](saves.md) |
| **Settings** | The game's own options and [visual filters](in-game.md#visual-filters) |
| **Mods** | [Mods](mods.md) |
| **Screenshots** | Pictures taken with F12 |
| **Manage** | [Shortcuts, Steam, DLSS, moving and more](managing-games.md) |

Which tabs show up depends on the game. For example, Saves and Mods are only for recomps.

## See also

- [Recomps](recomps.md)
- [Managing a game](managing-games.md)
