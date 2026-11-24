# While you play

Tools that work while a game opened by Cerberus is running.

> These features work for games Cerberus starts itself: recomps and games added by hand. Steam and Epic games start through their own launchers.

## Play time

Cerberus records how long each session lasts and saves the total every 15 seconds, so a crash or power cut loses almost nothing.

For games that open a launcher first, go to **Manage → Tracked process** and choose which program counts as playing.

Programs that come along with a game but aren't it, like a crash reporter, an updater or Windows' error reporting, don't keep the session going: once only those are left, the session ends.

Steam and Epic games are tracked by watching their programs while Cerberus is running, even from the tray.

## Achievement notifications

While a Steam game runs, Cerberus tells you the moment an achievement unlocks, says when it's rare, and celebrates 100%. Achievements in RetroArch are announced too. See [Achievements](achievements.md#unlock-notifications).

The notice, its corner, F12 and the menu can each be turned off in [Settings › While playing](settings.md#while-playing).

## Screenshots

Press <kbd>F12</kbd> (or **View + RB** on a controller) while the game is in front. Cerberus takes a picture of **the game window only**, never the rest of the screen, and saves it as a PNG.

- The screenshots show up in the game's **Screenshots** tab, newest first. **Open folder** shows the files.
- F12 still reaches the game and Steam, so their own screenshot keys keep working.
- In exclusive full screen, Windows can't capture the game and the picture would be black. Cerberus warns you instead of saving it. Use windowed or borderless mode.

## The in-game menu

Press <kbd>Ctrl</kbd>+<kbd>F12</kbd> to open a menu over the game. On a controller, press **View + Menu** together, or hold **View** for a second (holding it while pressing other buttons, as in a game, doesn't count). It shows:

- The game, how long you've been playing this session, and your total time.
- **Back to the game**.
- **Take screenshot**: the menu steps aside so the picture shows only the game.
- **Open in Cerberus**: opens the game's page and leaves the game running.
- **Close the game**: asks for confirmation, then closes the game the way its window's X would.
- Your latest screenshots.

Move with the arrow keys or the D-pad, press <kbd>Enter</kbd> or **A** to choose, and <kbd>Esc</kbd> or **B** to go back. <kbd>Ctrl</kbd>+<kbd>F12</kbd> again also closes it.

The menu needs the game in a window or borderless full screen. Nothing can be drawn over a game in exclusive full screen.

## Visual filters

The game's **Settings** tab has picture filters, applied with [ReShade](https://reshade.me):

| Filter | Effect |
|---|---|
| **Sharp** | Brings out details and edges |
| **Vivid colors** | More saturation and contrast |
| **CRT** | Scanlines and the curved screen of an old TV |
| **Film** | Warm tones and dark corners |

Each one has a **Strength** slider. The first time, Cerberus downloads ReShade from its official website, about 5 MB.

- Filters work in games with **Direct3D 10, 11 or 12**.
- ReShade is placed in the game folder only while the game runs, and removed when it closes.
- If the game folder already has its own ReShade or a similar mod, Cerberus leaves it alone and turns the filter off.
- In game, the <kbd>Home</kbd> key opens ReShade's menu for fine-tuning.

## The game's own settings

For recomps the catalog knows, the **Settings** tab can also change the game's own options, such as full screen, resolution and frame rate. They're written straight to the game's settings file. Close the game before changing them, since games write their own settings when they close.

## See also

- [Big Picture Mode](big-picture.md)
- [Troubleshooting](../help/troubleshooting.md)
