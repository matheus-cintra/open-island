# Entrega, recuperação e ditado local

Esta evolução está implementada neste checkout e foi validada pelos testes
automatizados e pela QA nativa Linux descritos abaixo. Esses resultados não
substituem a matriz gráfica e de áudio Linux/macOS. A entrega desta etapa é um
pré-release Linux; macOS continua experimental e será publicado depois.

## Estado da validação desta execução

- Suítes Rust, TypeScript/Bun, scripts e políticas de pacote passaram sem
  processos de teste restantes.
- A QA nativa Linux passou em boot offline, reconexão, rascunho, três estados de
  entrega, 50 sessões, rajada de renderização e diagnóstico offline/pronto. Ela
  usa WebKitGTK em Sway headless privado, não uma sessão física do usuário.
- A inferência real com o modelo pinado passou no host para a fixture PT-BR e
  silêncio. O cancelamento cronometrado ainda excede a janela de 10 segundos;
  permanece reportado como falha de responsividade, sem promessa de parada
  imediata.
- Não houve host macOS, microfone físico, teclado físico ou validação do backend
  Hyprland nesta execução. Esses gates ficam explicitamente bloqueados.

## Mensagens e recuperação

Digite no composer da sessão e use Enter para enviar; Shift+Enter quebra a linha.
O texto só sai do editor depois da admissão pelo shell/daemon.

| Estado | Significado |
| --- | --- |
| Enfileirada | Aguarda a sessão e a disponibilidade do executor. |
| Enviando | Uma tentativa está em andamento; não é possível prometer cancelamento dos bytes já enviados. |
| Entregue ao canal | O canal confirmou o recebimento. Não confirma execução da tarefa pelo agente. |
| Falha na entrega | A tentativa falhou antes de causar efeito no destino. O texto pode ser copiado ou descartado. |
| Entrega não confirmada | Pode ter ocorrido efeito, mas não houve confirmação suficiente. Confira a sessão antes de enviar novamente. |

Não há repetição automática de falhas ou entregas não confirmadas. Descartar um
registro não desfaz uma mensagem que o agente possa ter recebido. Copiar também
não envia texto.

A fila mantém a semântica por turno: não drena várias mensagens em um único Stop.
Enquanto a sessão trabalha ou precisa de atenção, a entrega aguarda. O período de
atenção após Stop usa a configuração existente; o padrão é dez minutos.

O shell emissor guarda uma cópia em memória antes de enviar. Essa recuperação
sobrevive ao reload da WebView e ao restart do daemon, mas termina ao fechar o
aplicativo. Não há banco nem histórico durável de prompts. Um registro de uma
conexão anterior permanece separado da nova sessão; não é redirecionado sozinho.

As ações usam a identidade capturada no estado exibido. Um PID ou ID textual
reutilizado não autoriza um clique antigo a atuar na nova sessão. Canais sem
prova suficiente de destino podem recusar envio; abrir ou focar um terminal não
conta como confirmação de entrega.

## Conexão e diagnóstico

Ao perder a conexão, a ilha sinaliza que os dados podem estar desatualizados e
desabilita ações dependentes. A reconexão recupera um snapshot autoritativo,
inclusive sem eventos novos. O transporte divide esse snapshot em páginas;
clientes antigos continuam no protocolo legado, com limites para respostas grandes.

Em Ajustes → Sobre, use **Atualizar diagnóstico** e **Copiar relatório**. O CLI
equivalente é `open-islandd doctor --json`. Ele não instala nem reinicia serviços.
O relatório distingue daemon indisponível, incompatibilidade e conexão saudável,
e separa a versão do processo ativo da versão encontrada em disco.

O relatório exclui prompts, transcrições, caminhos pessoais e nomes de dispositivos
de áudio. No Linux, a detecção ALSA consulta metadados do kernel; não abre o
microfone. Entrada detectada não comprova permissão, funcionamento da captura ou
disponibilidade de todas as fontes virtuais. Um resultado desconhecido permanece
explícito.

## Ditado para uma sessão escolhida

Em Ajustes → Voz local, escolha um arquivo de modelo já existente na máquina.
O aplicativo não baixa modelos automaticamente. O caminho fica somente no arquivo
local `voice.json`, com permissão 0600; não entra na configuração compartilhada
do daemon nem no diagnóstico.

O modelo de avaliação do plano é Whisper **small multilingual**, em formato GGML:

- Arquivo: `ggml-small.bin`, 487.601.967 bytes.
- SHA-256: `1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b`.
- [Artefato público na revisão fixada](https://huggingface.co/ggerganov/whisper.cpp/resolve/90a64d80ea254cf67575b41a5971f972c79f7b45/ggml-small.bin).
- Licença dos pesos: [MIT, conforme o projeto Whisper](https://github.com/openai/whisper#license).

Na sessão de destino, o botão de microfone inicia a gravação; ela encerra sozinha
cerca de um segundo e meio depois que a voz para, e clicar de novo encerra na
hora. Escape cancela. O limite é sessenta segundos. Áudio e transcrição são
processados localmente, em CPU, e não são enviados à nuvem nem persistidos como
gravação. No Mac, a autorização pertence ao aplicativo, não ao daemon.

Revise o texto antes de enviar. Uma transcrição não envia mensagem sozinha. Se o
rascunho foi editado ou o destino mudou durante a transcrição, o resultado fica
separado para inserção ou cópia explícita. Perder o destino não escolhe outra
sessão automaticamente. Remover o modelo da configuração cancela o trabalho de
voz e não apaga o arquivo de modelo.

Cancelar impede o aproveitamento do resultado, mas a inferência nativa pode levar
dezenas de segundos para liberar o trabalho em andamento. Nesse intervalo, a ilha
mostra “Cancelando…” e aguarda antes de permitir outra gravação. Fechar o aplicativo
ou remover a configuração também pode aguardar essa liberação. A responsividade
desse cancelamento permanece uma limitação medida, não uma garantia de parada imediata.

## Builds e alcance da validação

O wrapper portátil usa x86-64-v1/SSE2 no Linux/Intel e ARMv8-A/NEON no Mac ARM,
sem exigir GPU. A inferência carrega o modelo apenas para o trabalho e o libera
ao terminar. Isso pode consumir memória e levar tempo perceptível; o tempo depende
da CPU e do áudio.

`bun run tauri build` recompila o sidecar, mas não instala nem reinicia o daemon
em execução. `node scripts/portable-build.mjs paths`, em `app`, informa a saída.
QA fica em diretório separado. Os bundles normais excluem modelos e fixtures e
incluem avisos de terceiros em `licenses/`.

Consulte [QA nativa](native-qa.md), [fixtures sintéticas](../app/test-fixtures/voice/README.md)
e [macOS](macos.md). A evidência distingue testes de lógica, execução com modelo
real, WebView nativa em compositor privado e verificação física. Não há alegação
de ganho geral de performance sem comparação válida contra a baseline.

## Possibilidades futuras

Estas opções não fazem parte da entrega atual:

| Possibilidade | Decisões que ainda faltam |
| --- | --- |
| Inbox de agentes | Priorização, origem e duração dos itens; evitar duplicar pendências existentes. |
| Busca e favoritos | Quais dados são indexados e quando uma referência de sessão deixa de ser válida. |
| Histórico persistente | Retenção, exclusão, criptografia e consentimento para armazenar prompts/transcrições. |
| Resposta falada | Modelo, custo de CPU, privacidade em ambientes compartilhados e interrupção. |
| Conversa contínua | Ativação explícita, orçamento de recursos e prevenção de gravação/envio involuntários. |
