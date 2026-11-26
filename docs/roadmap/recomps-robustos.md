# Recomps robustos: plano de implementação

Aprofunda o item 9 do Mapa rápido do `ROADMAP.md` (com 5.3, 6.7 e 7.5). Levantamento de 25/11/2026 lendo o código do
Cerberus (`ajustes-rapidos`), o RecompLauncher-SDK (MIT, pode ser adaptado com crédito) e o RecompLauncher (GPL: **só
ideias**, nenhuma linha copiada). Tudo que não foi conferido no código ou numa fonte primária está marcado
**não verificado**.

## Objetivo

O jogador de recomp nunca fica preso: volta para a versão que funcionava, joga com a cópia que já tem (RVZ, WBFS...)
sem programa de fora, recebe do jogo o que o desenvolvedor descreveu no `recomplauncher.json`, entende por que o jogo
não abre (com botão que corrige) e recupera o espaço que o build deixou para trás. Sempre sem perder saves, tempo de
jogo nem a versão anterior, mesmo com a luz caindo no meio.

Regras que valem para todos os capítulos:

- Nunca pirataria: o Cerberus só trabalha sobre a cópia do jogador. Nada de baixar jogos, BIOS, firmware ou chaves.
- Nada é baixado de terceiros sem aprovação explícita do jogador na hora (DLSS é o único caso previsto, capítulo 2.6).
- RecompLauncher GPL: só comportamento observado; o código é escrito do zero.
- Cada entrega fecha com teste automático **e** um teste real num recomp instalado (regra do ROADMAP).

---

## 1. Escolher uma versão anterior, fixar e voltar

### Estado atual

- Pausar: `set_updates_paused` em `src-tauri/src/main.rs:2854` grava o id em `Settings::paused_updates`
  (`src-tauri/src/settings.rs:66`, um `BTreeSet<String>`); `main.rs:1657` copia para `Game::updates_paused`
  (`shared/src/lib.rs:90`). A UI é o interruptor "Pausar atualizações" em `ui/src/game.rs:1457`, a lista de pausados em
  `ui/src/pages.rs:185` e o selo em `ui/src/tiles.rs:105`.
- Instalar/atualizar sempre vai para a versão mais nova: `recomp::install` → `install_now` → `choose_download`
  (`src-tauri/src/recomp/mod.rs:1248`). Usa o arquivo direto do catálogo (`CatalogEntry::tag` + `files[os].sha256`) ou
  `releases::latest` (`recomp/releases.rs:71`), que já pede **10 releases** (`Forge::releases_api`, `recomp/forge.rs:65`)
  e fica só com a primeira que não é rascunho/beta (`parse_latest`).
- A troca é transacional: `recomp/update.rs` (`Update::begin` → `candidate()` → `publish()` → `register` → `finish`),
  com `intent.json` e a `generation` em `library.json` como registro de commit. Ao dar certo, `cleanup()` **apaga**
  `previous`: não sobra a versão anterior em disco. Antes de atualizar, `backup_saves(..., "Antes de atualizar")`.
- `ui/src/versions.rs` não trata versões de release: agrupa **projetos** diferentes do mesmo jogo (forks). Não confundir.
- `recomp::preparation` (`mod.rs:446`) já refaz o `prepare` quando a versão muda (a chave em `.cerberus-prepared`
  começa pela versão), então voltar de versão já dispara a preparação de novo.

### Desenho

Três ideias separadas na UI e no estado:

| Conceito | Significado | Onde fica |
| --- | --- | --- |
| Pausar (existe) | Não oferece versão nova; fica na que está. | `settings.paused_updates` |
| Fixar | Fica numa versão **escolhida**; se outra estiver instalada, oferece "Voltar para vX". | `settings.pinned_versions: BTreeMap<String, Pin>` |
| Versão anterior guardada | A pasta da versão substituída, mantida por N dias para voltar sem baixar. | `<installs>/.cerberus-previous-<hash>/` + `previous.json` |

```rust
// settings.rs
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct Pin {
    pub tag: String,          // "v1.2.2"
    pub asset: String,        // nome do arquivo escolhido para este sistema
    pub sha256: Option<String>,
    pub pinned_at: u64,
}
pub pinned_versions: BTreeMap<String, Pin>, // por game id; fixar implica pausar

// shared/src/lib.rs (UI)
pub struct ReleaseChoice {
    pub tag: String, pub name: String, pub published: String,
    pub prerelease: bool, pub installed: bool, pub newest: bool,
    pub size: u64, pub has_build_for_this_pc: bool, pub kept_on_disk: bool,
}
```

Backend:

1. `releases::list(client, cache, forge, fresh) -> Vec<Release>`: o mesmo JSON que `latest` já busca (cache de 6 h),
   mas devolvendo todas (até 10; subir para 30 com `per_page`). `Release { version, prerelease, published, assets }`.
   `latest` passa a ser `list(..).first_stable()`.
2. Comando `recomp_versions(id) -> Vec<ReleaseChoice>`: filtra pelas que têm build para este sistema
   (`assets::by_pattern` / `assets::pick`, a mesma escolha de `choose_download`), marca instalada, mais nova, fixada e
   guardada em disco.
3. `install_now` ganha `wanted: Option<String>` (tag). `choose_download_at(entry, tag)` usa `Forge::release_file(tag,
   name)` quando o nome é conhecido; senão pega o asset da release da lista. Sem checksum do catálogo, usa o `digest`
   que a API do GitHub dá por asset (`sha256:...`, quando houver — **não verificado** para GitLab/Forgejo).
4. Comando `install_recomp_version(id, tag, pin: bool)`. Com `pin`, grava `Pin` e põe o id em `paused_updates`.
   `set_updates_paused(false)` também remove o `Pin` ("Voltar a atualizar").
5. Versão anterior guardada: em `Update::finish(Ok)`, em vez de apagar `previous`, renomeá-la para
   `.cerberus-previous-<hash>` (mesmo volume, rename atômico) com `previous.json { game, tag, generation, kept_at,
   bytes }`. Uma só por jogo (a nova substitui a velha). Limpeza: ao abrir o Cerberus, apagar as com mais de 14 dias
   ou quando o disco tiver menos de 10% livre; o usuário pode desligar em Configurações › Instalação.
   Voltar com ela é uma `Update` cujo `candidate` vem de um rename da pasta guardada em vez de download + unpack.
6. `register` grava `version = tag`; a `generation` sobe como numa atualização (o registro de commit não muda).

Saves entre versões: antes de qualquer troca já há `backup_saves`. Ao voltar para uma versão **mais antiga**, o nome do
backup passa a ser "Antes de voltar para vX" e a UI avisa: "Saves feitos na versão nova podem não abrir na antiga. Uma
cópia fica em Saves." Se o `recomplauncher.json` da versão nova tiver `saves` diferentes da antiga, mostrar os dois
caminhos (não mover nada sozinho).

### Casos de borda

- **Interrompido no meio** (luz, Cerberus morto): o mecanismo de `update.rs` já cobre; o rename de `previous` para
  `.cerberus-previous-*` acontece **depois** do commit (generation avançou), então `reconcile` vê
  `intent.generation + 1 == generation` e só limpa. Testar a queda entre o commit e o rename (pasta `previous` órfã:
  vira guardada se `intent.json` ainda existir; senão apagada).
