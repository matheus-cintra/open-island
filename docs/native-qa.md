# QA nativa local

## Linux: baseline e boot sem daemon

Os cenários `baseline` e `daemon-unavailable` executam o binário Tauri em Sway headless com
HOME, diretórios XDG, barramento D-Bus e socket do daemon privados. O supervisor
encerra os processos que criou e recolhe descendentes órfãos do WebKit.
Nenhum daemon é iniciado nesse cenário.

Requisitos locais: Python 3 com Pillow, `dbus-daemon`, Sway com backend headless
e as bibliotecas Linux usadas pelo aplicativo. As ferramentas podem ficar no
diretório de build; não é necessário instalá-las globalmente.

Em `app`, compile um artefato exclusivo de QA:

```sh
bun run tauri build --no-bundle --features qa-webdriver \
  --config '{"identifier":"app.open-island.qa.native","productName":"Open Island QA"}'
```

O wrapper coloca builds com `qa-harness`, `qa-webdriver` ou `--all-features` em
`target/portable-qa`, separado de `target/portable`. O WebDriver é uma dependência
opcional habilitada por `qa-webdriver`; essa feature inclui `qa-harness`.
O servidor só pode iniciar com ambiente privado válido e
`OPEN_ISLAND_QA_WEBDRIVER_PORT` explícita. A implementação do driver liga apenas
em loopback no desktop, e o runner confere que o socket pertence ao aplicativo.

Defina `OPEN_ISLAND_QA_APP` no shell local com o caminho emitido pelo build e
`OPEN_ISLAND_QA_COMPOSITOR` com o executável Sway local. Não grave esses caminhos
em configurações compartilhadas entre máquinas.

```sh
node scripts/native-qa.mjs --platform linux --case daemon-unavailable \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/daemon-unavailable-new
```

Use `--case baseline` para a mesma verificação com uma saída separada destinada
à comparação de renderização. O resultado inclui o PNG, dimensões e hashes do
aplicativo e compositor; não é uma medição geral de desempenho.

Também é possível passar `--app` e `--compositor` diretamente. O diretório de
saída deve ser novo. O runner não procura nem inicia o aplicativo instalado.

O teste aguarda a inicialização da UI, abre a ilha pelo evento Tauri real,
aguarda a geometria expandida estabilizar e verifica o estado inicial de conexão,
ações desabilitadas e ausência de sessões inventadas. A captura usa a API nativa
de snapshot do WebKitGTK: somente o conteúdo do WebView, sem outros aplicativos.
Capturas vazias ou incompatíveis com a geometria observada falham. `result.json`
registra assertions, hashes, dimensões, capacidades do driver e limpeza.

## Linux: reconexão sem sessões

O cenário `reconnect-empty` acrescenta dois processos sucessivos do daemon de QA.
Depois do boot offline, verifica a hidratação, encerra o primeiro daemon, aguarda
os controles desabilitados na WebView e inicia o segundo. A aprovação exige nova
geração de conexão, novo epoch do daemon e controles novamente habilitados.

Compile o daemon separadamente, em `app`:

```sh
bun scripts/portable-build.mjs cargo build --release -p open-islandd --features qa-harness
```

Defina `OPEN_ISLAND_QA_DAEMON` no shell local com o caminho emitido por esse build
ou passe `--daemon`. O runner primeiro exige que `--help` anuncie `--version`,
pois versões antigas iniciam o daemon ao receber opções desconhecidas.
Depois exige o marcador `qa-harness=registered-only` de `--version` antes de
iniciar o daemon. Isso distingue o daemon de QA do sidecar
comum que o build Tauri também pode colocar em `target/portable-qa`.

```sh
node scripts/native-qa.mjs --platform linux --case reconnect-empty \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/reconnect-empty-new
```

O teste usa o socket real e consulta o cache real do shell por IPC Tauri, sem
substituir comandos ou eventos. O PNG desse cenário registra o boot offline;
as transições de conexão são registradas em `connection_observations`.
Esse caso não verifica rascunhos, envio ou recuperação de sessões.

## Linux: diagnóstico e clipboard

`diagnostics-offline` abre o painel Sobre sem daemon. `diagnostics-ready` inicia
um daemon de QA e verifica seu PID, epoch e capabilities no relatório copiado.
Ambos usam Atualizar diagnóstico e Copiar relatório na janela real de ajustes,
leem o clipboard privado com `wl-paste` e salvam `settings.png` e `clipboard.json`.
Também verificam a ausência dos caminhos privados no JSON e a preservação do
arquivo de configuração.

