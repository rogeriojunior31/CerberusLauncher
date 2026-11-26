# Lojas e contas: o que falta para ficar completo

Revisão de 25/11/2026 da feature entregue entre 23 e 25/11 (branch `ajustes-rapidos`): login da Epic e da conta
Microsoft, bibliotecas das duas contas, jogos do app Xbox, catálogo do PC Game Pass, página da loja para Epic e Xbox e
selo de origem nas capas. Foi feita só lendo o código, sem compilar nem abrir o app. O que não deu para confirmar está
marcado como **não verificado**.

Complementa o `ROADMAP.md` (6.2 e 7.2). Itens que já estão lá (GOG, DLC, fila de downloads, mesmo jogo da Steam e da
Epic como um só, argumentos da Steam) só aparecem aqui quando a revisão achou um detalhe novo.

Esforço: **P** (até meio dia), **M** (1 a 3 dias), **G** (mais que isso).

---

## 1. Estado atual

| O quê | Como funciona | Onde |
| --- | --- | --- |
| Login da Epic | Página da Epic numa janela própria e privada (`incognito`). Quando chega em `/id/api/redirect` com o nosso `clientId`, um script lê o `authorizationCode` e manda a janela para `https://cerberus.invalid/epic?code=…`. A navegação é barrada, o código é validado (`valid_code`) e trocado por token. Os pop-ups de Xbox/Google/Apple abrem no WebView2. Só o refresh token e o nome ficam guardados. | `src-tauri/src/epic_account.rs` (`login_url`, `is_return`, `valid_code`, `token`), `main.rs` (`epic_login`, `finish_epic_login`) |
| Biblioteca da Epic | `library-service` paginado (até 50 páginas) e depois o catálogo em lotes de 40 por namespace. Filtro de jogo igual ao do Hydra (`is_game`: sem `ue`, DLC, addons, mods, software). Cache de 1 h em `epic-library.json`, renovado em segundo plano. Os não instalados entram com `NextStep::Install`, e a instalação abre `com.epicgames.launcher://…?action=install`. | `epic_account.rs` (`library`, `merge`, `keep`, `stale`), `main.rs` (`epic_library`, `play`) |
| Conta Microsoft/Xbox | OAuth do Live com o client do app Xbox (o mesmo fluxo do Playnite), depois os tokens de usuário e XSTS. O código volta em `login.live.com/oauth20_desktop.srf`. A lista vem do `titlehub` (`titlehistory`), filtrada para `type == Game` e `devices` com PC, e cada jogo é consultado no `displaycatalog` para achar produto e arte. Cache de 1 h em `xbox-library.json`. | `src-tauri/src/xbox_account.rs`, `main.rs` (`xbox_login`, `finish_xbox_login`, `xbox_library`) |
| Jogos instalados do app Xbox | Lê `.GamingRoot` em cada drive de C a Z, `Content/MicrosoftGame.config` e `appxmanifest.xml`, e calcula a família do pacote (hash do publisher, com teste). Abre por `explorer.exe shell:AppsFolder\<família>!<app>`. A arte vem da Steam pelo nome exato, guardada em `xbox-art.json`. | `src-tauri/src/xbox.rs` |
| PC Game Pass | Catálogo público (`catalog.gamepass.com/sigls` e `displaycatalog`), cache de 12 h por mercado. Põe arte e `game_pass: true` nos instalados e adiciona os outros como não instalados. Com `drop_owned_elsewhere`, a entrada do Game Pass some quando há outro jogo com o mesmo nome "plano" (só letras e números, minúsculas). | `src-tauri/src/gamepass.rs` |
| Página da loja (Epic/Xbox) | `store_page` tenta primeiro a Steam pelo nome exato (`steam_store::app_for`, cache em `steam-match.json`) e, se não achar, a Microsoft Store pelo produto ou pela busca (`ms_store::find`/`details`, cache de 7 dias). | `main.rs` (`store_page`), `src-tauri/src/ms_store.rs`, `src-tauri/src/steam_store.rs`, UI `ui/src/game.rs` (`StoreOverview`) |
| Selo de origem | Um selo por fonte (Steam, Epic, Game Pass, Xbox, Recomp) no canto da capa. | `ui/src/tiles.rs` (`source_marks`, `marks_view`), `ui/styles.css` |
| Cartões de conta | Entrar/Sair, o nome conectado, a contagem de instalados e o interruptor do Game Pass. Os dados são relidos quando a janela principal volta ao foco. | `ui/src/settings_page.rs` (`XboxCard`, `EpicCard`) |
| Biblioteca | Filtro de plataforma (Todas, Steam, Epic, Xbox, Adicionados) com contagem e filtro Todos/Instalados/Não instalados. | `ui/src/library_page.rs` (`Filter`, `counts`, `presence`) |
| Segurança já atendida | As janelas de login não aparecem em `capabilities/default.json` (só `main`, `overlay` e `unlock`). Senha nunca passa pelo Cerberus. O log do Xbox Live grava só o endereço sem query e 200 caracteres do corpo da resposta, nunca o token. | `src-tauri/capabilities/default.json`, `achievements/xbox.rs` |
| Testes existentes | Página de retorno e código da Epic, filtro `is_game`, `merge` da Epic, `code_in`/`refused` e `pc_games` do Xbox, `merge` da conta Xbox, família do pacote, `.GamingRoot`, leitura do config, página do catálogo do Game Pass, `merge` e `drop_owned_elsewhere`, `read_product` da MS Store. | `#[cfg(test)]` de cada arquivo |

