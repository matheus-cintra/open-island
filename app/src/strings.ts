const AGENT_LABELS: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  opencode: "OpenCode",
};

const CLAUDE_MODEL = /^claude-([a-z]+)-(\d+)(?:-(\d+))?$/;

const BLOCKED_REASONS: Record<string, string> = {
  daemon_unavailable: "Espere reconectar para enviar.",
  daemon_incompatible: "Atualize o daemon para enviar mensagens daqui.",
  stale_epoch: "A conexão caiu e voltou. Confira a sessão antes de enviar de novo.",
  stale_session: "A sessão mudou. O texto ficou no editor.",
  message_too_large: "A mensagem passou do limite de 64 KB.",
  recovery_full: "Descarte ou copie um dos textos recuperados antes de enviar outra.",
  queue_full: "A fila está cheia. O texto ficou no editor.",
  delivery_in_progress: "A entrega já começou e não dá para cancelar.",
  host_unsupported: "Este terminal não tem como receber texto pela ilha.",
  kitty_remote_control_off:
    "Ligue allow_remote_control e listen_on no kitty para escrever daqui.",
  wezterm_socket_missing: "O wezterm não está com o socket da GUI aberto.",
  pane_gone: "O pane desta sessão não existe mais.",
  unverified_target: "A ilha ainda não confirmou o destino desta sessão. Tente de novo em alguns segundos.",
  unaddressable_child: "Subagente não recebe mensagem direta. Escreva para a sessão principal.",
};

