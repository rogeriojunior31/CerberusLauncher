# Settings

Every setting, in the order the **Settings** page shows them.

## App

### General

| Setting | What it does |
|---|---|
| **Start page** | Where the window opens: Home, Library or Discover |
| **Monitor** | Where the window and Big Picture Mode open. Shows only with more than one monitor |
| **Initial setup** | **Show again** brings back the first-launch guide |
| **Start with Windows** | Cerberus opens when you sign in to Windows |
| **Start hidden in the tray** | When it starts with Windows, it waits in the tray without showing the window |
| **Keep running in the tray** | Closing the window keeps Cerberus next to the clock. To quit, right-click the tray icon and choose **Quit** |

While a game is running, closing the window always keeps Cerberus in the tray, so it can count play time and back up saves. It quits by itself when the game ends.

### Appearance

| Setting | What it does |
|---|---|
| **Language** | **Automatic** follows Windows: Portuguese for Portuguese speakers, English for everyone else. You can also pick one. The window reloads in the new language |
| **Fewer animations** | No transitions or motion |

### While playing

| Setting | What it does |
|---|---|
| **Menu over the game** | <kbd>Ctrl</kbd>+<kbd>F12</kbd>, or View + Menu on a controller, opens the [in-game menu](in-game.md#the-in-game-menu). On by default |
| **Pictures with F12** | <kbd>F12</kbd> saves a [screenshot](in-game.md#screenshots) of the game. On by default |
| **Step aside while playing** | Cerberus minimizes when a game opens and comes back when it closes. Off by default |
| **Achievement notice** | The small notice over the game when an achievement unlocks, the **screen corner** it shows in, and **Test** to see it now |

### Big Picture Mode

| Setting | What it does |
|---|---|
| **Open in Big Picture Mode** | Start full screen, for the controller, every time |
| **Navigation sounds** | A short sound when you move, select and go back |

## Games

### Library and ROMs

**ROM and game folders**: the folders Cerberus scans for ROMs and disc images. **Scan again** looks for new files.

### Installation

**Recomps folder**: where new recomps are installed, with the free space shown. Games already installed stay where they are.

**Speed limit**: caps recomp downloads (1 to 50 MB/s), or none.

**Slower while you play** (on by default): while a game is open, downloads stay at up to 3 MB/s so the game doesn't lag, and go back to normal when it closes.

### Saves and cloud

**Cloud saves**: the synced folder for save backups, with **Check uploads** and **Turn off**. See [Saves](saves.md).

### Game art

- **SteamGridDB key**: your free API key for community art.
- **Saved images**: covers and backgrounds kept on disk, so pages open fast and work offline. **Clear** deletes them, and they come back as pages need them.

### Catalog and sources

- **recomplauncher.com catalog**: the large public list, turned on by default. When it's off, Discover shows Cerberus's own catalog and your sources.
- **Update now** refreshes the catalog. It also refreshes by itself every 6 hours.
- **Catalog sources**: your own JSON lists. See [Catalog sources](../development/catalog-sources.md).

## Classics

### Emulation

RetroArch (install or use yours), **Cores**, **BIOS** and **Achievements in RetroArch**. See [Classics](classics.md).

## Accounts

### Steam

Optional. Connect your free Steam Web API key so the [Achievements](achievements.md) page shows every game on your account, with achievements earned on any PC. Cerberus finds the Steam account signed in on this PC; **Use another account** takes a profile link or SteamID64. Your profile's **Game details** must be public. **Get my key on Steam** and **Profile privacy** open the right Steam pages.

### RetroAchievements

Connect with your username and Web API key to see your profile, points and recent games.

## System

### Your PC

Checks what games need from Windows, and installs it from Microsoft's website with your permission:

- **Visual C++ 2015-2022 (64-bit)**. Most recomps need it. When it's missing or old, games close right after opening, with error `0xC0000135` or `0xC0000139`.
- **DirectX (legacy components)**: D3DX, XInput 1.3 and XAudio 2.7, for older PC ports.

### Storage

**Cerberus data**: opens the data folder and shows the free space for downloads.

### About

The version, the license, and **Request a recomp**.

## See also

- [First launch](../getting-started/first-launch.md)
- [Data and privacy](../help/data-and-privacy.md)