- **Release apagada do servidor** (nightly removido): a versão fixada some da lista. Mostrar "fixado em vX (não está
  mais no servidor)" e não oferecer a mais nova como "voltar". Ideia observada no RecompLauncher (teste
  `a_deleted_nightly_is_not_updated_back...`).
- **Tag sem build para este sistema** ou só com instalador (`kind == "installer"`): não aparece; instalador pede
  confirmação extra porque roda programa do desenvolvedor.
- **Limite do GitHub** (60/h sem login): a lista usa o mesmo cache de 6 h; o botão "Atualizar lista" força com
  `fresh = true` no máximo uma vez por minuto.
- **Espaço**: voltar baixando precisa de `asset.size + tamanho da instalação` (a nova fica em `next` antes do rename);
  usar `disk::require` antes do download. A versão guardada mostra o tamanho em Gerenciar com "Apagar agora".
- **Antivírus prendendo arquivo** no rename: `throttle::rename_patiently` já existe; usar o mesmo para a pasta guardada.
- **Jogo aberto**: `install` já recusa ("Feche o jogo antes de atualizá-lo").
- **Mods e DLSS trocados** pelo jogador: `preserve` já leva arquivos que a release não tem; documentar que a versão
  guardada leva os mods do momento.
- **Recomp `external`**: nada disso aparece (os arquivos são do jogador).
- **Vários jogos numa release** (`part_of`): versão é da release; a escolha aparece só na release.

### UI

- Página do jogo › Gerenciar, no lugar do interruptor solitário, um cartão **Versão**:
  - "Instalada: v1.3.0 (12/11/2026)". Botão **Trocar de versão** abre uma lista (radio) com as releases: data, selo
    "mais nova", "teste" (pre-release, escondidas atrás de "Mostrar versões de teste"), "guardada no PC" (instantânea).
  - Na lista, caixa "Ficar nesta versão (não atualizar)" marcada por padrão quando se escolhe uma mais antiga.
  - Confirmação: "Voltar para v1.2.2? Seus saves ganham uma cópia antes. Saves feitos na v1.3.0 podem não abrir."
  - Estado fixado: "Fixado em v1.2.2 · Voltar a atualizar". O interruptor "Pausar atualizações" continua para quem só
    quer pausar.
- `ui/src/updates.rs`: um jogo fixado não aparece em "Atualizações disponíveis"; aparece em "Pausados" com a tag.
- Fila de downloads: a tarefa diz "Voltando para v1.2.2".

### Critério de aceite

1. Num recomp real com 2+ releases (ex.: Zelda64Recompiled), escolher a anterior instala, abre e o cartão mostra a
   tag certa; `library.json` tem `version` = tag e `generation` +1.
2. Fixado, um novo `catalog-changed` com tag mais nova não oferece atualização nem a baixa sozinho.
3. "Voltar a atualizar" oferece a mais nova; atualizar e depois "voltar" usa a pasta guardada sem rede (desligar a
   rede para provar).
4. Matar o processo do Cerberus em cada passo (download, unpack, entre os renames, depois do commit) e reabrir: o jogo
   abre na versão antiga ou na nova, nunca num estado misto, e os saves estão intactos.
5. Backup "Antes de voltar para vX" aparece em Saves.

### Testes

- `releases.rs`: `list` com JSON sintético do GitHub/GitLab/Forgejo (rascunho, pre-release, sem asset do sistema).
- `update.rs`: os testes de interrupção já existentes (`interrupt_during_preparation...`) ganham variantes com
  `keep_previous = true`: queda depois do commit e antes do rename; pasta guardada órfã sem `intent.json`.
- `mod.rs`: `choose_download_at` com catálogo com `files` e sem; tag que não existe → erro claro.
- Função pura `offered_update(installed, pin, paused, releases) -> Option<tag>` com tabela de casos (fixado, pausado,
  fixado numa tag apagada, versão instalada mais nova que a do catálogo).
- Limpeza das guardadas por idade e por pouco espaço com `tempdir` e um `available` injetado.

Esforço: **M**.

---

## 2. Campos do `recomplauncher.json` ainda ignorados

Fonte: `RecompLauncher-SDK/docs/setup-inputs.md` e `schema/recomplauncher.json` (MIT). O leitor do Cerberus é
`src-tauri/src/recomp/manifest.rs`; struct `Input` (linha ~49) **não tem** `extract`, `contains`, `destination` nem
`check.nup_build_id`. Serde ignora o que não conhece, então nada quebra hoje: só não acontece.

### 2.1 `extract` (desempacotar o disco na preparação)

**Estado atual.** O catálogo próprio já faz isso: `CopyHandoff::Folder { extract }` → `put_in_folder`
(`recomp/mod.rs:924`) chama `xdvdfs::extract` ou `iso9660::extract`. Pelo manifest, `use_game_copy` (`mod.rs:848`) só
chama `manifest::place`, que copia o arquivo.

**Desenho.** `Input.extract: Option<Extract>` com `enum Extract { Xdvdfs, Iso9660 }` (valor desconhecido → `None` e
uma nota em `trail`). Na escolha da cópia, `manifest::place` passa a:

1. Destino = `into` ou `game-files` (padrão do SDK) dentro de `install`, via `inside()`.
2. Extrair para `<destino>.cerberus-extract` e renomear no fim (nunca deixar meio extraído com o nome certo).
3. Gravar `<destino>/.cerberus-extracted` com `{ copy, size, mtime, kind }` para não extrair de novo e para o capítulo
   5 saber que a pasta vem da cópia.
4. Se `copy` for CHD/CSO/RVZ..., abrir pelo `disc_image::open` (e pelo conversor do capítulo 3) em vez do arquivo cru.

**Borda.** Espaço: tamanho do volume ISO (`iso9660` sabe o tamanho do volume) + 5%; `disk::require` antes. Nomes do
disco fora do padrão (`..`, `:`) já são filtrados nos extratores — conferir com teste. Extração interrompida: pasta
temporária apagada em `sweep_stale_downloads`. Atualização: `preserve` em `update.rs` leva a pasta extraída para a
versão nova (ela não vem na release) — **não verificado** se `preserve` leva pastas grandes por hard link ou cópia;
conferir `update.rs:231`.

**Aceite.** Manifest sintético com `extract: "iso9660"` e uma ISO pequena: escolher a cópia deixa os arquivos em
`game/`; trocar de versão não extrai de novo; apagar `game/` faz Jogar pedir a preparação.

### 2.2 `contains` (o que a pasta precisa ter)

**Desenho.** `Input.contains: Vec<String>`. Duas regras do SDK:
- depois de colocar/extrair, conferir com `holds()` (já existe, insensível a maiúsculas) e falhar com "Faltou X";
- **se a pasta `into` já tem tudo**, o input conta como escolhido mesmo sem `inputs[id]` (o jogador pôs à mão). Isso
  entra em `next_step` (`mod.rs:692`) e em `setup_inputs` (`mod.rs:622`, campo novo `already_there: bool`).

**Aceite.** Pasta `into` com `default.xex` e `Media/`: o cartão mostra "Já está na pasta do jogo" e Jogar abre.

### 2.3 `destination` (cópia gerenciada, reaplicada)

