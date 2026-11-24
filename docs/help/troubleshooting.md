# Troubleshooting

Common problems and how to fix them.

## The installer is blocked by Windows

SmartScreen shows "Windows protected your PC" because the installer isn't code-signed yet. Click **More info → Run anyway**. You can check the file's SHA-256 against the release notes; see [Installation](../getting-started/installation.md).

## A game closes right after opening

When a game Cerberus started closes with an error, the game page and **What's new** explain why. The game's **History**, under **Manage**, keeps the details and the path to the game's log when there is one.

| Code | Meaning | Fix |
|---|---|---|
| `0xC0000135` | A DLL the game needs is missing | Install Visual C++ in **Settings → Your PC** |
| `0xC0000139` | A DLL is too old for the game | Cerberus names the DLL. It can be an old copy in the game folder (delete or rename it), Visual C++ (update it in **Settings → Your PC**) or the Vulkan in your graphics driver (update the driver) |
| `0xC000007B` | A 32-bit or corrupted DLL | Reinstall Visual C++ in **Settings → Your PC** |
| `0xC0000005` | Invalid memory access (crash) | If it always happens at the same point, update the recomp or your graphics driver |
| `0xC00000FD` | Stack overflow | Usually a bug in the game. Check for a newer version |
| `0xC0000409`, `0xC0000602` | The game stopped itself after an internal error | Check for a newer version |
| `0xE06D7363` | Unhandled exception | Check for a newer version |
| `0x80000003` | Debug breakpoint | Usually a test build of the recomp |

Older PC ports may need the legacy DirectX components, also in **Settings → Your PC**.

## Windows won't let the game open

Cerberus says which one stopped it:

- **Smart App Control:** it blocks programs that aren't signed, and recomps usually aren't. It has no exceptions; to play, turn it off in **Windows Security → App & browser control**.
- **Antivirus:** if you trust the recomp, allow its folder in the antivirus, then use **Repair installation** in the game's **Manage** tab, since the antivirus may have removed a file.
- **A Windows policy** (from your company, or AppLocker): install recomps in a folder that's allowed, in **Settings → Installation**.
- **Administrator rights:** Cerberus doesn't open games as administrator. See if the project has a version that doesn't need it.

## The game can't find my copy

- Make sure the folder with your ROM or disc image is in **Settings → Library and ROMs**, then click **Scan again**.
- For Nintendo 64, Cerberus checks the region and revision. The message tells you which version the recomp needs.
- Pick the file by hand with **Choose another...** in Easy Setup. See [Easy Setup](../guide/easy-setup.md).

## A download fails

- **The connection dropped:** try again. The download resumes where it stopped.
- **The checksum doesn't match:** the file was discarded. Try again; if it keeps happening, the release may have changed. Use **Update now** in **Settings → Catalog and sources**.
- **GitHub is busy or limiting requests:** wait a minute and try again. When a site fails or asks for a pause, Cerberus leaves it alone on its own for a while (30 seconds at first, doubling up to an hour, or what the site asks for) and tries again later, so it doesn't make things worse.

## Screenshots are black, or the in-game menu doesn't show

The game is in **exclusive full screen**. Switch it to windowed or borderless full screen. Both features also need the game to be started by Cerberus.

## Visual filters don't apply

Filters work in games with Direct3D 10, 11 or 12. Cerberus tells you when a game uses something else, such as Vulkan or Direct3D 9. It also turns them off when the game folder already has its own ReShade or a similar mod.

## Play time for Steam or Epic games isn't counted

Cerberus counts it while it's running. Turn on **Settings → General → Start with Windows** and **Start hidden in the tray** so it's always there.

## Something else

[Open an issue](https://github.com/rogeriojunior31/CerberusLauncher/issues/new/choose) with what you did, what happened, and the game's **History** entry when it applies.

## See also

- [FAQ](faq.md)
- [While you play](../guide/in-game.md)
