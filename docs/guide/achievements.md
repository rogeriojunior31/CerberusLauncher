# Achievements

The **Achievements** page brings Steam and RetroAchievements together: every game with achievements, what you unlocked lately, and each game's full list.

## Where the achievements come from

| Source | What you need | What you get |
|---|---|---|
| **Steam, from this PC** | Nothing. Steam just has to be installed | The games Steam has already synced on this PC, with play time and unlock dates |
| **Steam, with the Web API** (optional) | Your free Steam Web API key and public game details | Every game on your account, with achievements earned on any PC |
| **RetroAchievements** | Your account, connected in **Settings → Accounts → RetroAchievements** | Your classics, points, hardcore unlocks and awards |

Without the Web API, Steam achievements come from the Steam client's own cache on this PC. Games you only played on another PC, or that Steam hasn't synced here, are missing. Turning the Web API on fills them in.

### Turning on the Steam Web API

1. Open **Settings → Accounts → Steam**. Cerberus already shows the Steam account signed in on this PC.
2. Click **Get my key on Steam**, create a key (Steam asks for a domain; any name works, such as `localhost`) and paste it.
3. Click **Connect**.
4. In **Profile privacy**, set **Game details** to **Public**. "Friends only" isn't enough, because the Web API doesn't count as a friend.

> [!NOTE]
> After you change the privacy setting, Steam can take several minutes to let achievements through. Until then, Cerberus says that game details are private.

To read another account, use **Use another account** and type its profile link or SteamID64. **Disconnect** removes the key from this PC.

## The page

- **Summary:** unlocked achievements, games, platinums (games at 100%), beaten games, hardcore awards and RetroAchievements points, with an overall progress bar.
- **Platinums:** every game with all its achievements unlocked, newest first, with the date of the last one. Their rows in the list carry a gold **Platinum** badge, and so does the bar on the Library cover.
- **Close to 100%:** the four games nearest to completion, with how many achievements are left.
- **Latest achievements:** your most recent unlocks, newest first. Click one to open its game.
- **All games:** search, sort (recent unlocks, closest to completion, most achievements, most played, A–Z), filter by platform (Steam, RetroAchievements) and by state (in progress, beaten, platinums, not started), each with how many games it holds.

The page opens at once with what's already on this PC, then updates from Steam and RetroAchievements in the background. **Refresh** asks again. Answers are kept on disk, and a game's Steam achievements are asked again only after you play it.

## A game's achievements

Click a game to see every achievement, with:

- Unlock date, and hardcore date for RetroAchievements.
- **Rarity:** the share of players who have it. Rare ones are marked.
- **Secret achievements** stay hidden until you click **Click to reveal** or turn on **Show secret ones**.
- Order by the game's own order, recent unlocks, rarest or most common.

Steam games in your Library also have an **Achievements** tab on their game page.

## Awards

RetroAchievements gives awards per game, and Cerberus shows them as badges:

| Award | Meaning |
|---|---|
| **Beaten** | You finished the game (softcore) |
| **Beaten in hardcore** | Finished in hardcore mode. Gold, with a shine |
| **Completed** | Every achievement in softcore. Steam games at 100% get this too |
| **Mastered** | Every achievement in hardcore. Gold, with a shine |

## Unlock notifications

- **Steam:** while a Steam game runs, Cerberus watches the file where Steam stores your stats and tells you the moment an achievement unlocks. It says when the achievement is rare (fewer than 10% of players have it), and when you reach 100%.
- **Classics:** achievements unlocked in RetroArch show up as a notification too.

Notifications use Windows notifications. A game in exclusive full screen may hide them; borderless or windowed mode shows them.

## Troubleshooting

| Problem | What to do |
|---|---|
| "Game details are private" | In Steam's privacy settings, set **Game details** to **Public**, wait a few minutes, then click **Refresh** |
| "Steam turned requests away for a moment" | Steam limits how fast a key can ask. Wait a few minutes and click **Refresh**; Cerberus continues where it stopped |
| A Steam game is missing without the Web API | Open the game once through Steam on this PC, or turn on the Web API |
| RetroAchievements games are missing | Check the connected username in **Settings → Accounts → RetroAchievements** |

## See also

- [Classics](classics.md) for RetroArch and RetroAchievements in games
- [Settings](settings.md#accounts)
- [Data and privacy](../help/data-and-privacy.md)