export const strings = {
  voice: {
    title: "Voz local", start: "Gravar mensagem", stop: "Parar gravação", cancel: "Cancelar gravação",
    model: "Selecionar modelo…", configured: "Modelo local configurado", unavailable: "Escolha um modelo para transcrever",
    modelRequired: "Escolha um modelo em Ajustes › Voz local",
    removeModel: "Remover configuração do modelo", removeConfirm: "Cancelar o ditado e remover a configuração? O arquivo do modelo fica onde está.",
    removeHint: "Para a gravação ou transcrição em curso. Não apaga o arquivo do modelo nem rascunho pronto.",
    refreshModel: "Verificar modelo local", modelStatus: "Modelo", modelRemoved: "Configuração do modelo removida.",
    explanation: "A transcrição roda neste computador, na CPU e na memória. Escolha um modelo GGML multilíngue; modelo maior acerta mais e demora mais. A ilha não baixa modelo nem guarda gravação. Revise o texto antes de enviar.",
    privacy: "Abrir ajustes do microfone", text: "Texto transcrito", applied: "Entrou no rascunho. Revise antes de enviar.", dismiss: "Fechar cartão",
    separate: "O texto está guardado. Escolha uma sessão para inserir no rascunho.",
    targetUnavailable: "A sessão original saiu. Escolha outra e confirme para inserir o texto.",
    insert: (label: string): string => `Inserir em ${label}`, chooseTarget: "Escolha um campo de mensagem para inserir",
    cancelling: "Cancelando… espere para gravar de novo.",
    phase: { idle: "", requesting_permission: "Esperando a permissão do microfone", recording: "Gravando", transcribing: "Transcrevendo…", ready: "Transcrição pronta", cancelled: "Gravação cancelada", error: "Não deu para transcrever" },
    error: (reason: string): string => ({
      model_unavailable: "Escolha um modelo disponível.", invalid_model: "O arquivo não é um modelo GGML compatível.",
      model_language_unsupported: "Escolha um modelo multilíngue, que entenda português.",
      microphone_permission_denied: "Permissão do microfone negada. Confira os ajustes de privacidade.",
      audio_unavailable: "O microfone padrão não está disponível.", audio_device_lost: "O microfone foi desconectado.",
      audio_format_unsupported: "O formato do microfone não é compatível.", audio_overrun: "A captura não acompanhou o áudio. Tente de novo.",
      no_speech: "Nenhuma fala foi reconhecida.", voice_busy: "Já tem uma gravação ou transcrição rodando.",
      voice_model_busy: "Conclua a seleção do modelo antes de gravar.", voice_results_full: "Feche um cartão de transcrição para gravar de novo. Copie o texto antes, se for usar.",
      voice_target_stale: "A sessão mudou. Escolha o destino de novo.", daemon_unavailable: "Espere conectar ao daemon para gravar.",
      daemon_incompatible: "Atualize o daemon para usar a voz nesta versão.",
    }[reason] ?? "Não deu para concluir. O rascunho ficou no editor."),
  },
  recovery: {
    title: "Textos recuperáveis",
    text: "Texto da mensagem preservada",
    memory: "Os textos ficam aqui até você descartar ou fechar a ilha. Copiar não reenvia nem interrompe o agente.",
    previousConnection: "Conexão anterior · entrega não confirmada",
    missingRecord: "Registro indisponível · entrega não confirmada",
    failed: "Falha na entrega",
    unconfirmed: "Entrega não confirmada",
    discardFailed: "Não deu para descartar o texto. Tente de novo.",
    loadFailed: "Não deu para ler os textos recuperáveis.",
  },
  connection: {
    discovering: "Procurando sessões…",
    connecting: "Conectando ao Open Island…",
    reconnecting: "Reconectando… o que está na tela pode estar velho.",
    incompatible: "Atualize o daemon para usar esta versão da ilha.",
    connected: "Conectado",
  },
  island: {
    label: "Open Island",
    sessions: (count: number): string => (count === 1 ? "1 sessão" : `${count} sessões`),
    wordmark: "open island",
    noConnection: "sem conexão",
    waiting: (count: number): string => `${count} esperando`,
    finished: (count: number): string => (count === 1 ? "1 concluída" : `${count} concluídas`),
    kicker: { permission: "permissão", question: "pergunta", done: "concluído" },
    empty: "Nenhum agente rodando.",
    emptyHint: "Abra uma sessão com + ou rode um agente no terminal. Ela aparece aqui sozinha.",
  },
  usage: {
    label: "Limites de uso",
    stale: "antigo",
    unavailable: "sem dados de uso",
    separator: "|",
    windowTitle: (label: string, value: string, reset?: string): string =>
      reset === undefined ? `${label}: ${value}` : `${label}: ${value}, zera em ${reset}`,
    resetIn: (milliseconds: number): string => {
      const minutes = Math.max(0, Math.round(milliseconds / 60_000));
      if (minutes < 60) return `${minutes}m`;
      const hours = Math.floor(minutes / 60);
      const rest = minutes % 60;
      if (hours < 24) return rest === 0 ? `${hours}h` : `${hours}h${rest}m`;
      return `${Math.floor(hours / 24)}d`;
    },
    percent: (value: number): string => `${Math.round(value)}%`,
    resetCards: (count: number): string =>
      count === 1 ? "1 Reset Card" : `${count} Reset Cards`,
    credits: (balance: number): string => `${balance.toLocaleString("pt-BR")} créditos`,
    dollars: (balance: number): string =>
      `US$${(balance / 25).toLocaleString("pt-BR", {
        minimumFractionDigits: 2,
        maximumFractionDigits: 2,
      })}`,
    creditsUnlimited: "créditos ilimitados",
  },
  session: {
    messageOpen: "Mandar mensagem para a sessão",
    messagePlaceholder: "Mensagem para o agente. Enter envia, Shift+Enter quebra a linha.",
    messageQueued: (count: number): string =>
      count === 1 ? "1 mensagem na fila" : `${count} mensagens na fila`,
    messageCancel: "Cancelar esta mensagem",
    deliveryState: { queued: "Enfileirada", sending: "Enviando", delivered: "Entregue ao terminal", failed: "Falha na entrega", unconfirmed: "Entrega não confirmada" },
    deliveryConfirmed: "O terminal recebeu a mensagem. Isso não quer dizer que o agente já agiu.",
    deliverySending: "A entrega já começou. O que saiu não volta.",
    deliveryDiscard: "Descartar registro",
    deliveryDetached: "Mensagens de outras sessões",
    deliveryDiscardFailed: "Não deu para descartar a mensagem. O texto ficou guardado.",
    deliveryFailed: "A mensagem não chegou. Copie o texto e revise antes de enviar de novo.",
    messageCopy: "Copiar texto",
    messageUnconfirmed: "Envio sem confirmação. Confira a sessão antes de reenviar.",
    messageDiscard: "Descartar texto recuperado",
    messageCopyFailed: "Não deu para copiar. Selecione o texto e copie na mão.",
    messageFailed: (reason: string): string => `Não deu para mandar a mensagem: ${reason}`,
    messageBlocked: (code: string): string =>
      BLOCKED_REASONS[code] ?? "Não dá para enviar daqui agora. Confira a sessão no terminal.",
    messageReason: (reason: string): string =>
      BLOCKED_REASONS[reason] ?? reason,
    promptPrefix: "Você:",
    done: "Concluído",
    tasksChip: (done: number, total: number): string => `Tarefas ${done}/${total}`,
    agentsChip: (live: number, total: number): string =>
      live > 0
        ? `Agentes ${live} ${live === 1 ? "ativo" : "ativos"}`
        : `Agentes ${total} ${total === 1 ? "concluído" : "concluídos"}`,
    stateChip: { waiting_for_input: "esperando você", needs_attention: "concluído", idle: "parada" },
    subagentTool: "└",
    separator: "·",
    bypass: "BYPASS",
    /// The activity line never goes empty, or the row loses a line and the list stops
    /// having one rhythm. `strings.attention.working` is deliberately blank for the pill.
    working: "trabalhando",
    waitingApproval: "esperando sua permissão",
    waitingAnswer: "esperando sua resposta",
    branchLabel: (branch: string): string => `worktree ${branch}`,
    agent: (agent: string): string => AGENT_LABELS[agent] ?? agent,
    /// `claude-opus-5[1m]` reads as `Opus 5`; anything the pattern does not know keeps the
    /// id the agent reported, minus a context-window suffix and a build date.
    model: (model: string): string => {
      const id = model.replace(/\[[^\]]*\]$/, "").replace(/-\d{8}$/, "");
      const claude = CLAUDE_MODEL.exec(id);
      if (!claude) return id;
      const [, family, major, minor] = claude;
      const name = family.charAt(0).toUpperCase() + family.slice(1);
      return minor ? `${name} ${major}.${minor}` : `${name} ${major}`;
    },
    elapsed: (milliseconds: number): string => {
      const seconds = Math.max(0, Math.floor(milliseconds / 1000));
      if (seconds < 60) return `${seconds}s`;
      const minutes = Math.floor(seconds / 60);
      if (minutes < 60) return `${minutes}m`;
      const hours = Math.floor(minutes / 60);
      return hours < 24 ? `${hours}h` : `${Math.floor(hours / 24)}d`;
    },
  },
  attention: {
    waiting_for_input: "esperando você",
    needs_attention: "parou e ninguém viu",
    working: "",
    idle: "parada",
  },
  approval: {
    label: "Permissão pendente",
    actionsLabel: "Ações da permissão",
    kicker: "Permissão solicitada",
    kickerFor: (project: string): string =>
      project === "" ? "permissão solicitada" : `permissão solicitada · ${project}`,
    missingTool: "Ferramenta do agente",
    fallback: "Esperando sua decisão",
    planTool: "Plano",
    planFallback: "O agente quer sair do modo plano",
    allow: "Permitir",
    always: "Sempre",
    alwaysTitle: "Permitir e não perguntar de novo nesta sessão",
    deny: "Negar",
    diffLabel: "Alteração proposta",
    invalid: "Pedido de permissão inválido",
    failed: (decision: "allow" | "deny" | "allow_always", reason: string): string =>
      `Falha ao ${decision === "deny" ? "negar" : "permitir"}: ${reason}`,
  },
  question: {
    label: "Pergunta pendente",
    kickerFor: (project: string): string => (project === "" ? "pergunta" : `pergunta · ${project}`),
    count: (count: number): string =>
      count === 1 ? "1 pergunta" : `${count} perguntas`,
    optionsLabel: "Opções da resposta",
    multiSelectHint: "Escolha uma ou mais",
    customPlaceholder: "Outra resposta",
    submit: "Responder",
    terminalFallback: "Responda no terminal do agente",
    countdown: (seconds: number): string => `expira em ${seconds}s`,
    jump: "Ir para a sessão",
    invalid: "Pergunta inválida",
    failed: (reason: string): string => `Falha ao responder: ${reason}`,
  },
  jump: {
    failed: (title: string, reason: string): string =>
      `Falha ao focar ${title}: ${reason}`,
  },
  header: {
    settings: "Abrir ajustes",
    newSession: "Abrir uma sessão",
    mute: "Silenciar os sons",
    unmute: "Voltar a tocar os sons",
    update: (version: string): string => `${version} disponível. Clique para atualizar.`,
    updatePrompt: "Aperte Enter para fechar.",
    updateFailed: (reason: string): string =>
      `Não deu para abrir um terminal (${reason}). Rode você mesmo: curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh`,
  },
  launch: {
    kicker: "Abrir uma sessão nova",
    pickFolder: (agent: string): string => `Escolha a pasta para o ${agent}`,
    accept: "Abrir aqui",
    cancel: "Cancelar",
    close: "Fechar",
    missing: "não encontrado no PATH",
    failed: (reason: string): string => `Não deu para abrir a sessão: ${reason}`,
    agents: { claude: "Claude", codex: "Codex", opencode: "OpenCode" },
  },
  settings: {
    title: "Ajustes",
    panes: {
      general: "Geral",
      integrations: "Integrações",
      display: "Exibição",
      sound: "Som",
      usage: "Uso",
      filters: "Notificações",
      about: "Sobre",
    },
    about: {
      name: "Open Island",
      diagnostics: "Diagnóstico",
      diagnosticHint: "Consulta local, sem subir nem reiniciar serviço. O relatório não leva sessão, mensagem nem caminho seu.",
      diagnosticServiceLabel: "Serviço em segundo plano",
      diagnosticServiceActive: "Ativo",
      diagnosticServiceInactive: "Instalado, mas inativo",
      diagnosticServiceMissing: "Não instalado",
      diagnosticUnchecked: "Não deu para verificar",
      diagnosticAudioLabel: "Entrada de áudio",
      diagnosticAudioDetected: "Entrada ALSA encontrada; captura não testada",
      diagnosticAudioNotDetected: "Nenhuma entrada ALSA; fonte virtual não verificada",
      diagnosticAudioUnchecked: "Entrada e captura ainda não verificadas",
      diagnosticHooksLabel: "Hooks dos agentes",
      diagnosticHooksCurrent: "Nada antigo nem em conflito",
      diagnosticHooksReview: (agents: string): string => `Revisar configuração: ${agents}`,
      diagnosticCapacityLabel: "Atividade do daemon",
      diagnosticCapacity: (connections: number, pending: number, fallback: number): string => `${connections} conexões · ${pending} pendências · ${fallback} retornos ao terminal`,
      diagnosticRefresh: "Atualizar diagnóstico",
      diagnosticCopy: "Copiar relatório",
      diagnosticEmpty: "Ainda não consultado",
      diagnosticFailed: "Não deu para consultar o diagnóstico.",
      diagnosticCopied: "Relatório copiado.",
      diagnosticCopyFailed: "Não deu para copiar o relatório.",
      diagnosticReady: "Conectado",
      diagnosticOffline: "Daemon indisponível",
      diagnosticIncompatible: "Daemon incompatível",
      diagnosticInvalid: "Resposta inválida do daemon",
      diagnosticVersions: (app: string, daemon: string): string => `App ${app} · daemon ${daemon}`,

      removeAutoConfig: "Remover toda a configuração automática",
      removeAutoConfigConfirm: "Tocar de novo para remover",
      removeAutoConfigHint:
        "Tira os hooks de todos os agentes configurados, o atalho do Hyprland e as duas units do systemd. A ilha e o daemon fecham junto.",
      removeDone: (count: number): string =>
        count === 1
          ? "1 arquivo removido. O autostart está saindo e a ilha vai fechar."
          : `${count} arquivos removidos. O autostart está saindo e a ilha vai fechar.`,
      quit: "Sair do Open Island",
      quitConfirm: "Tocar de novo para sair",
      acknowledgements: "Agradecimentos",
      credits: "Departure Mono, Tauri, GTK",
      updateCheck: "Avisar quando sair uma versão nova",
      updateCheckHint:
        "Uma vez por dia o daemon pergunta ao GitHub qual é a versão mais recente. Nada seu vai junto. Desligue para ele não perguntar.",
      checkUpdate: "Buscar atualização",
      checkUpdateHint: "Pergunta ao GitHub agora, sem esperar a consulta diária.",
      checkUpdateNow: "Buscar agora",
      updateFound: (version: string): string => `Versão ${version} disponível.`,
      updateNone: "Você já está na versão mais recente.",
      updateAvailable: "Versão nova disponível",
      updateAvailableHint:
        "Para atualizar, rode de novo: curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh",
      actionFailed: (reason: string): string => `Não deu para concluir: ${reason}`,
    },
    filters: {
      panel: "Notificações do painel",
      panelFooter: "Nada aqui muda o comportamento das aprovações.",
      expandOnCompletion: "Abrir o painel quando uma sessão termina",
      expandOnCompletionHint:
        "Desligue para manter o painel recolhido quando uma sessão termina.",
      expandOnQuestion: "Abrir o painel quando chega uma pergunta",
      expandOnQuestionHint: "Desligue para manter o painel recolhido até você abri-lo.",
      subagentTiming: "Subagentes e Agent Team",
      subagentRoot: "Conforme o agente principal responde",
      subagentAllFinished: "Depois que todos os subagentes terminam",
      subagentEvery: "A cada subagente concluído",
      reminders: "Lembretes de acompanhamento",
      reminderDelay: "Lembrar de novo",
      reminderDelayHint:
        "Lembra uma vez, depois do tempo escolhido. Se o estado mudar, o lembrete não vem; a sessão que você está olhando fica em silêncio.",
      reminderOff: "Desativado",
      reminderAfter: (label: string): string => `Depois de ${label}`,
      reminderInclude: "Quando ativado, incluir",
      reminderNeedsResponse: "Precisa da sua resposta",
      reminderNeedsResponseHint: "Aprovações e perguntas.",
      reminderCompletedTasks: "Tarefas concluídas",
      reminderCompletedTasksHint: "Só enquanto o resultado não foi lido.",
      quietScenes: "Cenários silenciosos",
      quietScenesHint:
        "Fica em silêncio enquanto qualquer cenário abaixo estiver ativo: não abre sozinha e não toca nada, nem para aprovação.",
      quietFocusMode: "Modo Foco",
      quietFocusModeHint: "Enquanto o Não Perturbe do swaync estiver ligado.",
      quietScreenOff: "Tela apagada ou bloqueada",
      quietScreenOffHint:
        "Enquanto todos os monitores estiverem com o DPMS desligado, ou a sessão estiver bloqueada.",
      launchers: "Aplicativos de origem bloqueados",
      launchersHint:
        "Descarta a sessão iniciada pelo aplicativo escolhido antes que ela apareça na ilha.",
      launchersFooter:
        "Use para sondas em segundo plano e aplicativos auxiliares. Para filtrar projeto, use Diretório ou Primeiro prompt.",
      launchersEmpty: "Nenhum aplicativo de origem bloqueado.",
      launcherPlaceholder: "ex.: kitty",
      launcherSeen: "Visto agora",
      directory: "Diretório",
      directoryHint: "Esconde a sessão quando o diretório de trabalho casa com o padrão.",
      prompt: "Primeiro prompt",
      promptHint: "Esconde a sessão quando o primeiro prompt casa com o padrão.",
      footer:
        "A sessão filtrada não aparece na ilha, não toca som e não segura o agente esperando aprovação.",
      preset: "Predefinido",
      add: "Adicionar",
      remove: "Remover",
      empty: "Nenhum filtro aqui ainda.",
      patternCwd: "ex.: /chronicle/dev-experiments",
      patternPrompt: "ex.: ## Memory Writing Agent",
      matchContains: "Contém",
      matchPrefix: "Começa com",
      matchEquals: "É igual a",
    },
    usage: {
      limits: "Limites de uso",
      showLimits: "Mostrar os limites de uso",
      showLimitsHint: "Aparecem no cabeçalho do painel da ilha.",
      useClaudeLogin: "Usar o login do Claude Code para o uso",
      useClaudeLoginHint:
        "Lê o login do Claude Code guardado nesta máquina só para buscar o uso do seu plano na Anthropic. Nada sai daqui para outro lugar. Desligue para cortar todo o acesso à credencial; o uso do Codex continua.",
      valueMode: "Valor na tela",
      valueUsed: "Usado",
      valueRemaining: "Restante",
      preferredProvider: "Provedor preferido",
      providerAuto: "Automático (segue a sessão)",
      providerAnthropic: "Anthropic",
      providerCodex: "Codex",
      showResetCards: "Mostrar os Reset Cards",
      showResetCardsHint:
        "Quantos você tem e qual vence primeiro, no cabeçalho de uso.",
      codexCredits: "Créditos extras do Codex",
      codexCreditsCredits: "Créditos",
      codexCreditsDollars: "US$",
      codexCreditsHint: "A conta usa 25 créditos = US$1. A taxa pode mudar.",
      alert: "Aviso",
      warnThreshold: "Tocar o som de limite ao passar de",
      warnThresholdHint: "Toca o som escolhido na aba Som uma vez, quando passa do valor, e não a cada leitura.",
      refreshInterval: "Buscar o uso a cada",
      bridge: "De onde vem o número",
      bridgeAnthropic:
        "A Anthropic responde o uso do plano para o token que o Claude Code já guardou em ~/.claude/.credentials.json. A ilha só lê esse arquivo, nunca escreve nem renova.",
      bridgeCodex:
        "O número do Codex vem do próprio codex app-server, então a ilha não toca na credencial dele nem faz chamada de rede.",
    },
    integrations: {
      input: "Mensagens pela ilha",
      inputLabel: "Enviar mensagens aos agentes pelo terminal",
      inputHint:
        "Sessão aberta pelo botão + já vem com isso. Ligue também para os comandos que você digita no terminal.",
      inputFooter:
        "Vale para sessões novas em Bash, Zsh e Fish, em qualquer terminal. Depois de ligar ou desligar, abra uma aba nova. Seus aliases e funções continuam de pé.",
      agents: "Agentes",
      claude: "Claude Code",
      codex: "Codex",
      opencode: "OpenCode",
      agentsFooter:
        "A ilha instala os hooks sozinha em cada agente que encontra. Desligue o que não quiser.",
      autoConfigure: "Configurar novos agentes automaticamente",
      autoConfigureHint: "Vale para agente que aparecer depois, sem você abrir isto de novo.",
      codexSteps: "Passos manuais do Codex",
      codexTrust:
        'Na próxima vez que o Codex abrir, ele mostra "Hooks need review". Aperte t para confiar nos hooks da ilha; sem isso eles ficam instalados e nunca rodam.',
      codexUserInput:
        "As perguntas do agente precisam de experimental_request_user_input=true e features.default_mode_request_user_input=true no ~/.codex/config.toml.",
    },
    display: {
      notch: "Ilha",
      compactClean: "Limpo",
      compactCleanHint: "Sprite, faixa de estado e contagem.",
      compactDetailed: "Detalhado",
      compactDetailedHint: "Com o projeto e a ferramenta em uso.",
      monitor: "Tela",
      monitorAuto: "Automática",
      monitorHint: "Em qual saída a ilha aparece. Automática segue o compositor.",
      notchTuning: "Ajuste fino",
      notchWidth: "Largura da ilha",
      notchHeight: "Altura da ilha",
      islandHeight: "Altura fixa da ilha",
      islandHeightHint:
        "Fixa a altura da ilha em pixels. 0 usa a altura que o compositor reserva.",
      notchTuningHint:
        "Corrige a ilha quando ela não encaixa na sua barra. 0 usa o valor do compositor.",
      sessionCard: "Linha da sessão",
      project: "Mostrar o nome do projeto",
      worktree: "Mostrar o worktree",
      mascot: "Mascote",
      mascotSprite: "Sprite pixel",
      mascotSpriteHint: "O alien do agente, na cor dele.",
      mascotLogo: "Logo do agente",
      mascotLogoHint: "O ícone oficial. Agente sem sprite usa o logo de qualquer jeito.",
      composer: "Campo de mensagem",
      composerOnDemand: "Sob demanda",
      composerOnDemandHint: "Um botão por linha abre o campo. Esc fecha.",
      composerAlways: "Sempre aberto",
      composerAlwaysHint: "Em toda linha, o tempo todo.",
      terminalIcons: "Mostrar terminais como ícones",
      terminalIconsHint: "Ícone do terminal ao lado do projeto. Sem ícone instalado, fica o nome.",
      model: "Mostrar o modelo",
      effort: "Mostrar o esforço de raciocínio",
      effortHint: "O Claude e o OpenCode informam; o Codex não.",
      tasks: "Mostrar tarefas",
      tasksHint: "Chip Tarefas com o progresso. Clique abre a lista.",
      activity: "Mostrar o detalhe da atividade",
      activityHint: "A ferramenta em uso, à direita da segunda linha.",
      subagents: "Mostrar subagentes",
      subagentsHint: "Desligado, o chip Agentes mostra só a contagem e não abre.",
      size: "Tamanho",
      panelSize: "Tamanho do painel",
      contentFont: "Tamanho da fonte do conteúdo",
      contentFontHint: "Vale para o texto da linha de sessão. O cabeçalho e a pílula não mudam.",
      panelMaxWidth: "Largura máxima do painel",
      panelMaxHeight: "Altura máxima do painel",
      panelMaxHeightHint: "A ilha nunca passa de um terço da altura da tela, mesmo com um valor maior.",
      completionCardHeight: "Altura do cartão de conclusão",
      uiScale: "Escala da interface",
      uiScaleHint: "Automática segue a densidade da tela. A ilha aplica na hora.",
      uiScaleAuto: (value: string | null): string =>
        value === null ? "Automática" : `Automática (${value})`,
      uiScaleValue: (value: number): string =>
        `${value.toLocaleString("pt-BR", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}×`,
    },
    status: {
      active: "Ativo",
      notDetected: "Não detectado",
    },
    stepper: {
      increase: "Aumentar",
      decrease: "Diminuir",
    },
    pickerDefault: (label: string): string => `${label} (Padrão)`,
    lockedByEnv: (variable: string): string => `Definido por ${variable}`,
    loadFailed: (reason: string): string => `Não deu para ler os ajustes: ${reason}`,
    saveFailed: (reason: string): string => `Não deu para salvar: ${reason}`,
    general: {
      system: "Sistema",
      autostart: "Iniciar com a sessão",
      autostartHint:
        "Sobe o daemon e a ilha junto com o Hyprland, e põe o Open Island no lançador de aplicativos.",
      hyprland: "Integração com o Hyprland",
      hyprlandHint:
        "Instala o atalho SUPER + I e a regra de janela desta tela. Reabra esta janela depois de instalar.",
      island: "Ilha",
      expandOnHover: "Abrir ao passar o mouse",
      collapseOnLeave: "Fechar ao tirar o mouse",
      hideInFullscreen: "Ocultar em tela cheia",
      hideInFullscreenHint: "Vale para qualquer janela em tela cheia, não só vídeos.",
      hideWhenIdle: "Ocultar quando os agentes param",
      hideWhenIdleHint:
        "A ilha some depois de 30 segundos sem nenhum agente trabalhando e volta quando algum retoma.",
      clickToJump: "Clicar na sessão abre o terminal",
      clickToJumpHint: "Desligado, a linha continua clicável, mas o foco não muda de janela.",
      cleanupAfter: "Remover sessões paradas depois de",
      cleanupAfterHint:
        "A sessão sai da lista e só volta se o agente responder de novo. Com 0, nunca sai.",
      hoverDwell: "Abrir quando o mouse ficar parado por",
      smartSuppression: "Não abrir sobre o terminal em foco",
      smartSuppressionHint:
        "Se o terminal do agente já estiver em primeiro plano, a ilha não abre sozinha.",
      autoCollapse: "Fechar automaticamente depois de",
      idleFade: "Ocultar quando não estiver em uso",
      idleFadeHint:
        "A ilha some depois do tempo abaixo sem o mouse por cima e sem novidade nas sessões. Volta ao passar o mouse ou quando uma sessão muda. Útil em telas OLED.",
      idleFadeAfter: "Ocultar depois de",
      sessions: "Sessões",
      idleAfter: "Marcar a sessão como parada depois de",
      idleAfterHint: "Muda a cor do indicador na linha da sessão.",
      idleReminderAfter: "Avisar de sessão parada depois de",
      idleReminderAfterHint:
        "Toca o som escolhido na aba Som. Não aparece no centro de notificações.",
    },
    sound: {
      output: "Saída",
      enabled: "Ativar sons",
      behaviour: "Como tocar",
      volume: "Volume",
      quiet: "Modo silencioso",
      quietHint: "Cala todos os sons. A ilha continua mostrando tudo.",
      followDnd: "Respeitar o Não Perturbe",
      followDndHint: "Não toca nada enquanto a swaync estiver em Não Perturbe.",
      events: "Som por evento",
      off: "Desligado",
      sessionStart: "Sessão começou",
      taskComplete: "Agente terminou",
      approvalNeeded: "Permissão ou pergunta",
      taskAcknowledge: "Prompt enviado",
      idleReminder: "Lembrete de sessão parada",
      contextLimit: "Limite de uso perto do fim",
      userSpam: "Prompts em rajada",
      userSpamHint: "Toca quando você dispara vários prompts em poucos segundos.",
      preview: (label: string): string => `Ouvir: ${label}`,
      previewFailed: (reason: string): string => `Não deu para tocar: ${reason}`,
      quietHours: "Horário silencioso",
      quietHoursHint: "Cala tudo no período. Se o fim vier antes do início, passa da meia-noite.",
      quietHoursStart: "Começa às",
      quietHoursEnd: "Termina às",
      spam: "Rajada de prompts",
      spamThreshold: "Considerar rajada a partir de",
      spamThresholdUnit: "prompts",
      spamWindow: "Dentro de",
      mySounds: "Meus sons",
      mySoundsHint: (dir: string): string =>
        `Ponha arquivos .oga, .ogg, .wav, .flac ou .mp3 em ${dir} e eles aparecem nas listas acima.`,
    },
    units: {
      milliseconds: "ms",
      seconds: "s",
      minutes: "min",
      percent: "%",
    },
    shortcut: {
      title: "Atalho global",
      label: "Mostrar ou ocultar a ilha",
      hint: "Deixe vazio para desativar.",
      placeholder: "Command+Shift+I",
      save: "Salvar",
      saved: "Atalho salvo.",
    },
    focus: {
      title: "Permissão de Foco",
      label: "Respeitar o estado de Foco do Mac",
      authorize: "Autorizar…",
      grantedWithoutState:
        "Autorizado, mas o sistema não compartilhou o estado. Confira Compartilhar Estado de Foco nos Ajustes do Sistema.",
      granted: "Autorizado. Os controles de Não Perturbe abaixo usam o estado compartilhado pelo Mac.",
      denied:
        "Acesso negado. Autorize Open Island nos ajustes de privacidade de Foco do macOS e reabra esta tela.",
      restricted: "O acesso ao Foco está restrito neste Mac. O modo silencioso e os horários continuam disponíveis.",
      unknown: "Não deu para consultar a permissão de Foco. Reabra os ajustes e tente de novo.",
      prompt: "Autorize o compartilhamento do estado de Foco para usar os controles de Não Perturbe.",
      requesting: "Esperando o macOS responder ao pedido…",
    },
    macos: {
      autostartHint: "Inicia a ilha e o daemon ao entrar na sua conta do Mac.",
      islandHeightHint: "0 usa a altura automática. Abaixo de 16 vale 16 pontos.",
      focusHint: "Segue o Foco que o macOS compartilha. Precisa da autorização acima.",
      screenOff: "Telas desligadas ou sessão bloqueada",
      screenOffHint:
        "Cala tudo com as telas em repouso, ao bloquear o Mac ou trocar de usuário. A detecção de bloqueio é experimental.",
      fullscreenHint:
        "Esconde enquanto o aplicativo ativo estiver em tela cheia do macOS. Janela só maximizada não conta.",
      notchFooter:
        "Ajuste a largura e a altura da ilha recolhida, em pontos da tela. Os ajustes partem de 0; a área da câmera continua reservada.",
      experimental:
        "macOS experimental: ainda não foi validado em um Mac real. O foco ativa o aplicativo; a janela ou aba exata depende do terminal.",
      manualUpdate: "A atualização abre o DMG da sua arquitetura. Troque o Open Island na mão, em Aplicativos.",
      newSessions: "Novas sessões",
      terminal: "Abrir no terminal",
      terminalHint: "Escolha um aplicativo instalado. A sessão abre em uma janela nova, na pasta que você escolher.",
    },
  },
} as const;