---

## 2. Lacunas para ficar completo

### L1. Configurações e tokens escritos ao mesmo tempo por várias tarefas — M

**Problema.** Várias tarefas em segundo plano fazem `settings::load` → `await` na rede → `settings::save` com a cópia
velha, e todas salvam pelo mesmo arquivo temporário `settings.json.tmp`. São elas: `epic_library`, `xbox_library`,
`achievements::sync_epic` (em `achievements/mod.rs`, perto da linha 417), `save_xbox_refresh`, `finish_*_login` e
qualquer comando de configuração. Isso traz três defeitos:

1. **Mudança perdida.** Em `achievements/mod.rs` o `load` acontece *antes* do `await` do token. Uma opção que o
   jogador mudou nesse intervalo, ou um refresh do Xbox renovado por outra tarefa, é sobrescrito.
2. **Conta que volta sozinha depois de Sair.** Se o jogador clica em Sair enquanto `epic_library` espera o token, a
   tarefa grava `epic_refresh = Some(...)` em cima da configuração já sem conta. `epic_user` fica `None`, mas
   `epic_library` só olha `epic_refresh`: os jogos da conta voltam para a Biblioteca enquanto o cartão diz "não
   conectado". O mesmo vale para o Xbox (`xbox_library` e `save_xbox_refresh`).
3. **Arquivo corrompido.** Duas escritas no mesmo `.tmp` podem se misturar. `settings::load` cai então no padrão e
   todas as escolhas e contas somem sem aviso.

**Como reproduzir.** Com uma rede lenta (por exemplo, um proxy com atraso de 5 s em `account-public-service-prod`),
abra a Biblioteca com o cache vencido e clique em Sair da Epic durante a espera. Depois do "catalog-changed", os jogos
da Epic voltam.

**Como resolver.** Criar um `settings::update(data, |s| ...)` que segura um `Mutex` global, relê o arquivo dentro da
trava, aplica a mudança e grava num temporário com nome único. Trocar todos os pares `load`/`save` das contas por ele.
A renovação de token grava com uma condição: só troca `epic_refresh` se o valor atual ainda for o que foi usado
(compare-and-swap). Se a conta saiu ou mudou, o token novo é descartado.

**Arquivos.** `src-tauri/src/settings.rs`, `main.rs` (`epic_library`, `xbox_library`, `finish_*_login`, `*_logout`),
`achievements/mod.rs` (`save_xbox_refresh`, `epic_access`/`sync_epic`).

**Aceite.** Sair durante uma renovação nunca reconecta a conta. 100 `update` simultâneos em threads não perdem nenhuma
chave e não corrompem o arquivo.

**Teste.** Unitário em `settings.rs` com 8 threads gravando chaves diferentes 50 vezes cada e todas presentes no fim.
Unitário do compare-and-swap: renovar com `refresh = A` quando o arquivo já tem `None` não grava nada.

### L2. Um único dono da sessão de cada conta — P/M

**Problema.** O token da Epic é renovado em dois lugares independentes: `epic_library` (direto, a cada hora) e
`achievements::epic_access` (com o cache `EPIC_SESSION` de 30 min). No Xbox também: `xbox_library` chama `sign_in`
direto, enquanto as conquistas usam `achievements::xbox::session`. Os dois disparam no mesmo `get_games` com o
*mesmo* refresh token. Além disso, `EPIC_SESSION` **não é chaveado pela conta**: se o jogador sai e entra com outra
conta Epic em menos de 30 min, as conquistas da conta nova são pedidas com o access token e o `account_id` da antiga
e gravadas no escopo da nova. O Xbox compara `from == refresh`, então não tem esse problema.

**Por que importa.** Se a Epic invalidar o refresh antigo quando emite um novo (**não verificado**; o Hydra trata essa
rotação explicitamente em `legendary-sync.ts` e `legendary-managed-runtime.ts`), uma das duas renovações falha e a
conta parece revogada.

**Como resolver.** Um módulo `accounts.rs` com `async fn epic_session()` e `xbox_session()` em *single flight*: uma
renovação por vez, atrás de um `tokio::sync::Mutex`, como o `refresh ??=` do Hydra em `achievement-source.ts`. O
access token fica guardado até o `expires_in` que a resposta informa, com a chave `(account_id, refresh usado)`.
Biblioteca e conquistas passam a usar só esse módulo. `*_logout` limpa o cache em memória.

**Arquivos.** Novo `src-tauri/src/accounts.rs`, `main.rs`, `achievements/mod.rs`, `achievements/xbox.rs`,
`epic_account.rs` (ler `expires_in` e `refresh_expires_at` em `TokenReply`).

**Aceite.** Um `get_games` com o cache vencido faz no máximo uma chamada a `oauth/token` da Epic e uma a
`oauth20_token.srf`. Depois de Sair e entrar com outra conta, nenhuma requisição usa o access token antigo.

