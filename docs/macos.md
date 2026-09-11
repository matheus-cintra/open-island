# macOS (experimental)

Open Island tem uma implementação para macOS 12 ou superior, Apple Silicon e Intel.
O suporte permanece **experimental até a validação em um Mac real**. Compilar no CI não
comprova hover, teclado, Spaces, tela cheia, suspensão, login ou permissões de Automação.

## Instalação e atualização

Baixe `open-island-macos-aarch64.dmg` (Apple Silicon) ou
`open-island-macos-x86_64.dmg` (Intel) da release. Confira `SHA256SUMS`, abra o DMG e
arraste Open Island para Aplicativos antes da primeira execução. O instalador `install.sh`
abre o download correspondente no Darwin.

O novo fluxo de atualização pelo aplicativo está em validação: ao clicar no botão,
ele consulta `latest.json`, verifica a assinatura do arquivo `.app.tar.gz`, valida
identidade, versão, arquitetura e sons do bundle e faz a troca atômica. O daemon
é reiniciado e sua versão é confirmada antes do reinício da interface. Se essa etapa
falhar, o pacote anterior é restaurado. Aprovações e perguntas pendentes precisam ser
respondidas antes. A instalação exige escrita na pasta do aplicativo; se não houver
permissão, use o DMG manualmente. Releases antigas sem o índice assinado continuam
disponíveis pelo DMG. Não há instalação silenciosa em segundo plano.

