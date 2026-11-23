# Installation

How to install, update and uninstall Cerberus on Windows.

## Requirements

- Windows 10 or 11, 64-bit.
- Microsoft WebView2. Windows 11 has it built in. On older systems, the installer adds it.
- An internet connection to install recomps, mods, RetroArch and art. Games you already have play offline.

## Install

1. Open the [latest release](https://github.com/rogeriojunior31/CerberusLauncher/releases/latest) and download `Cerberus-Launcher_<version>_x64-setup.exe`.
2. Run it. The installer speaks English or Brazilian Portuguese, following your Windows language.
3. Accept the MIT license, pick a folder and a Start menu folder.
4. On the last page, choose whether to **Run Cerberus Launcher** and **Create a desktop shortcut**.

Cerberus installs **for your user only**, in `%LOCALAPPDATA%\Cerberus Launcher`, so it never asks for administrator rights.

### "Windows protected your PC"

The installer isn't code-signed yet, so Windows SmartScreen may block the first run. Click **More info**, then **Run anyway**.

To make sure the file is the one we published, compare its SHA-256 with the one in the release notes:

```powershell
Get-FileHash .\Cerberus-Launcher_0.10.0_x64-setup.exe -Algorithm SHA256
```

## Update

Download the new installer and run it over the old version. Your library, settings and save backups stay where they are.

## Uninstall

Use **Settings → Apps → Installed apps** in Windows, or run `uninstall.exe` from the install folder.

- The program, its shortcuts, its entry in Installed apps and the "Start with Windows" entry are removed.
- **Your data stays** by default. Tick **Also delete library, save backups and downloaded recomps** to remove `%APPDATA%\app.cerberus.launcher` as well.

> Recomps installed in the default folder live inside that data folder. If you tick the box, they're deleted too.

## See also

- [First launch](first-launch.md)
- [Data and privacy](../help/data-and-privacy.md)