**Desenho.** `Input.destination: Option<String>` validado como o schema (sem `/` inicial, sem `\`, `:`, `{}`, `..`,
`//`, sem terminar em `/`) e por `inside()`; destinos repetidos → o manifest fica com erro de configuração visível.

- Cópia **gerenciada**: registrar em `<install>/.cerberus/managed.json` `{ dest, source, size, mtime, sha256? }`.
- Antes de cada Jogar e depois de setup/atualização, reaplicar se faltar ou se a origem mudou (`prepare_start`).
- **Nunca sobrescrever arquivo não gerenciado** que já esteja lá (o SDK exige): erro claro com o caminho.
- Remover o input opcional → apagar só a cópia gerenciada.
- Hard link no mesmo volume, cópia em outro (`copy_whole`); chave compartilhada **sempre cópia** privada.
- Não seguir junction/symlink que saia da instalação: resolver com `dunce::canonicalize` o diretório pai e conferir que
  continua dentro de `install`.
- Excluir dos backups de saves e da nuvem (`saves.rs`).

**Aceite.** Manifest com `destination: "keys/prod.keys"` e uma chave falsa de formato válido: aparece em
`keys/prod.keys`; apagar à mão e Jogar → volta; um arquivo do jogador no mesmo nome → erro, arquivo intacto.

### 2.4 `check.nup_build_id`

**Estado.** `Check` não tem o campo. O SDK diz: build ID (hex, 8 a 64 dígitos, prefixo vale) lido do `exefs/main`
(NSO) do título mais novo de um pacote `.nup`, ou de uma pasta com `exefs/main`; não precisa de chave; reconhecido pelo
conteúdo, não pela extensão.

**Desenho.** Módulo novo `src-tauri/src/nso.rs`:
- Pasta: abrir `exefs/main`, conferir magia `NSO0`, ler o module ID em `0x40..0x60` (32 bytes) — layout do NSO
  público no switchbrew (`https://switchbrew.org/wiki/NSO`), **não verificado** nesta sessão.
- Arquivo `.nup`: o formato **não tem especificação pública que eu tenha achado**. A única descrição está num comentário
  do RecompLauncher (GPL). Fato técnico pode ser reimplementado, mas é **não verificado**: implementar a pasta primeiro
  e o `.nup` só com um arquivo real de teste fornecido por um desenvolvedor, lendo só a tabela e os primeiros 0x60
  bytes (nunca o pacote todo, limite de 64 KiB lido).
- Comparação: `found.to_lowercase().starts_with(wanted.to_lowercase())`; erro "Essa cópia é o build {found8}, o jogo
  precisa de {wanted}".

**Aceite.** Pasta sintética `exefs/main` com cabeçalho NSO de 0x100 bytes e ID conhecido passa; outro ID é recusado.

### 2.5 Entradas como link em vez de cópia

**Estado.** `manifest::place` → `copy_whole` (arquivo) ou `copy_tree` (pasta). Uma ISO de 8 GB ocupa 16 GB. O
`preserve` de `update.rs:249` já usa `fs::hard_link` com recuo para cópia: o padrão já existe no código.

**Desenho.** Função única `recomp::link_or_copy(from, to) -> Placed { Hardlink | Copy }`:
1. Mesmo volume → `fs::hard_link` (não precisa de administrador nem do Modo Desenvolvedor, ao contrário de symlink).
2. Volume diferente, FAT32/exFAT (sem hard link) ou erro → `copy_whole` com `disk::require`.
3. Pasta: o SDK diz "o conteúdo como links"; fazer hard link arquivo a arquivo (`copy_tree` ganha o mesmo recuo).
   Junction de diretório **não** (o jogo pode apagar dentro e apagar o original do jogador).
- Colocar **antes de cada Jogar** (o SDK diz isso para `into`), só se faltar ou o tamanho/mtime mudou: barato.
- Registro em `managed.json` para o capítulo 5 e para "Reparar instalação".
- Desinstalar apaga o link, nunca a origem: `delete_files` remove o diretório, o que só tira o nome do link (o
  conteúdo continua no arquivo do jogador). Teste obrigatório.

**Borda.** Hard link compartilha o conteúdo: se o **jogo escreve** no arquivo colocado, altera a cópia do jogador.
Para inputs `role: game/disc` (só leitura) é aceitável; para `update`/`dlc` também. Marcar o link como somente
leitura? Não: o atributo é do arquivo, mudaria o original. Documentar e, no Resolver problemas, avisar se o mtime da
cópia do jogador mudou depois de jogar. Antivírus: alguns tratam hard links igual a arquivo normal (sem problema
conhecido, **não verificado**). OneDrive: hard link dentro de pasta sincronizada pode subir o arquivo inteiro; se a
origem ou o destino estiver no OneDrive, copiar e avisar.

**Aceite.** ISO de 1 MB no mesmo volume: depois de escolher, `GetFileInformationByHandle` mostra `nNumberOfLinks = 2`
e o espaço livre não cai; em outro volume (pendrive), copia e diz "copiado (outro disco)".

### 2.6 Teclas do jogo (`settings[].bindings`)

**Estado.** `game_settings.rs` lê `choices` e `slider` (e `"in": "args"`); `bindings` é ignorado (o setting some ou
fica sem controle — **não verificado** qual dos dois; conferir `offer_of`, linha 163).

**Desenho.**
- Backend: `Binding { label, key, pad }`, `Bindings { separator = "|", max = 2, actions, names }`. `GameSetting` em
  `shared` ganha `bindings: Option<BindingsView>` com, por ação, as teclas atuais lidas pelo mesmo `ConfigFile::get`.
- Comando `set_binding(game, setting, action, keys: Vec<String>)`: valida o vocabulário do SDK (letras, dígitos,
  F1–F12, Space, Return, Escape, Tab, Backspace, Shift, Ctrl, Alt, setas, Mouse Left/Right/Middle/4/5, Wheel Up/Down;
  com `pad`, A/B/X/Y, Cross/Circle/Square/Triangle, LB/RB/LT/RT, Start/Back, sticks, D-pad), aplica `names` (e mantém
  o nome que o arquivo já usa para aquela tecla), junta com `separator` e grava **só as ações alteradas**.
- "Restaurar padrão" reaproveita `reset` (`defaults` do setting).
- Depois de fechar o jogo, reler (já é o comportamento do SDK; conferir se o Cerberus relê os ajustes ao fechar).
- UI (`ui/src/game_settings.rs`): uma linha por ação com "chips" das teclas; clicar no chip → "Pressione uma tecla…"
  captura `keydown`/`mousedown`/`wheel` e, com `pad`, o controle pelo XInput que o backend já lê (`in_game.rs` /
  `overlay.rs`, **não verificado** se há um evento reutilizável). Escape cancela; Backspace apaga o chip. Conflito
  (mesma tecla em duas ações) mostra aviso, não bloqueia.

**Aceite.** Exemplo do `guide.md` (`key.cross`, `key.options`) num INI sintético: trocar Cross para `Return` grava
`key.cross=Return` e não toca `key.options` nem os comentários.

### 2.7 DLSS que o launcher baixaria (`nvidia.dlss`) — exige aprovação do jogador

**Estado.** `Input::fetched()` (`manifest.rs:160`) marca o input como "o launcher busca" e ele é **pulado**: os
argumentos `{input:dlss}` saem e o jogo roda sem DLSS. `dlss.rs` só troca DLL que o jogador já tem; o Cerberus não
baixa nada. `SHARED_KEYS` (linha 76) não inclui `nvidia.dlss`, de propósito.

**Desenho (só depois de decisão do mantenedor, ver Riscos).**
- Nunca baixar sozinho. O padrão é **desligado**. Na preparação (passo Extras), linha "NVIDIA DLSS — baixado do
  GitHub da NVIDIA" com a licença da NVIDIA (link) e um botão **Baixar e aceitar a licença** por PC; por jogo,
  um interruptor (o SDK prevê desligar por jogo).
- Só em Windows com placa NVIDIA RTX (DXGI: vendor `0x10DE`; RTX por nome/geração — **não verificado** como detectar
  "RTX" com segurança; na dúvida, mostrar a opção a qualquer NVIDIA com aviso).
- Origem: só `https://raw.githubusercontent.com/NVIDIA/` ou `https://github.com/NVIDIA/` (o SDK fixa esses hosts). O
  SDK usa `recomplauncher.com/launcher/dlss.json` para saber a versão: **não usar o servidor deles sem permissão**;
  em vez disso, fixar no Cerberus a versão e o SHA-256 (atualizado por release do Cerberus) ou ler a release pública
  do repositório `NVIDIA/DLSS` (**não verificado** o caminho exato do DLL no repositório).
- Conferir tamanho, SHA-256 e a assinatura Authenticode "NVIDIA Corporation" (`WinVerifyTrust`) antes de usar.
- Guardar uma vez em `<data>/shared-runtime/nvidia.dlss/<versão>/`; cada jogo recebe cópia gerenciada (`destination`)
  ou o caminho por `{input:dlss}`. Nunca substituir um DLL que a release já traz.
- Falha no download nunca impede de jogar.

**Aceite.** Sem clicar em aceitar, nenhum pedido de rede para a NVIDIA (provar com log de `net.rs`); depois de aceitar,
o DLL aparece com assinatura válida, e desligar por jogo tira o argumento.

### Testes do capítulo 2

- `manifest.rs`: parse de cada campo novo com os exemplos do SDK em `src-tauri/tests/sdk/` (adicionar um JSON com
  `extract`, `contains`, `destination`, `nup_build_id`, `bindings` e `nvidia.dlss`, adaptado do SDK com crédito).
- `destination` inválidos (tabela: `/a`, `a/../b`, `a\b`, `C:x`, `{install}/a`, `a/`, `a//b`) → erro.
- `extract` com ISO 9660 sintética de poucos KB (gerada no teste: PVD + diretório raiz com 2 arquivos) e XISO mínima.
- `link_or_copy`: mesmo volume → `nNumberOfLinks == 2`; destino em volume inexistente forçado → cópia.
- `nso.rs`: cabeçalho NSO sintético.
- `bindings`: INI/TOML/JSON com `names`, `max`, `separator`, ação inalterada não escrita.
- DLSS: teste de que `fetched()` continua pulando quando não há aprovação gravada.

Esforço: `extract`/`contains`/links **P**; `destination` **P/M**; `nup_build_id` **P** (pasta) / **M** (`.nup`);
`bindings` **M**; DLSS **M** (mais a decisão).

---

## 3. Converter RVZ, WIA, WBFS, GCZ, CISO e NKit para ISO sem programa de fora

### Estado atual

- `disc_image.rs` abre **CHD** (crate `chd` 0.4) e **CSO** como disco plano (`Kind::{Plain, Chd, Cso}`, detecção por
  magia) e `write_iso` gera ISO; `put_in_folder` (`recomp/mod.rs:924`) converte quando o catálogo pede `.iso` e recebe
  CHD/CSO.
- `roms.rs:103` já lista `rvz`, `wbfs`, `wia` como extensões de disco e `disc_console.rs:159` sabe que `.wbfs` é Wii,
  mas **nenhum** desses formatos é lido: não dá para ler o ID (`check.ids` de GameCube/Wii), nem extrair, nem gerar
  ISO. GCZ, CISO de GameCube (diferente do CSO de PSP) e NKit não são reconhecidos.
- Dependências já presentes que ajudam: `crc32fast`, `md-5`, `sha2`; falta SHA-1 (crate `sha1`, MIT/Apache).

### Formatos (resumo das estruturas)

Fonte primária: `docs/WiaAndRvz.md` do Dolphin e os headers de `Source/Core/DiscIO` (`CompressedBlob.h`,
`CISOBlob.h`, `WbfsBlob.cpp`). O repositório do Dolphin é GPL-2.0+; a especificação descreve um formato (fato técnico)
e pode ser seguida, mas **o código do Dolphin não pode ser copiado**. O documento não declara licença própria
(**não verificado** o que isso implica; ler a spec é seguro, copiar trechos de código dela não).

| Formato | Estrutura | Dificuldade |
| --- | --- | --- |
| **CISO** (GC/Wii) | Header de 0x8000: `CISO`, `block_size` u32 LE, mapa de 0x7FF8 bytes (1 = bloco presente). Blocos presentes em sequência depois do header; ausentes = zeros. | P |
| **WBFS** | `WBFS`, `hd_sector_count`, `hd_sector_shift`, `wbfs_sector_shift`, `disc_table[500]`; no setor seguinte, 0x100 do header do disco e a tabela `wlba` (u16 BE por setor WBFS). Endereço = `wlba[off >> shift] * wbfs_sector_size + (off & mask)`; `wlba == 0` = bloco ausente (inferido, **não verificado**). | P |
| **GCZ** | Header de 32 bytes (magia 0xB10BC0DE — o comentário do Dolphin diz 0xB10BB10B, **conferir com arquivo real**), `sub_type`, `compressed_data_size` u64, `disc_size` u64, `block_size` u32, `num_blocks` u32; tabela de offsets u64 (bit alto = bloco cru), tabela de hash u32 (provavelmente Adler-32, **não verificado**); blocos zlib/deflate. | P |
| **WIA** | Header 1 (0x48: `WIA\x01`, versões, `disc_size`, SHA-1s), header 2 (`disc_type`, `compression`, `chunk_size`, 0x80 primeiros bytes do disco, tabelas de partição/raw/grupos). Partições Wii guardadas **descriptografadas e sem hash** (0x7C00 de cada 0x8000) com exceções de hash por bloco de 2 MiB. Compressão: 0 none, 1 purge, 2 bzip2, 3 LZMA, 4 LZMA2. | G |
| **RVZ** | WIA + zstd (5), sem purge, chunk ≥ 32 KiB potência de 2, bit alto do `data_size` = grupo comprimido, `rvz_packed_size`; *packing*: trechos com tamanho u32, bit alto = dados "lixo" gerados por Lagged Fibonacci (seed de 68 bytes, j=32, k=521). | G |
| **NKit** | ISO/GCZ "encolhida": tira o lixo e, no Wii, a partição de update. GameCube: reconstrução sem dados extras (**não verificado** em fonte primária). Wii: devolver a partição de update exige os arquivos de recuperação do NKit (fontes secundárias). NKit é MIT. | M (GC) / fora (Wii 1:1) |

Para gerar a ISO Wii **original** a partir de WIA/RVZ é preciso reencriptar as partições com a chave comum do Wii e
recalcular as árvores de hash H0–H3. Isso significa o Cerberus carregar a chave comum do Wii — exatamente o tipo de
chave que o projeto não distribui. Consequência importante para o desenho (abaixo).

### Avaliação de crates

| Crate | Licença | Situação | Formatos | Observações |
| --- | --- | --- | --- | --- |
| **`nod`** (encounter/nod) | MIT OR Apache-2.0 | 1.4.4 (out/2024) estável; 2.0.0-alpha.9 (mai/2026) | ISO/GCM, WIA/RVZ, WBFS, CISO, GCZ, TGC, NFS (Wii U VC, só leitura), NKit 2 *lossless* em WBFS/CISO | Lê e escreve ISO/RVZ; `Compression` = None, Bzip2, Deflate (GCZ), Lzma, Lzma2, Zstandard (**sem purge** — ler WIA com purge é **não verificado**). Calcula CRC32, MD5, SHA-1 e XXH64 ao processar. `PartitionEncryption::Original` reconstrói criptografia e hashes do Wii (logo, embute a chave comum: **não verificado** como ela entra, conferir o código). Codecs em C (`zstd-sys`, `liblzma-sys`, `bzip2-sys`, `libz-sys`, compilados com `cc`, *vendored* por padrão; não é programa de fora, mas aumenta o build). `default-features = false` deixa escolher codecs. `nodtool` (mesmo workspace e licença) tem `info/extract/convert/verify` e embute DATs do Redump. |
| `disc-riider` 0.2.2 | MIT | jan/2025 | ISO de Wii (montar/ler) | Para randomizers; não lê formatos comprimidos. Não serve. |
| `rvz-rs` 0.1.0 | GPL-2.0+ | nov/2026 | RVZ | Licença incompatível. Não usar. |
| `wbfs-rs` 0.1.0 | **não verificada** | nov/2026 | WBFS | Muito novo; não usar sem conferir. |
| `romux`, `rom-weaver-containers` | **não avaliadas** | nov/2026 | — | Novas demais. |
| Codecs puros em Rust | `ruzstd` (MIT), `lzma-rs` (MIT), `bzip2-rs`/`libbz2-rs-sys` (**não verificado**), `miniz_oxide`/`flate2` (já no projeto) | — | — | Só se o leitor for próprio. `sevenz-rust2` (já dependência) traz LZMA/LZMA2 em Rust puro via `lzma-rust2` (**não verificado** se é exposto para uso direto). |

**Recomendação.** Usar `nod` **1.4.4** (estável), com `default-features = false` e as features *vendored* de zstd,
lzma e bzip2 (zlib para GCZ), atrás de uma feature `gc-wii-images` do Cerberus para medir o custo no build e no
tamanho do executável. Isolar num módulo `src-tauri/src/nintendo_disc.rs` com uma interface pequena para poder trocar
por um leitor próprio depois:

```rust
pub enum NinFormat { Iso, Ciso, Wbfs, Gcz, Wia, Rvz, Nkit, Tgc }
pub struct NinInfo { pub format: NinFormat, pub id6: String, pub title: String,
                     pub console: &'static str /* gamecube | wii */, pub disc_size: u64, pub nkit: bool }
pub fn probe(path: &Path) -> Option<NinInfo>;                // só o header: barato
pub fn to_iso(src: &Path, dst: &Path, halt: &Halt, progress: impl Fn(u64, u64))
    -> Result<Hashes, String>;                                 // dst.part + rename
pub struct Hashes { pub crc32: u32, pub md5: [u8; 16], pub sha1: [u8; 20] }
```

Se a 2.0 sair estável durante o trabalho, reavaliar (multithread no `preloader_threads`). Plano B, sem a chave do Wii
e sem C: leitor próprio de CISO, WBFS e GCZ (P cada; são mapas de blocos) e de WIA/RVZ **só GameCube** e só leitura
de arquivos (sem gerar ISO Wii original).

### Desenho no Cerberus

1. `probe` entra em `disc_console::console_of` e `disc_ids`: o ID6 do header (0x00..0x06 do disco, que WIA/RVZ guardam
   nos 0x80 bytes do header 2) faz `check.ids` funcionar **sem converter**. Esse é o primeiro ganho e é P.
2. Conversão sob demanda, nunca automática: quando o input pede `iso` (extensões do manifest ou `put_in_folder` com
   nome `.iso`) e a cópia é RVZ/WIA/WBFS/GCZ/CISO/NKit GC, a tela de escolha oferece **Converter para ISO** com o
   tamanho final e o destino (padrão: ao lado da instalação, em `<install>/game-files/<nome>.iso`, ou uma pasta que o
   jogador escolhe). A cópia do jogador nunca é alterada nem apagada.
3. Roda na fila única de downloads (5.3) como tarefa "Convertendo", com progresso, pausa/cancelar (`install::Halt`) e
   `disk::require(dst, disc_size)` antes.
4. Ao terminar: grava os hashes num `<dst>.cerberus.json` (`{ src, src_size, src_mtime, crc32, md5, sha1 }`) para não
   converter de novo e para o selo verificado.
5. Verificação: se o input tem `dumps` (CRC32 do Redump fornecido pelo desenvolvedor no manifest), compara o CRC32
   calculado **durante** a conversão (sem segunda leitura) e dá o selo "Bate com o disco verificado" (6.7). Sem
   `dumps`, mostrar SHA-1 para o jogador comparar se quiser.
6. **Wii**: gerar a ISO original exige a chave comum. Opções, em ordem de preferência:
   a. só oferecer Wii quando o recomp aceita a imagem descriptografada (`ForceDecrypted`, **não verificado** se algum
      recomp aceita) — nada de chave;
   b. usar a chave **fornecida pelo jogador**, guardada como chave compartilhada nova (`wii.common-key`, que o SDK não
      lista hoje — teria de ir ao SDK primeiro);
   c. não suportar Wii no primeiro corte.
   Nunca embutir a chave no Cerberus. Confirmar como o `nod` obtém a chave antes de adotar (se ela vier embutida na
   crate, compilar com a opção que a desliga ou desistir de Wii pelo `nod`).
7. NKit: GameCube pelo `nod` (NKit 2 *lossless* em CISO/WBFS); NKit Wii sem os dados de recuperação gera um disco que
   **não** bate com o Redump — mostrar "convertido (não idêntico ao original)" e não dar o selo.

### Desempenho

Sem números publicados em MB/s para `nod`, `nodtool` ou `DolphinTool` (**não verificado**). Estimativa a medir: RVZ
zstd é limitado pelo disco e pelo zstd (centenas de MB/s por núcleo); LZMA bem mais lento (dezenas de MB/s). Medir em
release com uma ISO GameCube (1,4 GB) e uma Wii de camada única (4,7 GB) num SSD e num HD, alvo: GameCube em menos de
1 minuto num SSD. Calcular CRC32+SHA-1 durante a escrita; MD5 só se o `dumps` pedir (o próprio `nod` avisa que é lento).

### Casos de borda

- Espaço: a ISO tem o tamanho do disco (1,4 GB GC; 4,7/8,5 GB Wii), mesmo que o RVZ tenha 600 MB.
- Interrupção: `.part` + rename; o `.part` órfão entra em `sweep_stale_downloads`.
- Antivírus lendo o `.iso` novo no rename: `throttle::rename_patiently`.
- Arquivo RVZ truncado (download quebrado): erro "o arquivo está incompleto" com o offset, não pânico — `nod` deve
  devolver `Err`; envolver em `catch_unwind` por segurança na primeira versão.
- Pasta no OneDrive: avisar que a ISO vai sincronizar gigabytes.
- WBFS dividido (`.wbf1`, `.wbf2`): **não verificado** se o `nod` lê; testar.

### Critério de aceite

1. Um RVZ de GameCube feito pelo jogador com o DolphinTool a partir de uma ISO própria: o Cerberus lê o ID6, aceita
   no `check.ids` e converte para uma ISO cujo SHA-1 é igual ao da ISO original.
2. O mesmo com WIA (LZMA2), GCZ, CISO e WBFS.
3. Cancelar no meio não deixa `.iso`; espaço insuficiente é recusado antes de começar.
4. Com `dumps` no manifest, o selo verificado aparece sem segunda leitura do arquivo.

### Testes

- Arquivos sintéticos pequenos: uma "ISO GameCube" falsa de 2–4 MiB (header de disco com ID6 `GTEST1`, magia
  `0xC2339F3D` em 0x1C, resto em padrão conhecido). Gerar CISO, WBFS e GCZ **no próprio teste** (são triviais) e
  conferir que `to_iso` devolve os bytes originais.
- WIA/RVZ: gerar com o próprio `nod` no teste (escrita RVZ suportada) a partir da ISO sintética e voltar — ida e volta
  com o mesmo SHA-1, para cada compressão (none, bzip2, lzma, lzma2, zstd).
- Truncado: cortar o arquivo ao meio → `Err`, sem pânico.
- Teste real (manual, `#[ignore]`): um jogo do próprio mantenedor, comparado com o hash publicado no redump.org.

Esforço: **M** com `nod` (sem Wii original), **G** com leitor próprio ou com Wii.

---

## 4. Janela "Resolver problemas"

### Estado atual

- `crash.rs`: `reason(code, seconds)` explica códigos do Windows; `old_dll_clues` / `old_dll_cause` comparam DLLs do
  Visual C++ trazidas pelo jogo com as do Windows e a versão do Vulkan; `newest_log` acha o log escrito na sessão.
- `sessions.rs::report_crash` usa `exit_codes` do manifest, depois `crash::reason`, procura o log em `folders.logs` e
  na pasta do jogo e grava no `trail` + notificação.
- `sessions.rs::blocked_by_windows` explica Smart App Control, antivírus, política e elevação.
- `runtimes.rs`: versão do VC++ e DirectX faltando, com instaladores da Microsoft.
- **Não existe**: saída do jogo (stdout/stderr) guardada — o jogo herda as alças do Cerberus, que numa build GUI não
  vão a lugar nenhum (`Command::new(exe)` em `sessions.rs:186` sem `.stdout`); remoção do Zone.Identifier;
  preferência de GPU; aviso de caminho com acento; leitura da tabela de import; zip de logs. `data_backup.rs` já usa
  `zip::ZipWriter` (serve de modelo).

### Desenho

Módulo novo `src-tauri/src/troubleshoot.rs` com uma lista de verificações puras (fáceis de testar) e comandos de
correção separados:

```rust
pub enum Level { Ok, Info, Warn, Bad }
pub struct Check { pub id: &'static str, pub level: Level, pub title: String,
                   pub detail: String, pub fix: Option<Fix> }
pub enum Fix { Unblock, FastGpu, SetAsideOldDlls, InstallVc, InstallDx, MoveInstall,
               PickProgram, Repair, RunSetup, OpenFolder(PathBuf) }
#[tauri::command] fn troubleshoot(id: String, code: Option<i32>) -> Vec<Check>;
#[tauri::command] fn troubleshoot_fix(id: String, fix: Fix) -> Result<String, String>;
#[tauri::command] fn export_logs(id: Option<String>) -> Result<PathBuf, String>;
```

Verificações, em ordem (a primeira `Bad` que impede o resto encerra a lista):

1. **Pasta do jogo existe** (disco desconectado?) → `MoveInstall`/reinstalar.
2. **Programa existe** e é PE do Windows (magia `MZ`/`PE`), não é ferramenta/instalador → `PickProgram`.
3. **Arquitetura**: PE x64 vs ARM64 vs x86 do PC.
4. **DLL faltando pela tabela de import**: ler o PE (crate `object` 0.36, MIT/Apache, só leitura; ou `pelite`, MIT —
   preferir `object`, já é dependência indireta do Rust toolchain, **não verificado** se já está no `Cargo.lock`).
   Para cada DLL importada (e as de delay-load): procurar na pasta do programa, na pasta do jogo, em `System32` (ou
   `SysWOW64` para x86) e no `PATH`; ignorar API sets (`api-ms-win-*`, `ext-ms-*`). Só o primeiro nível, não
   recursivo (limitar a 64 DLLs). Classificar: VC++ (`msvcp*`, `vcruntime*`, `concrt*`, `vcomp*`) → `InstallVc`;
   D3DX/XInput 1.3/XAudio 2.7 → `InstallDx`; arquivo que o `prepare` gera (está em `prepare.done` ou o jogo tem
   `main_input`) → `RunSetup`; resto → `Repair`.
5. **DLL antiga trazida pelo jogo**: o que `crash::old_dll_clues` já faz, agora com botão `SetAsideOldDlls` que
   renomeia para `.old` (reversível, anotado no `trail`; "Reparar instalação" desfaz).
6. **Marca "baixado da internet"**: existe o fluxo alternativo `<arquivo>:Zone.Identifier` no programa ou nas DLLs →
   `Unblock` apaga esse fluxo (`fs::remove_file("x.exe:Zone.Identifier")`) em todos os `.exe/.dll` da pasta do jogo
   (limite de 20 000 arquivos, nunca fora da pasta). O Cerberus extrai os zips ele mesmo, então a marca só aparece se
   o jogador trouxe a pasta (adopt) ou o antivírus marcou — **não verificado** se o `zip` crate propaga a marca
   (não deve: ela é do Windows Explorer).
7. **Placa de vídeo**: com 2+ adaptadores (DXGI `EnumAdapters1`, ou WMI), oferecer `FastGpu`: gravar em
   `HKCU\Software\Microsoft\DirectX\UserGpuPreferences` o valor de nome = caminho completo do exe e dado
   `GpuPreference=2;` (documentado pela Microsoft como a mesma chave da tela Configurações › Vídeo › Gráficos —
   **não verificado** link oficial nesta sessão). Usar `winreg` (já dependência), não `reg.exe`. Mostrar "já está na
   mais rápida" quando o valor existe. Desfazer = apagar o valor.
8. **Caminho com acento ou fora do ASCII** (pasta do jogo, cópia escolhida, pasta do usuário/AppData onde ficam
   saves): `Warn` com "Mover para C:\Jogos\Recomps" → `MoveInstall` (`relocate.rs` já move). Também nome muito longo
   (> 240 caracteres) e pasta do usuário com acento (não tem correção, só explicação).
9. **OneDrive**: pasta do jogo dentro de `%OneDrive%`, `%OneDriveConsumer%`, `%OneDriveCommercial%` ou com atributo
   `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`/`OFFLINE` no exe (Arquivos sob demanda) → `Warn` + `MoveInstall`.
   `update.rs:220` já fala disso: reaproveitar a detecção.
10. **Requisitos** do manifest: `system_problem` (Windows), DX12/Vulkan (**não verificado** como checar Vulkan sem
    carregar o driver: `vulkan-1.dll` presente + versão do arquivo, como `old_dll_clues` já faz).
11. **Cópia do jogo**: `copy_problem` (existe) e, se mudou desde a preparação, `RunSetup`.
12. **Preparação pendente**: `needs_preparation`.
13. **Última saída do jogo**: as últimas 40 linhas do log guardado (abaixo) e o código de saída explicado.
14. **Antivírus / Smart App Control**: se o último erro de abertura foi 225/226/4551..., repetir a explicação.

**Guardar a saída do jogo num log.** Em `sessions::start`, abrir `<data>/game-output/<id-seguro>.log` (truncado a cada
partida; manter a anterior como `.prev.log`) e passar `Stdio::from(file)` para stdout e stderr. Exceções:
- recomp que pede console (manifest futuro ou `tracking_mode`): não redirecionar;
- limite de 8 MiB: o Windows não corta sozinho; usar um pipe e uma thread que grava até o limite e depois descarta
  (o pipe precisa ser drenado ou o jogo trava ao encher o buffer de 4 KiB). O pipe com thread é o desenho certo; o
  arquivo direto é mais simples mas pode crescer sem limite — começar pelo pipe.
- Não colocar o log dentro da pasta do jogo (iria para backups e nuvem).
- A `report_crash` passa a anexar as últimas linhas do log ao `trail`.

**Exportar logs num zip.** `export_logs(id)` monta em `Downloads\Cerberus-logs-<jogo>-<data>.zip`: saída do jogo
(atual e anterior), `trail` do jogo, `newest_log` dos `folders.logs`, ReShade.log se houver filtro, `recomplauncher.json`
efetivo, `library.json` **só a entrada do jogo**, versão do Cerberus/Windows/GPU/driver e o resultado do
`troubleshoot`. Privacidade: trocar `C:\Users\<nome>` por `%USERPROFILE%`, nunca incluir `shared-keys`, tokens de
contas (`epic_account`, `xbox_account`, RetroAchievements), `settings.json` inteiro nem saves. Limite de 2 MiB por
arquivo (pegar o final). Mostrar a lista do que vai no zip antes de gerar e abrir a pasta depois.

### Casos de borda

- Jogo que abre outro processo (launcher → runtime): o log captura só o primeiro; o filho herda as alças se o jogo não
  as trocar (comum). Documentar.
- Pipe cheio com o Cerberus fechado no meio da partida: o jogo continua (o job object não mata), mas o pipe quebra e
  escritas no stdout falham com `EPIPE`/`ERROR_NO_DATA`; alguns jogos fecham com isso. **Mitigação**: arquivo direto
  em vez de pipe quando o Cerberus pode fechar (configuração "Fechar o Cerberus ao jogar")? Testar com um recomp real
  antes de decidir; marcar como risco.
- Zone.Identifier em pasta com milhares de arquivos: progresso e cancelamento.
- Registro do GPU com caminho que muda depois de mover a instalação: reaplicar após `relocate`.
- Antivírus bloqueando a leitura do PE (arquivo em quarentena): tratar `PermissionDenied` como "o antivírus pode ter
  tirado o arquivo".
- Recomp não-Windows (Linux/Proton futuro): só as verificações 1, 2, 8, 11–13.

### UI

- Botão **Resolver problemas** na página do jogo (Gerenciar) e no aviso "fechou com erro" (`notify` + `trail`).
- Janela/modal com a lista: ícone por nível, título curto, detalhe, botão de correção à direita ("Tirar a marca",
  "Usar a placa mais rápida", "Instalar Visual C++", "Mover a pasta"). Depois de corrigir, a verificação roda de novo.
- Rodapé: "Ver a saída do jogo" (abre o log), "Exportar logs (.zip)", "Abrir a pasta do jogo".
- Modo TV (`ui/src/tv`): mesma lista navegável pelo controle.

### Critério de aceite

1. Um recomp real com `msvcp140.dll` renomeado no System32 de uma VM (ou um exe sintético que importa
   `naoexiste.dll`) mostra "Falta naoexiste.dll".
2. Copiar `x.exe` baixado pelo navegador para a pasta do jogo → aparece a marca; "Tirar a marca" remove e a checagem
   seguinte mostra ok (conferir com `Get-Item -Stream *`).
3. Num notebook com duas placas, "Usar a placa mais rápida" grava a chave e a tela de Gráficos do Windows mostra
   "Alto desempenho" para o exe.
4. Instalação em `C:\Usuários\João\Jogos` mostra o aviso de acento.
5. A saída de um jogo que escreve no stdout (exe de teste) aparece no log; o zip exportado não contém o nome do
   usuário nem chaves.

### Testes

- Funções puras: `classify_missing(dlls) -> Fix`, `non_ascii(path)`, `inside_onedrive(path, env)`,
  `redact(text, home)`.
- PE sintético: gerar em teste um PE mínimo com tabela de import (ou usar um exe pequeno compilado em
  `tests/fixtures/`, com import de `naoexiste.dll`; o `cmd.exe` do sistema serve para o caso "nada falta").
- Zone.Identifier: escrever o fluxo `arquivo:Zone.Identifier` com `fs::write` num `tempdir` NTFS e remover.
- Registro do GPU: chave de teste em `HKCU\Software\Cerberus-test\...` com o mesmo código parametrizado pela raiz.
- Log da partida: `cmd /c echo oi` (padrão de `sessions.rs` testes) e limite de bytes com um programa que escreve 20
  MiB.
- Zip: conferir a lista de nomes e que nenhuma entrada contém `shared-keys` nem o nome do usuário.

Esforço: **M** (o log com pipe e o zip são a maior parte; as verificações são P cada).

---

## 5. Liberar espaço depois de um build com `keeps_copy`

### Estado atual

- `Prepare.keeps_copy` é lido (`manifest.rs:186`) e usado só em `preparation` (`mod.rs:471`): sem a cópia, o passo só
  roda se `keeps_copy` for verdade.
- O marcador `.cerberus-prepared` guarda `"<versão>\n<cópia>\n<opções>"`; `manifest::done(prepare, install)` confere os
  arquivos de `prepare.done`.
- Não há "Liberar espaço". A cópia do jogador fica intacta onde ele a tem; o que ocupa espaço extra hoje é a **cópia
  colocada** por `place` (capítulo 2.5) e a pasta extraída (2.1).

### Desenho

Duas coisas diferentes, que a UI precisa separar:

| O que | Pode apagar quando | Como |
| --- | --- | --- |
| Arquivos que o Cerberus colocou na pasta do jogo (`into`, `destination`, pasta extraída) | build feito na versão instalada, `prepare.done` presente, o `start` não lê o input (`{input:id}` não aparece em `start.args/env` nem em `games[].args`) e `keeps_copy` (ou extração do próprio Cerberus, que refaz da cópia) | apagar direto (são do Cerberus) |
| A cópia original do jogador (ISO/RVZ em outra pasta) | as mesmas condições **e** `keeps_copy` **e** nenhum outro jogo da biblioteca usa o mesmo arquivo | **Lixeira** do Windows (`IFileOperation` com `FOFX_RECYCLEONDELETE`, crate `windows` já tem `Win32_UI_Shell`), nunca `remove_file`; com confirmação |

```rust
pub struct SpareFiles { pub files: Vec<PathBuf>, pub bytes: u64, pub why: Spare }
pub enum Spare { PlacedCopies, PlayersCopy }
#[tauri::command] fn spare_files(id) -> Option<SpareFiles>;
#[tauri::command] fn free_up_space(id, include_players_copy: bool) -> Result<u64, String>;
```

Condição central, numa função pura testável:
`can_free(manifest, game, built_version, others) = keeps_copy && built_on_installed_version && done_present &&
!start_reads_input && !used_by_other_game`.

Depois de liberar: marcar no `.cerberus-prepared` que a cópia foi liberada (`copy_freed = true`) para que
`preparation` aceite rodar sem cópia (já aceita com `keeps_copy`) e `copy_problem`/`next_step` **não** peçam a cópia de
novo. `still_fits`/`check` não podem tratar a cópia ausente como erro nesse estado.

### Casos de borda

- **Nova versão** do recomp: a preparação roda de novo; com `keeps_copy` roda do que ficou. Se falhar sem a cópia,
  a mensagem deve dizer "escolha sua cópia de novo" (não "arquivo corrompido").
- **Versão anterior guardada** (capítulo 1): ela também tem um build; voltar para ela não precisa preparar. OK.
- **Atualização interrompida** depois de liberar: `previous` tem o build; o rollback devolve tudo. OK.
- O desenvolvedor mente sobre `keeps_copy`: o próximo build falha. Por isso a cópia do jogador vai para a **Lixeira**,
  não some, e a primeira versão do recurso só oferece as cópias colocadas; a cópia original fica atrás de uma caixa
  "Também mandar minha cópia para a Lixeira".
- Cópia num drive de rede/pendrive: a Lixeira não existe lá → oferecer só "Abrir a pasta" (o jogador apaga à mão).
- Hard link (2.5): apagar o link não libera nada se a origem continuar; calcular `bytes` só com arquivos cujo
  `nNumberOfLinks == 1`, ou dizer "liberado 0 B (era um link)". Importante para não prometer espaço falso.
- Jogo aberto ou preparação rodando → recusar.
- Vários jogos com a mesma cópia (`part_of`, ou dois recomps do mesmo disco) → nunca a cópia do jogador.

### UI

- Cartão em Gerenciar: "Liberar 7,9 GB — o jogo já foi preparado e não precisa mais desses arquivos." Lista o que sai.
- Caixa opcional para a cópia do jogador, com o texto: "Vai para a Lixeira. Se uma versão nova do jogo pedir, você
  escolhe a cópia de novo."
- Depois: toast "7,9 GB liberados" e o cartão some.
- Na página de Configurações › Instalação, um total "Pode liberar X GB em N jogos" (opcional, segunda etapa).

### Critério de aceite

1. Manifest sintético com `prepare.keeps_copy = true`, `done = ["{install}/Game/game.pak"]`, `into: "game"`: depois da
   preparação, o cartão aparece; liberar apaga `game/` e Jogar abre sem pedir a cópia.
2. Sem `keeps_copy`, o cartão não aparece.
3. Com `start.args` contendo `{input:game}`, o cartão não aparece.
4. Mudar a versão (capítulo 1) depois de liberar roda a preparação sem a cópia.
5. Hard link: o número mostrado é 0 B ou o cartão some.

### Testes

- `can_free` com tabela de casos (cada condição desligada uma vez).
- `free_up_space` em `tempdir` com arquivos colocados e o marcador; conferir que a cópia do jogador fica quando a
  caixa não está marcada.
- Lixeira: teste só em Windows, marcado `#[ignore]` por padrão (mexe na Lixeira do usuário), rodado à mão.

Esforço: **P** (só cópias colocadas) / **M** (com a cópia do jogador na Lixeira).

---

## Riscos e licenças

| Item | Licença / risco | Decisão |
| --- | --- | --- |
| RecompLauncher-SDK (docs, schema, exemplos) | MIT | Pode adaptar com crédito no topo do arquivo e em `THIRD_PARTY`. Os exemplos já estão em `src-tauri/tests/sdk/`. |
| RecompLauncher (`lib.rs` versions/troubleshoot, `convert.rs`, `logs_zip.rs`, `nup.rs`, `dllcheck.rs`, `win.rs`) | GPL | Só ideias e comportamento observado. Nenhum trecho copiado nem "traduzido linha a linha". Quem implementa não deve ter o arquivo aberto ao escrever. |
| Formato `.nup` | Sem especificação pública achada | Implementar só com arquivo real de teste; começar pela pasta `exefs/main`. |
| `nod` 1.4.4 | MIT OR Apache-2.0; codecs C *vendored* (zstd, liblzma, bzip2, zlib) | Pode usar; conferir as licenças dos `-sys` (BSD/0BSD/bzip2/zlib, **não verificado**) com `cargo deny`; conferir se embute a chave comum do Wii. |
| `rvz-rs` | GPL-2.0+ | Não usar. |
| Especificação WIA/RVZ do Dolphin | Repositório GPL-2.0+, documento sem licença própria | Seguir a especificação; não copiar código do Dolphin. |
| Chave comum do Wii | Não pode ser distribuída pelo Cerberus | Sem Wii "original" no primeiro corte, ou só com chave do jogador. |
| DLSS | Licença proprietária da NVIDIA; o SDK sugere o servidor do RecompLauncher para a versão | Só com aprovação explícita do jogador e decisão do mantenedor; nunca usar `recomplauncher.com` sem permissão; hosts fixos da NVIDIA e assinatura conferida. |
| DATs do Redump | Sem termos de redistribuição publicados achados; parte pode ser só para membros (**não verificado**). O `nodtool` embute DATs de GC/Wii, mas isso é precedente, não permissão. | Não embutir sem autorização escrita do Redump; usar os `dumps` (CRC32) que o desenvolvedor põe no manifest. |
| Hard links | Escrita do jogo altera a cópia do jogador | Só para inputs de leitura; aviso no Resolver problemas. |
| Log por pipe | Cerberus fechado no meio quebra o pipe | Testar com recomps reais; recuo para arquivo direto. |
| Registro (GPU) | Mexe em HKCU | Só com clique do jogador, reversível, anotado no `trail`. |
| Lixeira | Apagar cópia do jogador | Sempre Lixeira, sempre confirmação, nunca em drive sem Lixeira. |

## Ordem sugerida

1. **2.1–2.3 e 2.5** (`extract`, `contains`, `destination`, links): P, destrava jogos que já publicam o manifest e
   corta o espaço em dobro; base para o capítulo 5.
2. **4. Resolver problemas** começando pelo log da saída do jogo e pelo zip (é o que mais ajuda a diagnosticar o
   resto), depois DLL faltando, Zone.Identifier, GPU e caminhos.
3. **1. Versões**: lista de releases + instalar tag + fixar; depois a versão anterior guardada.
4. **5. Liberar espaço** (só cópias colocadas; depois a cópia do jogador na Lixeira).
5. **3. Conversor** (GameCube/Wii), atrás de `nod` se a avaliação confirmar.
6. **2.4 `nup_build_id`** (pasta), **2.6 teclas**.
7. **2.7 DLSS**, só depois da decisão escrita do mantenedor.

## O que NÃO fazer

- Não baixar jogos, BIOS, firmware, chaves, nem DLSS sem o clique do jogador; não aceitar "chave compartilhada" além de
  `switch.prod-keys`, `switch.title-keys` e `wiiu.common-key`.
- Não copiar código do RecompLauncher (GPL), nem "portar" funções dele.
- Não usar o servidor do RecompLauncher (`recomplauncher.com`) para versões, DLSS ou relatos sem permissão.
- Não apagar a cópia do jogador com `remove_file`; não liberar espaço que é só um link.
- Não deixar arquivo pela metade com o nome final (sempre `.part`/pasta temporária + rename).
- Não usar junction/symlink de diretório para colocar inputs.
- Não incluir chaves, tokens, saves ou o nome do usuário no zip de logs.
- Não converter NKit Wii para ISO "1:1" sem os dados de recuperação e chamar de verificado.
- Não decriptar nem reencriptar partições Wii com chaves embutidas no Cerberus (ver capítulo 3).
- Não rodar a lista de releases a cada visita (limite do GitHub): usar o cache de 6 h.
