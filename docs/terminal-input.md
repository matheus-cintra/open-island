# Mensagens em qualquer terminal

A ilha pode enviar texto ao agente por uma ponte PTY própria. O terminal externo
(Warp, Terminal, iTerm2, Ghostty, Alacritty, terminais de editores etc.) continua
mostrando a sessão e recebendo o teclado normalmente. Não é necessário ativar uma
janela, usar o clipboard ou autorizar Acessibilidade para esse canal.

## Como ativar

- Sessões abertas pelo botão **+** da ilha já usam a ponte.
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
  O destino é conferido pela ancestralidade e pelo grupo de processos em primeiro plano.
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

Em 11/09/2026, os cinco testes com PTYs reais passaram em Apple Silicon e Intel
na [execução nativa 34633880538](https://github.com/matheus-cintra/open-island/actions/runs/34633880538),
commit `feae1fa`. Incluem envio pelo daemon, isolamento entre sessões, teclado,
redimensionamento, execução pelas funções de Bash/Zsh e limpeza após hangup.
O teste recupera também a capacidade de entrada de sessões conhecidas apenas por
hooks, quando o macOS apresenta um interpretador como executável do processo.
O teste consome a saída da PTY enquanto aguarda o shell encerrar, assim como um
terminal real. A suíte Linux passou, além dos 157 testes do frontend e do build da
interface. A confirmação visual e interativa no Warp do usuário ainda é necessária.
