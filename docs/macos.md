# macOS (experimental)

Open Island tem uma implementação para macOS 12 ou superior, Apple Silicon e Intel.
O suporte permanece **experimental até a validação em um Mac real**. Compilar no CI não
comprova hover, teclado, Spaces, tela cheia, suspensão, login ou permissões de Automação.

## Instalação e atualização

Baixe `open-island-macos-aarch64.dmg` (Apple Silicon) ou
`open-island-macos-x86_64.dmg` (Intel) da release. Confira `SHA256SUMS`, abra o DMG e
arraste Open Island para Aplicativos antes da primeira execução. O instalador `install.sh`
abre o download correspondente no Darwin. A atualização pelo aplicativo faz o mesmo;
encerre a versão anterior e substitua o aplicativo manualmente.

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
  Uma faixa reserva a câmera; textos, botões e expansão ficam abaixo dela.
- O painel não ativa o aplicativo no hover. Entrada de perguntas habilita o teclado.
- A descoberta usa `libproc` e `sysctl`; só as variáveis de terminal permitidas entram
  nas sessões. Falhas de inspeção não removem um hook cujo processo ainda existe.
- Terminal.app e iTerm2 são reconhecidos. O foco por PID ativa o aplicativo; painéis
  identificáveis continuam usando os resolvers CLI existentes. Não há garantia de aba
  ou janela exata no fallback.
- Novas sessões abrem no Terminal.app. Se a Automação for recusada, permita Open Island
  → Terminal em Ajustes do Sistema → Privacidade e Segurança → Automação.
- Não Perturbe, tela desligada e detecção de fullscreen não são consultados no macOS.
  Os ajustes automáticos correspondentes ficam ocultos. Mute e horários silenciosos funcionam.
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

### Ajustes após as primeiras capturas no Mac

A faixa recolhida abaixo da câmera passou de 46 para 30 pontos na escala padrão.
Os cantos côncavos foram retirados nas telas com notch, e o painel expandido usa
cantos arredondados. O campo de mensagem fica oculto quando o host não oferece
envio; mensagens já enfileiradas continuam disponíveis para cancelamento.

Validação local: 150 testes do frontend, três testes de geometria e build
TypeScript/Vite aprovados. O frontend foi conferido em Chromium com medidas de
notch e Retina simuladas. [Build dos DMGs ajustados](https://github.com/matheus-cintra/open-island/actions/runs/34608578758).
A validação do novo visual no Mac físico continua pendente.
