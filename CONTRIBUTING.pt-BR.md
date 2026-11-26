# Contribuindo com o Cerberus

[English](CONTRIBUTING.md) · **Português (Brasil)**

Valeu pela ajuda! Este guia mostra como o código é organizado, como desenvolver e como manter o catálogo de recomps.

## Como ajudar

- **Relatar um bug:** [abra uma issue](https://github.com/rogeriojunior31/CerberusLauncher/issues/new/choose). Conte o que você fez e o que aconteceu. Quando fizer sentido, inclua o registro do jogo em **Gerenciar → Histórico**.
- **Pedir um recomp:** use o formulário "Pedir um recomp" ou o botão com o mesmo nome no Descobrir.
- **Mandar um pull request:** PRs pequenos e focados são mais fáceis de revisar. As verificações abaixo precisam passar.

A [documentação](docs/README.md), em inglês por enquanto, vai mais fundo: [arquitetura](docs/development/architecture.md), [compilar](docs/development/building.md), [catálogo de recomps](docs/development/catalog.md), [fontes de catálogo](docs/development/catalog-sources.md), [traduções](docs/development/translations.md) e [publicar uma versão](docs/development/releasing.md).

## Estrutura do projeto

```text
shared/                  tipos usados pelos dois lados (Game, SettingsView, ...)
src-tauri/               backend: app Tauri 2
├── src/
│   ├── recomp/          catálogo, releases, instalação, atualizações, saves
│   ├── library.rs       a biblioteca do jogador (library.json)
│   ├── sessions.rs      jogos abertos, tempo de jogo, backup no fim
│   ├── emulation.rs     RetroArch e núcleos dos clássicos
│   ├── overlay.rs       o menu por cima do jogo (Ctrl+F12)
│   └── ...
├── catalog/             recomps.json (catálogo próprio) e candidates.json
└── installer/           hooks, textos e imagens do instalador do Windows
ui/                      interface: Leptos 0.8 compilado para WebAssembly
└── src/tv/              Big Picture Mode
tools/catalog-builder/   mantém o catálogo em dia a partir do GitHub
```

## Desenvolvimento

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cargo install tauri-cli --version "^2" --locked

cargo tauri dev                 # roda com recarga automática
cargo test --workspace --locked # o que o CI roda
cargo build -p cerberus-ui --target wasm32-unknown-unknown --locked
```

Regras do projeto:

- **Downloads são uma escolha do jogador.** Recomps vêm das releases dos desenvolvedores e usam a cópia do jogador. Fontes Hydra adicionadas pelo usuário podem instalar links HTTP(S) diretos quando ele pedir; magnet e torrent ficam só para consulta.
- **Não se copia código do RecompLauncher**, que é GPL. Só ideias e fatos. O Hydra é MIT.
- **Os dados do jogador valem ouro.** Tudo que escreve na pasta de um jogo, num save ou na configuração de outro programa precisa de backup e de um jeito de desfazer.
- **Os textos são escritos em português do Brasil**, curtos e simples, e traduzidos para o inglês. Código e comentários são em inglês.

### Traduções

Todo texto que o jogador vê passa por `tr!`, de `shared/src/i18n.rs`. O texto em português é a própria chave, e o `shared/locales/en.json` guarda o inglês:

```rust
tr!("Biblioteca")                       // "Library"
tr!("{} jogos", n)                      // "{} games"
tr!("{1} não está mais em {0}.", a, b)  // partes numeradas, quando a ordem muda
```

- Coloque o inglês de todo texto novo no `en.json`. O `cargo test -p cerberus-shared` falha se um texto ficar sem tradução, se o `en.json` guardar um texto que o código não usa mais ou se uma tradução perder uma das partes `{}`.
- Tabelas `const` usam `tk!("...")`, que marca o texto sem traduzir, e chamam `cerberus_shared::i18n::t(texto)` onde ele aparece.
- Dados salvos ficam em português (passos do histórico, motivos de backup) e são traduzidos na tela com `i18n::translate`, para um arquivo nunca misturar idiomas.
- Chaves, nomes de arquivo e pasta, URLs e nomes de marca nunca são traduzidos.
- Para um idioma novo: um valor novo em `Lang`, o `locales/<código>.json` e o idioma em `bundle.windows.nsis.languages` para o instalador.

### Publicando uma versão

1. Suba a `version` no `Cargo.toml` da raiz e no `src-tauri/tauri.conf.json`.
2. Escreva as notas da versão no topo do `CHANGELOG.md`.
3. Se as imagens do instalador mudarem, rode `pwsh src-tauri/installer/make-images.ps1 -Version x.y`.
4. Rode `cargo tauri build`. O instalador fica em `target/release/bundle/nsis/`.
5. Crie a tag `vX.Y.Z` e publique a release no GitHub com o instalador e as notas.

## Mantendo o catálogo

O catálogo próprio é o `src-tauri/catalog/recomps.json`. O app busca esse arquivo da `main` a cada 6 horas. Por isso, o que entra na `main` chega a todos sem uma nova versão.

Quem mantém o arquivo é o `catalog-builder`, na pasta `tools/`, com uma pessoa revisando:

```sh
GITHUB_TOKEN=... cargo run -p catalog-builder -- update     # atualiza o que vem do GitHub
GITHUB_TOKEN=... cargo run -p catalog-builder -- discover   # procura projetos novos
```

- O `update` só mexe no que vem do GitHub:
  - versão;
  - arquivo de cada sistema, com SHA-256;
  - padrão do nome;
  - tamanho, estrelas e sistemas.

  O que foi escrito à mão nunca é alterado.
- O `discover` grava o `src-tauri/catalog/candidates.json` e não coloca nada no catálogo sozinho.
- A Action **Catálogo** roda os dois todo dia às 9h de Brasília, ou quando alguém a dispara em Actions. Ela abre um pull request com o resultado.

**Para aceitar um candidato**, copie a entrada do `candidates.json` para o `recomps.json` e complete:

| Campo | O que pôr |
|---|---|
| `name`, `game`, `console`, `origin` | Nome do recomp, jogo original, console por extenso e o código curto (`n64`, `x360`...) |
| `title` | O mesmo valor para as versões do mesmo jogo (ex.: `majoras-mask`), para aparecerem juntas |
| `needs` | O que o jogador precisa ter: "Sua cópia de ..." |
| `about`, `adds` | Uma frase sobre o projeto e o que esta versão traz |
| `cover`, `hero`, `icon` | Imagens: capa 2:3, fundo largo e ícone, com links diretos e públicos |
| `saves` | Onde ele salva, como `{local}/Nome/saves` ou `{install}/Save`. Confira no código do projeto |

Ferramentas, bibliotecas e projetos sem jogo jogável ficam de fora.

## Fontes de catálogo

Em **Configurações → Catálogo e fontes**, o jogador pode adicionar uma lista JSON de jogos publicados no GitHub pelos próprios desenvolvedores, como jogos livres, homebrew e ports:

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

Só `id`, `name` e `repo` são obrigatórios. Cada jogo é instalado da release oficial do repositório, conferida pelo checksum do GitHub.

Entradas de fontes de projetos com links próprios de download (`uris`, `magnet`, `url`, `download`) são ignoradas. Listas do Hydra com a chave `downloads` podem ser adicionadas por URL HTTPS: publicações com links HTTP(S) diretos em formatos suportados entram no Descobrir e instalam quando o jogador pedir. Magnet e torrent continuam só para consulta.
