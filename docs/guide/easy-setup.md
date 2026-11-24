# Easy Setup

Recomps need data from **your own copy** of the original game. Easy Setup finds that copy and hands it to the recomp the way it expects. Everything happens on your PC; nothing is uploaded.

## How it finds your copy

Cerberus scans the [ROM folders](settings.md#library-and-roms) you added:

- **Nintendo 64 ROMs** are recognized by their header. Cerberus also checks the region and revision, and warns you when the recomp needs a different one (for example, the USA version).
- **Everything else** is matched by name, including disc images.

When a match is found, the game page shows **We found your copy**. Click **Use this copy**, or **Choose another...** to pick a file or folder yourself.

## How the copy is handed over

Each recomp says in the catalog how it wants the game. Cerberus supports these ways:

| The recomp… | Cerberus… |
|---|---|
| Asks for the file itself on first start | Opens it and lets the recomp ask |
| Takes the file as a command-line option | Starts it with that option pointing to your copy |
| Reads the file from an environment variable | Sets that variable when it starts the game |
| Reads files from a folder | Copies your file there, or extracts the game's files into it |

## Disc images

When a recomp needs the files inside a disc, Cerberus extracts them itself, without any extra program. It reads:

| Format | Typical use |
|---|---|
| ISO 9660 (`.iso`) | PlayStation 2, PSP, most CDs and DVDs |
| XDVDFS | Xbox and Xbox 360 discs |
| CHD (`.chd`) | Compressed CD and DVD images (the data track) |
| CSO (`.cso`) | Compressed PSP and PS2 images |

When a recomp wants the image file itself, a CHD or CSO is converted to an `.iso` in the game's folder. **Your original file is never changed.**

Not supported yet: RVZ, ZSO (LZ4) and "child" CHDs that depend on a parent file.

## Folders instead of files

Some recomps need a folder with the game's files, such as Switch titles with `exefs` and `romfs`. Easy Setup then asks for a folder and checks that it contains what the recomp needs.

## See also

- [Recomps](recomps.md)
- [Classics](classics.md)
