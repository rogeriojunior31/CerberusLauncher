# Catalog sources

Players can add their own lists of games in **Settings → Catalog and sources → Catalog sources**, by HTTPS URL.

## Cerberus format

A list of games published by their own developers on GitHub, such as free games, homebrew and ports:

```json
{
  "name": "Free games",
  "games": [
    {
      "id": "srb2",
      "name": "Sonic Robo Blast 2",
      "repo": "STJr/SRB2",
      "assets": { "windows": "SRB2-*-Full.zip" },
      "cover": "https://.../cover.png",
      "about": "One sentence about the game."
    }
  ]
}
```

| Field | Required | Meaning |
|---|---|---|
| `id` | Yes | Unique within the list |
| `name` | Yes | The game's name |
| `repo` | Yes | `owner/project` on GitHub |
| `assets` | No | The release file to pick per system, as a pattern with `*` |
| `cover` | No | A 2:3 cover image |
| `about` | No | One sentence |

Games from these lists show up in **Discover** and install from the repository's official release, checked against GitHub's checksum.

## Rules

- Only `https://` addresses are accepted.
- The list must be UTF-8 and at most 64 MiB.
- Entries in project sources with their own download links (`uris`, `magnet`, `url`, `download`) are ignored; those projects install from official releases.

## Hydra lists

Lists in the Hydra format (with a `downloads` key) can also be added. Entries with direct HTTP(S) links to `.zip`, `.7z`, `.tar.gz`, `.tgz`, `.exe` or `.msi` files appear in **Discover** and install only when the player clicks **Install**. PixelDrain, MediaFire and MEGA (single-file) links are resolved to their file before downloading; MEGA's content is decrypted locally from the key in the link's fragment. Magnet and torrent entries remain available under **Browse**, which searches titles and shows the size and date the source declares. GoFile is not installed from (its API needs a token obtained by running the site's JavaScript).

Each source keeps its full local copy, so browsing and its Discover entries still work offline. **Update** syncs it again. Hydra links may not provide a checksum; use only sources you trust.

## See also

- [Recomp catalog](catalog.md)
- [Settings](../guide/settings.md#catalog-and-sources)
