# Saves

Cerberus backs up recomp saves on its own and can keep them in sync between PCs through a cloud folder.

## Automatic backups

A backup of a recomp's saves is made:

- **After every session**, when the game closes.
- **Before updating** the recomp.
- **Before restoring** another backup.
- **Before uninstalling** the recomp.

The **10 most recent** backups of each game are kept. They stay even if you uninstall the game. When nothing changed since the last backup, no new one is made.

Each recomp's save location comes from the catalog. Saves can live in the game's folder, in `%LOCALAPPDATA%`, or in `%APPDATA%`.

## Restore

1. Open the game page and go to the **Saves** tab.
2. Pick a backup and click **Restore**, then **Confirm: replace the current saves**.

Your current saves go into a new backup first, so a restore can always be undone. Close the game before restoring.

**Back up now** makes a backup by hand, and **Open folder** shows the backup files.

## Cloud saves

Cerberus doesn't run its own cloud. It uses a folder that a sync service keeps up to date: OneDrive, Google Drive, Dropbox or any other.

1. Go to **Settings → Saves and cloud** and pick that folder.
2. After each backup, a copy goes to `Cerberus Saves\<game>` inside it.
3. When you open a game, Cerberus checks the cloud folder. If another PC has newer saves, it backs up the local ones first and then brings the newer ones in.

If the folder is unavailable, for example a drive that isn't connected, uploads wait. Cerberus retries when it opens and every minute after that. **Check uploads** retries right away.

## See also

- [Recomps](recomps.md#updates)
- [Data and privacy](../help/data-and-privacy.md)
