# Novidades

## 0.11.0 — conquistas, Novidades e do seu jeito

### Idiomas

- O Cerberus agora fala **inglês** e **português do Brasil**: segue o idioma do Windows, ou você escolhe na primeira tela e em **Configurações → Aparência → Idioma**.
- Mensagens, erros, histórico e o catálogo aparecem no idioma escolhido, com datas no formato de cada um.
- Documentação completa em `docs/` e README em inglês e português.

### Início, menu lateral e Novidades

- Banner com o logo do jogo, sua última sessão, tempo total e conquistas; botão **Jogar** na própria capa; fileira de Favoritos.
- Menu lateral com **Prontos para jogar**: os jogos que dá para abrir agora, com filtro. Embaixo, **+** (adicionar jogo), Big Picture e Configurações.
- **Novidades** mostra os anúncios dos desenvolvedores dos seus jogos da Steam, além das atualizações e da atividade recente.

### Biblioteca e Descobrir

- Jogos da sua conta Steam que não estão instalados, com **Instalar**.
- Situação, nota e anotações de cada jogo, e jogo aleatório.
- Barra de conquistas na capa; a 100% o jogo vira **Platinado**.
- Descobrir com filtro por gênero e pelo que o recomp traz (controle, conquistas, multijogador, tela larga, mods).

### Conquistas

- Hub com Steam e RetroAchievements: platinados, perto dos 100%, últimas desbloqueadas e a lista de cada jogo com raridade.
- Aviso por cima do jogo quando uma conquista cai, no canto que você escolher.
- Chave opcional da Web API da Steam para a conta inteira.

### Página do jogo, Downloads e Saves

- Dados da loja da Steam, galeria com trailers, gráfico de jogatina e coluna lateral.
- Downloads em seções, limite de velocidade e mais devagar enquanto você joga.
- Saves com status de cada jogo, nuvem e histórico na própria página.

### Big Picture Mode

- Página de jogo completa, filtros na Biblioteca, gêneros no Descobrir e aba de Conquistas com a lista de cada jogo.
- O jogo aberto aparece no topo, e os avisos aparecem no tamanho da TV.

### Configurações

- Busca: digite parte do nome e vá direto para a opção.
- **Aparência:** tema Escuro ou Preto (OLED), cinco cores de destaque e tamanho da janela (90% a 125%).
- **Durante o jogo**, **Notificações**, **Controles** e **Epic Games**.
- **Armazenamento:** copiar os seus dados num `.zip` (sem chaves nem senhas) e restaurar de uma cópia.
- Configuração inicial começa pelo idioma e termina mostrando o que o Cerberus achou no PC.
- O próprio Cerberus avisa quando tem versão nova.

## 0.10.0 — primeira versão para Windows

Primeira versão com instalador. Baixe `Cerberus-Launcher_0.10.0_x64-setup.exe` em Assets e instale.

### Instalação

- Instalador em português. Instala só para o seu usuário, sem pedir administrador.
- Cria atalho no Menu Iniciar e, se você quiser, na área de trabalho.
- Pode abrir o Cerberus ao terminar. Na primeira abertura aparece a configuração guiada: como você joga, pastas de ROMs, onde instalar e saves na nuvem.
- Instala o WebView2 da Microsoft se o PC não tiver.
- O Windows pode mostrar o aviso do SmartScreen: o instalador ainda não é assinado.
  Clique em "Mais informações" e depois em "Executar assim mesmo".
- Ao desinstalar, seus dados ficam, a menos que você marque para apagar. Os dados são a biblioteca, os saves, os backups e os recomps baixados.

### Biblioteca

- Jogos da Steam e da Epic aparecem sozinhos, e você pode adicionar qualquer `.exe`.
- Abas Todos, Favoritos, Coleções, Recomps, PC e Clássicos, e jogos ocultos.
- Arte de qualquer jogo pelo SteamGridDB ou de um arquivo seu.
- Tempo de jogo, última vez jogado e histórico de cada jogo, inclusive dos jogos da Steam e da Epic.
- Botão direito numa capa abre um menu com jogar, favoritar, coleções, ocultar e mais.

### Recomps

- Catálogo próprio com 117 jogos, mais o catálogo do recomplauncher.com, que é opcional e vem ligado.
- Os recomps vêm direto do GitHub, GitLab ou Forgejo de cada projeto, com conferência do arquivo.
- O download pausa e continua.
- Atualizações com backup dos saves antes.
- Ações na aba Gerenciar: reparar a instalação, mover para outro disco ou trazer recomps que você já tinha instalado.
- Easy Setup entrega a sua cópia do jogo ao recomp. Funciona com pasta, ISO, imagem de Xbox e Xbox 360, e agora também CHD e CSO.
- Mods do Thunderstore e do GameBanana, inclusive Unleashed e Marathon Recompiled.

### Saves

- Backup depois de cada partida, antes de atualizar e antes de desinstalar. Os dez backups mais recentes ficam guardados.
- Saves na nuvem por uma pasta sincronizada (OneDrive, Google Drive, Dropbox).

### Durante o jogo

- **F12** tira foto só da janela do jogo. As fotos ficam na aba Fotos da página do jogo.
- **Ctrl+F12**, ou **View+Menu** no controle, abre um menu por cima do jogo. Ele mostra o tempo de jogo e tem tirar foto, abrir no Cerberus e fechar o jogo.
- Filtros visuais (Nítido, Cores vivas, CRT, Filme) pelo ReShade.
- Jogo que fecha com erro tem o motivo explicado: DLL faltando, Visual C++ antigo ou travamento.

### Clássicos

- Suas ROMs pelo RetroArch, que o Cerberus instala numa pasta própria. Funciona com N64, PlayStation 1 e 2, PSP, GameCube, Wii e mais.
- Jogos com vários discos aparecem como um jogo só, e a troca de disco fica no menu do RetroArch.
- Conquistas do RetroAchievements com aviso na hora. A senha nunca fica guardada.

### Big Picture Mode

- Tela de TV com controle, teclado ou mouse: abas, página do jogo, downloads e configurações.
- Abre no monitor que você escolher.

### Mais

- Adicionar jogos à Steam e criar atalho na área de trabalho.
- Ver e trocar a versão do DLSS de um jogo com uma DLL que você já tem, com volta à original.
- Configurações › Seu PC confere o Visual C++ e o DirectX antigo.
- Bandeja do Windows, abrir com o Windows e avisos do Windows.
