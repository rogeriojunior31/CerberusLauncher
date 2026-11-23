# Mods

Install mods for recomps with one click, from [Thunderstore](https://thunderstore.io) and [GameBanana](https://gamebanana.com).

## Where mods come from

| Source | Games |
|---|---|
| **Thunderstore** | Known communities and `*-recompiled` communities found by name (Zelda 64: Recompiled and others) |
| **GameBanana** | Ship of Harkinian, Starship, SpaghettiKart, Ghostship, Unleashed Recompiled, Marathon Recompiled |

## Install and remove

1. Open the game page and go to the **Mods** tab.
2. Search, then click **Install**. The mod takes effect the next time the game opens.
3. Installed mods show **Update** when a newer version is out, and **Remove** to take them out.

Mods only come from the mod sites themselves. Downloads are size-limited, and archives are checked for unsafe paths.

## How each game loads them

- **Ship of Harkinian, Starship, SpaghettiKart, Ghostship:** `.otr` and `.o2r` packs go into the game's `mods` folder.
- **Unleashed and Marathon Recompiled:** each mod is a folder with a `mod.ini`. Cerberus enables it in the `ModsDB.ini` that the game's `cpkredir.ini` points to, the same list Hedge Mod Manager uses. That file is backed up before the first change.
- **Thunderstore recomps:** mod files go where the recomp loads them.

Some mods can't be installed automatically. These are mods that come only as `.rar`, mods with several versions to choose from, and mods without loadable files. Cerberus tells you why and links to the mod's page.

## See also

- [Recomps](recomps.md)
- [While you play](in-game.md)