Esses cenários exigem `wtype` e `wl-paste` disponíveis no PATH padrão do sistema.
O runner foca a janela pelo socket IPC do Sway privado e ativa Copiar usando
teclado virtual Wayland. Isso não comprova foco automático nem teclado físico.

```sh
node scripts/native-qa.mjs --platform linux --case diagnostics-offline \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/diagnostics-offline-new
node scripts/native-qa.mjs --platform linux --case diagnostics-ready \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/diagnostics-ready-new
```

O segundo comando requer `OPEN_ISLAND_QA_DAEMON` ou `--daemon`, além dos caminhos
do app e compositor. Os cenários não consultam serviços instalados, não gravam
áudio e não comprovam disponibilidade do microfone.

## macOS: WebView e reconexão

O runner macOS (`app/scripts/qa/native_macos.py`) usa o WebDriver do Tauri sobre a
janela AppKit/WKWebView real. Ele cria HOME, configuração, socket e processos de
QA privados, verifica o identificador do bundle, captura PNG pela janela do
WebView e encerra os grupos de processos que criou. Não abre o aplicativo
instalado e não usa caminhos absolutos em configurações compartilhadas.

Compile um binário Tauri e o daemon com o perfil `qa-webdriver` em um diretório
`target/portable-qa` no próprio Mac. O perfil deve usar o mesmo checkout e o
mesmo commit que será registrado na evidência; o daemon precisa anunciar
`qa-harness=registered-only` em `--version`.

```sh
cd app
bun run tauri build --no-bundle --features qa-webdriver \
  --config '{"identifier":"app.open-island.qa.native","productName":"Open Island QA"}'
bun scripts/portable-build.mjs cargo build --release -p open-islandd --features qa-harness
bun scripts/native-qa.mjs --platform macos --case full \
  --app "$OPEN_ISLAND_QA_MACOS_APP" --daemon "$OPEN_ISLAND_QA_MACOS_DAEMON" \
  --out ../.omo/evidence/post-mvp-evolution/native/macos/full
```

`OPEN_ISLAND_QA_MACOS_APP` pode apontar para o executável release ou para um
`.app` dentro de `target/portable-qa`; `OPEN_ISLAND_QA_MACOS_DAEMON` deve apontar
para o daemon correspondente. A execução precisa ocorrer dentro de uma sessão
gráfica macOS ativa com WebDriver e `lsof`; uma sessão SSH sem WindowServer,
permissão de Acessibilidade ou permissão de microfone é registrada como
`BLOCKED`, sem contornar o sistema. O caso `full` cobre os mesmos onze cenários
de estado, reconexão, entrega, renderização e diagnóstico do Linux. Entrada é
gerada por eventos DOM do WebDriver, portanto `physical_input_tested` permanece
`false`; isso não comprova teclado, ponteiro ou microfone físicos.

O gate integrado (`post-mvp-qa.mjs --task 22 --case happy`) executa a matriz
macOS somente quando rodado no próprio Darwin com os caminhos
`OPEN_ISLAND_QA_MACOS_APP` e `OPEN_ISLAND_QA_MACOS_DAEMON`; em Linux, a parte
macOS permanece explicitamente `BLOCKED` e não é simulada.

## Linux: rascunho e identidade após reconexão

`reconnect-draft` cria uma sessão cujo processo pertence ao daemon de QA.
O helper usa socket privado, valida identidade de processo e registra somente
entregas explícitas em um arquivo temporário. Não executa o texto recebido.
O daemon registra apenas seus próprios helpers no ProcessSource de QA; o
build normal não inclui essa fábrica nem o modo de helper.

Além dos requisitos anteriores, compile o ponteiro virtual local (compilador C,
`wayland-scanner` e headers de `wayland-client`):

```sh
node scripts/build-qa-pointer.mjs
node scripts/native-qa.mjs --platform linux --case reconnect-draft \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/reconnect-draft-new
```

O cenário exige `--daemon` ou `OPEN_ISLAND_QA_DAEMON`. Clica uma vez no editor,
digita com teclado virtual, verifica a transferência de foco para ajustes,
derruba o daemon e hidrata uma nova instância da mesma sessão lógica.
O rascunho deve permanecer, com nova identidade de ação e sem entrega automática.
Também verifica a recusa de uma requisição sem identidade na ponte de entrada.

O ponteiro só aceita o ambiente privado de QA e coordenadas do output headless
1280×720 usado pelo runner. O XML do protocolo mantém sua licença MIT em
`scripts/qa/protocols`. Esses helpers são ferramentas de QA e não entram no bundle.
O cenário não prova envio explícito, terminal físico, permissões de microfone ou macOS.

