# Recomps

A **recomp** is a native PC version of a console game, rebuilt by the community from the original game's code. Examples are Zelda 64: Recompiled, Ship of Harkinian and Unleashed Recompiled. Recomps don't include the game's data: each one needs **your own copy** of the game.

![Discover](../screenshots/discover.jpg)

## Discover

**Discover** lists every recomp and native port Cerberus knows, over 1,500 of them.

- Search by game, recomp or author.
- Show the list **by recomp**, or **by game**, which groups every version of the same game.
- **I own the game** keeps only games found in your ROM folders.
- Sort by **Popular**, **Recently added**, **Recently updated** or **A to Z**, and filter by platform.
- **Genre** chips (Action, Platformers, Racing, RPG...) show how many games each has; click one to keep only that kind, again to clear it.
- **Feature** keeps the recomps that add something: controller support, achievements, local multiplayer, online, higher FPS, widescreen or mods.

The catalog combines Cerberus's own list with the public list from [recomplauncher.com](https://recomplauncher.com). The second one can be turned off in **Settings → Catalog and sources**. See [Recomp catalog](../development/catalog.md).

## Install

1. Open a recomp's page and click **Install**.
2. Cerberus downloads the release straight from the developers, from GitHub, GitLab or the project's own Forgejo server.
3. The file is checked against the checksum the host publishes. A file that doesn't match is discarded.
4. The archive is unpacked into its own folder. Supported formats are `.zip`, `.7z` and `.tar.gz`, plus releases that ship a single `.exe` or an installer.

While a recomp downloads or installs, the PC doesn't go to sleep (the screen still turns off as usual). You can **pause** a download and **resume** it later from where it stopped. **Cancel** deletes the partial file. Progress also shows on the **Downloads** page and in Big Picture Mode.

Some recomps ship a Windows installer. Cerberus runs it the first time you click **Play** and then looks for the installed game. If it can't find the game, it asks you to pick the program.

## Play

The **Play** button always shows the next step:

| Button | Meaning |
|---|---|
| **Install** | Not installed yet |
| **Choose your game copy** | The recomp needs your copy. See [Easy Setup](easy-setup.md) |
| **Open installer** | The release is an installer that hasn't run yet |
| **Locate program** | The game was installed but its program moved or wasn't found |
| **Update to …** | A newer version is out |
| **Play** | Ready |

## Updates

Cerberus checks for new versions on its own. Updates show as a badge on the cover, in **What's new**, and on the **Downloads** page under **Updates ready**, with **Update all** when there are several.

To keep a recomp on the version you have (a new one broke your mods, say), turn on **Manage → Pause updates**. It leaves the update lists until you resume.

The **Downloads** page shows the download in progress with its speed and time left, installs that **didn't finish** with the reason and **Try again**, and your **recent** installs by day. If Windows or an antivirus holds a file while a recomp installs, Cerberus tries again for a few seconds before giving up.

Before updating, Cerberus backs up your saves. It then prepares the new version next to the old one and swaps them only when everything is in place. If an update is interrupted, the previous version is restored the next time Cerberus opens.

## Versions of the same game

Some games have several recomps or ports, like Majora's Mask with Zelda 64: Recompiled and 2 Ship 2 Harkinian. The game page shows **N versions**, where one of them is marked **Recommended**.

## Recomps you already have

If you installed recomps without Cerberus:

- **Discover → Bring your recomps** scans a folder and lists the recomps it recognizes.
- On a recomp's page, **I already have it installed** lets you pick its program.

Cerberus brings them in **where they are**. It tracks play time and backs up saves, but never updates or deletes those files.

## Request a recomp

Missing one? Use **Request a recomp** in Discover, which opens a form on GitHub.

## See also

- [Easy Setup](easy-setup.md)
- [Saves](saves.md)
- [Troubleshooting](../help/troubleshooting.md)
