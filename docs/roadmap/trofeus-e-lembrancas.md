# Troféus do RPCS3 e do shadPS4, e lembranças (foto no momento da conquista)

Plano de implementação dos itens do `ROADMAP.md`: Mapa rápido 8; 5.4 ("Troféus do RPCS3 (PS3) no hub de
conquistas"); 6.1 ("Foto no momento da conquista"); 6.5 ("troféus do RPCS3 e do shadPS4"). Levantamento de
25/11/2026. Lemos o código do Cerberus, o do Hydra (MIT, pode virar código com crédito) e o código-fonte do RPCS3 e do
shadPS4 no GitHub (GPL: lido só para entender o **formato dos arquivos**, nada copiado).

Legenda: **[conferido]** = visto no código-fonte citado; **[Hydra]** = como o Hydra faz (MIT); **não verificado** =
de memória ou inferido, conferir antes de depender disso.

---

## 1. Objetivo

1. **Troféus de PS3 (RPCS3) e de PS4 (shadPS4) no hub de conquistas**, como uma fonte a mais ao lado de Steam,
   RetroAchievements, Xbox e Epic: cartão por plataforma, lista por jogo com platina/ouro/prata/bronze, ícones
   originais, datas, "Platinados", "Perto dos 100%", linha do tempo. Tudo lido **só do disco** (o emulador já grava),
   sem conta nem servidor.
2. **Aviso ao vivo** quando um troféu cai durante uma partida aberta pelo Cerberus (o mesmo aviso de `unlock_toast.rs`),
   com o aviso do próprio RPCS3 desligado só durante a sessão (como já fazemos com RetroArch/DuckStation/PCSX2).
3. **Lembranças**: quando qualquer conquista detectada ao vivo cai (Steam, RetroAchievements pelos logs dos emuladores,
   RPCS3, shadPS4), tirar uma foto da janela do jogo, guardar ao lado da conquista e mostrar na página da conquista,
   numa aba "Lembranças" do hub e no Big Picture. Só neste PC, nunca enviada.

Fora do escopo: troféus do PSN real (precisa de conta/servidor da Sony), sincronizar com RPCN/ShadNet, editar troféus.

---

## 2. Formatos

### 2.1 RPCS3: onde ficam

| O quê | Caminho | Fonte |
|---|---|---|
| Pasta de config | ao lado de `rpcs3.exe` (builds novas também em `config\`, com `config.yml`, `vfs.yml`, `GuiConfigs\`, `games.yml`) | [Hydra] `rpcs3ConfigRoots`; `config\` não verificado em todas as versões |
| `dev_hdd0` | `vfs.yml` chave `/dev_hdd0/`, padrão `$(EmulatorDir)dev_hdd0/`; `$(EmulatorDir)` pode estar redefinido no próprio `vfs.yml` | [Hydra] `resolveRpcs3VfsHdd0` |
| Usuários | `dev_hdd0\home\<8 dígitos>\` (padrão `00000001`) | [conferido] regex `^\d{8}$` no Hydra; o RPCS3 usa `Emu.GetUsr()` |
| Troféus de um jogo | `dev_hdd0\home\<usuário>\trophy\<NPCOMMID>\` | [conferido] `sceNpTrophy.cpp`: `/dev_hdd0/home/<usr>/trophy/<trp_name>` |
| `NPCOMMID` | `NPWRnnnnn_nn` (9 chars do communication id + `_` + 2 dígitos) | [conferido] `"%s_%02d"` em `sceNpTrophyCreateContext` |
| Origem do pacote | `<jogo>\PS3_GAME\TROPDIR\<NPCOMMID>\TROPHY.TRP` (disco/pasta), `dev_hdd0\game\<TITLEID>\TROPDIR\<NPCOMMID>\TROPHY.TRP` (PSN), ou dentro do ISO | [conferido] `sceNpTrophyRegisterContext`; ISO [Hydra] |

Arquivos dentro da pasta do troféu (depois da primeira vez que o jogo abre no RPCS3):

| Arquivo | Conteúdo |
|---|---|
| `TROPCONF.SFM` | XML com nomes, descrições, tipo, segredo. O RPCS3 renomeia `TROP_<idioma>.SFM` (ou `TROP.SFM`) para `TROPCONF.SFM` ao instalar, então **nomes e configuração vêm do mesmo arquivo** [conferido] |
| `TROPUSR.DAT` | binário big-endian: estado (bloqueado/desbloqueado) e data de cada troféu [conferido] |
| `ICON0.PNG` (ou `ICON0_<idioma>.PNG`) | ícone do conjunto (jogo) [conferido em `sceNpTrophyGetGameIcon`] |
| `TROP000.PNG`… | ícone de cada troféu, id com 3 dígitos [conferido `TROP%03d.PNG`] |
| `TROPTRNS.DAT`, outros | não usados por nós (não verificado o conteúdo) |

Importante: **a pasta só existe depois que o jogo registrou o contexto de troféus** (primeira partida). Antes disso, o
jogo "não tem troféus" para nós (dá para ler o `TROPHY.TRP` do disco, ver 2.4, mas é opcional).

### 2.2 `TROPUSR.DAT` (RPCS3) — big-endian

Fonte: `rpcs3/Loader/TROPUSR.h` e `TROPUSR.cpp` [conferido]; parser equivalente no Hydra `rpcs3-trophies.ts`.

```
Cabeçalho (0x30 bytes)
  0x00 u32 magic        = 0x818F54AD
  0x04 u32 unk1         (gerado como 0x00010000)
  0x08 u32 tables_count (gerado como 2)
  0x0C u32 unk2         (0)
  0x10 [32] reserved

Tabela de cabeçalhos, a partir de 0x30, tables_count × 0x20 bytes
  +0x00 u32 type          (4 = tipos, 6 = estado)
  +0x04 u32 entries_size  (tamanho do conteúdo de cada entrada, sem o cabeçalho de 0x10)
  +0x08 u32 unk1          (1)
  +0x0C u32 entries_count
  +0x10 u64 offset        (absoluto no arquivo)
  +0x18 u64 reserved

Cada entrada: cabeçalho de 0x10 + entries_size bytes. Passo = 0x10 + entries_size.

Entrada tipo 4 (entries_size = 0x50; total 0x60)
  +0x00 u32 entry_type = 4     +0x04 u32 entry_size = 0x50
  +0x08 u32 entry_id           +0x0C u32 0
  +0x10 u32 trophy_id
  +0x14 u32 trophy_grade       1 platina, 2 ouro, 3 prata, 4 bronze, 0 desconhecido
  +0x18 u32 trophy_pid         0xFFFFFFFF = não conta para a platina / sem link
  +0x1C [68] zeros

Entrada tipo 6 (entries_size = 0x60; total 0x70)
  +0x00 u32 entry_type = 6     +0x04 u32 entry_size = 0x60
  +0x08 u32 entry_id           +0x0C u32 0
  +0x10 u32 trophy_id
  +0x14 u32 trophy_state       0 bloqueado, 1 desbloqueado
  +0x18 u32 unk4               +0x1C u32 unk5
  +0x20 u64 timestamp1         CellRtcTick (microssegundos desde 0001-01-01 00:00 UTC)
  +0x28 u64 timestamp2         o RPCS3 grava o mesmo tick nos dois; GetTrophyTimestamp devolve timestamp2
  +0x30 [64] zeros
```

- Data Unix (s) = `tick / 1_000_000 − 62_135_596_800` (719 162 dias × 86 400). Tick 0 ou resultado ≤ 0 → usar o
  `mtime` do arquivo como data aproximada [Hydra faz igual].
- Tabelas na ordem 4 e depois 6, ambas começando em `0x30 + 2 × 0x20 = 0x70` [conferido em `Generate`]; **ler pelo
  `offset` de cada cabeçalho, nunca por posição fixa**, e procurar a tabela pelo `type`.
- O RPCS3 grava o arquivo inteiro de novo num arquivo temporário e troca (`fs::pending_file` + `commit`) [conferido].
  Ou seja: o arquivo é **substituído** (o identificador antigo fica órfão), e uma leitura no meio é improvável mas
  possível em outros cenários (antivírus, cópia). Se o magic ou os limites não baterem, descartar a leitura e tentar
  no próximo ciclo.
- A grade também está no `ttype` do XML; preferir o XML (texto legível) e usar a tabela 4 só como reserva.
- O "platina" é desbloqueado pelo próprio RPCS3 junto com o último troféu que conta (mesmo tick) [conferido: o tick é
  repassado para a platina]. Dois desbloqueios no mesmo instante são normais.

### 2.3 `TROPCONF.SFM` (RPCS3) — XML

```xml
<?xml version="1.0" encoding="UTF-8"?>
<trophyconf version="1.1" policy="large">
  <npcommid>NPWR01234_00</npcommid>
  <trophyset-version>01.00</trophyset-version>
  <parental-level license-area="default">0</parental-level>
  <title-name>Nome do jogo</title-name>
  <title-detail>Descrição do conjunto</title-detail>
  <group id="001"> … </group>                       <!-- DLC; opcional -->
  <trophy id="000" hidden="no" ttype="P" pid="-1"> <!-- gid="001" nos de DLC -->
    <name>Platina</name>
    <detail>Conquiste todos os troféus.</detail>
  </trophy>
  …
</trophyconf>
```

- `id`: decimal com zeros à esquerda; é o mesmo `trophy_id` da tabela 6 (o nosso `Achievement.id` = `"0"`, `"1"`…,
  sem zeros, como o Hydra).
- `ttype`: `P`/`G`/`S`/`B` [conferido]. `hidden`: `yes`/`no`. `pid`: id da platina ou `-1`. `gid`: grupo (DLC).
- Os nomes dos elementos `title-name`, `name`, `detail` e o `group` são **não verificados** byte a byte (memória + uso
  no RPCS3 por `sceNpTrophyGetGameInfo`); o parser deve ser tolerante: achar `<trophyconf` em qualquer posição (o
  RPCS3 sobrescreve `GetRoot` para procurar `trophyconf` entre os filhos da raiz, o que sugere variações), ignorar
  bytes antes do `<?xml`, aceitar entidades (`&amp;`, `&#...;`) e CDATA.
- Biblioteca XML: `quick-xml` (MIT) ou `roxmltree` (MIT/Apache). Ambas pequenas; `roxmltree` é mais simples para
  arquivos de poucos KB.

### 2.4 `TROPHY.TRP` (PS3, opcional)

Só serve para mostrar a lista **antes da primeira partida** ou para achar o `NPCOMMID` de um jogo.

```
0x00 u32 magic = 0xDCA24D00 (BE)   [conferido]
     u32 version, u64 file_size, u32 files_count, u32 element_size, u32 dev_flag,
     [20] sha1 (versão ≥ 2: SHA-1 do arquivo com esse campo zerado), padding   [campos conferidos por nome; offsets não verificados]
Entradas (files_count × element_size): name[32], u64 offset, u64 size, …    [nomes conferidos; layout não verificado]
```

O RPCS3 não decifra nada no TRP de PS3 [conferido]. Para o `NPCOMMID` basta **o nome da subpasta** de `TROPDIR`
(regex `^NPWR\d{5}_\d{2}$`), sem abrir o TRP. Recomendação: **não** ler TRP na primeira versão.

### 2.5 shadPS4: onde ficam

Fonte: `src/core/libraries/np/np_trophy.cpp` e `src/common/path_util.{h,cpp}` do shadPS4 [conferido, versão `main`
de 11/2026].

| O quê | Caminho |
|---|---|
| Pasta do usuário | `user\` no **diretório atual** do processo se já existir (modo portátil), senão `%APPDATA%\shadPS4` [conferido]. O Hydra usa `user\` ao lado do exe. Para o Cerberus: abrir o shadPS4 com o diretório de trabalho = pasta do exe e procurar nos dois |
| `home_dir` | `<user>\home`, ou o configurado (`home_dir` no `config.json`/`config.toml`, conforme a versão; não verificado qual) [Hydra lê `home_dir`] |
| Estado do usuário | `<home>\<user_id>\trophy\<NPCOMMID>.xml` [conferido] (`user_id` numérico) |
| Configuração/nomes | `<user>\trophy\<NPCOMMID>\Xml\TROP_<idioma>.XML`, senão `TROP.XML` [conferido] |
| Ícones | `<user>\trophy\<NPCOMMID>\Icons\TROP000.PNG`, `ICON0.PNG` [conferido] |
| Log | `<user>\log\shad_log.txt` e `<user>\log\<TITLEID sem hífen>.log` [Hydra] |
| Versões antigas | `<user>\game_data\<TITLEID>\TrophyFiles\trophy00\Xml\TROP.XML` (não verificado; o Hydra já não usa) |
| `NPCOMMID` do jogo | `<jogo>\sce_sys\npbind.dat`, primeiro `NPWR\d{5}_\d{2}` no binário (latin1) [Hydra] |
| `TITLEID` | `<jogo>\sce_sys\param.sfo`, chave `TITLE_ID` [Hydra `param-sfo.ts`] |

O XML de estado é o mesmo formato do `TROP.XML` com dois atributos a mais no `<trophy>`:

```xml
<trophy id="003" hidden="no" ttype="B" pid="000" unlockstate="true" timestamp="1764080000">
```

- `timestamp`: **segundos Unix** (`system_clock`, `duration_cast<seconds>`) [conferido].
- `ttype` `P/G/S/B`, `hidden` booleano, `pid`, `gid` (−1 padrão) [conferido].
- A gravação **não é atômica**: `pugi::xml_document::save_file` direto no arquivo [conferido]. Uma leitura pode pegar o
  arquivo pela metade → só aceitar se o texto contiver `</trophyconf>` [Hydra faz isso] e, se não, tentar de novo.
- Log ao desbloquear: `"Trophy {} successfully saved."` [conferido]; o Hydra também usa a linha
  `[Lib.NpTrophy] … Unlocking trophy id N` junto com `Game id: <TITLEID>` para saber de qual jogo é. Usar o log só
  como reserva (ex.: troféus sem arquivo de estado) e como "sinal rápido" para a foto.
- **Chave de troféus**: o shadPS4 precisa da `ReleaseTrophyKey` (32 hex, em `keys.json` → `TrophyKeySet` [Hydra],
  ou na configuração, conforme a versão) para decifrar o `trophy00.trp` do jogo. **Essa chave vem do console do
  próprio jogador** (ferramenta de exportação citada no guia do shadPS4). O Cerberus **nunca** baixa, embute, sugere
  site ou preenche essa chave; só detecta se está configurada e explica, com texto neutro, que sem ela o shadPS4 não
  grava troféus.

### 2.6 `PARAM.SFO` (PS3 e PS4) — little-endian

Para ler o `TITLE_ID` (serial, ex. `BLUS30443`, `CUSA00001`) da pasta do jogo. [Hydra `param-sfo.ts`; layout de
memória, não verificado offset a offset]

```
0x00 "\0PSF"  0x04 u32 version  0x08 u32 key_table_start  0x0C u32 data_table_start  0x10 u32 entries
índice (16 bytes cada): u16 key_offset, u16 fmt (0x0204 utf8, 0x0404 u32), u32 len, u32 max_len, u32 data_offset
```

O Cerberus já lê PS3 por `games.yml` (`standalone::rpcs3_paths`) e reconhece discos (`disc_console.rs`, `iso9660.rs`);
o `TITLE_ID` provavelmente já é lido em algum ponto (conferir `disc_console.rs` antes de escrever outro leitor).

### 2.7 Como casar troféu ↔ jogo da biblioteca

Ordem de tentativas para um clássico de PS3 (`classic:<caminho>`):

1. **Pasta do jogo**: `<caminho>\PS3_GAME\TROPDIR\*`, `<caminho>\TROPDIR\*`, e, se o caminho for o `EBOOT.BIN`,
   subir até achar `TROPDIR` [Hydra `listDiscTrophyDirNames`].
2. **ISO**: listar `PS3_GAME/TROPDIR` com o `iso9660.rs` que já temos.
3. **Jogo PSN instalado**: `dev_hdd0\game\<TITLEID>\TROPDIR\*` (TITLEID do `PARAM.SFO`).
4. **Reserva por nome**: `title-name` do `TROPCONF.SFM` × `TITLE` do `PARAM.SFO` / nome na biblioteca, com o
   `plain_title` de `achievements/classic.rs`. Só aceitar casamento único.

Para PS4: `sce_sys\npbind.dat` da pasta do jogo dá o `NPCOMMID` direto.

Um jogo pode ter **vários** `NPCOMMID` (`_00` e `_01`, regiões, DLC em conjunto separado). Guardar a lista; mostrar
como um jogo no hub quando todos apontam para o mesmo clássico (cada conjunto é uma linha `games` com o mesmo
`library_id`, ou juntar na UI; ver 3.3).

Jogos de troféus **sem** clássico casado (ex.: aberto no RPCS3 fora do Cerberus) aparecem mesmo assim no hub, com o
`title-name` e o `ICON0.PNG`.

---

## 3. Desenho

### 3.1 Módulos novos

```
src-tauri/src/achievements/
  trophies/mod.rs       tipos comuns (TrophyGrade, TrophySet, TrophyState) e "news" (o que mudou)
  trophies/tropusr.rs   parse do TROPUSR.DAT (puro: &[u8] -> Result<Vec<State>>)
  trophies/sfm.rs       parse do TROPCONF.SFM / TROP.XML / XML de estado do shadPS4 (puro: &str -> TrophySet)
  trophies/rpcs3.rs     achar dev_hdd0 (vfs.yml), usuários, conjuntos; casar com clássico; copiar ícones
  trophies/shadps4.rs   achar user/home, conjuntos, chave presente?; log do shadPS4
  trophies/watch.rs     vigia por sessão (RPCS3 e shadPS4), mesmo contrato do live.rs
src-tauri/src/souvenirs.rs   captura, codificação, gravação, limite de espaço, apagar
src-tauri/src/capture/       (etapa 8) desktop_dup.rs e wgc.rs como alternativas ao BitBlt
```

Os parsers são funções puras sem E/S, para testes com arquivos sintéticos. `rpcs3.rs`/`shadps4.rs` fazem só leitura
nas pastas dos emuladores (nunca escrevem lá, exceto a config da sessão de 3.5, pelo mecanismo do `cheevos.rs`).

### 3.2 Tipos compartilhados (`shared/src/lib.rs`)

- `AchievementSource::Ps3` (`"ps3"`) e `AchievementSource::Ps4` (`"ps4"`): duas fontes, porque o chip, o cartão do hub
  e o emulador são diferentes. `code()`/`from_code()` ganham os dois. Toda `match` exaustiva na UI (`achievements.rs`,
  `tv/achievements.rs`, `library_page.rs`) vai acusar no compilador onde falta tratar.
- `Achievement` ganha `#[serde(default)] pub grade: Option<TrophyGrade>` (`Platinum/Gold/Silver/Bronze`). `serde(default)`
  mantém os JSON antigos do `achievements.db` legíveis.
- `points`: preencher com a convenção da PSN (bronze 15, prata 30, ouro 90, platina 180; convenção conhecida, **não
  oficial/não verificado**) para "pontos" e para ordenar; mostrar como "pontos de troféu" só se a UI já mostra pontos.
- `AchievementGame` ganha `#[serde(default)] pub library_id: Option<String>` (ex. `classic:D:\PS3\Demon`), útil também
  para Steam/Xbox depois. `platform`: `"PlayStation 3"` / `"PlayStation 4"`.
- `rarity`: sempre `None` (offline). "Suas mais raras" ignora essas fontes; "Próximas" usa a grade (bronze primeiro).
- `UnlockNotice` ganha `#[serde(default)] pub grade: Option<TrophyGrade>` para o aviso desenhar a taça na cor certa; a
  platina usa `UnlockKind::Complete`.
- `Souvenir { source, game_key, achievement_id, game_name, achievement_name, taken_at, file, thumb, width, height }`.

### 3.3 Banco local (`store.rs`)

Sem tabela nova para troféus; eles entram como as outras fontes:

| Tabela | source | account | key/game | Observação |
|---|---|---|---|---|
| `games` | `ps3`/`ps4` | `rpcs3:<usuário 8 dígitos>` / `shadps4:<user_id>` | `NPWR01234_00` | `name` = `title-name`; `art` = `cache\trophies\<NPCOMMID>\ICON0.PNG`; `total`/`unlocked`; `last_unlock`; `library_id` |
| `lists` | idem | idem | `NPWR…` | `played` = `mtime` do `TROPUSR.DAT` em s (re-ler só se mudou) |
| `unlocks` | idem | idem | `NPWR…`, id `"0".."n"` | `keep_unlocks` já mantém a data mais antiga e nunca apaga: um troféu visto não se perde mesmo se o jogador apagar a pasta do emulador |
| `stamps` | idem | idem | `name = "mark"` | assinatura (contagem + maior mtime) dos `TROPUSR.DAT`, para não reler tudo a cada abertura do hub (igual ao `Mark` da Steam em `mod.rs`) |

- Vários usuários do RPCS3: cada um é uma `Scope` (conta). O hub mostra todos; a UI só mostra o nome do usuário
  quando houver mais de um (o RPCS3 guarda o nome em `home\<usuário>\localusername`, não verificado).
- `hub()` em `achievements/mod.rs` ganha um passo `trophies_part(data_dir)` em `spawn_blocking`, logo depois do
  `steam_part`: acha RPCS3 (`standalone::locate(...)` com o caminho escolhido em `settings.emulators["rpcs3"]`) e
  shadPS4, compara a assinatura, e só então lê. Grava com `keep_games` + `keep_list` e empurra a `Scope` em `scopes`.
- `game()` ganha os ramos `Ps3`/`Ps4`: lê do banco (como Epic), e relê do disco se o `mtime` mudou.
- Ícones: **copiar** `ICON0.PNG` e `TROPnnn.PNG` para `<data>\cache\trophies\<emulador>\<NPCOMMID>\` (só se o tamanho
  ou mtime mudou) e servir de lá, liberando só essa pasta no `asset_protocol_scope()` (como `main.rs:3441..3461` faz
  com capas e fotos). Não abrir o escopo para a pasta inteira do emulador. O ícone "bloqueado" é o mesmo PNG com filtro
  cinza no CSS (a PSN não tem ícone cinza separado).

### 3.4 Lembranças no banco

Tabela nova em `achievements.db` (criada no `with()`):

```sql
CREATE TABLE IF NOT EXISTS souvenirs(
  source TEXT NOT NULL, account TEXT NOT NULL, game TEXT NOT NULL, id TEXT NOT NULL,
  taken_at INTEGER NOT NULL,          -- ms Unix
  file TEXT NOT NULL,                 -- caminho relativo a <data>\souvenirs
  thumb TEXT NOT NULL,
  width INTEGER NOT NULL, height INTEGER NOT NULL, bytes INTEGER NOT NULL,
  library_id TEXT,                    -- jogo da biblioteca que estava em frente
  PRIMARY KEY(source, account, game, id));
CREATE INDEX IF NOT EXISTS souvenirs_when ON souvenirs(taken_at DESC);
```

- Uma foto por conquista (a primeira vence; não substituir). Várias conquistas no mesmo instante **compartilham** o
  mesmo arquivo (várias linhas, mesmo `file`); apagar o arquivo só quando a última linha que o usa sai.
- Separada de `unlocks` porque o aviso ao vivo chega antes da sincronização (RetroAchievements: o log dá só o id; o
  jogo é descoberto depois em `main.rs::announce`). Junta-se na leitura por `(source, account, game, id)`; para
  RetroAchievements o id da conquista já é único no site, então `game` pode ser preenchido depois
  (`UPDATE souvenirs SET game=? WHERE source='retro' AND id=? AND game=''`).
- Funções: `keep_souvenir`, `souvenirs(scopes, limit, offset)`, `souvenirs_of(scope, game)`, `forget_souvenir`,
  `souvenir_bytes()`.

### 3.5 Vigias ao vivo

**Contrato comum** (o mesmo do `live.rs`): `start` lê a **linha de base** (o que já está desbloqueado não vira aviso),
`check` compara com `news()` (já existe e serve: devolve só os novos e se o jogo completou), e avisa.

**RPCS3** (`trophies/watch.rs`), ligado em `main.rs::watch_classic`, que já roda a cada 500 ms enquanto o clássico está
aberto:

1. Ao abrir um clássico de PS3 com o RPCS3: resolver os `NPCOMMID` (2.7) e os caminhos `TROPUSR.DAT` de cada usuário.
   Se a pasta ainda não existe (primeira partida), tentar resolver de novo a cada ~5 s; quando aparecer, a primeira
   leitura é a linha de base.
2. A cada ~1 s: `metadata()` de cada arquivo (mtime + tamanho). Mudou → **esperar 300 ms** (debounce: o RPCS3 pode
   gravar mais de uma vez seguida) → ler o arquivo inteiro (≈10 KB) → parse. Falhou (magic, limites)? Não atualizar a
   assinatura; tentar no próximo ciclo.
3. Novos desbloqueios → `souvenirs::snap` (3.6) **antes** do aviso → `unlock_toast::show` com nome, descrição e ícone
   copiado → `trail::note` → `keep_unlocks` no banco → `emit("achievements-changed")`. A platina vem no mesmo ciclo: um
   aviso "Platina!" (`UnlockKind::Complete`) depois dos outros.
4. Ao fechar o jogo: uma leitura final (sem foto) e `stop`.
5. Não travar o emulador: só leituras (`std::fs::read` abre com `FILE_SHARE_READ|WRITE|DELETE`, o padrão do Rust no
   Windows, então não impede o `rename` do RPCS3); nada de `notify`/`ReadDirectoryChangesW` no começo (polling barato
   e igual ao resto do código; se um dia precisar, o `notify` crate é MIT/CC0). Não segurar o arquivo aberto.

**Configuração da sessão do RPCS3** (pelo mesmo mecanismo `apply`/`undo_all`/`restore_pending` do `cheevos.rs`, que já
devolve o que o jogador tinha):

- `Miscellaneous: Show trophy popups: false` [conferido em `system_config.h`] — o aviso passa a ser só o do Cerberus.
- `Video: Vulkan: Exclusive Fullscreen Mode: Disable` [conferido; o Hydra faz isso por sessão] — para a foto (e o
  nosso aviso) funcionarem.
- O `config.yml` é YAML aninhado; o `cheevos.rs` hoje edita INI. Precisa de um editor YAML mínimo por linha (achar o
  bloco `Miscellaneous:` e a chave indentada) ou o crate `serde_yaml`/`yaml-rust2`; preferir edição por linha para não
  reformatar o arquivo do jogador (o `undo` conta com isso). Configuração por jogo (`custom_configs\config_<TITLEID>.yml`)
  pode sobrepor a global (não verificado se tem precedência nessas duas chaves); se existir, editar as duas.

**shadPS4** (quando ele virar emulador do Cerberus; hoje **não está** em `standalone.rs`):

- Vigiar `<home>\<uid>\trophy\<NPCOMMID>.xml` como acima, mas: só aceitar texto com `</trophyconf>`; se faltar,
  repetir em 200 ms até 3 vezes (gravação não atômica).
- Em paralelo, ler o log do fim (como `cheevos::read_new`) procurando `Unlocking trophy id N` depois de `Game id:` do
  jogo certo: dá o **momento exato** para a foto, mesmo que o XML seja gravado depois. A foto tirada pelo log é ligada à
  conquista quando o XML confirmar (até 10 s); se o XML nunca confirmar, guardar mesmo assim com a data do log.
- O shadPS4 também tem aviso próprio de troféu (não verificado se dá para desligar por configuração); se não der, a foto
  sai com o aviso dele, como no PPSSPP hoje.

**Steam** (`live.rs`): hoje o `check` roda a cada 5 s no laço de `store_time.rs`. Para a foto ficar perto do momento,
enquanto houver jogo Steam vigiado, checar a cada 1 s (`metadata()` é barato) e manter 5 s sem jogo. Depois de
`news()`, chamar `souvenirs::snap` antes do `unlock_toast::show`.

**RetroAchievements pelos logs** (`main.rs::watch_classic` → `announce`): `snap` logo na linha do log (antes do
`spawn` do `announce`), e `announce` grava a lembrança quando souber o jogo. Para o **RetroArch**, há a opção dele de
foto ao desbloquear (`cheevos_auto_screenshot`, **nome não verificado**) que tira a foto do quadro do núcleo (sem
sobreposição, funciona em tela cheia exclusiva). Ideia da etapa 8: ligar só durante a sessão com
`screenshot_directory` apontando para uma pasta temporária nossa e adotar o PNG mais novo; o BitBlt fica como reserva.

### 3.6 Captura (`souvenirs.rs`)

**Quando capturar**

- **Logo que a conquista é detectada, antes de mostrar o nosso aviso.** O `grab` atual (`screenshots.rs`) copia a tela
  (`GetDC(null)` + `BitBlt` com `CAPTUREBLT`), então pegaria a nossa janela de aviso se ela já estivesse na tela. Ordem
  no código: `snap()` copia os pixels de forma síncrona (BitBlt de 1080p ≈ dezenas de ms; não verificado em 4K) e
  devolve; o aviso aparece em seguida; a codificação JPEG e a gravação vão para uma thread.
- **Defesa extra**: marcar a janela do aviso (`unlock_toast.rs`) e a do menu (`overlay.rs`) com
  `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` (Windows 10 2004+). Ela some de BitBlt, Desktop
  Duplication e WGC, então a ordem deixa de importar e o F12 também não pega o aviso. Em Windows mais antigo a função
  falha; **não** usar `WDA_MONITOR` (a janela sairia preta na foto). Feature `Win32_UI_WindowsAndMessaging` já está no
  `windows-sys`.
- Não há atraso "para esperar o aviso sumir", porque o aviso não entra. Atraso **opcional** só para avisos que não
  controlamos (Steam overlay, PPSSPP, shadPS4): configuração "Atraso da foto" 0 s (padrão) / 1 s / 3 s. Com 0 s a
  Steam pode aparecer com o aviso dela, porque o arquivo dela muda depois que o aviso já está na tela; isso é aceitável
  e honesto ("o momento").
- Rajadas: até 2 s depois de uma foto, novas conquistas do mesmo jogo reaproveitam a mesma foto (agrupamento, como o
  Hydra faz com "grouped souvenirs").

**Qual janela**

- **Só se a janela em primeiro plano for do jogo.** Para clássicos: o processo do emulador (`child.id()` em
  `watch_classic`, ou um processo filho dele). Para Steam: o processo cujo exe está na pasta do jogo (lógica de
  `screenshots::owner`, mas sobre a lista de `store_time`, porque jogos da Steam abertos fora do Cerberus não estão em
  `Sessions`). Se o jogador deu alt+tab, **não** fotografar (privacidade: sairia outra coisa da tela) e gravar a
  conquista sem foto.
- Recortar para a área do cliente (como `grab` faz) para não sair a barra de título.

**Tela cheia exclusiva (sai preta)**

1. BitBlt (atual). Se sair toda preta (o `grab` já detecta), tentar:
2. **Desktop Duplication** (`IDXGIOutputDuplication`, D3D11): duplica o monitor onde a janela está e recorta o
   retângulo do cliente. Costuma pegar jogos em tela cheia "moderna" (flip model/independent flip) e muitos em tela
   cheia exclusiva; **não verificado** para DirectX 9 antigo em modo exclusivo real. Cuidados: `AcquireNextFrame` só
   entrega quando há quadro novo (esperar até ~200 ms), o primeiro quadro pode vir vazio, `DXGI_ERROR_ACCESS_LOST`
   em troca de modo, e em notebooks híbridos falha quando o Cerberus e o monitor estão em GPUs diferentes
   (`DXGI_ERROR_UNSUPPORTED`). Monitor HDR entrega `R16G16B16A16_FLOAT` e precisa converter para SDR (não verificado
   como fica a cor; documentar como limitação). Não deixar a duplicação aberta entre fotos (custa memória de vídeo);
   abrir, pegar um quadro, fechar.
3. **Windows.Graphics.Capture** (WGC) da janela (`CreateForWindow`): captura a janela mesmo coberta e **não** pega
   outras janelas por cima (melhor para privacidade e para avisos de outros apps). Mostra uma borda amarela no Windows
   10; no 11 dá para pedir sem borda (`IsBorderRequired = false`, pede permissão `graphicsCaptureWithoutBorder`; não
   verificado em app Win32 sem pacote). Não funciona em tela cheia exclusiva real. Fica como opção futura para trocar o
   BitBlt.
4. Se tudo falhar: conquista sem foto, e na página dela "A foto não saiu (tela cheia exclusiva)". Sugerir "tela cheia
   sem bordas" nas opções do jogo.

Precisa da feature `Win32_Graphics_Dxgi` + `Win32_Graphics_Direct3D11` no `windows`/`windows-sys` (crates Microsoft,
MIT/Apache). Esse mesmo trabalho atende o item 6.4 "Fotos de jogos em tela cheia exclusiva" (F12): fazer uma vez só,
em `src-tauri/src/capture/`, e o `screenshots.rs` passa a usar.

**Onde guardar, tamanho**

- `<data>\souvenirs\<pasta do jogo>\<fonte>_<id-saneado>_<ms>.jpg` e miniatura `…_t.jpg`. Pasta do jogo = `folder_name`
  do `screenshots.rs` aplicado a `library_id` quando houver, senão `<fonte>_<chave>`. Sanear o id (só
  `[A-Za-z0-9._-]`, máx. 80 chars, mais 8 hex de um hash do id original, como o Hydra faz em
  `achievement-screenshot-path.ts`) para ids com espaço/acento (Steam usa nomes como id).
- **JPEG qualidade 85**, lado maior limitado a 1920 px (o Hydra limita a 1080p e usa qualidade 80). 1080p sai em torno
  de 250–600 KB (estimativa, não verificado). Miniatura de 480 px de largura para grades. PNG (o F12 usa PNG) ficaria
  2–6 MB por foto: grande demais para algo automático.
- Codificador: `jpeg-encoder` (MIT/Apache, puro Rust, sem dependências) e redimensionamento simples (média de
  blocos/bilinear) feito à mão ou `fast_image_resize` (MIT/Apache). Evitar o `image` inteiro só por isso.
- Gravar em `.tmp` e renomear (uma queda no meio não deixa JPEG quebrado), depois inserir a linha no banco.
- Liberar `<data>\souvenirs` no `asset_protocol_scope()` como as fotos.

**Limite de espaço e apagar**

- Limite padrão 2 GB (configurável: 500 MB / 2 GB / 10 GB / sem limite). Ao passar: **parar de tirar** e avisar uma vez
  ("As lembranças chegaram ao limite de 2 GB"), com botão para a aba. Não apagar lembranças sozinho por padrão (são
  memórias); opção "Apagar as mais antigas quando encher", desligada.
- Apagar: uma lembrança (lightbox), todas de um jogo, todas. Apagar uma lembrança não apaga o desbloqueio. Apagar o jogo
  da biblioteca **não** apaga as lembranças (ficam no hub sob o nome do jogo).
- Ao abrir o app (em segundo plano, uma vez por dia): remover linhas cujo arquivo sumiu e arquivos órfãos da pasta que
  nenhuma linha usa (só dentro de `<data>\souvenirs`, só `.jpg`).
- Backup da biblioteca (`data_backup.rs`): incluir `souvenirs` só se o jogador marcar (pode ser grande).

**Privacidade**

- Tudo local; nada enviado; não aparece em nenhuma integração online.
- Desligado até o jogador aceitar uma vez: na primeira conquista ao vivo com o recurso ainda indeciso, o aviso normal
  aparece e, depois da partida, uma notificação do Cerberus pergunta "Guardar uma foto do momento de cada conquista?"
  (e o mesmo convite fica num cartão da aba Lembranças). Chave em Configurações › Durante o jogo: "Foto no momento da
  conquista" (`InGame.souvenirs: Option<bool>`, `None` = ainda não perguntado).
- Nunca fotografar se o jogo não está em primeiro plano (ver acima). Com BitBlt, outras janelas por cima do jogo (chat,
  notificação do Windows) podem sair; o texto da opção diz isso. WGC resolve no futuro.

---

## 4. UI

**Hub (`ui/src/achievements.rs`)**

- `Platform::Ps3` e `Platform::Ps4` no filtro e cartões "PlayStation 3 · RPCS3" / "PlayStation 4 · shadPS4" com
  contagem de platinas, ouros, pratas e bronzes (quatro taças pequenas com números), não "pontos".
- `source_label` / `platform_chip`: classes `ach-src ps3`, `ach-src ps4`.
- "Platinados" passa a contar jogos com a platina; "Perto dos 100%" funciona igual.
- Cartão vazio quando o RPCS3 está instalado mas não há troféus: "Os troféus aparecem aqui depois que um jogo de PS3
  abre no RPCS3." Para o shadPS4 sem chave: "O shadPS4 precisa da chave de troféus do seu PS4 para gravar troféus.
  Veja o guia do shadPS4." (sem link para chave, sem campo para colar).
- Nova aba/seção **Lembranças**: grade de miniaturas agrupada por jogo (cabeçalho com nome e ícone, recolhível, como
  `souvenirs-tab.tsx` do Hydra), mais novas primeiro, filtro por plataforma, "carregar mais" de 60 em 60. Clique abre
  um **lightbox**: foto grande, nome e descrição da conquista com o ícone, jogo, data e hora, setas para a anterior e
  a próxima, botões "Abrir", "Mostrar na pasta", "Copiar" (etapa futura), "Apagar" (com confirmação). Estado vazio
  explica o recurso e tem o botão de ligar.

**Página da conquista / lista do jogo (`GameBody`, `AchRow`)**

- Taça colorida pela grade ao lado do ícone (platina azul-acinzentada, ouro, prata, bronze), e no topo a soma por grade.
- Linha desbloqueada com lembrança: miniatura à direita (ou um ícone de câmera que expande), clique abre o lightbox.
  Linha sem foto por tela cheia exclusiva: ícone discreto com dica.
- Segredo (`hidden="yes"`): igual às outras fontes (oculto até desbloquear ou "mostrar").

**Biblioteca (aba Conquistas da página do jogo)**

- Clássico de PS3/PS4 com `NPCOMMID` casado: a aba mostra `AchievementsTab(source="ps3", key=NPWR…)`. Vários conjuntos:
  um seletor no topo.
- A aba **Fotos** do jogo mostra as lembranças numa faixa separada ("Lembranças") acima das fotos do F12.

**Big Picture (`ui/src/tv/achievements.rs`)**

- `library_id()` deixa de aceitar só Steam: usa `AchievementGame.library_id` quando existir, então A num jogo de PS3 abre
  o clássico.
- Fileira "Lembranças recentes" na tela de conquistas; A abre a foto em tela cheia, ←/→ navega, X apaga (com
  confirmação), B volta. Na `GameList`, a conquista focada mostra a miniatura se houver.

**Aviso (`ui/src/unlock.rs`)**: taça na cor da `grade`; texto "Troféu de ouro desbloqueado" etc.; "Platina!" na
platina. Som: o mesmo do aviso atual (um som próprio de platina pode vir do item 5.5 "Temas").

**Configurações › Durante o jogo**: "Foto no momento da conquista", "Atraso da foto", "Espaço máximo", botão "Abrir
pasta das lembranças". **Configurações › Emulação › RPCS3**: linha "Troféus: lidos de `<dev_hdd0>`" (mostra o caminho
resolvido, útil quando o `vfs.yml` muda o `dev_hdd0`).

---

## 5. Casos de borda

- **Jogo sem troféus** (homebrew, demos, alguns PSN): sem `TROPDIR` → nenhuma linha no hub; a aba Conquistas do jogo
  diz "Este jogo não tem troféus".
- **Antes da primeira partida**: pasta de troféus inexistente; o vigia tenta de novo a cada 5 s e a primeira leitura
  vira linha de base (o jogo pode registrar o contexto vários segundos depois do boot).
- **Linha de base com desbloqueios antigos**: nada de aviso para eles; vão para o banco com a data real.
- **Platina junto com o último troféu**: dois novos no mesmo ciclo; avisar o troféu e depois "Platina!".
- **Jogo com DLC** (`gid`/grupos): total do jogo inclui DLC (como a PSN); separar base e DLC fica para o item 6.1
  "Separar conquistas do jogo base e das DLCs".
- **Vários usuários do RPCS3** e **vários `NPCOMMID`**: cada usuário é uma conta; cada conjunto é um jogo, todos com o
  mesmo `library_id`.
- **Idioma**: o RPCS3 instala o `TROP_<idioma>.SFM` do idioma configurado na **primeira** instalação; trocar o idioma
  depois não muda os nomes (não verificado se reinstala). Aceitar o que estiver lá.
- **Jogador apaga ou reseta os troféus no emulador**: o banco mantém o que já viu (regra do `store.rs`). Opção
  "Esquecer os troféus deste jogo" na página (apaga `games`/`lists`/`unlocks` dessa fonte+chave). Durante uma sessão,
  um troféu que "volta a bloquear" não gera aviso.
- **Datas**: tick 0, futuro (> agora + 1 dia) ou antes de 2006 → usar `mtime`. O relógio do PS3 emulado segue o do PC
  (não verificado se o RPCS3 aplica deslocamento de fuso; o tick é UTC).
- **Arquivo truncado/corrompido** (magic errado, `offset + count × passo > tamanho`, `entries_size < 0x20`): ignorar a
  leitura, não apagar nada do banco, registrar no `trail` uma vez.
- **shadPS4 gravando no meio da leitura**: sem `</trophyconf>` → tentar de novo.
- **shadPS4 sem chave de troféus**: sem arquivos; mensagem neutra (2.5). Nunca oferecer a chave.
- **RPCS3 portátil × `vfs.yml` com outro disco**: sempre resolver pelo `vfs.yml`; caminho com `$(…)` desconhecido → não
  ler e mostrar o motivo em Configurações.
- **RPCS3 aberto fora do Cerberus**: o hub lê os troféus ao abrir (sem aviso ao vivo nem foto). Avisar ao vivo nesse
  caso (vigiar `rpcs3.exe` como o `store_time.rs` vigia a Steam) fica como melhoria.
- **Cerberus fecha no meio da partida**: os troféus entram na próxima abertura do hub; sem foto.
- **Jogo fora de foco / minimizado / outra área de trabalho**: sem foto (privacidade), conquista registrada.
- **Vários monitores e DPI**: usar coordenadas físicas da área do cliente (`ClientToScreen`) e, na Desktop
  Duplication, o monitor de `MonitorFromWindow`.
- **HDR**: cores lavadas ou erradas na foto (limitação documentada; não verificado).
- **Disco cheio / pasta sem permissão**: a foto falha em silêncio (só `trail`), o desbloqueio segue.
- **Steam com ids textuais** (`ACH_WIN_ONE_GAME`) e Xbox/Epic (sem vigia ao vivo hoje): Xbox e Epic não ganham fotos
  nesta etapa porque não temos detecção ao vivo deles.
- **Conquista de outro PC** (Steam Web API, RA no site): sem foto, claro.
- **Caminhos com acento/Unicode e longos**: tudo `PathBuf`/UTF-16; nomes de arquivo nossos são ASCII.

---

## 6. Plano em etapas

Cada etapa compila e passa `cargo test` sozinha, e é um commit. Testes seguem o estilo atual (módulo `tests` no fim
do arquivo, `tempfile` para pastas). **Arquivos de exemplo são sintéticos, gerados no próprio teste** (nada de
`TROPUSR.DAT` de jogo real; o `fixtures/rpcs3-tropusr.dat` do Hydra é MIT, mas parece tirado de um jogo de verdade:
não copiar).

Gerador de testes (em `trophies/tropusr.rs`, `#[cfg(test)]`):

```rust
/// Um TROPUSR.DAT como o RPCS3 gera: cabeçalho, tabela 4 e tabela 6.
fn tropusr(trophies: &[(u32 /*id*/, u32 /*grade*/, Option<u64> /*tick*/)]) -> Vec<u8>
```

monta `0x818F54AD`, `0x00010000`, `2`, `0`, 32 zeros; dois cabeçalhos de tabela (`4, 0x50, 1, n, 0x70, 0` e
`6, 0x60, 1, n, 0x70 + n×0x60, 0`); entradas 4 e 6 com `trophy_state = tick.is_some()` e os dois ticks iguais. E um
`TROPCONF.SFM` de texto com 4 troféus (P, G, S, B; um `hidden="yes"`; um com `&amp;` no nome).

### Etapa 1 — Parsers puros (P)

- `tropusr::parse(&[u8]) -> Result<Vec<TrophyState>, Bad>` e `sfm::parse(&str) -> Result<TrophySet, Bad>`;
  `tick_to_unix(u64) -> Option<u64>`.
- Aceite: lê o arquivo do gerador; ordem das tabelas trocada (6 antes de 4) ainda funciona; magic errado, arquivo
  curto, `offset` além do fim, `entries_size < 0x20`, `tables_count` gigante → `Err` sem pânico; tick conhecido
  (`63_871_000_000_000_000` µs → data Unix calculada à mão) bate; tick 0 → `None`.
- XML: lixo antes do `<?xml`, entidades, CDATA, `hidden="yes"`, `ttype` minúsculo, `id="007"` → `"7"`; XML sem
  `</trophyconf>` → `Err` (regra do shadPS4).
- Teste de fuzz leve: `parse` em 10 000 cortes/bytes trocados do arquivo gerado nunca entra em pânico.

### Etapa 2 — Achar RPCS3 e ler os conjuntos (M)

- `rpcs3::hdd0(exe) -> Result<PathBuf, Motivo>` (config ao lado do exe e em `config\`, `vfs.yml`), `rpcs3::users`,
  `rpcs3::sets(hdd0) -> Vec<(usuário, NPCOMMID, PathBuf)>`, `rpcs3::read_set(dir) -> (TrophySet, Vec<TrophyState>, mtime)`.
- Cópia de ícones para o cache (só se mudou).
- Aceite (testes com `tempfile`): árvore falsa `dev_hdd0\home\00000001\trophy\NPWR00001_00\{TROPCONF.SFM,TROPUSR.DAT,
  ICON0.PNG,TROP000.PNG}` vira um `AchievementGame` + lista correta; `vfs.yml` com `/dev_hdd0/: D:/outro/` e com
  `$(EmulatorDir)` redefinido; dois usuários; pasta sem `TROPUSR.DAT` (ignorada); um teste `#[ignore]` que lê o RPCS3
  real do PC (como `real_steam_hub_reads_this_pc`).

### Etapa 3 — Fonte `ps3` no banco e no hub (M)

- `AchievementSource::Ps3`, `grade`, `library_id` em `shared`; `trophies_part` no `hub()`, ramo `Ps3` no `game()`,
  assinatura em `stamps`.
- UI: filtro, cartão, chips, taças por grade, lista do jogo; Big Picture mostra os jogos (A abre a lista).
- Aceite: teste do `store.rs` com `Store::memory()` — duas leituras seguidas não duplicam; um troféu que some do
  arquivo continua no banco; data mais antiga vence. Teste de `serde`: JSON antigo de `Achievement` sem `grade` lê.
  Manual: com o RPCS3 do PC, o hub mostra o jogo com ícones e a platina conta em "Platinados" (conferir em tela, com
  captura, como pede a regra do projeto).

### Etapa 4 — Casar com o clássico (P/M)

- `rpcs3::npcommids_for(rom_path) -> Vec<String>` (pasta, `EBOOT.BIN`, ISO via `iso9660.rs`, `dev_hdd0\game\<TITLEID>`,
  reserva por nome); preencher `library_id`.
- UI: aba Conquistas do clássico de PS3; Big Picture abre o clássico.
- Aceite: árvores falsas para cada forma (pasta de disco, PSN instalado, ISO sintético pequeno se o `iso9660.rs` já
  tiver gerador de teste; senão `#[ignore]` real); dois conjuntos para o mesmo jogo; nome ambíguo não casa.

### Etapa 5 — Ao vivo no RPCS3 (M)

- `trophies::watch::Rpcs3Watch { start, check, finish }` chamado do laço de `watch_classic` quando `emulator.id ==
  "rpcs3"`; configuração da sessão (`Show trophy popups`, `Exclusive Fullscreen Mode`) pelo `cheevos.rs` com
  edição YAML por linha e `undo`.
- Aceite (testes sem RPCS3): `start` com arquivo do gerador = linha de base sem avisos; reescrever o arquivo (via
  arquivo temporário + `rename`, como o RPCS3) com um troféu a mais → `check` devolve exatamente esse; platina junto →
  dois e `complete`; arquivo truncado no meio → nada e não muda a assinatura; pasta que aparece depois do `start` →
  primeira leitura é linha de base. Edição YAML: aplicar e desfazer devolve o arquivo byte a byte; chave mudada pelo
  jogador durante a sessão fica (igual ao teste `a_setting_the_player_changed_during_the_game_stays`).
- Manual: abrir um jogo de PS3, ganhar um troféu, ver o aviso do Cerberus e não o do RPCS3; ao fechar, o
  `config.yml` está como antes.

### Etapa 6 — Lembranças: núcleo (M)

- `souvenirs.rs`: `snap(app, alvo) -> Option<Pixels>` (BitBlt atual, só se o alvo está em primeiro plano), `keep(...)`
  em thread (redimensiona, JPEG 85, miniatura, `.tmp` + rename, linha no banco), agrupamento de 2 s, limite de espaço,
  limpeza de órfãos. `WDA_EXCLUDEFROMCAPTURE` nas janelas do aviso e do menu.
- Ganchos: Steam (`live.rs`, checagem a 1 s com jogo aberto), RetroAchievements pelos logs (`watch_classic`/`announce`),
  RPCS3 (etapa 5). Configuração `InGame.souvenirs`, convite único.
- Aceite: testes puros do nome do arquivo (ids com espaço, acento, `..`, 300 chars), do agrupamento (três conquistas em
  1 s = um arquivo, três linhas; apagar uma linha mantém o arquivo; apagar a última apaga), do limite (passou do limite
  → `snap` recusa), da limpeza de órfãos (só `.jpg` dentro da pasta), do JPEG (codifica 4×2 px e o arquivo começa com
  `FF D8` e termina com `FF D9`; redimensionar 3840×2160 → 1920×1080). Manual: Steam e RetroArch em janela sem bordas →
  foto sem o nosso aviso; alt+tab antes da conquista → sem foto.

### Etapa 7 — Lembranças: UI (M)

- Comandos `list_souvenirs(limit, offset, plataforma)`, `game_souvenirs(source, key)`, `delete_souvenir`,
  `delete_game_souvenirs`, `open_souvenirs_folder`; evento `souvenir-taken`.
- Aba Lembranças, lightbox, miniatura na linha da conquista, faixa na aba Fotos, Big Picture.
- Aceite: páginas conferidas em captura de tela (desktop e Big Picture, claro/escuro), teclado e controle navegam o
  lightbox, apagar pede confirmação e some da grade sem recarregar.

### Etapa 8 — Tela cheia exclusiva e captura melhor (M/G)

- `capture::desktop_dup` como reserva quando o BitBlt sai preto (e passa a servir ao F12 também, item 6.4); opção
  RetroArch `cheevos_auto_screenshot` por sessão (conferir o nome da chave no `configuration.h` do RetroArch antes).
- Aceite: teste manual com um jogo DX11 em tela cheia exclusiva e um DX9; medir o tempo da captura (alvo < 150 ms) e
  registrar resultados aqui. Falha de duplicação (GPU híbrida) cai para "sem foto" sem travar.

### Etapa 9 — shadPS4 (G; depende de entrar em `standalone.rs`)

- Emulador `shadps4` em `standalone.rs` (releases oficiais `shadps4-emu/shadPS4`; nome do arquivo `.zip` de Windows a
  conferir na release), detecção da chave **só para avisar**, fonte `ps4`, parser do XML de estado (já pronto na etapa 1),
  `npbind.dat`/`param.sfo`, vigia com XML + log.
- Aceite: árvore falsa `user\trophy\NPWR…\Xml\TROP.XML`, `user\home\1000\trophy\NPWR….xml` com `unlockstate`/
  `timestamp` em segundos; XML cortado ao meio → tentar de novo; linhas de log sintéticas (`Game id: CUSA00001`,
  `[Lib.NpTrophy] … Unlocking trophy id 3`) de outro jogo são ignoradas.

Ordem sugerida: 1 → 2 → 3 → 4 → 5 entregam o item 5.4 inteiro; 6 → 7 entregam o 6.1 (fotos) para Steam/RA/RPCS3;
8 melhora as fotos e fecha parte do 6.4; 9 fica junto com o item 6.5 "Emuladores fora do RetroArch".

---

## 7. Riscos

| Risco | Efeito | Mitigação |
|---|---|---|
| Formato do `TROPUSR.DAT` muda numa versão do RPCS3 | Sem troféus | Parser lê pelos cabeçalhos e `type`, falha sem pânico; teste `#[ignore]` com o RPCS3 real em cada atualização |
| Nomes dos elementos do SFM diferentes do esperado | Nomes vazios | Parser tolerante; nome vazio → "Troféu N"; conferir com um jogo real na etapa 2 |
| Edição do `config.yml` estraga a config do jogador | RPCS3 não abre/perde ajustes | Edição por linha, `undo` byte a byte, `restore_pending` na abertura (já existe), testes |
| Foto pega conteúdo privado | Confiança | Só em primeiro plano; desligado até aceitar; local; WGC no futuro |
| Foto pega o nosso aviso | Lembrança feia | Capturar antes de mostrar + `WDA_EXCLUDEFROMCAPTURE` |
| Desktop Duplication instável (GPU híbrida, HDR, troca de modo) | Foto preta ou erro | Só como reserva, com tempo limite; nunca na thread do jogo nem da UI |
| Captura atrasa o jogo | Engasgo | Uma cópia por conquista; codificação fora da thread; medir |
| Espaço em disco | Pasta enorme | JPEG, limite com aviso, limpeza de órfãos |
| Chave do shadPS4 | Tentação de "facilitar" | Política: só detectar e explicar; nunca baixar, embutir ou indicar fonte |
| Polling de 1 s da Steam | CPU/disco | `metadata()` só de um arquivo por jogo aberto; volta a 5 s sem jogo |

---

## 8. O que NÃO fazer

- Não baixar, embutir, sugerir ou aceitar colar PKG, firmware, `PS3UPDAT.PUP`, `keys.json`, chave de troféus do PS4,
  RAP/licenças ou qualquer outra chave. O firmware do PS3 continua sendo instalado pelo próprio RPCS3 a partir do
  arquivo do jogador (`standalone.rs` já faz assim).
- Não copiar código do RPCS3 nem do shadPS4 (GPL): só os fatos do formato descritos aqui. Do RecompLauncher (GPL), nada.
  Do Hydra (MIT), pode portar lógica com crédito no cabeçalho do arquivo.
- Não escrever nos arquivos de troféus dos emuladores (nem "consertar", nem resetar). O "Esquecer" mexe só no nosso banco.
- Não segurar arquivos do emulador abertos nem usar travas; não ler na thread da UI.
- Não enviar lembranças a lugar nenhum; não criar "perfil público".
- Não fotografar fora do primeiro plano; não fotografar a área de trabalho inteira.
- Não apagar lembranças sozinho por padrão.
- Não ler `TROPHY.TRP`/`trophy00.trp` na primeira versão (PS4 é cifrado; PS3 não é necessário).
- Não usar `WDA_MONITOR` como reserva do `WDA_EXCLUDEFROMCAPTURE`.

---

## 9. Fontes

- RPCS3 `rpcs3/Loader/TROPUSR.h`, `TROPUSR.cpp`, `TRP.cpp`, `Emu/Cell/Modules/sceNpTrophy.cpp`, `Emu/system_config.h`
  — <https://github.com/RPCS3/rpcs3> (GPL-2.0; só leitura do formato)
- shadPS4 `src/core/libraries/np/np_trophy.cpp`, `src/common/path_util.h`, `path_util.cpp` —
  <https://github.com/shadps4-emu/shadPS4> (GPL-2.0; só leitura do formato)
- Hydra (MIT) `src/main/services/emulators/rpcs3-trophies.ts`, `rpcs3-trophy-watcher.ts`, `rpcs3-trophy-sources.ts`,
  `shadps4-trophies.ts`, `shadps4-trophy-watcher.ts`, `shadps4-files.ts`, `trophy-file-watcher.ts`,
  `rpcs3-session-config.ts`, `emulator-souvenir-watcher.ts`, `cloud-save/rpcs3-save-layout.ts`, `rpcs3-config-root.ts`,
  `screenshot.ts`, `achievement-screenshot-path.ts`, `renderer/.../souvenirs-tab.tsx`
- Guia do shadPS4 (chave de troféus exportada do console do próprio jogador):
  <https://git.panintegr.al/Backup/shadPS4/wiki/I.-Quick-start-%5BUsers%5D> (espelho; conferir no wiki oficial)
- Microsoft: `SetWindowDisplayAffinity` (`WDA_EXCLUDEFROMCAPTURE`), Desktop Duplication API
  (`IDXGIOutputDuplication`), `Windows.Graphics.Capture` — documentação do Win32/WinRT (não consultada nesta rodada;
  comportamento descrito de memória, **não verificado**).
- psdevwiki "Trophy files" (PS3) recusou acesso automático (403); os campos do TRP ficaram como não verificados.