## Linux: entrega explícita e recuperação

`message-success`, `message-failure` e `message-unconfirmed` continuam o fluxo de
rascunho/reconexão com um Enter real pelo teclado virtual. Verificam admissão na
fila, retenção no shell e ausência de recebimento enquanto a sessão está ocupada.
O cenário publica Stop pelo socket real e usa `sessions.idle_after_ms=1000` apenas
no arquivo privado de QA, registrado no resultado. O padrão de dez minutos de
atenção após Stop permanece no produto.

O helper confirma o recebimento, recusa antes de receber ou recebe e fecha o
socket sem responder, respectivamente. Não substitui RPCs Tauri nem o executor.
O sucesso exige uma única entrega do texto exato e liberação do registro de
recuperação. Falha e confirmação perdida exigem texto preservado depois de
recarregar a WebView real e nenhuma repetição automática. Após reiniciar o daemon,
o registro deve continuar local, vinculado ao epoch anterior, sem migrar para a
nova sessão. Os resultados incluem
`helper-deliveries.json`, `recovery-after-restart.json` nos casos de erro e
capturas `delivered.png` ou `recovery.png`. A captura da recuperação rola o painel
limitado até o texto; essa ação é registrada no resultado.

```sh
node scripts/native-qa.mjs --platform linux --case message-success \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/message-success-new
node scripts/native-qa.mjs --platform linux --case message-failure \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/message-failure-new
node scripts/native-qa.mjs --platform linux --case message-unconfirmed \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/message-unconfirmed-new
```

## Linux: 50 sessões e foco do editor

`many-sessions` inicia o daemon de QA com 50 sessões sintéticas, hidrata o
snapshot autoritativo e verifica IDs e identidades de ação distintos. O runner
mantém 50 composers no WebView, dá foco ao último editor, faz leituras
autoritativas adicionais e rola a lista limitada até uma sessão no fim. A
captura `many-sessions.png` é recortada ao WebView e comprova que uma linha
rolada permanece visível; ela não é um benchmark de CPU/RSS.

```sh
node scripts/native-qa.mjs --platform linux --case many-sessions \
  --out ../.omo/evidence/post-mvp-evolution/native/linux/many-sessions-new
```

O cenário continua usando o compositor Sway privado com o patch de foco
descrito acima e não substitui teste com 50 sessões reais, Hyprland, macOS ou
teclado físico.

### Compositor do teste de foco

O Sway 1.12 limpa o foco de uma layer ON_DEMAND quando reorganiza as layers.
Isso interrompe a retomada de teclado após a janela de ajustes. A comparação
com o código upstream identificou a condição que preserva ON_DEMAND e limpa
somente NONE. A correção usada no compositor local de QA está em
`scripts/qa/sway-1.12-focus.patch`; não é aplicada ao compositor instalado.

As evidências de entrega registram o hash desse Sway compilado localmente.
A proveniência do código, patch e ferramentas deve acompanhar o resultado.
Não interpretar aprovação nesse compositor como aprovação no Sway 1.12 sem
patch, no Hyprland ou com teclado físico. O aplicativo adquire foco ao clicar
no editor e volta a ON_DEMAND após recebê-lo; a transferência para ajustes é
verificada separadamente no mesmo cenário.

## Escopo da evidência

Os cenários Linux exercitam o WebKitGTK real, e os cenários macOS exercitam a
WKWebView/AppKit real, sempre com o escopo específico descrito acima. O uso de
Sway exercita layer-shell, mas não comprova o backend específico de monitor,
fullscreen e foco do Hyprland. A abertura por evento Tauri e a entrada DOM do
WebDriver não são prova de teclado ou ponteiro físicos. Captura de microfone não
é feita.

Os cenários além de `baseline`, `daemon-unavailable`, `reconnect-empty`, `reconnect-draft`, `message-success`,
`message-failure`, `message-unconfirmed`, `many-sessions`, `render-burst`, `diagnostics-offline` e `diagnostics-ready` ainda retornam `BLOCKED` explicitamente. O caso `full` executa essa matriz de forma sequencial e grava um `result.json` agregado; ele continua exigindo os mesmos artefatos QA e não inclui voz. A matriz macOS exige execução no Darwin; um resultado Linux não pode representar essa plataforma.
Esse resultado
não é aprovação da QA completa nem da comparação de performance com a baseline.

Referências: [WebDriver no Tauri](https://v2.tauri.app/develop/tests/webdriver/)
e [plugin de WebDriver embutido](https://github.com/webdriverio/desktop-mobile).
