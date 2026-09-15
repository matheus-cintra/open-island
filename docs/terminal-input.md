# Mensagens em qualquer terminal

A ilha pode enviar texto ao agente por uma ponte PTY própria. O terminal externo
(Warp, Terminal, iTerm2, Ghostty, Alacritty, terminais de editores etc.) continua
mostrando a sessão e recebendo o teclado normalmente. Não é necessário ativar uma
janela, usar o clipboard ou autorizar Acessibilidade para esse canal.

## Como ativar

- Sessões abertas pelo botão **+** da ilha já usam a ponte. No macOS, selecione
  antes **Ajustes → Geral → Novas sessões → Abrir no terminal → Warp** (ou o
  aplicativo desejado). O padrão é Terminal.app; o terminal em uso não é escolhido
  automaticamente.
- Para comandos digitados no terminal, habilite **Ajustes → Integrações → Mensagens
  pela ilha → Enviar mensagens aos agentes pelo terminal**. Abra uma nova aba depois.
  A integração instala funções em Bash, Zsh e Fish; funções e aliases próprios têm
  precedência e não são substituídos. A instalação usa os arquivos de inicialização
  padrão da pasta pessoal; configurações com ZDOTDIR/XDG_CONFIG_HOME personalizado
  podem carregar o script `~/.config/open-island/input.sh` manualmente.
- Em qualquer outro shell, ou quando houver um alias personalizado, execute
  `open-islandd run -- claude` (ou outro agente e seus argumentos). No macOS, o
  executável também fica em `/Applications/Open Island.app/Contents/MacOS/open-islandd`.
- Desativar a integração ou remover a configuração automática remove os blocos
  gerenciados. Abra uma nova aba para descartar as funções já carregadas.

Sessões antigas que não usam tmux/Zellij/Kitty/WezTerm precisam ser reabertas com
a ponte. A ilha não pode assumir o lado de entrada de uma PTY já criada por outro
aplicativo. Os canais de envio anteriores continuam disponíveis.

## Entrega e limites

- Cada sessão tem um socket dentro de um diretório privado e temporário por usuário.
  O destino é conferido pela ancestralidade, pelo grupo em primeiro plano e pelo
  dispositivo de entrada do processo, para não confundir subprocessos com o agente.
  A ponte desaparece quando o agente termina; o daemon pode reiniciar sem encerrá-la.
- O protocolo de fila, aprovação e perguntas existente continua sendo usado. Uma
  mensagem é entregue ao terminal como texto seguido de Enter; isso não é uma
  confirmação semântica de que o agente aceitou ou terminou a tarefa.
- Unicode e várias linhas usam colagem delimitada quando o agente a habilita.
  Sem esse modo, texto com várias linhas/tabulações é recusado. Escape, NUL e outros
  caracteres de controle não são aceitos; o limite é 64 KiB por mensagem.
- Uma falha de confirmação não causa reenvio automático: verifique o terminal
  antes de repetir, pois parte do texto pode já ter sido recebida.
- Os comandos de shell preservam chamadas com entrada/saída redirecionadas e
  comandos comuns de execução não interativa. Aliases/funções personalizados não
  são interceptados. Sessões remotas precisam da ilha/ponte no mesmo host do agente.

## Validação

Testes usam PTYs reais: duas sessões simultâneas, recusa de destino cruzado,
Unicode/multilinha, teclado, redimensionamento, fechamento e descoberta/envio pelo
daemon. A instalação é testada quanto a idempotência, remoção, preservação do perfil
de login e funções próprias. A confirmação em terminais reais no Mac permanece
necessária antes de considerar a integração estabilizada.