**Teste.** Unitário do single flight com uma função de renovação falsa que conta chamadas: 5 pedidos simultâneos dão 1
chamada. Unitário: trocar a conta invalida o cache.

### L3. Token revogado ou vencido vira o estado "entre de novo" visível — M

**Problema.** `epic_account::token` e `xbox_account::sign_in` devolvem a mesma mensagem ("recusou o acesso: entre de
novo") para qualquer status que não seja 2xx: 400 `invalid_grant` (revogado), 429, 5xx e até manutenção. O erro só vai
para o histórico (`trail::note`). O cartão continua mostrando "Conectada como X", a Biblioteca mostra o cache velho
para sempre e cada abertura da Biblioteca tenta de novo, porque o cache continua vencido.

**Como reproduzir.** Revogue o acesso em epicgames.com › Conta › Aplicativos e contas (ou em
account.live.com/consent/Manage) e abra a Biblioteca: nada avisa. Desconectado da rede, a mesma coisa acontece, com
uma tentativa a cada `catalog-changed`.

**Como resolver.** Classificar o erro em `enum AccountError { Revoked, Offline, Busy(Retry-After), Other }`. Só
`invalid_grant`/401 conta como `Revoked`. Guardar `epic_state`/`xbox_state` (`ok | needs_login | failing`) com a data
da última sincronização boa e o último erro. Com `needs_login`, não tentar mais e mostrar no cartão "A Epic pediu para
entrar de novo" com o botão "Entrar". Com erro temporário, usar `crate::net::send`, que já tem backoff e
`Retry-After`; hoje os módulos de conta usam `client.post/get` direto e não têm nenhuma espera.

**Arquivos.** `epic_account.rs`, `xbox_account.rs`, `settings.rs` (estado), `shared/src/lib.rs` (`SettingsView`),
`ui/src/settings_page.rs`, `shared/locales/en.json`.

**Aceite.** Com o token revogado, o cartão mostra "entre de novo" em até 1 sincronização e nenhuma nova chamada de
token é feita até o jogador entrar. Desconectado da rede, o cartão diz "sem conexão, mostrando a lista de <data>" e
não há mais de 1 tentativa a cada 30 s.

**Teste.** Unitário da classificação com respostas fixas: 400 com `errorCode: errors.com.epicgames.account.auth_token.invalid_refresh_token`
dá `Revoked`; 429 com `Retry-After: 60` dá `Busy(60)`; 503 dá `Busy`. Teste manual com a revogação real.

### L4. A conta identificada pelo id, e Sair sem apagar conquistas — M

**Problema.**
- O escopo das conquistas e o cartão usam o **nome de exibição** (`epic_user`, `xbox_user` = gamertag). Os dois podem
  ser trocados pelo jogador. Depois de trocar o gamertag, o hub cria um escopo novo e o antigo fica órfão.
  `xbox_logout` apaga pelo nome atual e não acha o antigo.
- **Sair apaga as conquistas** da conta no `achievements.db` (`db.forget` em `xbox_logout`/`epic_logout`), o que
  contraria a promessa do hub de "nunca perder um desbloqueio".
- **Trocar de conta**: `epic-library.json`/`xbox-library.json` não dizem de qual conta são. Se a primeira busca da
  conta nova falhar, a Biblioteca mostra os jogos da conta antiga como se fossem da nova.
- **Duas contas** na mesma loja não são possíveis (os campos são únicos). Por enquanto basta deixar isso claro na
  interface.

**Como resolver.** Guardar `epic_account_id` (o `Session.account` já existe) e `xbox_xuid`. Usar os ids no escopo das
conquistas e como dono do cache (`Cache { owner, at, games }`): cache de outro dono é ignorado. Migrar os escopos
antigos pelo nome uma única vez. Em Sair, perguntar "Manter as conquistas desta conta no hub?" com Manter marcado por
padrão.

**Arquivos.** `settings.rs`, `epic_account.rs`, `xbox_account.rs`, `main.rs` (`finish_*_login`, `*_logout`),
`achievements/store.rs` (migração de escopo), `ui/src/settings_page.rs`.

**Aceite.** Trocar o gamertag não duplica nem perde conquistas. Sair e entrar de novo mantém o hub igual. Entrar com
outra conta nunca mostra os jogos da anterior, nem por um instante.

**Teste.** Unitário: `merge` com cache de outro dono não adiciona nada. Unitário da migração de escopo, de nome para
id, em `achievements/store.rs`.

### L5. Resposta parcial, limites de uso e custo de cada sincronização — M

**Problema.**
- **Epic, resposta parcial.** Em `epic_account::library`, um lote do catálogo que falha (`let Ok(..) else { continue }`)
  faz os jogos desse namespace sumirem, e a lista incompleta é gravada como completa por 1 h. Uma página do
  `library-service` vazia por erro do servidor apaga todos os não instalados. O limite de 50 páginas corta contas
  grandes sem avisar (quantos itens vêm por página: **não verificado**).
- **Epic, custo.** Os detalhes do catálogo são pedidos **toda hora** para **todos** os namespaces, em geral um por
  jogo: uma conta com 300 jogos grátis resgatados faz centenas de requisições por hora.
- **Xbox, custo.** `with_store_details` faz uma chamada ao `displaycatalog` por jogo, em sequência, toda hora (249 na
  conta de teste). Também não usa `net::send`, então ignora 429.
- **Xbox, lista.** O `titlehub` é lido sem paginação nem `maxItems` (se contas muito grandes são cortadas: **não
  verificado**).

**Como resolver.** Guardar os detalhes do catálogo por `item_id` (Epic) e por família (Xbox) num cache próprio de 30
dias, e consultar só o que é novo. Quando uma parte falha, manter as entradas anteriores dessas ids em vez de
removê-las. Gravar "completo" só quando todas as páginas e lotes responderam. Passar tudo por `crate::net::send`.
Avisar no histórico quando o limite de páginas for atingido.

**Arquivos.** `epic_account.rs`, `xbox_account.rs`, `net.rs` (sem mudança, só uso).

**Aceite.** A segunda sincronização de uma conta sem jogos novos faz no máximo 1 chamada de token, as páginas da
biblioteca e **zero** chamadas de catálogo. Um lote com 500 não remove nenhum jogo que já estava no cache.

**Teste.** Separar a leitura (JSON → `Vec<Owned>`) da busca, como já foi feito em `pc_games`, e testar com respostas
fixas: página com `nextCursor`, lote faltando e cache anterior preservado.

### L6. Tokens em texto puro e indo para o backup — P (backup) / M (cofre do Windows)

**Problema.**
- `data_backup.rs` tira do zip só `SECRETS = ["steam_key", "ra_key", "ra_token", "steamgriddb_key"]`. **`epic_refresh`
  e `xbox_refresh` vão no backup**, e quem recebe o zip ganha acesso às duas contas. Na restauração, `THIS_PC` também
  não protege esses campos: restaurar a cópia de outro PC troca as contas deste.
- Os refresh tokens ficam em texto puro em `settings.json`. O Hydra usa `safeStorage` (DPAPI no Windows) para o token
  do RetroAchievements (`retro-achievements-emulator-account.ts`).

**Como resolver.** Primeiro passo (P): incluir `epic_refresh`, `xbox_refresh`, `epic_user`, `xbox_user` e os futuros
ids em `SECRETS` e `THIS_PC`, com teste. Segundo passo (M): guardar os tokens no Gerenciador de Credenciais do Windows
(crate `keyring`, MIT/Apache) ou cifrados com DPAPI (`CryptProtectData`, crate `windows`), mantendo só um marcador em
`settings.json`. Migrar na primeira leitura.

**Arquivos.** `src-tauri/src/data_backup.rs`, `settings.rs`, `Cargo.toml` (cofre).

**Aceite.** O zip de backup não contém a string de nenhum refresh token. Restaurar não muda as contas conectadas.
Depois da migração, `settings.json` não tem tokens.

**Teste.** Em `data_backup.rs`, como o teste que já existe para `steam_key`: criar as configurações com
`epic_refresh = "SEGREDO"`, gerar o zip e procurar "SEGREDO" nos bytes.

### L7. Endurecer a janela de login — P

**Problema.**
- A passagem do código para `cerberus.invalid` é aceita **de qualquer página** carregada na janela. `on_navigation`
  deixa abrir qualquer `https`. Uma página de terceiro aberta ali (um link da ajuda da Epic, um provedor) poderia
  navegar para `https://cerberus.invalid/epic?code=<código do atacante>` e conectar o Cerberus à conta de outra
  pessoa (login CSRF). O risco é baixo, mas a correção é barata.
- O login da Microsoft não usa `state` nem PKCE.
- Os pop-ups dos provedores (`NewWindowResponse::Allow`) são janelas do WebView2 fora do `on_navigation`: abrem
  qualquer `https`, sem barra de endereço (**não verificado** se herdam o modo privado; o comentário no código diz
  que sim).
- `withGlobalTauri: true` injeta `window.__TAURI__` em todas as janelas. A IPC de origem remota deve ser recusada
  porque nenhuma capability cobre `epic-login`/`xbox-login` nem tem `remote.urls`, mas isso **não foi verificado
  rodando**.

**Como resolver.** Gerar um nonce aleatório por login, guardado no estado do app. O script do `on_page_load` monta
`cerberus.invalid/epic?state=<nonce>&code=…`, e `on_navigation` só aceita o par se o nonce bater e a página anterior
for `is_return`. Para a Microsoft, usar PKCE (`code_challenge`); se o client aceita: **não verificado**. Restringir
`on_navigation` e os pop-ups a uma lista de domínios (`*.epicgames.com`, `*.live.com`, `*.microsoft.com`,
`*.xboxlive.com`, `accounts.google.com`, `appleid.apple.com`, `*.xbox.com`). Fora da lista, abrir no navegador do
sistema.

**Arquivos.** `main.rs` (`epic_login`, `xbox_login`), `epic_account.rs`, `xbox_account.rs`.

**Aceite.** Uma navegação para `cerberus.invalid` sem o nonce certo é ignorada. Um link para um domínio fora da lista
abre no navegador. Num build de debug, `window.__TAURI__.core.invoke('epic_logout')` dentro da janela de login falha.

**Teste.** Unitário da validação de nonce e da lista de domínios (função pura sobre `Url`). Teste manual do `invoke`
pela devtools no build de debug.

### L8. Cartões de conta completos: carregando, erro, sincronizado há, atualizar agora — P/M

**Problema.**
- Não existe "Atualizar agora" nem "sincronizado há X". O cache tem `at`, mas ele não chega à interface.
- O botão "Entrar" não mostra que a janela está aberta nem um "Conectando…" enquanto o código é trocado. Fechar a
  janela de login não dá retorno.
- `finish_epic_login` emite `accounts-changed`, mas **nenhum código da UI escuta esse evento**, e `finish_xbox_login`
  nem o emite. O cartão depende do foco da janela para se atualizar.
- Textos desatualizados: o `EpicCard` diz "conquistas da Epic ainda não", mas elas já existem desde 25/11.
- No `XboxCard`, `pass` conta **todos** os Xbox não instalados (conta e Game Pass juntos) e o texto diz que são "do
  Game Pass".
- Sair não pede confirmação (e apaga conquistas, ver L4).

**Como resolver.** Comando `account_status() -> { epic: {user, synced_at, state, games}, xbox: {...} }`, uma linha
"Sincronizado há 12 min · 143 jogos" e o botão "Atualizar agora", que chama `refresh_accounts(force)` (o mesmo
caminho de `epic_library`/`xbox_library`, ignorando `stale`). A UI escuta `accounts-changed` em `api.rs`, ao lado de
`catalog-changed`, e os dois `finish_*` emitem esse evento. Contar Game Pass e conta separadamente.

**Arquivos.** `main.rs`, `shared/src/lib.rs`, `ui/src/api.rs`, `ui/src/settings_page.rs`, `shared/locales/en.json`.

**Aceite.** Depois de entrar, o cartão muda sem precisar trocar o foco da janela. "Atualizar agora" mostra
"Sincronizando…" e depois a hora nova. Os números do cartão batem com o filtro Xbox da Biblioteca.

**Teste.** Teste da UI (se houver `wasm-bindgen-test`) ou checagem manual com captura de tela, seguindo a memória de
verificar com screenshots.

### L9. Xbox: "jogos da conta" vem do histórico, não da posse — M/G

**Problema.** `titlehistory` lista o que a conta **jogou**, não o que ela **possui**:
- Um jogo do Game Pass jogado no passado, com a assinatura já vencida ou o jogo já fora do catálogo, aparece como
  "Xbox" (comprado) não instalado. Ao instalar, o app Xbox pede para comprar.
- Um jogo comprado e nunca aberto não aparece.
- Demos, betas e testes técnicos com `type: Game` entram na lista (**não verificado** com contas reais).

**Como resolver.** Primeiro passo (P): chamar esses itens de "Jogados nesta conta" e não de "comprados". Quando a
família também estiver no catálogo atual do Game Pass, mostrar o selo Game Pass. Filtrar nomes com "Demo", "Beta",
"Test" e "Trial" (lista configurável). Depois (G): procurar um endpoint de posse (entitlements/collections da
Microsoft Store). Se ele existe e é estável para o client do Xbox: **não verificado**. O Playnite não está no
checkout local (a extensão de Xbox fica no repositório PlayniteExtensions) e não foi comparado.

**Arquivos.** `xbox_account.rs` (`pc_games`, `merge`), `ui/src/tiles.rs`, `ui/src/settings_page.rs`.

**Aceite.** Um jogo do Game Pass fora do catálogo nunca aparece com o selo "Xbox" (comprado). Demos não aparecem por
padrão.

**Teste.** Unitário de `pc_games` com "Halo Infinite (Beta)" e "Forza Demo" filtrados. Unitário de `merge` com uma
família que está no Game Pass ganhando `game_pass: true`.

### L10. Xbox instalado: pacotes registrados de verdade — M

**Problema.** Só valem as pastas de `.GamingRoot`. Ficam de fora:
- jogos da Microsoft Store instalados em `WindowsApps` (os mais antigos, ou quem mudou a pasta padrão);
- uma pasta que sobrou depois de desinstalar, ou um pacote desregistrado, que aparece como **instalado** e falha ao
  abrir.

Além disso, ler `X:\.GamingRoot` de C a Z a cada listagem pode travar por segundos numa unidade de rede mapeada e
desconectada.

**Como resolver.** Enumerar os pacotes do usuário com `Windows.Management.Deployment.PackageManager`
(`FindPackagesForUser`), como o Playnite faz em `Programs.GetUWPApps` (`Playnite/Common/Programs2.cs`, MIT). Uma pasta
do `.GamingRoot` só conta se a família estiver registrada. Pular unidades que não são locais (`GetDriveTypeW` diferente
de `DRIVE_FIXED`/`DRIVE_REMOVABLE`).

**Arquivos.** `src-tauri/src/xbox.rs`, `Cargo.toml` (feature `Management_Deployment` da crate `windows`).

**Aceite.** Desinstalar um jogo pelo app Xbox e voltar ao Cerberus muda o jogo para não instalado (ou o remove),
mesmo que a pasta fique. Uma unidade de rede desconectada não atrasa a Biblioteca.

**Teste.** Unitário da regra "pasta sem pacote registrado não é instalado", com o enumerador injetado. Teste manual
com um jogo do Game Pass.

### L11. Epic: plataforma, demos e limite de páginas — M

**Problema.** A biblioteca não filtra a plataforma. Jogos só para Mac ou para celular, resgatados na loja móvel da
Epic, entram como "Instalar" e o launcher não consegue instalar no Windows. O Hydra usa os assets de Windows
(`asset_infos.Windows` em `services/epic/library-normalizer.ts`). Demos e protótipos com `games` na categoria também
passam (**não verificado** quantos são).

**Como resolver.** Cruzar com a lista de assets de Windows da Epic (o endpoint de assets do launcher, usado pelo Hydra
e pelo Legendary; o Legendary é GPL, então só o protocolo, sem copiar código) ou com `releaseInfo[].platform` do item
do catálogo, que contém `Windows` (**não verificado**). Ver também L5 (páginas).

**Arquivos.** `epic_account.rs` (`Item`, `is_game`).

**Aceite.** Nenhum jogo sem versão de Windows aparece como "Instalar".

**Teste.** Unitário de `is_game` com um item só de Mac e outro só de iOS, ambos fora.

### L12. Game Pass: padrão, mercado e selos — P/M

**Problema.**
- `game_pass` vem **ligado por padrão** (`#[serde(default = "yes")]`). Quem não assina, ou nem tem o app Xbox, vê
  centenas de jogos "não instalados" que não pode jogar.
- **Mercado pelo idioma da interface** (`gamepass::market`, e o `country` da Epic em `epic_account::library`):
  português dá `BR` e o resto dá `US`. Quem mora em Portugal com a interface em português recebe o catálogo do Brasil.
  Quem mora no Brasil com a interface em inglês recebe o dos EUA, com jogos que não existem aqui.
- `by_family.get(family.as_str())` compara com maiúsculas e minúsculas e `here` compara sem: uma família com
  capitalização diferente fica sem arte e sem o selo (**não verificado** se acontece).
- Um jogo comprado que também está no Game Pass recebe o selo "Game Pass" (o `merge` do Game Pass roda antes do da
  conta) e não diz que é seu.
- Um jogo sem `PackageFamilyName` recebe o id `xbox:<produto>`. Quando é instalado, vira `xbox:<família>` e perde
  favorito, coleção, nota e situação.

**Como resolver.** Padrão ligado só quando o app Xbox está instalado. Na primeira vez, perguntar "Você assina o PC
Game Pass?". Mercado pela região do Windows (`GetUserDefaultGeoName`), com opção manual em Configurações. Comparar
famílias sempre em minúsculas. Dois selos quando há posse e Game Pass. Usar como id um mapeamento estável de produto
para família, ou migrar favoritos, coleções e notas do id antigo para o novo quando o jogo é instalado.

**Arquivos.** `settings.rs`, `gamepass.rs`, `epic_account.rs`, `xbox_account.rs`, `ui/src/tiles.rs`,
`ui/src/first_run.rs`, `collections.rs`/`favorites.rs`/`game_notes.rs` (migração de id).

**Aceite.** Uma instalação nova, sem o app Xbox, não lista jogos do Game Pass. Com a região do Windows em Portugal, o
pedido usa `market=PT`. Instalar um jogo do Game Pass mantém o favorito.

**Teste.** Unitário de `market()` com a região injetada. Unitário de `merge` com família de capitalização diferente.
Unitário da migração de id.

### L13. Mesmo jogo, nomes diferentes (e nomes iguais, jogos diferentes) — M

**Problema.** Toda a deduplicação compara o nome "plano" (`plain`/`normalize`, em quatro cópias: `gamepass.rs`,
`xbox_account.rs`, `ms_store.rs`, `steam_store.rs`):
- **Falsos negativos.** "Forza Horizon 5" e "Forza Horizon 5 Standard Edition", "Halo: The Master Chief Collection" e
  "Halo MCC", "DOOM Eternal" e "DOOM Eternal (PC)", "… for Windows" e "Windows Edition", "Game of the Year Edition",
  "Deluxe": o jogo aparece duas vezes.
- **Falsos positivos.** `drop_owned_elsewhere` compara com **todos** os jogos que não são catálogo puro, inclusive ROMs,
  recomps e jogos adicionados à mão. Um recomp ou uma ROM com o mesmo nome esconde a entrada do Game Pass. Na página
  da loja, `steam_store::app_for` aceita o primeiro app da Steam com o nome exato: "Prey" (2017 contra 2006) e "DOOM"
  (2016 contra 1993) podem mostrar a página de outro jogo.

**Como resolver.** Um único `names::key(&str)` em `shared` que tira sinais de marca, sufixos de edição conhecidos
("Standard/Deluxe/Ultimate/Gold/GOTY/Definitive/Complete Edition", "for Windows", "(PC)", "Windows 10") e converte
algarismos romanos. Somar a isso um `aliases.json` pequeno e versionado para os casos famosos. `drop_owned_elsewhere`
compara só com jogos de loja (Steam, Epic, Xbox comprado). Na página da loja de um jogo Xbox **com produto
conhecido**, preferir a Microsoft Store e só cair na Steam quando ela confirmar o desenvolvedor (o `appdetails` traz
`developers`). Isso conversa com "O mesmo jogo na Steam e na Epic aparece como um só" da 6.2: fazer a chave uma vez só.

**Arquivos.** Novo `shared/src/names.rs`, `gamepass.rs`, `xbox_account.rs`, `ms_store.rs`, `steam_store.rs`,
`main.rs` (`store_page`).

**Aceite.** "Forza Horizon 5 Standard Edition" do Game Pass some quando a Steam tem "Forza Horizon 5". Uma ROM "Halo"
não esconde nada do Game Pass. Um jogo Xbox com `product` abre a página da MS Store dele.

**Teste.** Tabela de casos para `names::key` (cerca de 30 pares iguais e diferentes, incluindo "Hotline Miami" e
"Hotline Miami 2"). Unitário de `drop_owned_elsewhere` com uma ROM homônima.

### L14. Página da loja: vazio, fonte e "abrir na loja" — P

**Problema.** Quando `store_page` não acha nada, o resultado é `Err("")` e a Visão geral fica sem conteúdo nenhum. Não
aparece "não achamos a página na loja", nem a fonte ("dados da Steam" ou "da Microsoft Store"), nem um botão para
abrir a loja. Para um jogo da Epic, a página da Steam aparece sem dizer que é da Steam (com requisitos e idiomas da
versão Steam).

**Como resolver.** Devolver `StoreDetails` com `origin: "steam" | "msstore"` e mostrar uma linha "Informações da loja
Steam". No estado vazio, mostrar um cartão "Sem página da loja para este jogo" com "Abrir na Epic"
(`com.epicgames.launcher://store/`; o caminho por jogo exige o slug, **não verificado**) ou "Abrir na Microsoft Store"
(`ms-windows-store://pdp/?productid=<id>`).

**Arquivos.** `shared/src/lib.rs`, `main.rs`, `ms_store.rs`, `ui/src/game.rs` (`StoreOverview`),
`ui/src/game_overview.rs`.

**Aceite.** Todo jogo Epic ou Xbox mostra conteúdo da loja ou um cartão vazio com ação. A fonte aparece sempre.

**Teste.** Unitário de que `read_product` define `origin`. Checagem manual com um jogo Epic que não está na Steam nem
na MS Store.

### L15. Big Picture e Início coerentes com a Biblioteca — M

**Problema.**
- `ui/src/tv/pages.rs` (`library_games`) **tira os não instalados**: no Big Picture não dá para ver nem instalar os
  jogos da conta Epic/Xbox nem os do Game Pass.
- `ui/src/tv/game.rs` busca página da loja e conquistas **só da Steam** (`api::steam_details`,
  `game_achievements("steam", …)`). Jogos Epic e Xbox ficam sem descrição e sem conquistas na TV, mas têm as duas
  coisas no desktop.
- O Big Picture não mostra o selo de origem.
- O Início (`ui/src/home.rs`) também tira os não instalados, o que é razoável. Só deve ficar escrito que é decisão.

**Como resolver.** No Big Picture, uma prateleira ou filtro "Não instalados" com ação "Instalar pela <loja>". Usar
`api::store_page` e as conquistas por fonte (o mesmo seletor do desktop em `game.rs`, linha ~191). Selo pequeno no
`Poster`.

**Arquivos.** `ui/src/tv/pages.rs`, `ui/src/tv/game.rs`, `ui/src/tiles.rs` (exportar `source_marks` para a TV).

**Aceite.** Um jogo da conta Epic não instalado aparece no Big Picture e instala pelo controle. Um jogo Xbox mostra
descrição e conquistas na TV iguais às do desktop.

**Teste.** Checagem manual com controle e capturas de tela das duas telas lado a lado.

### L16. Instalar, desinstalar e verificar pela loja, com o launcher ausente — P/M

Complementa a 7.2 ("Desinstalar, verificar e mover…") com o que a revisão achou:
- **Launcher da Epic ausente.** "Instalar" abre `com.epicgames.launcher://…` mesmo sem a Epic instalada, e o Windows
  mostra "procurar um app para abrir este link". `epic::launcher_found()` já existe: usar antes e oferecer o link de
  download da Epic.
- **Xbox sem produto.** Quando `with_store_details` falha, `product` fica vazio e "Instalar" abre
  `msxbox://game/?productId=` sem jogo. Esconder o botão ou abrir a busca do app Xbox pelo nome.
- **Desinstalar.** Steam: `steam://uninstall/<id>`. Xbox: não há URI de desinstalação; abrir `ms-settings:appsfeatures`
  ou a página do jogo no app Xbox. Epic: `?action=uninstall` e `?action=verify` no URI do launcher (**não
  verificado**). Hoje `remove_game` só vale para recomps e jogos adicionados; para jogos de loja só existe "ocultar".
- **Instalado e depois desinstalado.** A Epic e o Xbox voltam a "Instalar" sozinhos, porque a lista é refeita a cada
  leitura. Falta testar o caso da Epic quando o `installations.jsonl` some mas o `.item` fica.

**Arquivos.** `main.rs` (`play`, novo `store_action(game, "uninstall" | "verify")`), `ui/src/game.rs` (menu
Gerenciar), `src-tauri/src/epic.rs`.

**Aceite.** Sem a Epic instalada, "Instalar" mostra "Instale a Epic Games Launcher" com um link. Nenhum botão abre um
URI com id vazio. Cada loja tem "Desinstalar" no Gerenciar, ou uma explicação de onde fazer.

**Teste.** Unitário da URL por fonte e estado (função pura extraída de `play`): `product` vazio não dá URL.

### L17. Testes que faltam (resumo)

Além dos testes citados em cada item acima:
- `epic_account::library` e `xbox_account::with_store_details` separados em "buscar" e "ler", com JSON gravado de
  verdade e anonimizado em `src-tauri/tests/fixtures/` (página com cursor, lote com erro, item de DLC, item sem
  imagem).
- `ms_store::find` com resultados de autosuggest (Game contra Durable/DLC, título com ®).
- `gamepass::refresh` com um lote que falha: o cache antigo continua.
- `merge` dos três módulos na **ordem real** de `get_games` (Game Pass → Epic → Xbox → `drop_owned_elsewhere`) com
  um caso misto: comprado no Xbox e no Game Pass, na Epic e no Game Pass, ROM homônima.
- `epic_login`/`xbox_login`: teste da regra de navegação como função pura (`fn allow(url) -> Decision`).
- Nenhum `eprintln!`/`trail::note` com token: um teste que roda os erros de `token`/`sign_in` com um token falso
  "TOKEN123" e confere que a mensagem não o contém.

Esforço total dos testes: M, diluído nos itens.

---

## 3. Riscos e limites

- **APIs não oficiais.** O login da Epic usa o client público do próprio Epic Games Launcher (o mesmo de Legendary,
  Heroic e Hydra). O da Microsoft usa o client do app Xbox (o mesmo do Playnite). `library-service`, `catalog`,
  `titlehub`, `displaycatalog` e `catalog.gamepass.com` não são documentados para terceiros. Podem mudar ou bloquear
  sem aviso, e os termos de uso das lojas não autorizam esse uso de forma explícita (**não verificado** juridicamente).
  Mitigação: cache de longa duração, falhar mostrando o último estado bom (L3, L5) e nunca depender disso para abrir
  um jogo instalado.
- **Limites de uso.** Não há números publicados. O Xbox Live devolve 429 (já tratado nas conquistas), e a Epic tem
  limites por IP e por conta (os valores exatos: **não verificado**; o Hydra tem `EPIC_RATE_LIMIT_ATTEMPTS` e backoff
  em `source-retry.ts`). Toda chamada de conta deve passar por `net::send`.
- **Duração dos tokens.** A Epic informa `expires_in` e `refresh_expires_at` na resposta (o Hydra guarda os dois). O
  Cerberus ignora. Quanto o refresh da Epic dura sem uso e se ele gira a cada uso: **não verificado**. Para contas
  pessoais da Microsoft, a documentação fala em refresh token de longa duração renovado a cada uso (**não verificado**
  para esse client). Uma conta parada por meses vai pedir login de novo: L3 precisa mostrar isso direito.
- **Sair não revoga no servidor.** A Epic tem um endpoint para encerrar a sessão (`/account/api/oauth/sessions/kill/…`,
  **não verificado** para o refresh token). Hoje Sair só apaga localmente. Vale chamar o endpoint quando existir e
  dizer na interface que o acesso também pode ser revogado no site da loja.
- **Licenças.** Hydra (MIT) e Playnite (MIT) podem ser adaptados com crédito, por exemplo `Programs.GetUWPApps` do
  Playnite e a classificação de erro e o single flight do Hydra. Legendary (GPL-3), Heroic (GPL-3) e RecompLauncher
  (GPL): só ideias e conhecimento de protocolo, nunca trechos de código. As imagens das lojas são carregadas das CDNs
  delas (`store-images.s-microsoft.com`, `cdn1.epicgames.com`): não redistribuir em pacote.
- **Nunca pirataria.** Nada aqui toca em emuladores de Steam, cracks ou fontes de download. Instalar continua sendo
  sempre pela loja oficial.
- **Privacidade.** O nome da conta e o gamertag aparecem no histórico (`trail`). Se o histórico for exportado para
  suporte, isso precisa ser dito ou anonimizado.

---

## 4. Ordem sugerida

1. **L6 primeiro passo** (tokens fora do backup): P, é segurança e corrige um vazamento real.
2. **L1 + L2** (escrita das configurações e dono único da sessão): corrigem a conta que volta sozinha, as conquistas
   na conta errada e o arquivo corrompido. É a base para o resto.
3. **L3** (revogado ou offline visível, com backoff) e **L8** (cartões: sincronizado há, atualizar agora, eventos,
   textos).
4. **L4** (conta pelo id, Sair sem apagar conquistas, cache com dono).
5. **L5** (resposta parcial e custo das sincronizações) e **L11** (plataforma Windows na Epic).
6. **L12** (padrão do Game Pass, mercado pela região, ids estáveis) e **L9** primeiro passo (rótulo "jogados", demos).
7. **L7** (nonce e lista de domínios no login).
8. **L13** (chave de nome única, que também serve à 6.2) e **L14** (estado vazio da página da loja).
9. **L15** (Big Picture) e **L16** (launcher ausente, produto vazio, desinstalar/verificar, junto com a 7.2).
10. **L10** (PackageManager), **L6 segundo passo** (cofre do Windows) e **L9 segundo passo** (posse real no Xbox), à
    medida que os dados confirmarem a necessidade.