Os pacotes têm assinatura ad hoc e **não são notarizados**. O Gatekeeper pode impedir a
primeira abertura. Siga a autorização manual em Ajustes do Sistema → Privacidade e
Segurança → Abrir Mesmo Assim, apenas para o pacote que você verificou. Não desative o
Gatekeeper globalmente. Veja [a distribuição macOS do Tauri](https://v2.tauri.app/distribute/sign/macos/)
e [a orientação da Apple para abrir apps](https://support.apple.com/pt-br/102445).

O ícone na barra de menus abre Ajustes, alterna a ilha e encerra o aplicativo. Não há
ícone no Dock. O atalho inicial é `Command+Shift+I`; em Ajustes → Geral, altere-o ou
esvazie o campo para desativá-lo. Falhas de registro e conflitos aparecem nesse controle.

## Comportamento e limites

- O monitor principal é usado por padrão. A seleção em Ajustes é preservada.
- O painel usa coordenadas lógicas do AppKit, áreas seguras e
  [áreas auxiliares da tela](https://developer.apple.com/documentation/appkit/nsscreen/auxiliarytopleftarea-uglc).
  O painel começa no topo da tela. Texto e controles ocupam duas áreas laterais
  à câmera; somente os cartões expandidos ficam abaixo dela.
- O painel não ativa o aplicativo no hover. Entrada de perguntas habilita o teclado.
- A descoberta usa `libproc` e `sysctl`; só as variáveis de terminal permitidas entram
  nas sessões. Falhas de inspeção não removem um hook cujo processo ainda existe.
- Terminal.app e iTerm2 são reconhecidos. O foco por PID ativa o aplicativo; painéis
  identificáveis continuam usando os resolvers CLI existentes. Não há garantia de aba
  ou janela exata no fallback.
- Em Ajustes → Geral, escolha Terminal.app, iTerm2, Warp, WezTerm ou Kitty para novas
  sessões. Terminal é o padrão. Terminal e iTerm2 podem exigir autorização de Automação.
- Foco/Não Perturbe usa autorização explícita nas configurações de som. Estado indisponível
  não é tratado como Foco desligado. Telas em repouso são consultadas via CoreGraphics;
  bloqueio com a tela acesa e troca de usuário usam a sessão do WindowServer. O campo
  de bloqueio não é documentado pela Apple e esta detecção é experimental. A detecção de tela cheia considera
  o aplicativo ativo. Esses recursos ainda exigem validação física no Mac.
- WAVs originais acompanham o app e são reproduzidos pelo player nativo `afplay`.

## Dados e login

Configuração, dados e estado ficam nas subpastas `config`, `data` e `state` de
`~/Library/Application Support/Open Island`. `OPEN_ISLAND_CONFIG`, `OPEN_ISLAND_SOCKET`
e os overrides XDG existentes têm precedência. O socket padrão usa
`/tmp/open-island-<uid>/island.sock`, em diretório 0700 validado; o socket é 0600.

A integração de início no login escreve somente os LaunchAgents gerenciados:
`~/Library/LaunchAgents/app.open-island.daemon.plist` e `app.open-island.panel.plist`.
Os caminhos apontam para os binários dentro do app; movê-lo exige reinstalar a integração.
Os arquivos alheios não são substituídos nem removidos. Remover a configuração automática
remove hooks e LaunchAgents. Os dados do usuário são preservados. `install.sh --uninstall`
remove as integrações e orienta mover o aplicativo para a Lixeira.

## Build e verificação

No Mac, instale Rust, Bun e as ferramentas do Xcode. Execute em `app`:

```sh
bun install --frozen-lockfile
bun run tauri build --target aarch64-apple-darwin --bundles app,dmg
# Em Intel, use --target x86_64-apple-darwin.
python3 ../scripts/test-processes.py cargo test --workspace
bun run test
```

O prebundle compila o daemon com o mesmo target de Tauri. O CI executa testes e builds
nas duas arquiteturas e verifica binários, assinaturas e WAVs. Um único job publica todos
os artefatos e checksums. `scripts/test-processes.py` isola o grupo de processos e grava
logs em arquivo; não encadeie `cargo test` com pipes.

## Validação manual pendente

- [ ] macOS 12 e uma versão atual, Intel e Apple Silicon.
- [ ] Tela com notch e sem notch; Retina e escala; monitor externo à esquerda/acima.
- [ ] Hover sem roubar foco; perguntas com texto livre e retorno do foco ao sair.
- [ ] Aprovações/perguntas: responder, timeout, desconexão e reinício do daemon.
- [ ] Spaces, fullscreen, suspensão/retorno, troca de monitor e resolução.
- [ ] Terminal, iTerm2 e hosts CLI instalados; foco de aplicativo e de painel disponível.
- [ ] Abrir sessão com espaços, aspas e caracteres Unicode no caminho; recusar Automação.
- [ ] Atalho padrão, troca, conflito, desativação e restauração após reinício.
- [ ] Login, abrir duas vezes, sair, reinstalar e remover LaunchAgents/hooks.
- [ ] Volume, mute, horários silenciosos e sons em ambos os DMGs.

A execução local em Linux não substitui esses checks. Não remova o rótulo experimental
antes de registrar os resultados neste checklist.

## Verificação executada nesta implementação

Em Linux, em 11/09/2026: 519 testes Rust e 146 testes do frontend aprovados; build
TypeScript/Vite e bundles DEB/RPM concluídos. O daemon extraído do DEB corresponde ao
sidecar compilado. Os cinco WAVs foram conferidos como PCM mono de 16 bits a 44.1 kHz.
O executor de testes não detectou grupos de processos sobreviventes.

Em 11/09/2026, os [builds nativos no GitHub Actions](https://github.com/matheus-cintra/open-island/actions/runs/34606329574)
geraram DMGs Apple Silicon e Intel. Ambos passaram na verificação de arquitetura do
aplicativo/daemon, assinatura ad hoc, presença dos cinco WAVs e versão mínima 12.0.
Os checksums dos downloads foram conferidos localmente. Os 48 testes Rust da interface
passaram no Apple Silicon. Após corrigir colisões de nomes temporários nos testes,
os [448 testes de core, daemon e integrações](https://github.com/matheus-cintra/open-island/actions/runs/34607537988)
passaram nas duas arquiteturas, sem grupos de processos sobreviventes.

A validação visual/interativa do checklist continua pendente; os DMGs permanecem
experimentais e sem notarização.

### Integração visual com o notch

A geometria usa a altura total do painel; o backend não soma a altura da câmera.
O fundo preto começa na borda superior em ambos os estados. No modo recolhido,
a câmera fica entre o projeto à esquerda e a contagem à direita. O cabeçalho
expandido usa a mesma área de exclusão; cartões e perguntas crescem abaixo dele.
A escala da interface amplia as áreas laterais sem alterar a medida física do recorte.
Telas sem notch preservam o posicionamento abaixo da barra de menus.

A composição segue as [referências fornecidas pelo autor](https://github.com/matheus-cintra/open-island-private/blob/master/images/reference-island/README.md).
Foram verificadas em Chromium as posições reais dos elementos ao expandir/recolher,
com notch, escala de 150%, modo limpo e sem notch. O campo de mensagem fica oculto
em hosts sem suporte; mensagens enfileiradas continuam disponíveis para cancelamento.
A confirmação visual no Mac físico continua pendente.

O [build Apple Silicon da integração](https://github.com/matheus-cintra/open-island/actions/runs/34612912278)
concluiu com sucesso, incluindo verificação do bundle e testes Rust no macOS.
Os 151 testes do frontend passaram localmente; o checksum do DMG baixado foi conferido.

### Densidade dos ajustes no macOS

Após a confirmação do autor de que a integração com o notch funciona, as áreas laterais
foram reduzidas: 112 pontos no modo limpo e 208 no detalhado, somados à largura da câmera.
A geometria continua respeitando a escala e a exclusão física do notch.

As 42 imagens da pasta de referências foram revisadas. A folha de estilos exclusiva do
macOS usa a fonte do sistema, títulos de 17 px, rótulos de 13 px e descrições de 11 px.
O formulário de atalho segue os controles dos demais ajustes; o aviso experimental fica
na aba Sobre. Os estilos Linux permanecem sem alterações.

Verificação local: 152 testes do frontend, build TypeScript/Vite e inspeção renderizada
em Chromium das sete abas, sem transbordamento horizontal. Foram exercitados os estados
de erro e sucesso do atalho, além dos layouts de ilha limpo/detalhado, escala de 150% e
tela sem notch. Essa inspeção não substitui a confirmação visual no WebKit do Mac físico.