Em 11/09/2026, os sete testes com PTYs reais e todas as regressões do core/daemon
passaram em Apple Silicon e Intel na
[execução nativa 34635199008](https://github.com/matheus-cintra/open-island/actions/runs/34635199008),
commit `2e1d575`. Incluem envio pelo daemon, isolamento entre sessões e subprocessos,
teclado, redimensionamento, funções de Bash/Zsh, limpeza após hangup e conexão
estabelecida antes da chegada da mensagem.
O teste recupera também a capacidade de entrada de sessões conhecidas apenas por
hooks, quando o macOS apresenta um interpretador como executável do processo.
O teste consome a saída da PTY enquanto aguarda o shell encerrar, assim como um
terminal real. A suíte Linux passou, além dos 157 testes do frontend e do build da
interface. A confirmação visual e interativa no Warp do usuário ainda é necessária.

### Entrega final

Os DMGs do commit `c8129af052f05606359f70ef332157cf7ae70ece` passaram no
[CI completo 34635555896](https://github.com/matheus-cintra/open-island/actions/runs/34635555896)
em Apple Silicon e Intel, incluindo os sete testes da ponte, o workspace Rust e
a atualização assinada com reinício real do daemon. Os checksums dos dois DMGs
foram conferidos após o download. O teste físico no Mac falhou: o usuário informou abertura no Terminal.app em vez
do Warp e falha no envio. A aprovação automatizada não comprova a integração
funcionando no ambiente do usuário; investigação pendente.

## Estados de entrega na implementação pós-MVP

O daemon admite mensagens antes de executar o envio. A resposta legada
`send_message` conserva `{message_id, delivered}`, mas `delivered: false` no recibo
significa que a execução ainda não foi confirmada. Não use esse recibo como prova
que o terminal recebeu o texto.

Os registros em memória distinguem `queued`, `sending`, `delivered`, `failed` e
`unconfirmed`. `delivered` confirma a entrada pelo canal; não confirma a execução
pelo agente. Falhas e resultados incertos mantêm o texto e não são reenviados
automaticamente. Um envio em execução não pode ser cancelado pela fila.

Há até 32 registros não entregues por sessão, 256 no total e 4 MiB de texto bruto.
O daemon recusa novas admissões quando não há espaço. Registros órfãos expiram
após dez minutos; confirmações sem texto ficam por até cinco minutos. Não há
persistência desses registros após reiniciar o daemon.

A API `get_ui_state` inicia um documento imutável; `get_ui_state_page` lê páginas
sequenciais de até 32768 bytes, usando `snapshot_id` e `expected_page`. O token
pertence à conexão e expira após 30 segundos sem progresso. Respostas legadas
agregadas maiores que 256 KiB retornam `snapshot_requires_paging`.

A interface nova vincula navegação, envio e respostas ao epoch do daemon e à
instância exibida. Filhos agrupados mantêm identidade própria em `child_sessions`;
responder uma pendência do filho não usa a identidade do pai. A entrada pelo
terminal compartilhado não endereça a conversa do filho e fica bloqueada para
esse destino. Uma sessão sem PID verificável mantém a identidade do hook para
responder pendências, com envio bloqueado; o snapshot novo não adota um processo
apenas porque ele usa o mesmo diretório.

O shell conserva em memória até 256 submissões/4 MiB antes de enviá-las. Após uma
mudança de epoch, textos sem confirmação aparecem na recuperação para copiar ou
descartar, sem reenvio automático. Esse ledger sobrevive ao reload do WebView,
mas não ao fechamento/crash do app e não é sincronizado entre máquinas.

O servidor reserva 32 conexões para UI (`subscribe_ui`) e 32 para legado/handshake.
Um coletor somente leitura se identifica no primeiro `ping` com
`{client_role: "diagnostic"}`: não recebe eventos de sessões nem conta como ilha
disponível. Hooks também não contam como espectadores. Sem interface, aprovações
saem sem decisão e perguntas respondíveis retornam ao terminal imediatamente.

A integração pós-MVP ainda está em implementação: o daemon não anuncia o conjunto
final de capabilities, portanto o shell de produção permanece incompatível até
a conclusão dos contratos. Os testes automatizados da UI conectada usam fixtures;
isso não comprova foco, envio ou recuperação em uma WebView nativa.
