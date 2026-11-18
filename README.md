# Cerberus Launcher

Launcher de jogos para PC, 100% Rust: backend em [Tauri 2](https://tauri.app) e interface em
[Leptos](https://leptos.dev) (compilada para WebAssembly).

## O que faz

- **Biblioteca:** encontra jogos da Steam e da Epic, e aceita qualquer `.exe` adicionado à mão
  (nome e capa buscados na Steam automaticamente, editáveis).
- **Descobrir:** instala e atualiza recomps (versões nativas para PC de jogos de console) direto
  das releases no GitHub de cada desenvolvedor, conferindo o SHA-256 que o GitHub guarda de cada
  arquivo. O Cerberus nunca baixa arquivos de jogo: cada recomp pede a sua cópia quando abre.

O catálogo de recomps fica em `src-tauri/catalog/recomps.json`.

## Estrutura

- `shared/`: tipos usados pelo backend e pela UI (ex.: `Game`).
- `src-tauri/`: backend (biblioteca, launch de jogos, acesso ao disco).
  - `src/recomp/`: catálogo, consulta ao GitHub, escolha do arquivo e instalação.
- `ui/`: interface em Leptos.

## Rodando

Precisa de Rust, do target WASM, do Trunk e do Tauri CLI:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cargo install tauri-cli --version "^2" --locked
```

Depois, na raiz do projeto:

```sh
cargo tauri dev     # desenvolvimento, com reload
cargo tauri build   # instalador de release
cargo test -p cerberus-launcher
```

## Fontes de catálogo

Em Configurações → Fontes de catálogo dá para adicionar uma lista JSON de jogos publicados no
GitHub pelos próprios desenvolvedores (jogos livres, homebrew, ports). Formato:

```json
{
  "name": "Jogos livres",
  "games": [
    {
      "id": "srb2",
      "name": "Sonic Robo Blast 2",
      "repo": "STJr/SRB2",
      "assets": { "windows": "SRB2-*-Full.zip" },
      "cover": "https://.../capa.png",
      "about": "Uma frase sobre o jogo."
    }
  ]
}
```

Só `id`, `name` e `repo` são obrigatórios. Cada jogo é instalado da release oficial do repositório,
conferida pelo checksum do GitHub. Entradas com links próprios de download (`uris`, `magnet`,
`url`, `download`) são ignoradas: o Cerberus não distribui cópias de jogos de terceiros.

## Créditos

Ideias de comportamento vêm de dois projetos de referência, reimplementadas aqui do zero:

- [RecompLauncher](https://github.com/elripalda/RecompLauncher): como escolher o arquivo certo de
  uma release, conferir o checksum do GitHub, desembrulhar a pasta do topo do zip e atualizar sem
  apagar saves.
- [Hydra](https://github.com/hydralauncher/hydra): onde a Epic guarda as instalações.

A estrutura da interface (barra lateral, banner, fileiras, página do jogo com abas) segue a desses
launchers, escrita do zero com identidade própria.

Catálogo de recomps: a lista pública de [recomplauncher.com](https://recomplauncher.com)
(`pc-catalog.json`, a mesma do RecompLauncher), lida no formato dela, guardada em disco e pedida
de novo no máximo a cada 6 horas. Sem ela, o Cerberus usa a lista própria em
`src-tauri/catalog/recomps.json`. Os recomps em si vêm sempre do GitHub de cada desenvolvedor.

Recursos de terceiros:

- Fontes [Figtree](https://github.com/erikdkennedy/figtree) e
  [Archivo](https://github.com/Omnibus-Type/Archivo), sob a SIL Open Font License 1.1
  (`ui/fonts/OFL-*.txt`).
- Capas e screenshots dos jogos originais: [libretro-thumbnails](https://github.com/libretro-thumbnails).

## Licença

MIT. Veja [LICENSE](LICENSE).
