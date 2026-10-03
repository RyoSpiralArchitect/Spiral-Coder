(() => {
  "use strict";

  const root = document.getElementById("app-root");
  if (!root) return;

  const { readStoredValue, rootUserTextForRun, serverSupportsFeature } = window.SpiralCoderState;
  const e = React.createElement;
  const { useCallback, useEffect, useMemo, useRef, useState } = React;

  // ╔══════════════════════════════════════════════════════════╗
  // ║  SECTION: Constants & i18n                               ║
  // ╚══════════════════════════════════════════════════════════╝
  const LS = {
    lang: "spiral-coder.lang.v1",
    config: "spiral-coder.config.v1",
    threads: "spiral-coder.threads.v1",
    active: "spiral-coder.active.v1",
    // Bump version to reset the default split for readability (Observer critiques are the product).
    splitPct: "spiral-coder.splitPct.v2",
  };

  // ── i18n strings (en / ja / fr) ──────────────────────────────────────────────
  const I18N = {
    en: {
      threads: "Threads",
      newThread: "New",
      rename: "Rename",
      del: "Delete",
      delConfirm: "Delete this thread?",
      settings: "Settings",
      presets: "Presets",
      chat: "Chat",
      coder: "Coder",
      observer: "Observer",
      proposals: "Proposals",
      tasks: "Tasks",
      planningTasks: "Planning tasks…",
      sendToCoder: "Send to coder",
      sendToObserver: "Send to observer",
      done: "Done",
      approved: "approved",
      alreadyApproved: "Already approved",
      phaseMismatch: "Phase mismatch",
      phaseMismatchConfirm: "Phase mismatch (current: {cur} / proposal: {prop}). Send anyway?",
      applyMeta: "Apply",
      includeCoderContext: "Include coder context",
      chatAttachRuntime: "Attach runtime snapshot",
      chatAutoTasks: "Auto tasks",
      contextPreview: "Context preview",
      chatExplainLastError: "Explain last error",
      chatWhatsHappening: "What's happening?",
      insertCliTemplate: "CLI template",
      editApproval: "Edit approval",
      commandApproval: "Command approval",
      autoObserve: "Auto-observe",
      observerHint: "Type a message below to start observing, or enable Auto-observe in settings.",
      settingsHint: "Providers, models, approvals policy, and runtime defaults.",
      forceAgent: "Agent mode",
      toolRoot: "Tool root",
      workdir: "Workdir",
      findInThread: "Find in thread…",
      noMatches: "No matches",
      pendingEdits: "Pending edits",
      pendingCommands: "Pending commands",
      approvals: "Approvals",
      openApprovals: "Open approvals",
      runtimeApprovals: "Runtime approvals",
      openRuntimeApprovals: "Open runtime approvals",
      harnessReviews: "Harness reviews",
      openHarnessReviews: "Open harness reviews",
      mergeGateReviews: "Merge gate",
      openMergeGateReviews: "Open merge gate",
      approve: "Approve",
      hold: "Hold",
      applyToContract: "Apply to contract",
      harnessPromotions: "Harness promotions",
      harnessReviewHint: "Eval-green policy promotions waiting for a human decision before they reach shared/governor_contract.json.",
      mergeGateHint: "Runtime eval merge readiness with human approve/hold decisions and rollback previews. Rollback commands are never executed from this panel.",
      runtimeApprovalHint: "Pending edits and commands that need a human decision before execution can continue.",
      greenCases: "green case(s)",
      nothingPending: "Nothing pending.",
      needsReview: "needs review",
      held: "held",
      applied: "applied",
      upToDate: "up to date",
      blocked: "blocked",
      rollbackAvailable: "rollback available",
      rollbackPreview: "rollback preview",
      passed: "passed",
      failed: "failed",
      promotionNone: "No promotion candidates yet.",
      mergeGateNone: "No merge gate artifact yet.",
      requiresRustServer: "Requires the Rust server (spiral-coder serve).",
      reject: "Reject",
      send: "Send",
      stop: "Stop",
      exportMd: "Markdown",
      clear: "Clear",
      provider: "Provider",
      model: "Model",
      chatModel: "Chat model",
      codeModel: "Code model",
      fetchModels: "Fetch models",
      baseUrl: "Base URL",
      codeProvider: "Code provider",
      codeBaseUrl: "Code Base URL",
      observerProvider: "Observer provider",
      observerBaseUrl: "Observer Base URL",
      observerModel: "Observer model",
      apiKey: "API key",
      apiKeyChat: "Chat API key",
      apiKeyCode: "Code API key",
      apiKeyObserver: "Observer API key",
      sameAsChat: "(same as chat)",
      mode: "Mode",
      persona: "Persona",
      temperature: "temperature",
      maxTokens: "max_tokens",
      timeoutSeconds: "timeout_seconds",
      stream: "Streaming",
      cot: "CoT",
      brief: "Brief",
      structured: "Structured",
      deep: "Deep",
      on: "ON",
      off: "OFF",
      diff: "diff",
      diffHint: "Auto-injected in diff review mode",
      placeholder: "Type here. Enter to send (Shift+Enter for newline)",
      ready: "ready",
      sending: "sending…",
      streaming: "streaming…",
      error: "error",
      fetchFailed: "Failed to fetch (is Spiral-Coder serve running?)",
      copy: "Copy",
      reader: "Read",
      refresh: "Refresh",
      keys: "API keys",
      loopDetected: "Loop detected",
      observerIntensity: "Observer intensity",
      observerLang: "Observer language",
      polite: "polite",
      critical: "critical",
      brutal: "brutal",
      coderMaxIters: "Coder max iters",
      vibeAgent: "Vibe agent",
      vibeMaxTurns: "Vibe max turns",
      details: "details",
      hide: "hide",
      serverOutdated: "Server outdated",
      more: "More",
      less: "Less",
      shortcuts: "Keyboard shortcuts",
      close: "Close",
      delQ: "Delete?",
      yes: "Yes",
      no: "No",
      sendMsg: "Send message",
      newline: "New line",
      toggleHelp: "Toggle this help",
      closeModals: "Close modals / cancel rename",
      stopStreamingCoder: "Stop streaming (Coder/Observer)",
      focusCoder: "Focus coder",
      focusObserver: "Focus observer",
      splitHint: "Drag to resize. Double-click to reset.",
      metaDiagnose: "Meta diagnose",
      metaBadge: "META",
      nextActionBadge: "NEXT",
      whyFail: "Why did this fail?",
      suggestNext: "Suggest next step",
      nextActionRunning: "Observer is suggesting the next step…",
      nextActionMissingTarget: "No stuck/failing coder message found.",
      metaDiagnoseRunning: "Running meta diagnosis…",
      metaDiagnoseMissingTarget: "No failed message found to diagnose.",
      metaDiagnoseBadTarget: "Target message not found.",
      metaDiagnoseJsonRetry: "Meta diagnosis returned invalid JSON. Retrying once…",
      metaDiagnoseSaved: "Meta diagnosis saved",
      metaDiagnoseSaveFailed: "Failed to save meta diagnosis",
      metaViewer: "Meta",
      metaViewerRefresh: "Refresh",
      metaViewerParseOkOnly: "parse_ok only",
      metaViewerThreadFilter: "Filter thread",
      metaViewerEmpty: "No saved meta diagnoses.",
      metaViewerSelect: "Select a meta diagnosis.",
      metaViewerLoadFailed: "Failed to load meta diagnoses",
      metaViewerReadFailed: "Failed to load artifact",
      metaViewerOpenJson: "Open artifact JSON",
      metaViewerRerun: "Re-run diagnosis",
      metaViewerRerunSavedPacket: "Live target missing. Re-running from saved packet…",
      metaViewerSummary: "summary",
      metaViewerCauses: "causes",
      metaViewerExperiments: "experiments",
      metaViewerRawResponse: "raw_response",
    },
    ja: {
      threads: "スレッド",
      newThread: "新規",
      rename: "名前",
      del: "削除",
      delConfirm: "このスレッドを削除しますか？",
      settings: "設定",
      presets: "プリセット",
      chat: "チャット",
      coder: "コーダー",
      observer: "オブザーバー",
      proposals: "提案",
      tasks: "タスク",
      planningTasks: "タスク生成中…",
      sendToCoder: "Coderへ送る",
      sendToObserver: "Observerへ送る",
      done: "完了",
      approved: "承認済み",
      alreadyApproved: "すでに送信済み",
      phaseMismatch: "フェーズ不一致",
      phaseMismatchConfirm: "フェーズが一致しません（現在: {cur} / 提案: {prop}）。それでもCoderへ送りますか？",
      applyMeta: "適用",
      includeCoderContext: "Coder状況を付与",
      chatAttachRuntime: "ランタイム状況を付与",
      chatAutoTasks: "自動タスク化",
      contextPreview: "コンテキスト表示",
      chatExplainLastError: "直近エラー相談",
      chatWhatsHappening: "いま何してる？",
      send: "送信",
      stop: "停止",
      exportMd: "Markdown",
      clear: "消去",
      provider: "プロバイダ",
      model: "モデル",
      chatModel: "チャットモデル",
      codeModel: "コードモデル",
      fetchModels: "モデル一覧取得",
      baseUrl: "Base URL",
      apiKey: "APIキー",
      apiKeyChat: "チャットAPIキー",
      apiKeyCode: "コードAPIキー",
      apiKeyObserver: "Observer APIキー",
      codeProvider: "コード用プロバイダ",
      codeBaseUrl: "コード用Base URL",
      observerProvider: "Observerプロバイダ",
      observerBaseUrl: "Observer Base URL",
      observerModel: "Observerモデル",
      sameAsChat: "（チャットと同じ）",
      mode: "モード",
      persona: "ペルソナ",
      temperature: "temperature",
      maxTokens: "max_tokens",
      timeoutSeconds: "timeout_seconds",
      stream: "ストリーミング",
      cot: "CoT",
      brief: "簡易",
      structured: "構造化",
      deep: "本格",
      autoObserve: "自動実況",
      observerHint: "下の入力欄にメッセージを送信するか、設定で「自動実況」をONにしてください。",
      settingsHint: "provider / model / 承認ポリシー / runtime 既定値をまとめています。",
      forceAgent: "エージェント常時ON",
      toolRoot: "作業ルート",
      workdir: "作業ディレクトリ",
      findInThread: "スレッド内検索…",
      noMatches: "一致するメッセージがありません",
      on: "ON",
      off: "OFF",
      diff: "diff",
      diffHint: "diff批評モードで自動注入されます",
      placeholder: "ここに入力。Enterで送信 (Shift+Enterで改行)",
      ready: "ready",
      sending: "sending…",
      streaming: "streaming…",
      error: "error",
      fetchFailed: "通信できません（spiral-coder serve が起動してる？ポート合ってる？）",
      copy: "コピー",
      reader: "読む",
      refresh: "更新",
      keys: "APIキー状況",
      loopDetected: "ループ検出",
      observerIntensity: "Observer強度",
      polite: "丁寧",
      critical: "批評",
      brutal: "容赦なし",
      coderMaxIters: "Coder最大反復",
      vibeAgent: "Vibe agent",
      vibeMaxTurns: "Vibe max turns",
      details: "詳細",
      hide: "隠す",
      serverOutdated: "サーバ古い",
      more: "もっと見る",
      less: "折りたたむ",
      shortcuts: "ショートカット",
      close: "閉じる",
      delQ: "削除しますか？",
      yes: "はい",
      no: "いいえ",
      sendMsg: "送信",
      newline: "改行",
      toggleHelp: "ヘルプ表示切替",
      closeModals: "モーダルを閉じる / リネーム取消",
      stopStreamingCoder: "ストリーミング停止（Coder/Observer）",
      focusCoder: "コーダーを広げる",
      focusObserver: "オブザーバーを広げる",
      splitHint: "ドラッグで幅調整。ダブルクリックでリセット。",
      insertCliTemplate: "CLIテンプレート",
      editApproval: "編集承認",
      commandApproval: "コマンド承認",
      pendingEdits: "保留中の編集",
      pendingCommands: "保留中のコマンド",
      approvals: "承認",
      openApprovals: "保留中の承認",
      runtimeApprovals: "ランタイム承認",
      openRuntimeApprovals: "ランタイム承認を開く",
      harnessReviews: "ハーネスレビュー",
      openHarnessReviews: "ハーネスレビューを開く",
      mergeGateReviews: "マージゲート",
      openMergeGateReviews: "マージゲートを開く",
      approve: "承認",
      hold: "保留",
      applyToContract: "contractへ反映",
      harnessPromotions: "ハーネス昇格候補",
      harnessReviewHint: "eval を通った policy 昇格候補を、人間の判断で shared/governor_contract.json に上げるための受け皿です。",
      mergeGateHint: "runtime eval の merge 判定を、人間の approve / hold と rollback preview で確認します。この画面から rollback は実行しません。",
      runtimeApprovalHint: "実行継続の前に人間判断が必要な edits / commands をまとめています。",
      greenCases: "green case",
      nothingPending: "保留中の項目はありません。",
      needsReview: "要レビュー",
      held: "保留中",
      applied: "反映済み",
      upToDate: "最新",
      blocked: "ブロック中",
      rollbackAvailable: "rollback可能",
      rollbackPreview: "rollback preview",
      passed: "passed",
      failed: "failed",
      promotionNone: "昇格候補はまだありません。",
      mergeGateNone: "merge gate artifact はまだありません。",
      requiresRustServer: "Rust サーバー（spiral-coder serve）で利用できます。",
      reject: "却下",
      metaDiagnose: "メタ診断",
      metaBadge: "META",
      nextActionBadge: "NEXT",
      whyFail: "Why did this fail?",
      suggestNext: "次の一手",
      nextActionRunning: "Observerが次の一手を提案中…",
      nextActionMissingTarget: "煮詰まり・失敗状態のCoderメッセージが見つかりません。",
      metaDiagnoseRunning: "メタ診断を実行中…",
      metaDiagnoseMissingTarget: "診断対象の失敗メッセージが見つかりません。",
      metaDiagnoseBadTarget: "対象メッセージが見つかりません。",
      metaDiagnoseJsonRetry: "メタ診断が不正なJSONを返したため、1回だけ再試行します…",
      metaDiagnoseSaved: "メタ診断を保存しました",
      metaDiagnoseSaveFailed: "メタ診断の保存に失敗しました",
      metaViewer: "Meta",
      metaViewerRefresh: "更新",
      metaViewerParseOkOnly: "parse_ok のみ",
      metaViewerThreadFilter: "thread で絞り込み",
      metaViewerEmpty: "保存済みメタ診断はまだありません。",
      metaViewerSelect: "メタ診断を選択してください。",
      metaViewerLoadFailed: "メタ診断一覧の読み込みに失敗しました",
      metaViewerReadFailed: "artifact の読み込みに失敗しました",
      metaViewerOpenJson: "artifact JSON を開く",
      metaViewerRerun: "再診断",
      metaViewerRerunSavedPacket: "live の対象が無いため、保存済み packet から再診断します…",
      metaViewerSummary: "summary",
      metaViewerCauses: "causes",
      metaViewerExperiments: "experiments",
      metaViewerRawResponse: "raw_response",
    },
    fr: {
      threads: "Fils",
      newThread: "Nouveau",
      rename: "Renommer",
      del: "Suppr.",
      delConfirm: "Supprimer ce fil ?",
      settings: "Réglages",
      presets: "Préréglages",
      chat: "Chat",
      coder: "Codeur",
      observer: "Observateur",
      proposals: "Propositions",
      tasks: "Tâches",
      planningTasks: "Planification des tâches…",
      sendToCoder: "Envoyer au codeur",
      sendToObserver: "Envoyer à l'observer",
      done: "Fait",
      approved: "approuvé",
      alreadyApproved: "Déjà approuvé",
      phaseMismatch: "Phase incompatible",
      phaseMismatchConfirm: "Phase incompatible (actuelle: {cur} / proposition: {prop}). Envoyer quand même ?",
      applyMeta: "Appliquer",
      includeCoderContext: "Inclure contexte codeur",
      chatAttachRuntime: "Joindre snapshot runtime",
      chatAutoTasks: "Tâches auto",
      contextPreview: "Contexte",
      chatExplainLastError: "Expliquer la dernière erreur",
      chatWhatsHappening: "Que se passe-t-il ?",
      insertCliTemplate: "Template CLI",
      editApproval: "Approbation édition",
      commandApproval: "Approbation commande",
      toolRoot: "Racine outils",
      workdir: "Répertoire de travail",
      findInThread: "Rechercher dans le fil…",
      noMatches: "Aucun résultat",
      pendingEdits: "Éditions en attente",
      pendingCommands: "Commandes en attente",
      approvals: "Approbations",
      openApprovals: "Approbations en attente",
      runtimeApprovals: "Approbations runtime",
      openRuntimeApprovals: "Ouvrir les approbations runtime",
      harnessReviews: "Revues du harnais",
      openHarnessReviews: "Ouvrir les revues du harnais",
      mergeGateReviews: "Gate de merge",
      openMergeGateReviews: "Ouvrir la gate de merge",
      approve: "Approuver",
      hold: "Mettre en attente",
      applyToContract: "Appliquer au contrat",
      harnessPromotions: "Promotions du harnais",
      harnessReviewHint: "Promotions de policy passées au vert par l'eval et en attente d'une décision humaine avant d'atteindre shared/governor_contract.json.",
      mergeGateHint: "État de merge des evals runtime avec décisions humaines approve/hold et aperçu rollback. Les commandes rollback ne sont jamais exécutées depuis ce panneau.",
      runtimeApprovalHint: "Éditions et commandes qui demandent une décision humaine avant la reprise de l'exécution.",
      greenCases: "cas verts",
      nothingPending: "Rien en attente.",
      needsReview: "à revoir",
      held: "en attente",
      applied: "appliqué",
      upToDate: "à jour",
      blocked: "bloqué",
      rollbackAvailable: "rollback disponible",
      rollbackPreview: "aperçu rollback",
      passed: "passé",
      failed: "échoué",
      promotionNone: "Aucun candidat de promotion pour le moment.",
      mergeGateNone: "Aucun artifact merge gate pour le moment.",
      requiresRustServer: "Nécessite le serveur Rust (spiral-coder serve).",
      reject: "Rejeter",
      send: "Envoyer",
      stop: "Stop",
      exportMd: "Markdown",
      clear: "Effacer",
      provider: "Fournisseur",
      model: "Modèle",
      chatModel: "Modèle chat",
      codeModel: "Modèle code",
      fetchModels: "Charger modèles",
      baseUrl: "Base URL",
      codeProvider: "Provider code",
      codeBaseUrl: "Base URL code",
      observerProvider: "Provider observer",
      observerBaseUrl: "Base URL observer",
      observerModel: "Modèle observer",
      apiKey: "Clé API",
      apiKeyChat: "Clé API chat",
      apiKeyCode: "Clé API code",
      apiKeyObserver: "Clé API observer",
      sameAsChat: "(idem chat)",
      mode: "Mode",
      persona: "Persona",
      temperature: "temperature",
      maxTokens: "max_tokens",
      timeoutSeconds: "timeout_seconds",
      stream: "Streaming",
      cot: "CoT",
      brief: "Bref",
      structured: "Structuré",
      deep: "Approfondi",
      on: "ON",
      off: "OFF",
      diff: "diff",
      diffHint: "Auto-injecté en mode revue de diff",
      placeholder: "Tapez ici. Entrée pour envoyer (Maj+Entrée pour nouvelle ligne)",
      ready: "prêt",
      sending: "envoi…",
      streaming: "stream…",
      error: "erreur",
      fetchFailed: "Échec de requête (Spiral-Coder serve est-il lancé ?)",
      copy: "Copier",
      reader: "Lire",
      refresh: "Rafraîchir",
      keys: "Clés API",
      loopDetected: "Boucle détectée",
      observerIntensity: "Intensité observer",
      polite: "poli",
      critical: "critique",
      brutal: "brutal",
      coderMaxIters: "Iters max codeur",
      vibeAgent: "Agent Vibe",
      vibeMaxTurns: "Tours max Vibe",
      details: "détails",
      hide: "masquer",
      autoObserve: "Auto-commenter",
      observerHint: "Tapez un message ci-dessous pour commencer, ou activez Auto-commenter dans les paramètres.",
      settingsHint: "Fournisseurs, modèles, politique d'approbation et valeurs runtime par défaut.",
      forceAgent: "Mode agent (toujours)",
      serverOutdated: "Serveur obsolète",
      more: "Afficher plus",
      less: "Réduire",
      shortcuts: "Raccourcis clavier",
      close: "Fermer",
      delQ: "Supprimer ?",
      yes: "Oui",
      no: "Non",
      sendMsg: "Envoyer",
      newline: "Nouvelle ligne",
      toggleHelp: "Afficher/masquer l'aide",
      closeModals: "Fermer les modales / annuler le renommage",
      stopStreamingCoder: "Arrêter le streaming (Coder/Observer)",
      focusCoder: "Agrandir Coder",
      focusObserver: "Agrandir Observer",
      splitHint: "Glissez pour redimensionner. Double-cliquez pour réinitialiser.",
      metaDiagnose: "Meta diagnose",
      metaBadge: "META",
      whyFail: "Pourquoi cet échec ?",
      nextActionBadge: "NEXT",
      suggestNext: "Suggérer la suite",
      nextActionRunning: "L'observer propose la prochaine action…",
      nextActionMissingTarget: "Aucun message codeur bloqué/en échec trouvé.",
      metaDiagnoseRunning: "Diagnostic méta en cours…",
      metaDiagnoseMissingTarget: "Aucun message d'échec à diagnostiquer.",
      metaDiagnoseBadTarget: "Message cible introuvable.",
      metaDiagnoseJsonRetry: "Le diagnostic méta a renvoyé un JSON invalide. Nouvelle tentative unique…",
      metaDiagnoseSaved: "Diagnostic méta enregistré",
      metaDiagnoseSaveFailed: "Échec de l'enregistrement du diagnostic méta",
      metaViewer: "Meta",
      metaViewerRefresh: "Actualiser",
      metaViewerParseOkOnly: "parse_ok seulement",
      metaViewerThreadFilter: "Filtrer thread",
      metaViewerEmpty: "Aucun diagnostic méta enregistré.",
      metaViewerSelect: "Sélectionnez un diagnostic méta.",
      metaViewerLoadFailed: "Échec du chargement des diagnostics méta",
      metaViewerReadFailed: "Échec du chargement de l'artifact",
      metaViewerOpenJson: "Ouvrir l'artifact JSON",
      metaViewerRerun: "Relancer le diagnostic",
      metaViewerRerunSavedPacket: "Cible live introuvable. Relance depuis le packet sauvegardé…",
      metaViewerSummary: "summary",
      metaViewerCauses: "causes",
      metaViewerExperiments: "experiments",
      metaViewerRawResponse: "raw_response",
    },
  };

  function tr(lang, key) {
    return (I18N[lang] && I18N[lang][key]) || I18N.en[key] || key;
  }

  // ── Plugin Registry ───────────────────────────────────────────────────────────
  // Extend Spiral-Coder without forking the source.
  //
  // Usage — load your plugin via <script src="my-plugin.js"></script> before app.js:
  //   registerObserverPlugin({ name, onProposal, onHealth, onPhase })
  //   registerPhase(key, { label, color, description })
  //   registerValidator(fn)   // fn(proposals[]) -> proposals[]
  //
  // Hooks are currently scaffolded (no-op). Integration points will be added
  // in parseProposals() and parseHealthScore() as Phase C matures.
  const _SPIRAL_CODER_PLUGINS = { observer: [], phases: {}, validators: [] };
  function registerObserverPlugin(p) { _SPIRAL_CODER_PLUGINS.observer.push(p); }
  function registerPhase(key, cfg)   { _SPIRAL_CODER_PLUGINS.phases[key] = cfg; }
  function registerValidator(fn)     { _SPIRAL_CODER_PLUGINS.validators.push(fn); }

  // ╔══════════════════════════════════════════════════════════╗
  // ║  SECTION: Utils                                          ║
  // ╚══════════════════════════════════════════════════════════╝
  function safeJsonParse(s, fallback) {
    try {
      return JSON.parse(s);
    } catch (_) {
      return fallback;
    }
  }

  function uid() {
    try {
      if (crypto && crypto.randomUUID) return crypto.randomUUID();
    } catch (_) {}
    return "id-" + Math.random().toString(16).slice(2) + "-" + Date.now().toString(16);
  }

  function extractJsonObject(text) {
    const s = String(text || "").trim();
    if (!s) return null;
    const i = s.indexOf("{");
    const j = s.lastIndexOf("}");
    if (i < 0 || j <= i) return null;
    return s.slice(i, j + 1);
  }

  function parseTasksJson(text) {
    const raw = extractJsonObject(text);
    if (!raw) return [];
    const j = safeJsonParse(raw, null);
    if (!j || typeof j !== "object" || !Array.isArray(j.tasks)) return [];
    return j.tasks
      .filter((t) => t && typeof t === "object")
      .map((t) => ({
        id: typeof t.id === "string" && t.id ? t.id : uid(),
        target: String(t.target || "").trim().toLowerCase() === "observer" ? "observer" : "coder",
        title: String(t.title || "").trim(),
        body: String(t.body || "").trim(),
        phase: String(t.phase || "any").trim().toLowerCase(),
        priority: typeof t.priority === "number" ? Math.max(0, Math.min(100, t.priority)) : null,
        status: String(t.status || "new").trim().toLowerCase() || "new",
        createdAt: typeof t.createdAt === "number" ? t.createdAt : Date.now(),
      }))
      .filter((t) => t.title || t.body);
  }

  // Similarity + proposal parsing helpers live in `web/observer/logic.js`
  // and are exposed on `window.SpiralCoder.observer`.

  function autoObservePrompt(uiLang) {
    const l = String(uiLang || "").trim().toLowerCase();
    if (l === "fr") {
      return "[AUTO-OBSERVE] Le Coder vient de produire une nouvelle sortie. Fais une critique en mode commentaire live.\n"
        + "1) Commence par UNE phrase: qu'est-ce qui vient d'arriver.\n"
        + "2) Analyse les risques sur 5 axes: exactitude, sécurité, fiabilité, performance, maintenabilité.\n"
        + "3) Termine par LES QUATRE blocs structurés dans cet ordre exact:\n"
        + "   --- phase ---          (core | feature | polish)\n"
        + "   --- proposals ---      (scorées, avec quote pour warn/crit)\n"
        + "   --- critical_path ---\n"
        + "   --- health ---         (score: N  rationale: une phrase)";
    }
    if (l === "en") {
      return "[AUTO-OBSERVE] The Coder produced new output. Commentate and critique live.\n"
        + "1) Start with ONE sentence: what just happened.\n"
        + "2) Risk scan across 5 axes: correctness, security, reliability, performance, maintainability.\n"
        + "3) End with ALL FOUR structured blocks in this exact order:\n"
        + "   --- phase ---          (core | feature | polish)\n"
        + "   --- proposals ---      (scored, with quote for warn/crit)\n"
        + "   --- critical_path ---\n"
        + "   --- health ---         (score: N  rationale: one sentence)";
    }
    return "[AUTO-OBSERVE] コーダーが新しいアウトプットを生成した。実況しながら批評せよ。\n"
      + "1) まず一文で「何が起きたか」を述べる。\n"
      + "2) 5軸（正しさ/セキュリティ/信頼性/性能/保守性）でリスクを洗い出す。\n"
      + "3) 最後に以下の4ブロックを必ず順番通りに出力する:\n"
      + "   --- phase ---          (core | feature | polish)\n"
      + "   --- proposals ---      (スコア付き、warn/crit には quote 必須)\n"
      + "   --- critical_path ---\n"
      + "   --- health ---         (score: N  rationale: 一文)";
  }

  const PROVIDERS = [
    { k: "mistral", l: { ja: "Mistral", en: "Mistral", fr: "Mistral" } },
    { k: "codestral", l: { ja: "Codestral", en: "Codestral", fr: "Codestral" } },
    { k: "mistral-cli", l: { ja: "Mistral CLI", en: "Mistral CLI", fr: "Mistral CLI" } },
    { k: "openai-compatible", l: { ja: "OpenAI互換", en: "OpenAI compat", fr: "Compat OpenAI" } },
    { k: "gemini", l: { ja: "Gemini", en: "Gemini", fr: "Gemini" } },
    { k: "anthropic", l: { ja: "Anthropic", en: "Anthropic", fr: "Anthropic" } },
    { k: "hf", l: { ja: "HF local", en: "HF local", fr: "HF local" } },
  ];

  const MODES = [
    { k: "VIBE", l: { ja: "VIBE", en: "VIBE", fr: "VIBE" } },
    { k: "壁打ち", l: { ja: "壁打ち", en: "Ideation", fr: "Idéation" } },
    { k: "実況", l: { ja: "実況", en: "Live", fr: "Direct" } },
    { k: "Observer", l: { ja: "Observer", en: "Observer", fr: "Observateur" } },
    { k: "diff批評", l: { ja: "diff批評", en: "Diff review", fr: "Revue diff" } },
  ];

  const CODER_MODES = MODES.filter((m) => m && m.k !== "Observer");
  const OBSERVER_MODES = MODES.filter((m) => m && m.k === "Observer");

  const PERSONAS = ["default", "novelist", "cynical", "cheerful", "thoughtful", "sensei", "duck"];
  const CHAT_PERSONAS = [
    { key: "cheerful",   icon: "😊", ja: "陽気",   en: "Cheerful",    fr: "Enjoué" },
    { key: "thoughtful", icon: "🤔", ja: "思慮深い", en: "Thoughtful",  fr: "Réfléchi" },
    { key: "sensei",     icon: "🧙", ja: "師匠",    en: "Sensei",      fr: "Sensei" },
    { key: "cynical",    icon: "😏", ja: "皮肉屋",  en: "Cynical",     fr: "Cynique" },
    { key: "duck",       icon: "🦆", ja: "ゴム鴨",  en: "Duck",        fr: "Canard" },
  ];

  const PRESETS = {
    vibe: {
      vibe: true,
      provider: "mistral",
      model: "codestral-latest",
      chatModel: "mistral-small-latest",
      codeModel: "codestral-latest",
      baseUrl: "https://api.mistral.ai/v1",
      codeProvider: "codestral",
      codeBaseUrl: "https://codestral.mistral.ai/v1",
      mode: "VIBE",
    },
    openai: {
      vibe: false,
      provider: "openai-compatible",
      model: "gpt-4o-mini",
      chatModel: "gpt-4o-mini",
      codeModel: "gpt-4o-mini",
      baseUrl: "https://api.openai.com/v1",
      mode: "壁打ち",
    },
    gemini: {
      vibe: false,
      provider: "gemini",
      model: "gemini-2.0-flash",
      chatModel: "gemini-2.0-flash",
      codeModel: "gemini-2.0-flash",
      baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
      mode: "壁打ち",
    },
    codestral: {
      vibe: true,
      provider: "codestral",
      model: "codestral-latest",
      chatModel: "mistral-small-latest",
      codeModel: "codestral-latest",
      baseUrl: "https://codestral.mistral.ai/v1",
      mode: "VIBE",
    },
    anthropic: {
      vibe: false,
      provider: "anthropic",
      model: "claude-3-5-sonnet-latest",
      chatModel: "claude-3-5-sonnet-latest",
      codeModel: "claude-3-5-sonnet-latest",
      baseUrl: "https://api.anthropic.com/v1",
      mode: "壁打ち",
    },
  };

  const DEFAULT_CONFIG = {
    ...PRESETS.openai,
    cot: "brief",
    autonomy: "longrun",
    requireEditApproval: true,
    requireCommandApproval: true,
    autoObserve: false,
    forceAgent: true,
    coderMaxIters: "14",
    toolRoot: ".tmp",
    mistralCliAgent: "",
    mistralCliMaxTurns: "",
    codeProvider: "",
    codeBaseUrl: "",
    observerProvider: "",
    observerBaseUrl: "",
    observerModel: "",
    persona: "default",
    chatPersona: "cheerful",
    observerMode: "Observer",
    observerPersona: "novelist",
    observerIntensity: "critical",
    // Default to auto so Observer follows the conversation language even if the UI is in English.
    observerLang: "auto",
    includeCoderContext: true,
    chatAttachRuntime: true,
    chatAutoTasks: true,
    temperature: "0.7",
    maxTokens: "1024",
    timeoutSeconds: "120",
    stream: true,
  };

  function numOrUndef(v) {
    const s = String(v == null ? "" : v).trim();
    if (!s) return undefined;
    const n = Number(s);
    return Number.isFinite(n) ? n : undefined;
  }

  function strOrUndef(v) {
    const s = String(v == null ? "" : v).trim();
    return s ? s : undefined;
  }

  function buildReq(cfg, apiKey, history, input, diff) {
    return {
      input,
      history: history && history.length ? history : undefined,
      diff: strOrUndef(diff),
      vibe: cfg.vibe ? true : undefined,
      provider: strOrUndef(cfg.provider),
      model: strOrUndef(cfg.model),
      chat_model: strOrUndef(cfg.chatModel || cfg.model),
      code_model: strOrUndef(cfg.codeModel || cfg.model),
      base_url: strOrUndef(cfg.baseUrl),
      api_key: strOrUndef(apiKey),
      tool_root: strOrUndef(cfg.toolRoot),
      mistral_cli_agent: strOrUndef(cfg.mistralCliAgent),
      mistral_cli_max_turns: numOrUndef(cfg.mistralCliMaxTurns),
      mode: strOrUndef(cfg.mode),
      cot: strOrUndef(cfg.cot),
      autonomy: strOrUndef(cfg.autonomy),
      require_edit_approval: typeof cfg.requireEditApproval === "boolean" ? cfg.requireEditApproval : undefined,
      require_command_approval: typeof cfg.requireCommandApproval === "boolean" ? cfg.requireCommandApproval : undefined,
      persona: strOrUndef(cfg.persona),
      temperature: numOrUndef(cfg.temperature),
      max_tokens: numOrUndef(cfg.maxTokens),
      timeout_seconds: numOrUndef(cfg.timeoutSeconds),
    };
  }

  function avatarLabel(m) {
    if (m && m.role === "user") return "U";
    if (m && m.pane === "chat") return "A";
    return m && m.pane === "observer" ? "O" : "C";
  }

  function whoLabel(m, lang) {
    const L = String(lang || "ja").trim().toLowerCase();
    if (m && m.role === "user") return L === "fr" ? "Vous" : L === "en" ? "You" : "あなた";
    if (m && m.pane === "chat") return "AI";
    return tr(L, m && m.pane === "observer" ? "observer" : "coder");
  }

  function makeThread(title) {
    return {
      id: uid(),
      title: title || "Untitled",
      createdAt: Date.now(),
      updatedAt: Date.now(),
      workdir: "",
      messages: [],
      tasks: [],
      coderMem: { cmdStats: [] },
      coderObsEvidence: { reads: [], searches: [] },
      observerMem: { proposal_counts: {} },
    };
  }

  function titleFrom(text) {
    const t = String(text || "").trim().replace(/\\s+/g, " ");
    if (!t) return "Untitled";
    return t.length > 28 ? t.slice(0, 28) + "…" : t;
  }

  function downloadText(filename, text) {
    const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1500);
  }

  function transcriptMd(thread, meta) {
    const lines = ["# Spiral-Coder transcript", ""];
    if (meta) {
      lines.push("```");
      Object.keys(meta).forEach((k) => lines.push(`${k}: ${meta[k]}`));
      lines.push("```", "");
    }
    (thread.messages || []).forEach((m) => {
      const pane = m.pane === "observer" ? "observer" : m.pane === "chat" ? "chat" : "coder";
      lines.push(`## ${pane} / ${m.role}`, "", String(m.content || "").trimEnd(), "");
    });
    return lines.join("\\n");
  }

  async function readFileText(file) {
    return new Promise((resolve, reject) => {
      const r = new FileReader();
      r.onerror = () => reject(new Error("read failed"));
      r.onload = () => resolve(String(r.result || ""));
      r.readAsText(file);
    });
  }

  // ╔══════════════════════════════════════════════════════════╗
  // ║  SECTION: API & Streaming                                ║
  // ╚══════════════════════════════════════════════════════════╝
  async function postJson(url, body, signal) {
    const resp = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal,
    });

    const ct = (resp.headers.get("content-type") || "").toLowerCase();
    if (!resp.ok) {
      if (ct.includes("application/json")) {
        const j = await resp.json().catch(() => ({}));
        throw new Error(j.error || `HTTP ${resp.status}`);
      }
      const t = await resp.text().catch(() => "");
      throw new Error(t.trim() || `HTTP ${resp.status}`);
    }

    if (ct.includes("application/json")) return resp.json();
    return resp.text();
  }

  let governorContractCache = null;
  let governorContractPromise = null;
  async function getGovernorContract() {
    if (governorContractCache) return governorContractCache;
    if (!governorContractPromise) {
      governorContractPromise = fetch("/api/governor_contract")
        .then(async (resp) => {
          if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
          return resp.json();
        })
        .then((json) => {
          governorContractCache = json || null;
          return governorContractCache;
        })
        .catch(() => {
          governorContractCache = DEFAULT_GOVERNOR_CONTRACT;
          return governorContractCache;
        });
    }
    return governorContractPromise;
  }

  function parseSseFrame(frameText) {
    const lines = frameText.split(/\r?\n/);
    let event = "message";
    const dataLines = [];
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (!line) continue;
      if (line.startsWith(":")) continue;
      if (line.startsWith("event:")) event = line.slice(6).trim();
      if (line.startsWith("data:")) dataLines.push(line.slice(5).trimStart());
    }
    return { event, data: dataLines.join("\n") };
  }

  async function streamChat(body, onEvent, signal) {
    const resp = await fetch("/api/chat_stream", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal,
    });

    const ct = (resp.headers.get("content-type") || "").toLowerCase();
    if (!resp.ok) {
      if (ct.includes("application/json")) {
        const j = await resp.json().catch(() => ({}));
        throw new Error(j.error || `HTTP ${resp.status}`);
      }
      throw new Error(`HTTP ${resp.status}`);
    }

    if (!resp.body || !resp.body.getReader) throw new Error("stream not supported");

    const reader = resp.body.getReader();
    const dec = new TextDecoder();
    let buf = "";

    while (true) {
      const r = await reader.read();
      if (r.done) break;
      buf += dec.decode(r.value || new Uint8Array(), { stream: true });

      while (true) {
        let idx = buf.indexOf("\n\n");
        let sep = 2;
        const idx2 = buf.indexOf("\r\n\r\n");
        if (idx2 !== -1 && (idx === -1 || idx2 < idx)) {
          idx = idx2;
          sep = 4;
        }
        if (idx === -1) break;

        const frame = buf.slice(0, idx);
        buf = buf.slice(idx + sep);
        onEvent(parseSseFrame(frame));
      }
    }
  }

  // ── Provider colors ───────────────────────────────────────────────────────────
  const PROVIDER_COLORS = {
    "mistral":           "#2dd4bf",
    "codestral":         "#14b8a6",
    "mistral-cli":       "#34d399",
    "openai-compatible": "#60a5fa",
    "gemini":            "#a3e635",
    "anthropic":         "#fb7185",
    "hf":                "#fbbf24",
  };

  // ╔══════════════════════════════════════════════════════════╗
  // ║  SECTION: Markdown Renderer                              ║
  // ╚══════════════════════════════════════════════════════════╝
  function renderInlineMd(text, baseKey) {
    const parts = [];
    const re = /(\*\*(.+?)\*\*|`([^`]+)`)/g;
    let last = 0, m, k = 0;
    while ((m = re.exec(text)) !== null) {
      if (m.index > last) parts.push(text.slice(last, m.index));
      if (m[0].startsWith("**")) {
        parts.push(e("strong", { key: baseKey + k++ }, m[2]));
      } else {
        parts.push(e("code", {
          key: baseKey + k++,
          style: { fontFamily: "var(--mono)", fontSize: "11px", background: "rgba(255,255,255,0.1)", padding: "1px 5px", borderRadius: 4 },
        }, m[3]));
      }
      last = m.index + m[0].length;
    }
    if (last < text.length) parts.push(text.slice(last));
    return parts;
  }

  const SANDBOX = (window.SpiralCoder && window.SpiralCoder.sandbox) ? window.SpiralCoder.sandbox : {};
  const normalizePathSep = SANDBOX.normalizePathSep;
  const safeThreadId = SANDBOX.safeThreadId;
  const safeWorkdir = SANDBOX.safeWorkdir;
  const resolvedThreadRoot = SANDBOX.resolvedThreadRoot;
  const resolvedCwd = SANDBOX.resolvedCwd;
  if (!normalizePathSep || !safeThreadId || !safeWorkdir || !resolvedThreadRoot || !resolvedCwd) {
    throw new Error("Spiral-Coder UI: missing sandbox helpers (core/sandbox.js not loaded)");
  }

  const OBSERVER = (window.SpiralCoder && window.SpiralCoder.observer) ? window.SpiralCoder.observer : {};
  const normalizeForSim = OBSERVER.normalizeForSim;
  const tokenSetForSim = OBSERVER.tokenSetForSim;
  const jaccardSim = OBSERVER.jaccardSim;
  const similarity = OBSERVER.similarity;
  const parseProposals = OBSERVER.parseProposals;
  const parseCriticalPath = OBSERVER.parseCriticalPath;
  const parseHealthScore = OBSERVER.parseHealthScore;
  const parseCoderDiagnostic = OBSERVER.parseCoderDiagnostic;
  const parseBenchmarkPlan = OBSERVER.parseBenchmarkPlan;
  const stripObserverMeta = OBSERVER.stripObserverMeta;
  if (
    !normalizeForSim
    || !tokenSetForSim
    || !jaccardSim
    || !similarity
    || !parseProposals
    || !parseCriticalPath
    || !parseHealthScore
    || !parseCoderDiagnostic
    || !parseBenchmarkPlan
    || !stripObserverMeta
  ) {
    throw new Error("Spiral-Coder UI: missing observer helpers (observer/logic.js not loaded)");
  }

  const EXEC = (window.SpiralCoder && window.SpiralCoder.exec) ? window.SpiralCoder.exec : {};
  const isWindowsHost = EXEC.isWindowsHost;
  const stripShellTranscript = EXEC.stripShellTranscript;
  const dangerousCommandReason = EXEC.dangerousCommandReason;
  const gitRepoHint = EXEC.gitRepoHint;
  const normalizeExecScript = EXEC.normalizeExecScript;
  if (
    !isWindowsHost
    || !stripShellTranscript
    || !dangerousCommandReason
    || !gitRepoHint
    || !normalizeExecScript
  ) {
    throw new Error("Spiral-Coder UI: missing exec helpers (core/exec.js not loaded)");
  }

  function confirmDangerous(uiLang, reason) {
    const l = String(uiLang || "").trim().toLowerCase();
    const r = String(reason || "").trim();
    const msg =
      l === "fr"
        ? `Commande dangereuse détectée: ${r}\nExécuter quand même ?`
        : l === "en"
          ? `Dangerous command detected: ${r}\nRun anyway?`
          : `危険なコマンドを検出: ${r}\nそれでも実行しますか？`;
    try { return window.confirm(msg); } catch (_) { return false; }
  }

  function confirmPhaseMismatch(uiLang, curPhase, propPhase) {
    const cur = String(curPhase || "").trim() || "n/a";
    const prop = String(propPhase || "").trim() || "n/a";
    const tmpl = tr(uiLang, "phaseMismatchConfirm");
    const msg = String(tmpl || "")
      .replace(/\{cur\}/g, cur)
      .replace(/\{prop\}/g, prop);
    try { return window.confirm(msg); } catch (_) { return false; }
  }

  // (exec helpers moved to web/core/exec.js)

  // Render a unified diff block with per-line colours.
  function renderDiffBody(codeText) {
    const colorOf = (line) => {
      if (/^diff\s/.test(line) || /^index\s/.test(line) || /^new file/.test(line) || /^deleted file/.test(line))
        return "rgba(96,165,250,0.9)";   // blue  — file header
      if (/^(\+\+\+|---)/.test(line))
        return "rgba(255,255,255,0.85)"; // white — path line
      if (/^@@/.test(line))
        return "rgba(45,212,191,0.9)";  // cyan  — hunk header
      if (line.startsWith("+"))
        return "rgba(74,222,128,0.9)";  // green — addition
      if (line.startsWith("-"))
        return "rgba(251,113,133,0.9)"; // red   — deletion
      return "rgba(255,255,255,0.58)";  // faint — context
    };
    return e(
      "div",
      { style: { padding: "10px", overflow: "auto", maxHeight: 480 } },
      codeText.split("\n").map((line, i) =>
        e("div", {
          key: i,
          style: {
            color: colorOf(line),
            fontFamily: "var(--mono)",
            fontSize: 12,
            lineHeight: 1.45,
            whiteSpace: "pre",
            minHeight: "1em",
          },
        }, line || "")
      )
    );
  }

  const PRISM_LANG_MAP = {
    js: "javascript", jsx: "javascript",
    ts: "typescript", tsx: "typescript",
    py: "python",
    sh: "bash", shell: "bash", zsh: "bash", console: "bash",
    ps1: "powershell", pwsh: "powershell", ps: "powershell",
    rs: "rust",
    html: "markup", xml: "markup",
  };

  function hlCode(code, lang) {
    const norm = PRISM_LANG_MAP[lang.toLowerCase()] || lang.toLowerCase();
    const P = window.Prism;
    if (!P) {
      return code.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }
    const grammar = P.languages[norm];
    if (!grammar) {
      return code.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }
    try { return P.highlight(code, grammar, norm); } catch (_) {
      return code.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    }
  }

  function parseMarkdown(text, execRes, onRun, onOpen) {
    const lines = text.split("\n");
    const out = [];
    let i = 0, k = 0;
      while (i < lines.length) {
        const line = lines[i];
      if (line.startsWith("```")) {
        const rawLang = line.slice(3).trim();
        // Detect optional filepath appended to lang: e.g. "python src/main.py"
        const langParts = rawLang.split(/\s+/);
        const lang = langParts[0];
        const filePath = (onOpen && langParts.length >= 2 &&
          (langParts[1].includes('/') || langParts[1].includes('\\') || langParts[1].includes('.')))
          ? langParts.slice(1).join(' ') : null;
        const code = [];
        i++;
        while (i < lines.length && !lines[i].startsWith("```")) { code.push(lines[i]); i++; }
        const codeText = code.join("\n");
        const blockIdx = k;
        const win = isWindowsHost();
        const isRunnable = onRun && (win
          ? /^(bash|sh|shell|zsh|console|powershell|pwsh|ps1|ps)$/i.test(lang)
          : /^(bash|sh|shell|zsh|console)$/i.test(lang)
        );
        const res = execRes && execRes[blockIdx];
        const isDiff = /^(diff|patch)$/i.test(lang);
        out.push(e("div", { className: "code", key: k++ },
          e("div", { className: "code-head" },
            e("span", null, filePath ? (lang + " " + filePath) : (lang || "code")),
            isRunnable && e("button", {
              className: "btn-run",
              disabled: !!(res && res.running),
              onClick: () => onRun(blockIdx, lang, codeText),
            }, res && res.running ? "⏳" : "▶ run"),
            filePath && e("button", {
              className: "btn-open",
              title: filePath,
              onClick: () => onOpen(filePath),
            }, "📂 open"),
            e("button", {
              style: { marginLeft: (isRunnable || filePath) ? "0" : "auto", background: "none", border: "none", color: "var(--muted)", cursor: "pointer", fontSize: "11px" },
              onClick: () => navigator.clipboard && navigator.clipboard.writeText(codeText).catch(() => {}),
            }, "⎘ copy"),
          ),
          isDiff ? renderDiffBody(codeText) : e("pre", {
            className: "language-" + (PRISM_LANG_MAP[lang.toLowerCase()] || lang.toLowerCase()),
            dangerouslySetInnerHTML: { __html: hlCode(codeText, lang) },
          }),
          res && !res.running && e("div", {
              className: "exec-result " + (res.exit_code === 0 ? "exec-ok" : "exec-err"),
            },
            res.stdout && e("pre", null, res.stdout),
            res.stderr && e("pre", { style: { color: "var(--warn)" } }, res.stderr),
            e("div", { className: "exec-meta" }, "exit " + res.exit_code),
          ),
        ));
        i++; continue;
      }
      const hm = line.match(/^(#{1,3})\s+(.+)/);
      if (hm) {
        const lvl = hm[1].length;
        out.push(e("div", { key: k++, className: "md-h md-h" + String(lvl) }, hm[2]));
        i++; continue;
      }
      if (/^---+$/.test(line.trim())) {
        out.push(e("div", { key: k++, className: "hr" }));
        i++; continue;
      }
      const lm = line.match(/^(\s*[-*]|\s*\d+\.)\s+(.+)/);
      if (lm) {
        const mark = /^\s*\d+\./.test(lm[1]) ? String(lm[1]).trim() : "•";
        out.push(e("div", { key: k++, className: "md-li" },
          e("span", { className: "md-li-mark" }, mark),
          e("span", { className: "md-li-text" }, renderInlineMd(lm[2], k)),
        ));
        i++; continue;
      }
      if (line.trim() === "") {
        out.push(e("div", { key: k++, className: "md-blank" }));
        i++; continue;
      }
      out.push(e("div", { key: k++, className: "md-line" }, renderInlineMd(line, k)));
      i++;
    }
    return out;
  }

  function estimateTokens(text) {
    return Math.max(1, Math.ceil(String(text || "").length / 4));
  }

  function relativeTime(ts, uiLang) {
    const l = String(uiLang || "").trim().toLowerCase();
    const d = Date.now() - (ts || 0);
    if (d < 60000) return l === "fr" ? "à l'instant" : (l === "en" ? "just now" : "たった今");
    if (d < 3600000) {
      const m = Math.max(1, Math.floor(d / 60000));
      return l === "fr" ? `il y a ${m}m` : (l === "en" ? `${m}m ago` : `${m}分前`);
    }
    if (d < 86400000) {
      const h = Math.max(1, Math.floor(d / 3600000));
      return l === "fr" ? `il y a ${h}h` : (l === "en" ? `${h}h ago` : `${h}時間前`);
    }
    const days = Math.max(1, Math.floor(d / 86400000));
    return l === "fr" ? `il y a ${days}j` : (l === "en" ? `${days}d ago` : `${days}日前`);
  }

  // ── UI Render helpers ─────────────────────────────────────────────────────────

  function normalizeScratchEntry(s) {
    return String(s || "").trim().replace(/\s+/g, " ").toLowerCase();
  }

  function extractTagBlock(text, tag) {
    const re = new RegExp(`<${tag}>\\s*([\\s\\S]*?)\\s*<\\/${tag}>`, "i");
    const m = re.exec(String(text || ""));
    return m && m[1] ? String(m[1]).trim() : "";
  }

  function parseTagFields(body) {
    const out = [];
    let currentKey = "";
    let currentValue = "";
    const lines = String(body || "").replace(/\r\n/g, "\n").split("\n");
    for (const raw of lines) {
      const line = String(raw || "");
      const m = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(.*)$/.exec(line);
      if (m) {
        if (currentKey) out.push([currentKey, currentValue.trim()]);
        currentKey = String(m[1] || "").trim().toLowerCase();
        currentValue = String(m[2] || "").trim();
        continue;
      }
      if (currentKey && line.trim()) {
        currentValue += (currentValue ? " " : "") + line.trim();
      }
    }
    if (currentKey) out.push([currentKey, currentValue.trim()]);
    return out;
  }

  function parsePlanItems(s) {
    const text = String(s || "").replace(/\r\n/g, "\n").trim();
    if (!text) return [];
    const numbered = [];
    const re = /(?:^|\s)\d+[).:-]\s*([\s\S]*?)(?=(?:\s+\d+[).:-]\s)|$)/g;
    let m;
    while ((m = re.exec(text)) !== null) {
      const item = String(m[1] || "").trim().replace(/\s+/g, " ");
      if (item) numbered.push(item);
    }
    if (numbered.length) return numbered;
    return text
      .split(/\n|;/)
      .map((part) => String(part || "").trim().replace(/\s+/g, " "))
      .filter((part) => part && part !== "-");
  }

  const EMERGENCY_GOVERNOR_CONTRACT = Object.freeze({
    tool_names: [],
    diagnostic_tools: [],
    plan: { tag: "plan", fields: [], rules: [] },
    think: { tag: "think", fields: [], rules: [] },
    reflect: { tag: "reflect", fields: [], rules: [] },
    impact: { tag: "impact", fields: [], rules: [] },
    evidence: {
      tag: "evidence",
      fields: [
        { key: "target_files", kind: "list", required: true, aliases: ["target_file"] },
        { key: "target_symbols", kind: "list", required: false, aliases: ["target_symbol"] },
        { key: "evidence", kind: "string", required: true },
        { key: "open_questions", kind: "string", required: true },
        { key: "next_probe", kind: "string", required: true },
      ],
      rules: [],
    },
    done: {
      required_args: ["summary", "completed_acceptance", "remaining_acceptance", "acceptance_evidence"],
      acceptance_evidence_fields: ["criterion", "command"],
      rules: [],
    },
    prompt_layout: {
      block_order: ["plan", "think", "impact", "reflect"],
      done_title: "Done Protocol",
      done_args_template: "done must include {done_args}.",
      error_title: "Error Protocol",
      error_rules: [
        "If exit_code ≠ 0: STOP immediately.",
        "  1. Quote the exact error line.",
        "  2. State root cause in one sentence.",
        "  3. Fix with one corrected command.",
        "If the SAME approach fails 3 consecutive times: STOP, explain why,",
        "  and propose a completely different strategy. Never repeat a failing command.",
      ],
    },
    verification: {
      goal_test_terms: [],
      goal_build_terms: [],
      goal_repo_terms: [],
      goal_check_runners: [],
      repo_goal_requirements: [],
      goal_check_policy: {
        run_on_stop: false,
        require_longrun: false,
        require_exec_feature: false,
        require_command_approval_off: false,
        max_attempts_per_goal: 0,
        goal_order: [],
      },
      ignore_command_signatures: [],
      build_command_signatures: [],
      behavioral_command_signatures: [],
    },
    instruction_resolver: {
      title: "Instruction Resolver",
      priority_title: "Priority order (highest -> lowest):",
      rules_title: "Rules:",
      current_title: "Current higher-authority instructions:",
      priority_order: [
        { authority: "root", label: "root/runtime safety" },
        { authority: "system", label: "system/governor/task contract" },
        { authority: "project", label: "project instructions" },
        { authority: "user", label: "user request" },
        {
          authority: "execution",
          label: "execution scratchpad (<plan>/<think>/<evidence>/<reflect>/<impact>)",
        },
      ],
      rules: [
        "Execution scratchpad is never authoritative.",
        "If a scratchpad block conflicts with a higher layer, rewrite the scratchpad instead of following it.",
      ],
      project_rule_markers: [
        "[Project Context",
        "[Project Instructions",
        "AGENTS.md",
        ".spiral-coder.md",
      ],
      read_only_forbidden_terms: [
        "edit",
        "patch",
        "modify",
        "write",
        "create",
        "implement",
        "fix",
        "refactor",
        "build",
        "compile",
        "test",
        "behavioral",
        "smoke",
        "cargo",
        "pytest",
        "npm",
        "jest",
        "playwright",
        "vitest",
        "run",
        "exec",
        "execute",
      ],
      diagnostic_exec_signatures: [
        "pwd",
        "cd",
        "set-location",
        "pushd",
        "popd",
        "ls",
        "dir",
        "get-location",
        "get-childitem",
        "get-content",
        "select-string",
        "where",
        "which",
        "get-command",
        "echo",
        "write-output",
        "whoami",
        "hostname",
        "git status",
        "git rev-parse",
        "git remote",
        "git branch",
        "git diff",
        "rg",
        "grep",
        "cat",
        "head",
        "tail",
        "sed -n",
        "wc",
        "cargo --version",
        "rustc --version",
        "python --version",
        "node --version",
        "npm --version",
        "pnpm --version",
        "yarn --version",
        "go version",
        "dotnet --info",
      ],
    },
    messages: {},
  });

  const DEFAULT_GOVERNOR_CONTRACT = (() => {
    const embedded = typeof window !== "undefined"
      ? window.__SPIRAL_CODER_GOVERNOR_CONTRACT_FALLBACK__
      : null;
    return embedded && typeof embedded === "object"
      ? embedded
      : EMERGENCY_GOVERNOR_CONTRACT;
  })();

  function contractToolNames(contract) {
    const items = contract && Array.isArray(contract.tool_names) ? contract.tool_names : [];
    return items.length ? items : DEFAULT_GOVERNOR_CONTRACT.tool_names;
  }

  function contractDiagnosticTools(contract) {
    const items = contract && Array.isArray(contract.diagnostic_tools) ? contract.diagnostic_tools : [];
    return items.length ? items : DEFAULT_GOVERNOR_CONTRACT.diagnostic_tools;
  }

  function contractBlock(contract, tag) {
    if (!tag) return null;
    const source = contract && typeof contract === "object" ? contract : null;
    if (source && source[tag] && source[tag].tag === tag) return source[tag];
    return DEFAULT_GOVERNOR_CONTRACT[tag] || null;
  }

  function contractVerification(contract) {
    const source = contract && typeof contract === "object" && contract.verification && typeof contract.verification === "object"
      ? contract.verification
      : null;
    if (source) return source;
    return DEFAULT_GOVERNOR_CONTRACT.verification || EMERGENCY_GOVERNOR_CONTRACT.verification;
  }

  function contractInstructionResolver(contract) {
    const source = contract && typeof contract === "object" && contract.instruction_resolver && typeof contract.instruction_resolver === "object"
      ? contract.instruction_resolver
      : null;
    if (source) return source;
    return DEFAULT_GOVERNOR_CONTRACT.instruction_resolver || EMERGENCY_GOVERNOR_CONTRACT.instruction_resolver;
  }

  function verificationTerms(contract, key) {
    const verification = contractVerification(contract);
    const items = verification && Array.isArray(verification[key]) ? verification[key] : [];
    if (items.length) return items;
    const fallback = DEFAULT_GOVERNOR_CONTRACT.verification;
    return fallback && Array.isArray(fallback[key]) ? fallback[key] : [];
  }

  function textMatchesVerificationTerms(text, terms) {
    const haystack = normalizeScratchEntry(text);
    if (!haystack) return false;
    return (Array.isArray(terms) ? terms : []).some((term) => {
      const candidate = normalizeScratchEntry(term);
      return candidate && haystack.includes(candidate);
    });
  }

  function signatureMatchesVerificationTerms(sig, terms) {
    if (!sig) return false;
    return (Array.isArray(terms) ? terms : []).some((term) => {
      const candidate = normalizeScratchEntry(term);
      return candidate && sig.includes(candidate);
    });
  }

  function verificationLevelForCommand(command, contract) {
    const sig = normalizeScratchEntry(command);
    if (!sig) return "";
    if (signatureMatchesVerificationTerms(sig, verificationTerms(contract, "ignore_command_signatures"))) return "";
    if (signatureMatchesVerificationTerms(sig, verificationTerms(contract, "behavioral_command_signatures"))) return "behavioral";
    if (signatureMatchesVerificationTerms(sig, verificationTerms(contract, "build_command_signatures"))) return "build";
    return "";
  }

  function isDiagnosticExecCommand(command, contract) {
    const sig = normalizeScratchEntry(command);
    if (!sig) return false;
    const resolver = contractInstructionResolver(contract);
    const signatures = resolver && Array.isArray(resolver.diagnostic_exec_signatures)
      ? resolver.diagnostic_exec_signatures
      : [];
    return signatureMatchesVerificationTerms(sig, signatures);
  }

  function classifyExecKind(command, contract) {
    if (verificationLevelForCommand(command, contract)) return "verify";
    if (isDiagnosticExecCommand(command, contract)) return "diagnostic";
    return "action";
  }

  function goalCheckRunners(contract) {
    const verification = contractVerification(contract);
    const items = verification && Array.isArray(verification.goal_check_runners) ? verification.goal_check_runners : [];
    if (items.length) return items;
    const fallback = DEFAULT_GOVERNOR_CONTRACT.verification;
    return fallback && Array.isArray(fallback.goal_check_runners) ? fallback.goal_check_runners : [];
  }

  function psSingleQuote(value) {
    return `'${String(value || "").replace(/'/g, "''")}'`;
  }

  function shSingleQuote(value) {
    return `'${String(value || "").replace(/'/g, "'\"'\"'")}'`;
  }

  function goalCheckCondition(runner, isWindows) {
    const files = Array.isArray(runner && runner.detect_files_any)
      ? runner.detect_files_any.map((item) => String(item || "").trim()).filter(Boolean)
      : [];
    if (!files.length) return "";
    return isWindows
      ? files.map((file) => `(Test-Path -LiteralPath ${psSingleQuote(file)})`).join(" -or ")
      : files.map((file) => `[ -f ${shSingleQuote(file)} ]`).join(" || ");
  }

  function goalCheckCommand(contract, kind, isWindows) {
    const commandKey = kind === "build" ? "build_command" : "test_command";
    const noRunner = kind === "build" ? "NO_BUILD_RUNNER" : "NO_TEST_RUNNER";
    const branches = goalCheckRunners(contract)
      .map((runner) => {
        const condition = goalCheckCondition(runner, isWindows);
        const command = String(runner && runner[commandKey] || "").trim();
        return condition && command ? { condition, command } : null;
      })
      .filter(Boolean);
    if (!branches.length) {
      return isWindows ? `Write-Output ${psSingleQuote(noRunner)}` : `echo ${noRunner}`;
    }
    if (isWindows) {
      return branches
        .map((branch, idx) => `${idx === 0 ? "if" : "elseif"} (${branch.condition}) { ${branch.command} }`)
        .concat([`else { Write-Output ${psSingleQuote(noRunner)} }`])
        .join(" ");
    }
    return branches
      .map((branch, idx) => `${idx === 0 ? "if" : "elif"} ${branch.condition}; then ${branch.command};`)
      .concat([`else echo ${noRunner}; fi`])
      .join(" ");
  }

  function goalCheckRunnerSummary(contract, kind) {
    const commandKey = kind === "build" ? "build_command" : "test_command";
    return goalCheckRunners(contract)
      .map((runner) => {
        const files = Array.isArray(runner && runner.detect_files_any)
          ? runner.detect_files_any.map((item) => String(item || "").trim()).filter(Boolean)
          : [];
        const command = String(runner && runner[commandKey] || "").trim();
        if (!files.length || !command) return "";
        return `${files.join("/")} -> ${command}`;
      })
      .filter(Boolean)
      .join(", ");
  }

  function repoGoalRequirements(contract) {
    const verification = contractVerification(contract);
    const items = verification && Array.isArray(verification.repo_goal_requirements) ? verification.repo_goal_requirements : [];
    if (items.length) return items;
    const fallback = DEFAULT_GOVERNOR_CONTRACT.verification;
    return fallback && Array.isArray(fallback.repo_goal_requirements) ? fallback.repo_goal_requirements : [];
  }

  function goalCheckPolicy(contract) {
    const verification = contractVerification(contract);
    const policy = verification && verification.goal_check_policy && typeof verification.goal_check_policy === "object"
      ? verification.goal_check_policy
      : null;
    if (policy) return policy;
    const fallback = DEFAULT_GOVERNOR_CONTRACT.verification;
    return fallback && fallback.goal_check_policy && typeof fallback.goal_check_policy === "object"
      ? fallback.goal_check_policy
      : {};
  }

  function goalCheckMaxAttempts(contract) {
    const n = Number(goalCheckPolicy(contract).max_attempts_per_goal);
    return Number.isFinite(n) && n > 0 ? Math.floor(n) : 3;
  }

  function goalCheckAttemptsMade(goalChecks) {
    const state = goalChecks && typeof goalChecks === "object" ? goalChecks : {};
    return ["repo", "tests", "build"].some((key) => Number(state[key] && state[key].attempts) > 0);
  }

  function shouldShowGoalCheckContext(agentState, goalChecks, governor, impactRequired) {
    const pending = String(governor && governor.pendingDiag || "").trim().toLowerCase();
    return agentState === "verifying"
      || Boolean(String(impactRequired || "").trim())
      || goalCheckAttemptsMade(goalChecks)
      || /\b(goal_check|verify|verification|test|build)\b/.test(pending);
  }

  function shouldShowRecentRunsContext(agentState, goalChecks, governor, reflectionRequired, impactRequired) {
    return agentState === "recovery"
      || shouldShowGoalCheckContext(agentState, goalChecks, governor, impactRequired)
      || Boolean(String(reflectionRequired || "").trim());
  }

  function goalCheckOrder(contract) {
    const order = goalCheckPolicy(contract).goal_order;
    const items = Array.isArray(order) ? order.map((item) => String(item || "").trim()).filter(Boolean) : [];
    return items.length ? items : ["repo", "tests", "build"];
  }

  function shouldAutoRunGoalChecks(contract, status, longrun, requireCommandApproval) {
    const policy = goalCheckPolicy(contract);
    if (policy.run_on_stop === false) return false;
    if (policy.require_exec_feature !== false) {
      const canExec = !!(status && status.features && status.features.exec);
      if (!canExec) return false;
    }
    if (policy.require_longrun !== false && !longrun) return false;
    if (policy.require_command_approval_off !== false && requireCommandApproval) return false;
    return true;
  }

  function escapeRegExp(value) {
    return String(value || "").replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  }

  function repoGoalProbeCommand(contract, isWindows) {
    const requirements = repoGoalRequirements(contract);
    const commands = requirements
      .map((requirement) => {
        const key = String(requirement && requirement.key || "").trim();
        const probe = String(requirement && requirement.probe || "").trim();
        const path = String(requirement && requirement.path || "").trim();
        if (!key || !probe) return "";
        if (probe === "dir_exists" && path) {
          return isWindows
            ? `Write-Output (${psSingleQuote(key)} + '=' + (Test-Path -LiteralPath ${psSingleQuote(path)}))`
            : `${key}=0; [ -d ${shSingleQuote(path)} ] && ${key}=1; echo ${key}=$${key}`;
        }
        if (probe === "file_exists" && path) {
          return isWindows
            ? `Write-Output (${psSingleQuote(key)} + '=' + (Test-Path -LiteralPath ${psSingleQuote(path)}))`
            : `${key}=0; [ -f ${shSingleQuote(path)} ] && ${key}=1; echo ${key}=$${key}`;
        }
        if (probe === "git_head") {
          return isWindows
            ? `$spiral_coder_${key} = ''; try { $spiral_coder_${key} = (git rev-parse HEAD 2>$null).Trim() } catch { $spiral_coder_${key} = '' }; Write-Output (${psSingleQuote(key)} + '=' + $spiral_coder_${key})`
            : `echo ${key}=$(git rev-parse HEAD 2>/dev/null || true)`;
        }
        return "";
      })
      .filter(Boolean);
    return commands.join("; ");
  }

  function repoGoalMissingLabels(contract, stdoutRaw) {
    return repoGoalRequirements(contract)
      .filter((requirement) => {
        const key = String(requirement && requirement.key || "").trim();
        const probe = String(requirement && requirement.probe || "").trim();
        if (!key || !probe) return false;
        const re = new RegExp(`^${escapeRegExp(key)}=(.*)$`, "im");
        const match = re.exec(String(stdoutRaw || ""));
        const value = match ? String(match[1] || "").trim() : "";
        if (probe === "dir_exists" || probe === "file_exists") return !/^(true|1)$/i.test(value);
        if (probe === "git_head") return !value;
        return false;
      })
      .map((requirement) => String(requirement && requirement.label || requirement && requirement.key || "").trim())
      .filter(Boolean);
  }

  function contractFieldSpec(contract, tag, rawKey) {
    const block = contractBlock(contract, tag);
    const fields = block && Array.isArray(block.fields) ? block.fields : [];
    const want = normalizeScratchEntry(rawKey);
    if (!want) return null;
    return fields.find((field) => {
      const key = normalizeScratchEntry(field && field.key);
      if (key && key === want) return true;
      const aliases = field && Array.isArray(field.aliases) ? field.aliases : [];
      return aliases.some((alias) => normalizeScratchEntry(alias) === want);
    }) || null;
  }

  function contractFieldKeys(contract, tag) {
    const block = contractBlock(contract, tag);
    const fields = block && Array.isArray(block.fields) ? block.fields : [];
    const keys = fields
      .map((field) => String(field && field.key ? field.key : "").trim())
      .filter(Boolean);
    return keys.join("/");
  }

  function contractFieldAllowedValues(contract, tag, rawKey) {
    const field = contractFieldSpec(contract, tag, rawKey);
    if (!field) return [];
    if (field.allowed_values_from === "tool_names") return contractToolNames(contract);
    return Array.isArray(field.allowed_values) ? field.allowed_values.map((item) => String(item || "").trim()).filter(Boolean) : [];
  }

  function canonicalFieldValue(contract, tag, rawKey, value) {
    const field = contractFieldSpec(contract, tag, rawKey);
    if (!field) return "";
    const normalized = String(value || "").trim().toLowerCase().replace(/\s+/g, "_");
    if (!normalized) return "";
    const aliases = field && field.value_aliases && typeof field.value_aliases === "object" ? field.value_aliases : {};
    if (aliases[normalized]) return String(aliases[normalized]);
    const allowed = contractFieldAllowedValues(contract, tag, rawKey);
    return allowed.includes(normalized) ? normalized : "";
  }

  function parseBlockFields(contract, text, tag) {
    const body = extractTagBlock(text, tag);
    if (!body) return null;
    const out = {};
    for (const [rawKey, value] of parseTagFields(body)) {
      const field = contractFieldSpec(contract, tag, rawKey);
      if (!field || !field.key) continue;
      const key = String(field.key);
      const kind = String(field.kind || "string");
      if (kind === "list") out[key] = parsePlanItems(value);
      else if (kind === "positive_int") out[key] = parseFirstPositiveInt(value);
      else if (kind === "tool_name" || kind === "enum") out[key] = canonicalFieldValue(contract, tag, key, value);
      else out[key] = value;
    }
    return out;
  }

  function fieldValueMissing(field, value) {
    const kind = String(field && field.kind ? field.kind : "string");
    if (kind === "list") return !Array.isArray(value) || !value.length;
    if (kind === "positive_int") return !Number.isFinite(Number(value)) || Number(value) < Math.max(1, Number(field && field.min_value) || 1);
    return !String(value || "").trim();
  }

  function parsePlanBlock(text, contract) {
    const values = parseBlockFields(contract, text, "plan");
    if (!values) return null;
    return {
      goal: String(values.goal || ""),
      steps: Array.isArray(values.steps) ? values.steps : [],
      acceptanceCriteria: Array.isArray(values.acceptance) ? values.acceptance : [],
      risks: String(values.risks || ""),
      assumptions: String(values.assumptions || ""),
    };
  }

  function canonicalToolName(s, contract) {
    return canonicalFieldValue(contract, "think", "tool", s);
  }

  function parseThinkBlock(text, contract) {
    const values = parseBlockFields(contract, text, "think");
    if (!values) return null;
    return {
      goal: String(values.goal || ""),
      step: Number(values.step) || 0,
      tool: String(values.tool || ""),
      risk: String(values.risk || ""),
      doubt: String(values.doubt || ""),
      next: String(values.next || ""),
      verify: String(values.verify || ""),
    };
  }

  function validatePlanBlock(contract, plan) {
    if (!plan) return planMissingGoalMessage(contract);
    const goalField = contractFieldSpec(contract, "plan", "goal");
    if (goalField && fieldValueMissing(goalField, plan.goal)) return planMissingGoalMessage(contract);
    const stepsField = contractFieldSpec(contract, "plan", "steps");
    if (stepsField && fieldValueMissing(stepsField, plan.steps)) return planMissingStepsMessage(contract);
    const acceptanceField = contractFieldSpec(contract, "plan", "acceptance");
    if (acceptanceField && fieldValueMissing(acceptanceField, plan.acceptanceCriteria)) return planMissingAcceptanceMessage(contract);
    const stepCount = Array.isArray(plan.steps) ? plan.steps.length : 0;
    if (stepsField && Number(stepsField.min_items) > 0 && stepCount < Number(stepsField.min_items)) {
      return planMinStepsMessage(contract, Number(stepsField.min_items));
    }
    if (stepsField && Number(stepsField.max_items) > 0 && stepCount > Number(stepsField.max_items)) {
      return planMaxStepsMessage(contract, Number(stepsField.max_items));
    }
    const acceptanceCount = Array.isArray(plan.acceptanceCriteria) ? plan.acceptanceCriteria.length : 0;
    if (acceptanceField && Number(acceptanceField.min_items) > 0 && acceptanceCount < Number(acceptanceField.min_items)) {
      return planMinAcceptanceMessage(contract, Number(acceptanceField.min_items));
    }
    if (acceptanceField && Number(acceptanceField.max_items) > 0 && acceptanceCount > Number(acceptanceField.max_items)) {
      return planMaxAcceptanceMessage(contract, Number(acceptanceField.max_items));
    }
    if (!String(plan.risks || "").trim()) return planMissingRisksMessage(contract);
    if (!String(plan.assumptions || "").trim()) return planMissingAssumptionsMessage(contract);
    if (Array.isArray(plan.steps) && plan.steps.some((step) => !String(step || "").trim())) {
      return planEmptyStepMessage(contract);
    }
    if (Array.isArray(plan.acceptanceCriteria) && plan.acceptanceCriteria.some((criterion) => !String(criterion || "").trim())) {
      return planEmptyAcceptanceMessage(contract);
    }
    return "";
  }

  function thinkCommandSig(s) {
    return normalizeScratchEntry(String(s || "").replace(/[`"'“”]/g, ""));
  }

  function thinkNextMatchesExecCommand(next, command) {
    const nextSig = thinkCommandSig(next);
    const cmdSig = thinkCommandSig(command);
    if (!nextSig || !cmdSig) return false;
    if (cmdSig.includes(nextSig) || nextSig.includes(cmdSig)) return true;

    const nextPrefix = nextSig.split(/\s+/).slice(0, 2).join(" ");
    const cmdPrefix = cmdSig.split(/\s+/).slice(0, 2).join(" ");
    return !!nextPrefix && nextPrefix === cmdPrefix;
  }

  function validateThinkBlock(contract, think, plan, toolName, toolArgs) {
    if (!think) return thinkMissingGoalMessage(contract);
    const goalField = contractFieldSpec(contract, "think", "goal");
    if (goalField && fieldValueMissing(goalField, think.goal)) return thinkMissingGoalMessage(contract);
    const stepField = contractFieldSpec(contract, "think", "step");
    if (stepField && fieldValueMissing(stepField, think.step)) return thinkInvalidStepMessage(contract);
    if (!plan || !Array.isArray(plan.steps)) return thinkRequiresPlanMessage(contract);
    if (Number(think.step) > plan.steps.length) {
      return thinkStepOutOfRangeMessage(contract, Number(think.step), plan.steps.length);
    }
    if (fieldValueMissing(contractFieldSpec(contract, "think", "tool"), think.tool)) return thinkInvalidToolMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "think", "risk"), think.risk)) return thinkMissingRiskMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "think", "doubt"), think.doubt)) return thinkMissingDoubtMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "think", "next"), think.next)) return thinkMissingNextMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "think", "verify"), think.verify)) return thinkMissingVerifyMessage(contract);
    if (String(think.tool) !== String(toolName || "")) {
      return thinkToolMismatchMessage(contract, String(think.tool), String(toolName || ""));
    }
    if (String(toolName || "") === "exec") {
      let command = "";
      try {
        const args = JSON.parse(String(toolArgs || "{}"));
        command = String(args.command || "").trim();
      } catch (_) {
        command = String(toolArgs || "").trim();
      }
      if (!thinkNextMatchesExecCommand(think.next, command)) {
        return thinkExecPrefixMismatchMessage(contract);
      }
    }
    return "";
  }

  function parseStringListArg(value) {
    if (Array.isArray(value)) {
      return value
        .map((item) => String(item || "").trim().replace(/\s+/g, " "))
        .filter(Boolean);
    }
    if (typeof value === "string") return parsePlanItems(value);
    return [];
  }

  function doneRequiredArgs(contract) {
    const done = contract && contract.done && typeof contract.done === "object" ? contract.done : DEFAULT_GOVERNOR_CONTRACT.done;
    const items = Array.isArray(done.required_args) ? done.required_args : [];
    return items.length ? items : DEFAULT_GOVERNOR_CONTRACT.done.required_args;
  }

  function doneEvidenceFields(contract) {
    const done = contract && contract.done && typeof contract.done === "object" ? contract.done : DEFAULT_GOVERNOR_CONTRACT.done;
    const items = Array.isArray(done.acceptance_evidence_fields) ? done.acceptance_evidence_fields : [];
    return items.length ? items : DEFAULT_GOVERNOR_CONTRACT.done.acceptance_evidence_fields;
  }

  function parseDoneAcceptanceEvidence(value, contract) {
    if (!Array.isArray(value)) return [];
    const [criterionKey, commandKey] = doneEvidenceFields(contract);
    return value
      .map((item) => {
        const obj = item && typeof item === "object" ? item : null;
        const criterion = obj ? String(obj[criterionKey] || "").trim().replace(/\s+/g, " ") : "";
        const command = obj ? String(obj[commandKey] || "").trim().replace(/\s+/g, " ") : "";
        return criterion && command ? { criterion, command } : null;
      })
      .filter(Boolean);
  }

  function resolveAcceptanceReference(reference, plan) {
    const ref = normalizeScratchEntry(reference);
    if (!ref || !plan || !Array.isArray(plan.acceptanceCriteria)) return -1;
    if (/(acceptance|criterion|criteria)/.test(ref)) {
      const num = parseInt((String(reference || "").match(/\d+/) || [])[0] || "", 10);
      if (Number.isFinite(num) && num >= 1 && num <= plan.acceptanceCriteria.length) return num - 1;
    }
    return plan.acceptanceCriteria.findIndex((criterion) => {
      const want = normalizeScratchEntry(criterion);
      return want && (ref.includes(want) || want.includes(ref));
    });
  }

  function acceptanceReferenceLabel(plan, idx) {
    const criterion = plan && plan.acceptanceCriteria && plan.acceptanceCriteria[idx]
      ? String(plan.acceptanceCriteria[idx])
      : "-";
    return `acceptance ${idx + 1}: ${criterion}`;
  }

  function resolveKnownVerificationCommand(command, knownCommands) {
    const want = normalizeScratchEntry(command);
    if (!want) return "";
    for (const candidate of Array.isArray(knownCommands) ? knownCommands : []) {
      const sig = normalizeScratchEntry(candidate);
      if (sig && (sig === want || sig.includes(want) || want.includes(sig))) return String(candidate);
    }
    return "";
  }

  function validateDoneAcceptance(contract, plan, completedAcceptance, remainingAcceptance, acceptanceEvidence, knownCommands) {
    if (!plan) return { error: doneRequiresPlanMessage(contract) };
    if (!completedAcceptance.length && !remainingAcceptance.length) {
      return { error: doneMissingCriteriaMessage(contract) };
    }
    const covered = new Set();
    const completed = new Set();
    for (const entry of completedAcceptance) {
      const idx = resolveAcceptanceReference(entry, plan);
      if (idx < 0) return { error: doneCompletedInvalidReferenceMessage(contract) };
      if (covered.has(idx)) return { error: doneDuplicateCriteriaMessage(contract) };
      covered.add(idx);
      completed.add(idx);
    }
    for (const entry of remainingAcceptance) {
      const idx = resolveAcceptanceReference(entry, plan);
      if (idx < 0) return { error: doneRemainingInvalidReferenceMessage(contract) };
      if (covered.has(idx)) return { error: doneDuplicateCriteriaMessage(contract) };
      covered.add(idx);
    }
    if (covered.size !== plan.acceptanceCriteria.length) {
      return { error: doneIncompleteCoverageMessage(contract) };
    }
    if (acceptanceEvidence.length !== completed.size) {
      return { error: doneEvidenceIncompleteMessage(contract) };
    }
    const evidenceByIdx = new Map();
    for (const item of acceptanceEvidence) {
      const idx = resolveAcceptanceReference(item.criterion, plan);
      if (idx < 0) return { error: doneEvidenceInvalidReferenceMessage(contract) };
      if (!completed.has(idx)) return { error: doneEvidenceOnlyCompletedMessage(contract) };
      if (evidenceByIdx.has(idx)) return { error: doneEvidenceDuplicateCriteriaMessage(contract) };
      const known = resolveKnownVerificationCommand(item.command, knownCommands);
      if (!known) return { error: doneEvidenceUnknownCommandMessage(contract) };
      evidenceByIdx.set(idx, known);
    }
    return { evidenceByIdx };
  }

  function diagnosticToolHint(contract) {
    return contractDiagnosticTools(contract).join("/");
  }

  function toolNamesCsv(contract) {
    return contractToolNames(contract).join(", ");
  }

  const EMERGENCY_CONTRACT_MESSAGES = Object.freeze({
    multiple_tool_calls: "Multiple tool calls detected ({count}). Only ONE tool call per turn is supported.\nFix: call exactly one tool in your next assistant message.",
    plan_invalid: "[Plan Gate] Invalid <plan>: {error}\nRequired now: emit a valid <plan> with {plan_fields}, then emit <think>, then call ONE tool.",
    plan_missing: "[Plan Gate] Missing valid <plan>.\nRequired now: in your next assistant message, include a valid <plan> ({plan_fields}), then emit <think>, then call ONE diagnostic tool ({diagnostic_tools}) to start.",
    think_missing: "[Think Gate] Missing <think>.\nRequired now: emit a valid <think> with {think_fields} immediately before your tool call.",
    think_invalid: "[Think Gate] Invalid <think>: {error}\nRequired now: emit a valid <think> whose step exists in the current <plan> and whose tool matches the actual tool call.",
    reflection_missing: "Reflection required but missing.\nReason: {reason}\nEmit <reflect>...</reflect> before the next tool call.",
    reflection_invalid: "Invalid self-reflection: {error}\nReason: {reason}\nEmit a valid <reflect> block before the next tool call.",
    reflection_one_tool: "Reflection gate: expected exactly ONE tool call after <reflect>, got {count}.\nFix: emit ONE <reflect> block, then call exactly ONE tool.",
    reflection_stop: "Reflection required but no tool call followed.\nReason: {reason}\nEmit <reflect>...</reflect> and then call exactly one tool.",
    impact_missing: "Impact check required but missing.\nReason: {reason}\nEmit <impact>...</impact> before the next tool call.",
    impact_invalid: "Invalid impact check: {error}\nReason: {reason}\nEmit a valid <impact> block before the next tool call.",
    impact_one_tool: "Impact gate: expected exactly ONE tool call after <impact>, got {count}.\nFix: emit ONE <impact> block, then call exactly ONE tool.",
    impact_stop: "Impact check required but no tool call followed.\nReason: {reason}\nEmit <impact>...</impact> and then call exactly one tool.",
    task_contract_plan_drift: "plan drifted away from the requested task; rewrite it around the actual request",
    instruction_resolver_scratchpad_rule: "<plan>/<think>/<evidence>/<reflect>/<impact> are execution scratchpads, not authority. If they conflict with runtime/task/project/user instructions, rewrite the scratchpad.",
    instruction_resolver_conflict: "[Instruction Resolver] Higher-authority instruction wins: {winner_authority}/{winner_source} over {loser_authority}/{loser_source}.\n{reason}\nRewrite the execution scratchpad instead of following the conflicting <plan>/<think>/<evidence>/<reflect>/<impact>.",
    instruction_resolver_root_runtime_line: "[{authority}/runtime safety] sandbox, approval, and governor hard boundaries cannot be overridden.",
    instruction_resolver_user_task_line: "[{authority}/{source}] stay on the requested task: {task_summary}",
    instruction_resolver_read_only_line: "[{authority}/{source}] inspection-only task: no file mutations and no build/test/action exec.",
    instruction_resolver_done_requires_line: "[{authority}/{source}] done requires {verification} verification.",
    instruction_resolver_project_rules_line: "[{authority}/{source}] AGENTS.md / project context instructions outrank execution scratchpad.",
    instruction_resolver_read_only_plan_term: "read-only observation task plans must stay inspect-only; found `{term}` in {label}",
    instruction_resolver_read_only_mutation: "inspection-only task forbids file mutations",
    instruction_resolver_read_only_verify_exec: "inspection-only task forbids verification exec: `{command}`",
    instruction_resolver_read_only_action_exec: "inspection-only task forbids action exec: `{command}`",
    done_invalid_acceptance: "[Done Gate] Invalid acceptance summary: {error}\nRequired now: call `done` with `completed_acceptance`, `remaining_acceptance`, and `acceptance_evidence` that cover every current plan acceptance criterion and cite known-good verification commands.",
    evidence_invalid: "[Evidence Gate] Invalid <evidence>: {error}",
    evidence_missing_target_files: "evidence missing target_files",
    evidence_missing_evidence: "evidence missing evidence",
    evidence_missing_open_questions: "evidence missing open_questions",
    evidence_missing_next_probe: "evidence missing next_probe",
    evidence_unresolved_path: "evidence gate could not resolve mutation path",
    evidence_target_mismatch: "evidence.target_files must include the mutation path `{target_path}`",
    evidence_missing_observation: "mutation path `{target_path}` lacks prior read/search evidence",
    assumption_refuted_reuse: "refuted assumption would be reused: `{assumption}`{evidence_suffix}",
    goal_check_repo_start: "[goal_check:repo] checking {requirements}",
    goal_check_repo_ok: "[goal_check:repo] OK",
    goal_check_exec_run: "[goal_check:{label}] run `{command}`",
    goal_check_exec_ok: "[goal_check:{label}] OK `{command}`",
    goal_check_exec_fail: "[goal_check:{label}] FAIL `{command}`\n{digest_line}",
    goal_check_all_passed: "[goal_check] all requested stop checks passed",
    goal_check_supported_runners: "Supported runners: {summary}.",
    goal_check_tests_runner_fallback: "If tests are required, configure a supported test runner and re-run.",
    goal_check_build_runner_fallback: "If build is required, add build instructions/scripts for this repo and run them.",
    goal_check_repo_missing: "[goal_check]\nThe task is NOT complete yet.\nMissing: {missing}\nFix it by using exec/write_file. Do NOT stop until the goals are satisfied.",
    goal_check_tests_no_runner: "[goal_check]\nTests were requested, but no test runner was detected in the current directory.\n{supported_runners_line}\nOtherwise, explicitly explain why tests are not applicable and then stop.",
    goal_check_tests_failed: "[goal_check]\nTests are failing (or suspicious output indicates failure).\n{class_line}{digest_line}Fix the failures and re-run the tests. Do NOT stop until tests pass.",
    goal_check_build_no_runner: "[goal_check]\nA build step was requested, but no known build runner was detected in the current directory.\n{supported_runners_line}\nOtherwise, explicitly explain why build is not applicable and then stop.",
    goal_check_build_failed: "[goal_check]\nBuild is failing (or suspicious output indicates failure).\n{class_line}{digest_line}Fix the build failures and re-run. Do NOT stop until build passes.",
  });

  function emergencyContractMessage(key, replacements) {
    switch (String(key || "")) {
      case "plan_min_steps":
        return `plan must include at least ${replacements && replacements.min_steps ? replacements.min_steps : "?"} numbered steps in \`steps:\``;
      case "plan_max_steps":
        return `plan has too many steps (max ${replacements && replacements.max_steps ? replacements.max_steps : "?"})`;
      case "plan_min_acceptance":
        return `plan must include at least ${replacements && replacements.min_acceptance ? replacements.min_acceptance : "?"} acceptance criterion in \`acceptance:\``;
      case "plan_max_acceptance":
        return `plan has too many acceptance criteria (max ${replacements && replacements.max_acceptance ? replacements.max_acceptance : "?"})`;
      case "think_step_out_of_range":
        return `think.step=${replacements && replacements.step ? replacements.step : "?"} is outside the current plan (${replacements && replacements.plan_steps ? replacements.plan_steps : "?"} steps)`;
      case "think_tool_mismatch":
        return `think.tool=${replacements && replacements.think_tool ? replacements.think_tool : "?"} does not match actual tool=${replacements && replacements.actual_tool ? replacements.actual_tool : "?"}`;
      default:
        return String(key || "governor_message_missing").replaceAll("_", " ");
    }
  }

  function contractMessage(contract, key, replacements) {
    const messages = contract && contract.messages && typeof contract.messages === "object"
      ? contract.messages
      : (DEFAULT_GOVERNOR_CONTRACT && DEFAULT_GOVERNOR_CONTRACT.messages && typeof DEFAULT_GOVERNOR_CONTRACT.messages === "object"
        ? DEFAULT_GOVERNOR_CONTRACT.messages
        : null);
    let out = String(
      messages && messages[key]
        ? messages[key]
        : (EMERGENCY_CONTRACT_MESSAGES[key] || emergencyContractMessage(key, replacements))
    );
    const entries = replacements && typeof replacements === "object" ? Object.entries(replacements) : [];
    for (const [name, value] of entries) {
      out = out.replaceAll(`{${name}}`, String(value == null ? "" : value));
    }
    return out;
  }

  function goalCheckSupportLine(summary, fallback) {
    return summary
      ? contractMessage(governorContract, "goal_check_supported_runners", { summary })
      : fallback;
  }

  function goalCheckClassLine(errClass) {
    return errClass ? `class: ${errClass}\n` : "";
  }

  function goalCheckDigestLine(digest) {
    return digest ? `${String(digest).trim()}\n` : "";
  }

  function goalCheckRepoRequirementsSummary(contract) {
    const requirements = contract && contract.verification && Array.isArray(contract.verification.repo_goal_requirements)
      ? contract.verification.repo_goal_requirements
      : [];
    const labels = requirements
      .map((requirement) => {
        const label = String(requirement && requirement.label ? requirement.label : "").trim();
        if (label) return label;
        return String(requirement && requirement.key ? requirement.key : "").trim();
      })
      .filter(Boolean);
    return labels.length ? labels.join(" / ") : ".git / HEAD / README.md";
  }

  function goalCheckRepoStartMessage(contract) {
    return contractMessage(contract, "goal_check_repo_start", {
      requirements: goalCheckRepoRequirementsSummary(contract),
    });
  }

  function goalCheckRepoOkMessage(contract) {
    return contractMessage(contract, "goal_check_repo_ok");
  }

  function goalCheckExecRunMessage(contract, label, command) {
    return contractMessage(contract, "goal_check_exec_run", {
      label: String(label || "").trim(),
      command: String(command || "").trim(),
    });
  }

  function goalCheckExecOkMessage(contract, label, command) {
    return contractMessage(contract, "goal_check_exec_ok", {
      label: String(label || "").trim(),
      command: String(command || "").trim(),
    });
  }

  function goalCheckExecFailMessage(contract, label, command, digestLine) {
    return contractMessage(contract, "goal_check_exec_fail", {
      label: String(label || "").trim(),
      command: String(command || "").trim(),
      digest_line: String(digestLine || "").trim(),
    });
  }

  function goalCheckAllPassedMessage(contract) {
    return contractMessage(contract, "goal_check_all_passed");
  }

  function goalCheckRepoMissingMessage(contract, missing) {
    return contractMessage(contract, "goal_check_repo_missing", {
      missing: Array.isArray(missing) ? missing.join(", ") : String(missing || ""),
    });
  }

  function goalCheckTestsNoRunnerMessage(contract, summary) {
    return contractMessage(contract, "goal_check_tests_no_runner", {
      supported_runners_line: goalCheckSupportLine(
        summary,
        contractMessage(contract, "goal_check_tests_runner_fallback"),
      ),
    });
  }

  function goalCheckTestsFailedMessage(contract, errClass, digest) {
    return contractMessage(contract, "goal_check_tests_failed", {
      class_line: goalCheckClassLine(errClass),
      digest_line: goalCheckDigestLine(digest),
    });
  }

  function goalCheckBuildNoRunnerMessage(contract, summary) {
    return contractMessage(contract, "goal_check_build_no_runner", {
      supported_runners_line: goalCheckSupportLine(
        summary,
        contractMessage(contract, "goal_check_build_runner_fallback"),
      ),
    });
  }

  function goalCheckBuildFailedMessage(contract, errClass, digest) {
    return contractMessage(contract, "goal_check_build_failed", {
      class_line: goalCheckClassLine(errClass),
      digest_line: goalCheckDigestLine(digest),
    });
  }

  function renderGovernorPromptBlock(block) {
    if (!block || !block.tag || !Array.isArray(block.fields)) return "";
    const lines = [`[${String(block.title || "").trim()}]`, `<${block.tag}>`];
    for (const field of block.fields) {
      if (!field || !field.key) continue;
      lines.push(`${field.key}: ${field.hint || ""}`.trimEnd());
    }
    lines.push(`</${block.tag}>`);
    if (Array.isArray(block.rules)) {
      for (const rule of block.rules) {
        if (String(rule || "").trim()) lines.push(String(rule).trim());
      }
    }
    return lines.join("\n");
  }

  function promptLayout(contract) {
    const layout = contract && contract.prompt_layout && typeof contract.prompt_layout === "object"
      ? contract.prompt_layout
      : DEFAULT_GOVERNOR_CONTRACT.prompt_layout;
    return layout && typeof layout === "object" ? layout : EMERGENCY_GOVERNOR_CONTRACT.prompt_layout;
  }

  function buildSystemReasoning(contract) {
    const layout = promptLayout(contract);
    const doneRules = contract && contract.done && Array.isArray(contract.done.rules)
      ? contract.done.rules.filter((rule) => String(rule || "").trim()).map((rule) => String(rule).trim())
      : [
          "Use done only after real verification succeeds.",
          "Each completed acceptance criterion must cite a successful verification command.",
        ];
    const doneArgs = doneRequiredArgs(contract).join(", ");
    const doneLine = String(layout && layout.done_args_template ? layout.done_args_template : "done must include {done_args}.")
      .replaceAll("{done_args}", doneArgs);
    const parts = [];
    const order = layout && Array.isArray(layout.block_order) ? layout.block_order : ["plan", "think", "impact", "reflect"];
    order.forEach((tag) => {
      const rendered = renderGovernorPromptBlock(contractBlock(contract, tag));
      if (rendered) parts.push(rendered);
    });
    parts.push([
      `[${String(layout && layout.done_title ? layout.done_title : "Done Protocol")}]`,
      ...doneRules,
      doneLine,
    ].filter(Boolean).join("\n"));
    const errorRules = layout && Array.isArray(layout.error_rules)
      ? layout.error_rules.filter((rule) => String(rule || "").trim()).map((rule) => String(rule).trim())
      : EMERGENCY_GOVERNOR_CONTRACT.prompt_layout.error_rules;
    parts.push([
      `[${String(layout && layout.error_title ? layout.error_title : "Error Protocol")}]`,
      ...errorRules,
    ].filter(Boolean).join("\n"));
    return ["", ...parts].join("\n\n");
  }

  function multipleToolCallsMessage(contract, count) {
    return contractMessage(contract, "multiple_tool_calls", { count });
  }

  function invalidPlanMessage(contract, error) {
    return contractMessage(contract, "plan_invalid", {
      error,
      plan_fields: contractFieldKeys(contract, "plan"),
    });
  }

  function planMissingGoalMessage(contract) {
    return contractMessage(contract, "plan_missing_goal");
  }

  function planMissingStepsMessage(contract) {
    return contractMessage(contract, "plan_missing_steps");
  }

  function planMinStepsMessage(contract, minSteps) {
    return contractMessage(contract, "plan_min_steps", { min_steps: minSteps });
  }

  function planMaxStepsMessage(contract, maxSteps) {
    return contractMessage(contract, "plan_max_steps", { max_steps: maxSteps });
  }

  function planMissingAcceptanceMessage(contract) {
    return contractMessage(contract, "plan_missing_acceptance");
  }

  function planMinAcceptanceMessage(contract, minAcceptance) {
    return contractMessage(contract, "plan_min_acceptance", { min_acceptance: minAcceptance });
  }

  function planMaxAcceptanceMessage(contract, maxAcceptance) {
    return contractMessage(contract, "plan_max_acceptance", { max_acceptance: maxAcceptance });
  }

  function planMissingRisksMessage(contract) {
    return contractMessage(contract, "plan_missing_risks");
  }

  function planMissingAssumptionsMessage(contract) {
    return contractMessage(contract, "plan_missing_assumptions");
  }

  function planEmptyStepMessage(contract) {
    return contractMessage(contract, "plan_empty_step");
  }

  function planEmptyAcceptanceMessage(contract) {
    return contractMessage(contract, "plan_empty_acceptance");
  }

  function missingPlanMessage(contract) {
    return contractMessage(contract, "plan_missing", {
      plan_fields: contractFieldKeys(contract, "plan"),
      diagnostic_tools: diagnosticToolHint(contract),
    });
  }

  function missingThinkMessage(contract) {
    return contractMessage(contract, "think_missing", {
      think_fields: contractFieldKeys(contract, "think"),
    });
  }

  function invalidThinkMessage(contract, error) {
    return contractMessage(contract, "think_invalid", { error });
  }

  function thinkMissingGoalMessage(contract) {
    return contractMessage(contract, "think_missing_goal");
  }

  function thinkInvalidStepMessage(contract) {
    return contractMessage(contract, "think_invalid_step");
  }

  function thinkRequiresPlanMessage(contract) {
    return contractMessage(contract, "think_requires_plan");
  }

  function thinkStepOutOfRangeMessage(contract, step, planSteps) {
    return contractMessage(contract, "think_step_out_of_range", { step, plan_steps: planSteps });
  }

  function thinkInvalidToolMessage(contract) {
    return contractMessage(contract, "think_invalid_tool");
  }

  function thinkMissingRiskMessage(contract) {
    return contractMessage(contract, "think_missing_risk");
  }

  function thinkMissingDoubtMessage(contract) {
    return contractMessage(contract, "think_missing_doubt");
  }

  function thinkMissingNextMessage(contract) {
    return contractMessage(contract, "think_missing_next");
  }

  function thinkMissingVerifyMessage(contract) {
    return contractMessage(contract, "think_missing_verify");
  }

  function thinkToolMismatchMessage(contract, thinkTool, actualTool) {
    return contractMessage(contract, "think_tool_mismatch", {
      think_tool: thinkTool,
      actual_tool: actualTool,
    });
  }

  function thinkExecPrefixMismatchMessage(contract) {
    return contractMessage(contract, "think_exec_prefix_mismatch");
  }

  function reflectionMissingMessage(contract, reason) {
    return contractMessage(contract, "reflection_missing", { reason });
  }

  function reflectionInvalidMessage(contract, error, reason) {
    return contractMessage(contract, "reflection_invalid", { error, reason });
  }

  function reflectionOneToolMessage(contract, count) {
    return contractMessage(contract, "reflection_one_tool", { count });
  }

  function reflectionStopMessage(contract, reason) {
    return contractMessage(contract, "reflection_stop", { reason });
  }

  function reflectionMissingLastOutcomeMessage(contract) {
    return contractMessage(contract, "reflection_missing_last_outcome");
  }

  function reflectionMissingWrongAssumptionMessage(contract) {
    return contractMessage(contract, "reflection_missing_wrong_assumption");
  }

  function reflectionMissingNextMinimalActionMessage(contract) {
    return contractMessage(contract, "reflection_missing_next_minimal_action");
  }

  function reflectionInvalidGoalDeltaMessage(contract) {
    return contractMessage(contract, "reflection_invalid_goal_delta");
  }

  function reflectionInvalidStrategyChangeMessage(contract) {
    return contractMessage(contract, "reflection_invalid_strategy_change");
  }

  function reflectionRequiresStrategyChangeMessage(contract) {
    return contractMessage(contract, "reflection_requires_strategy_change");
  }

  function reflectionNonImprovingRequiresChangeMessage(contract) {
    return contractMessage(contract, "reflection_non_improving_requires_change");
  }

  function impactMissingMessage(contract, reason) {
    return contractMessage(contract, "impact_missing", { reason });
  }

  function impactInvalidMessage(contract, error, reason) {
    return contractMessage(contract, "impact_invalid", { error, reason });
  }

  function impactOneToolMessage(contract, count) {
    return contractMessage(contract, "impact_one_tool", { count });
  }

  function impactStopMessage(contract, reason) {
    return contractMessage(contract, "impact_stop", { reason });
  }

  function impactMissingChangedMessage(contract) {
    return contractMessage(contract, "impact_missing_changed");
  }

  function impactMissingProgressMessage(contract) {
    return contractMessage(contract, "impact_missing_progress");
  }

  function impactMissingRemainingGapMessage(contract) {
    return contractMessage(contract, "impact_missing_remaining_gap");
  }

  function impactRequiresPlanMessage(contract) {
    return contractMessage(contract, "impact_requires_plan");
  }

  function impactInvalidProgressReferenceMessage(contract) {
    return contractMessage(contract, "impact_invalid_progress_reference");
  }

  function taskContractPlanDriftMessage(contract) {
    return contractMessage(contract, "task_contract_plan_drift");
  }

  function evidenceInvalidMessage(contract, error) {
    return contractMessage(contract, "evidence_invalid", { error });
  }

  function evidenceMissingTargetFilesMessage(contract) {
    return contractMessage(contract, "evidence_missing_target_files");
  }

  function evidenceMissingEvidenceMessage(contract) {
    return contractMessage(contract, "evidence_missing_evidence");
  }

  function evidenceMissingOpenQuestionsMessage(contract) {
    return contractMessage(contract, "evidence_missing_open_questions");
  }

  function evidenceMissingNextProbeMessage(contract) {
    return contractMessage(contract, "evidence_missing_next_probe");
  }

  function evidenceUnresolvedPathMessage(contract) {
    return contractMessage(contract, "evidence_unresolved_path");
  }

  function evidenceTargetMismatchMessage(contract, targetPath) {
    return contractMessage(contract, "evidence_target_mismatch", { target_path: targetPath });
  }

  function evidenceMissingObservationMessage(contract, targetPath) {
    return contractMessage(contract, "evidence_missing_observation", { target_path: targetPath });
  }

  function assumptionRefutedReuseMessage(contract, assumption, evidenceSuffix) {
    return contractMessage(contract, "assumption_refuted_reuse", {
      assumption,
      evidence_suffix: evidenceSuffix,
    });
  }

  function doneInvalidAcceptanceMessage(contract, error) {
    return contractMessage(contract, "done_invalid_acceptance", { error });
  }

  function doneRequiresPlanMessage(contract) {
    return contractMessage(contract, "done_requires_plan");
  }

  function doneMissingCriteriaMessage(contract) {
    return contractMessage(contract, "done_missing_criteria");
  }

  function doneCompletedInvalidReferenceMessage(contract) {
    return contractMessage(contract, "done_completed_invalid_reference");
  }

  function doneRemainingInvalidReferenceMessage(contract) {
    return contractMessage(contract, "done_remaining_invalid_reference");
  }

  function doneDuplicateCriteriaMessage(contract) {
    return contractMessage(contract, "done_duplicate_criteria");
  }

  function doneIncompleteCoverageMessage(contract) {
    return contractMessage(contract, "done_incomplete_coverage");
  }

  function doneEvidenceIncompleteMessage(contract) {
    return contractMessage(contract, "done_evidence_incomplete");
  }

  function doneEvidenceInvalidReferenceMessage(contract) {
    return contractMessage(contract, "done_evidence_invalid_reference");
  }

  function doneEvidenceOnlyCompletedMessage(contract) {
    return contractMessage(contract, "done_evidence_only_completed");
  }

  function doneEvidenceDuplicateCriteriaMessage(contract) {
    return contractMessage(contract, "done_evidence_duplicate_criteria");
  }

  function doneEvidenceUnknownCommandMessage(contract) {
    return contractMessage(contract, "done_evidence_unknown_command");
  }

  function parseFirstPositiveInt(s) {
    const m = String(s || "").match(/\d+/);
    const n = m ? parseInt(m[0], 10) : 0;
    return Number.isFinite(n) && n > 0 ? n : 0;
  }

  function parseReflectionBlock(text, contract) {
    const values = parseBlockFields(contract, text, "reflect");
    if (!values) return null;
    return {
      lastOutcome: String(values.last_outcome || ""),
      goalDelta: String(values.goal_delta || ""),
      wrongAssumption: String(values.wrong_assumption || ""),
      strategyChange: String(values.strategy_change || ""),
      nextMinimalAction: String(values.next_minimal_action || ""),
    };
  }

  function validateReflectionBlock(contract, reflect, governor, fileToolConsecutiveFailures) {
    if (!reflect) return reflectionMissingLastOutcomeMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "reflect", "last_outcome"), reflect.lastOutcome)) return reflectionMissingLastOutcomeMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "reflect", "goal_delta"), reflect.goalDelta)) return reflectionInvalidGoalDeltaMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "reflect", "wrong_assumption"), reflect.wrongAssumption)) return reflectionMissingWrongAssumptionMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "reflect", "strategy_change"), reflect.strategyChange)) return reflectionInvalidStrategyChangeMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "reflect", "next_minimal_action"), reflect.nextMinimalAction)) return reflectionMissingNextMinimalActionMessage(contract);
    const repeatedFailure =
      (Number(governor.sameErrRepeats) || 0) >= 2
      || (Number(governor.sameCmdRepeats) || 0) >= 3
      || (Number(governor.sameOutRepeats) || 0) >= 2
      || Number(fileToolConsecutiveFailures || 0) >= 2;
    if (repeatedFailure && reflect.strategyChange === "keep") {
      return reflectionRequiresStrategyChangeMessage(contract);
    }
    if ((reflect.goalDelta === "same" || reflect.goalDelta === "farther")
      && (reflect.strategyChange === "keep" || reflect.strategyChange === "unknown")) {
      return reflectionNonImprovingRequiresChangeMessage(contract);
    }
    return "";
  }

  function buildReflectionPrompt(reason, agentState, governor, fileToolConsecutiveFailures) {
    return [
      "[Self Reflection Required]",
      `Reason: ${reason}`,
      `State: ${agentState}`,
      "Failure memory:",
      `- consecutive_failures: ${Number(governor.consecutiveFailures) || 0}`,
      `- same_command_repeats: ${Number(governor.sameCmdRepeats) || 0}`,
      `- same_error_repeats: ${Number(governor.sameErrRepeats) || 0}`,
      `- same_output_repeats: ${Number(governor.sameOutRepeats) || 0}`,
      `- file_tool_consecutive_failures: ${Number(fileToolConsecutiveFailures) || 0}`,
      "",
      "Before your next tool call, emit exactly:",
      "<reflect>",
      "last_outcome: success|failure|partial",
      "goal_delta: closer|same|farther",
      "wrong_assumption: <one short sentence>",
      "strategy_change: keep|adjust|abandon",
      "next_minimal_action: <one short sentence>",
      "</reflect>",
      "",
      "Rules:",
      "- One line per field.",
      "- Keep the whole block under 80 tokens.",
      "- If the same error/command/output repeated, strategy_change cannot be `keep`.",
      "- If file_tool_consecutive_failures >= 2, strategy_change cannot be `keep`.",
      "- If goal_delta is `same` or `farther`, choose a materially different next action.",
      "- After the <reflect> block, call exactly one tool.",
    ].join("\n");
  }

  function parseImpactBlock(text, contract) {
    const values = parseBlockFields(contract, text, "impact");
    if (!values) return null;
    return {
      changed: String(values.changed || ""),
      progress: String(values.progress || ""),
      remainingGap: String(values.remaining_gap || ""),
    };
  }

  function impactProgressMatchesEntry(progress, entry) {
    const progressSig = normalizeScratchEntry(progress);
    const entrySig = normalizeScratchEntry(entry);
    if (!progressSig || !entrySig) return false;
    return progressSig.includes(entrySig) || entrySig.includes(progressSig);
  }

  function impactProgressMatchesPlan(progress, plan) {
    const progressSig = normalizeScratchEntry(progress);
    if (!progressSig || !plan) return false;

    if (progressSig.includes("step")) {
      const n = parseFirstPositiveInt(progress);
      if (n && Array.isArray(plan.steps) && n <= plan.steps.length) return true;
    }

    if ((progressSig.includes("acceptance") || progressSig.includes("criterion") || progressSig.includes("criteria"))
      && resolveAcceptanceReference(progress, plan) >= 0) {
      return true;
    }

    return (Array.isArray(plan.steps) && plan.steps.some((step) => impactProgressMatchesEntry(progress, step)))
      || (Array.isArray(plan.acceptanceCriteria) && plan.acceptanceCriteria.some((criterion) => impactProgressMatchesEntry(progress, criterion)));
  }

  function validateImpactBlock(contract, impact, plan) {
    if (!impact) return impactMissingChangedMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "impact", "changed"), impact.changed)) return impactMissingChangedMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "impact", "progress"), impact.progress)) return impactMissingProgressMessage(contract);
    if (fieldValueMissing(contractFieldSpec(contract, "impact", "remaining_gap"), impact.remainingGap)) return impactMissingRemainingGapMessage(contract);
    if (!plan) return impactRequiresPlanMessage(contract);
    if (!impactProgressMatchesPlan(impact.progress, plan)) {
      return impactInvalidProgressReferenceMessage(contract);
    }
    return "";
  }

  function buildImpactPrompt(reason, plan) {
    const lines = [
      "[Impact Check Required]",
      `Reason: ${reason}`,
      `Current goal: ${plan && plan.goal ? plan.goal : "-"}`,
      "",
      "Before your next tool call, emit exactly:",
      "<impact>",
      "changed: <one short sentence>",
      "progress: <which plan step or acceptance criterion moved>",
      "remaining_gap: <one short sentence>",
      "</impact>",
      "",
      "Rules:",
      "- Keep the whole block under 60 tokens.",
      "- Mention the actual mutation effect, not intent.",
      "- `progress` must name a real step or acceptance criterion.",
      "- After the <impact> block, call exactly one tool.",
    ];
    if (plan && Array.isArray(plan.steps) && plan.steps.length) {
      lines.push("", "[Plan Steps]");
      plan.steps.forEach((step, idx) => lines.push(`- step ${idx + 1}: ${step}`));
    }
    if (plan && Array.isArray(plan.acceptanceCriteria) && plan.acceptanceCriteria.length) {
      lines.push("", "[Acceptance Criteria]");
      plan.acceptanceCriteria.forEach((criterion, idx) => lines.push(`- acceptance ${idx + 1}: ${criterion}`));
    }
    return lines.join("\n");
  }

  function keywordTokens(text) {
    const parts = String(text || "")
      .toLowerCase()
      .split(/[^a-z0-9]+/)
      .map((part) => part.trim())
      .filter((part) => part.length >= 3);
    return new Set(parts);
  }

  function tokenOverlapScore(a, b) {
    if (!(a instanceof Set) || !(b instanceof Set) || !a.size || !b.size) return 0;
    let overlap = 0;
    for (const item of a) if (b.has(item)) overlap++;
    return Math.max(0, Math.min(1, overlap / Math.max(1, Math.min(a.size, b.size))));
  }

  function parseAssumptionItems(text) {
    return parsePlanItems(text)
      .map((item) => String(item || "").trim().replace(/\s+/g, " "))
      .filter(Boolean)
      .slice(0, 6);
  }

  function isRootReadOnlyObservationTask(text) {
    const low = String(text || "").toLowerCase();
    const observeTerms = [
      "locate",
      "find",
      "where",
      "inspect",
      "identify",
      "read-only",
      "read only",
      "read the file",
      "look up",
      "trace",
      "do not edit",
      "don't edit",
      "no edit",
      "no edits",
      "without editing",
    ];
    const explicitNoEdit = [
      "read-only",
      "read only",
      "do not edit",
      "don't edit",
      "no edit",
      "no edits",
      "without editing",
    ].some((term) => low.includes(term));
    const strongMutateTerms = [
      "patch",
      "modify",
      "write",
      "create",
      "implement",
      "fix",
      "refactor",
      "rename",
      "delete",
    ];
    if (!observeTerms.some((term) => low.includes(term))) return false;
    if (low.includes("edit") && !explicitNoEdit) return false;
    if (strongMutateTerms.some((term) => low.includes(term))) return false;
    return true;
  }

  function inferTaskVerificationFloor(rootUserText, activePlan, contract) {
    const planText = activePlan
      ? [
          String(activePlan.goal || ""),
          ...(Array.isArray(activePlan.acceptanceCriteria) ? activePlan.acceptanceCriteria : []),
        ].join("\n")
      : "";
    const combined = [String(rootUserText || ""), planText].join("\n");
    if (textMatchesVerificationTerms(combined, verificationTerms(contract, "goal_test_terms"))) {
      return "behavioral";
    }
    if (textMatchesVerificationTerms(combined, verificationTerms(contract, "goal_build_terms"))) {
      return "build";
    }
    return "build";
  }

  function deriveTaskContract(rootUserText, rootReadOnly, contract) {
    const summary = String(rootUserText || "").trim().replace(/\s+/g, " ");
    const hardConstraints = [
      "Solve the user's requested task before cleanup or refactors.",
      "Keep edits evidence-backed and scope-bounded.",
    ];
    if (rootReadOnly) {
      hardConstraints.push("This task is inspection-only: do not edit files.");
      hardConstraints.push("Do not run tests/builds just to finish a read-only task.");
    } else {
      hardConstraints.push("If the request is underspecified, inspect first and avoid speculative edits.");
    }
    const nonGoals = [
      "Do not broaden scope into unrelated files or prompt/governor rewrites.",
      "Do not replace working code without evidence that it is the target.",
    ];
    const outputShape = rootReadOnly
      ? [
          "Name the confirmed file path or symbol you located.",
          "Summarize the confirmed handling context from observation evidence.",
        ]
      : [
          "Tie the final answer to changed files, verification, and remaining gaps.",
          "If unfinished, leave the next exact command or file to continue from.",
        ];
    return {
      taskSummary: summary || "complete the requested task",
      hardConstraints,
      nonGoals,
      outputShape,
      verificationFloor: inferTaskVerificationFloor(rootUserText, null, contract),
    };
  }

  const InstructionAuthority = Object.freeze({
    ROOT: "root",
    SYSTEM: "system",
    PROJECT: "project",
    USER: "user",
    EXECUTION: "execution",
  });

  const InstructionSource = Object.freeze({
    TASK_CONTRACT: "task_contract",
    PROJECT_RULES: "project_rules",
    USER_REQUEST: "user_request",
    PLAN: "plan",
    THINK: "think",
  });

  function instructionRank(contract, authority) {
    const resolver = contractInstructionResolver(contract);
    const layers = resolver && Array.isArray(resolver.priority_order) ? resolver.priority_order : [];
    const normalized = String(authority || "").trim().toLowerCase();
    const index = layers.findIndex((layer) => String(layer && layer.authority || "").trim().toLowerCase() === normalized);
    return index >= 0 ? index : layers.length;
  }

  function instructionPriority(authority, explicit, locality, sequence) {
    return {
      authority: String(authority || InstructionAuthority.EXECUTION),
      explicit: !!explicit,
      locality: Number(locality) || 0,
      sequence: Number(sequence) || 0,
    };
  }

  function instructionOutranks(contract, a, b) {
    const lhs = [
      -instructionRank(contract, a && a.authority),
      a && a.explicit ? 1 : 0,
      Number(a && a.locality) || 0,
      Number(a && a.sequence) || 0,
    ];
    const rhs = [
      -instructionRank(contract, b && b.authority),
      b && b.explicit ? 1 : 0,
      Number(b && b.locality) || 0,
      Number(b && b.sequence) || 0,
    ];
    for (let i = 0; i < lhs.length; i++) {
      if (lhs[i] > rhs[i]) return true;
      if (lhs[i] < rhs[i]) return false;
    }
    return false;
  }

  function hasProjectRulesContext(messages, contract) {
    const resolver = contractInstructionResolver(contract);
    const markers = resolver && Array.isArray(resolver.project_rule_markers)
      ? resolver.project_rule_markers
      : [];
    return (Array.isArray(messages) ? messages : []).some((msg) => {
      if (!msg || msg.role !== "system") return false;
      const content = String(msg.content || "");
      return markers.some((marker) => {
        const term = String(marker || "");
        return term && content.includes(term);
      });
    });
  }

  function buildInstructionResolver(taskSummary, rootReadOnly, projectRulesActive) {
    return {
      taskSummary: String(taskSummary || "").trim().replace(/\s+/g, " ") || "complete the requested task",
      rootReadOnly: !!rootReadOnly,
      projectRulesActive: !!projectRulesActive,
    };
  }

  function renderInstructionConflict(conflict, contract) {
    if (!conflict || typeof conflict !== "object") return "";
    return contractMessage(contract, "instruction_resolver_conflict", {
      winner_authority: String(conflict.winnerAuthority || "").trim(),
      winner_source: String(conflict.winnerSource || "").trim(),
      loser_authority: String(conflict.loserAuthority || "").trim(),
      loser_source: String(conflict.loserSource || "").trim(),
      reason: String(conflict.reason || "").trim(),
    });
  }

  function readOnlyPlanViolation(plan, contract) {
    const resolver = contractInstructionResolver(contract);
    const forbidden = resolver && Array.isArray(resolver.read_only_forbidden_terms)
      ? resolver.read_only_forbidden_terms
      : [];
    const checkField = (label, text) => {
      const tokens = keywordTokens(text);
      for (const term of forbidden) {
        if (tokens.has(term)) {
          return contractMessage(contract, "instruction_resolver_read_only_plan_term", {
            term,
            label,
          });
        }
      }
      return "";
    };
    return checkField("goal", plan && plan.goal)
      || (Array.isArray(plan && plan.steps) ? plan.steps.map((step, idx) => checkField(`step ${idx + 1}`, step)).find(Boolean) : "")
      || (Array.isArray(plan && plan.acceptanceCriteria) ? plan.acceptanceCriteria.map((criterion, idx) => checkField(`acceptance ${idx + 1}`, criterion)).find(Boolean) : "")
      || "";
  }

  function validatePlanAgainstInstructionResolver(plan, resolver, contract) {
    if (!resolver || !resolver.rootReadOnly) return "";
    const reason = readOnlyPlanViolation(plan, contract);
    if (!reason) return "";
    const winner = instructionPriority(InstructionAuthority.SYSTEM, true, 2, 1);
    const loser = instructionPriority(InstructionAuthority.EXECUTION, true, 3, 1);
    if (!instructionOutranks(contract, winner, loser)) return "";
    return renderInstructionConflict({
      winnerAuthority: InstructionAuthority.SYSTEM,
      winnerSource: InstructionSource.TASK_CONTRACT,
      loserAuthority: InstructionAuthority.EXECUTION,
      loserSource: InstructionSource.PLAN,
      reason,
    }, contract);
  }

  function instructionResolverToolConflict(resolver, toolName, toolArgs, contract) {
    if (!resolver || !resolver.rootReadOnly) return "";
    let reason = "";
    if (toolName === "write_file" || toolName === "patch_file" || toolName === "apply_diff") {
      reason = contractMessage(contract, "instruction_resolver_read_only_mutation");
    } else if (toolName === "exec") {
      let command = "";
      try { command = String(JSON.parse(String(toolArgs || "{}")).command || "").trim(); } catch (_) {}
      const kind = classifyExecKind(command, contract);
      if (kind === "verify") {
        reason = contractMessage(contract, "instruction_resolver_read_only_verify_exec", { command });
      } else if (kind === "action") {
        reason = contractMessage(contract, "instruction_resolver_read_only_action_exec", { command });
      }
    }
    if (!reason) return "";
    const winner = instructionPriority(InstructionAuthority.SYSTEM, true, 2, 1);
    const loser = instructionPriority(InstructionAuthority.EXECUTION, true, 3, 2);
    if (!instructionOutranks(contract, winner, loser)) return "";
    return renderInstructionConflict({
      winnerAuthority: InstructionAuthority.SYSTEM,
      winnerSource: InstructionSource.TASK_CONTRACT,
      loserAuthority: InstructionAuthority.EXECUTION,
      loserSource: InstructionSource.THINK,
      reason,
    }, contract);
  }

  function buildInstructionResolverPrompt(resolver, activePlan, contract) {
    const shared = contractInstructionResolver(contract);
    const priorityOrder = shared && Array.isArray(shared.priority_order) ? shared.priority_order : [];
    const rules = shared && Array.isArray(shared.rules) ? shared.rules : [];
    const verificationFloor = inferTaskVerificationFloor(resolver && resolver.taskSummary, activePlan, contract);
    const lines = [
      `[${String(shared && shared.title || "Instruction Resolver")}]`,
      String(shared && shared.priority_title || "Priority order (highest -> lowest):"),
      ...priorityOrder.map((layer) => `- ${String(layer && (layer.label || layer.authority) || "").trim()}`).filter((line) => line !== "-"),
      String(shared && shared.rules_title || "Rules:"),
      ...rules.map((rule) => `- ${String(rule || "").trim()}`).filter((line) => line !== "-"),
      String(shared && shared.current_title || "Current higher-authority instructions:"),
      `- ${contractMessage(contract, "instruction_resolver_root_runtime_line", {
        authority: InstructionAuthority.ROOT,
      })}`,
      `- ${contractMessage(contract, "instruction_resolver_user_task_line", {
        authority: InstructionAuthority.USER,
        source: InstructionSource.USER_REQUEST,
        task_summary: resolver && resolver.taskSummary ? resolver.taskSummary : "complete the requested task",
      })}`,
      `- ${contractMessage(contract, "instruction_resolver_done_requires_line", {
        authority: InstructionAuthority.SYSTEM,
        source: InstructionSource.TASK_CONTRACT,
        verification: verificationFloor === "behavioral" ? "real behavioral" : "real build/check/lint",
      })}`,
    ];
    if (resolver && resolver.rootReadOnly) {
      lines.splice(lines.length - 1, 0, `- ${contractMessage(contract, "instruction_resolver_read_only_line", {
        authority: InstructionAuthority.SYSTEM,
        source: InstructionSource.TASK_CONTRACT,
      })}`);
    }
    if (resolver && resolver.projectRulesActive) {
      lines.push(`- ${contractMessage(contract, "instruction_resolver_project_rules_line", {
        authority: InstructionAuthority.PROJECT,
        source: InstructionSource.PROJECT_RULES,
      })}`);
    }
    return lines.join("\n");
  }

  function buildTaskContractPrompt(taskContract, activePlan, contract) {
    const verificationFloor = inferTaskVerificationFloor(taskContract && taskContract.taskSummary, activePlan, contract);
    const lines = ["[Task Contract]", "Task summary:", `- ${taskContract && taskContract.taskSummary ? taskContract.taskSummary : "complete the requested task"}`, "Hard constraints:"];
    (taskContract && Array.isArray(taskContract.hardConstraints) ? taskContract.hardConstraints : []).forEach((item) => lines.push(`- ${item}`));
    lines.push("Non-goals:");
    (taskContract && Array.isArray(taskContract.nonGoals) ? taskContract.nonGoals : []).forEach((item) => lines.push(`- ${item}`));
    lines.push("Expected output shape:");
    (taskContract && Array.isArray(taskContract.outputShape) ? taskContract.outputShape : []).forEach((item) => lines.push(`- ${item}`));
    lines.push("Verification floor:");
    lines.push(`- ${verificationFloor === "behavioral" ? "real behavioral verification before done" : "real build/check/lint verification before done"}`);
    if (activePlan && Array.isArray(activePlan.acceptanceCriteria) && activePlan.acceptanceCriteria.length) {
      lines.push("Current plan acceptance:");
      activePlan.acceptanceCriteria.forEach((criterion, idx) => {
        lines.push(`- acceptance ${idx + 1}: ${criterion}`);
      });
    }
    lines.push("If the next action would violate this contract, inspect or replan first.");
    return lines.join("\n");
  }

  function validatePlanAgainstTaskContract(plan, taskContract, contract) {
    const taskTokens = keywordTokens(taskContract && taskContract.taskSummary);
    if (taskTokens.size < 3) return "";
    const planTokens = keywordTokens([
      String(plan && plan.goal || ""),
      ...(plan && Array.isArray(plan.steps) ? plan.steps : []),
      ...(plan && Array.isArray(plan.acceptanceCriteria) ? plan.acceptanceCriteria : []),
    ].join("\n"));
    return tokenOverlapScore(taskTokens, planTokens) < 0.2
      ? taskContractPlanDriftMessage(contract)
      : "";
  }

  function parseEvidenceBlock(text, contract) {
    const values = parseBlockFields(contract, text, "evidence");
    if (!values) return null;
    return {
      targetFiles: Array.isArray(values.target_files) ? values.target_files : [],
      targetSymbols: Array.isArray(values.target_symbols) ? values.target_symbols : [],
      evidence: String(values.evidence || "").trim().replace(/\s+/g, " "),
      openQuestions: String(values.open_questions || "").trim().replace(/\s+/g, " "),
      nextProbe: String(values.next_probe || "").trim().replace(/\s+/g, " "),
    };
  }

  function parseReadFileResultPath(content) {
    const first = String(content || "").split("\n")[0] || "";
    const m = /^\[([^\]]+)\]/.exec(first.trim());
    return m && m[1] ? String(m[1]).trim() : "";
  }

  function parseSearchHitCount(content) {
    const first = String(content || "").split("\n")[0] || "";
    const m = /—\s*(\d+)/.exec(first);
    return m ? Number(m[1]) || 0 : 0;
  }

  function parseSearchResultPaths(content) {
    return String(content || "")
      .split("\n")
      .slice(1)
      .map((line) => String(line || "").trim())
      .filter((line) => line && !line.startsWith("["))
      .map((line) => {
        const idx = line.indexOf(":");
        return idx >= 0 ? line.slice(0, idx).trim() : "";
      })
      .filter(Boolean)
      .slice(0, 8);
  }

  function toolContentSucceeded(content) {
    const text = String(content || "");
    if (!text.trim()) return false;
    if (/^(?:ERROR|error:|FAILED\b|GOVERNOR BLOCKED|Awaiting approval)/i.test(text.trim())) return false;
    return true;
  }

  function sanitizeObservationEvidence(value) {
    const src = value && typeof value === "object" ? value : {};
    const reads = Array.isArray(src.reads) ? src.reads : [];
    const searches = Array.isArray(src.searches) ? src.searches : [];
    return {
      reads: reads
        .filter((item) => item && typeof item === "object")
        .map((item) => ({
          command: String(item.command || "").trim().replace(/\s+/g, " ").slice(0, 200),
          path: String(item.path || "").trim().replace(/\s+/g, " ").slice(0, 160),
        }))
        .filter((item) => item.command && item.path)
        .slice(-8),
      searches: searches
        .filter((item) => item && typeof item === "object")
        .map((item) => ({
          command: String(item.command || "").trim().replace(/\s+/g, " ").slice(0, 200),
          pattern: String(item.pattern || "").trim().replace(/\s+/g, " ").slice(0, 120),
          hitCount: Math.max(0, Math.min(9999, Number(item.hitCount) || 0)),
          paths: (Array.isArray(item.paths) ? item.paths : [])
            .map((path) => String(path || "").trim().replace(/\s+/g, " ").slice(0, 160))
            .filter(Boolean)
            .slice(0, 8),
        }))
        .filter((item) => item.command && item.pattern)
        .slice(-8),
    };
  }

  function mergeObservationEvidence(base, extra) {
    const merged = sanitizeObservationEvidence(base);
    const more = sanitizeObservationEvidence(extra);
    const pushRead = (entry) => {
      const sig = `${normalizeScratchEntry(entry.command)}|${normalizeScratchEntry(entry.path)}`;
      const idx = merged.reads.findIndex((item) => `${normalizeScratchEntry(item.command)}|${normalizeScratchEntry(item.path)}` === sig);
      if (idx >= 0) merged.reads.splice(idx, 1);
      merged.reads.push(entry);
      while (merged.reads.length > 8) merged.reads.shift();
    };
    const pushSearch = (entry) => {
      const sig = `${normalizeScratchEntry(entry.command)}|${normalizeScratchEntry(entry.pattern)}`;
      const idx = merged.searches.findIndex((item) => `${normalizeScratchEntry(item.command)}|${normalizeScratchEntry(item.pattern)}` === sig);
      if (idx >= 0) merged.searches.splice(idx, 1);
      merged.searches.push(entry);
      while (merged.searches.length > 8) merged.searches.shift();
    };
    more.reads.forEach(pushRead);
    more.searches.forEach(pushSearch);
    return merged;
  }

  function collectObservationEvidence(messages) {
    const pending = new Map();
    const evidence = { reads: [], searches: [] };
    for (const msg of Array.isArray(messages) ? messages : []) {
      if (msg && msg.role === "assistant" && Array.isArray(msg.tool_calls)) {
        for (const tc of msg.tool_calls) {
          const id = String(tc && tc.id || "").trim();
          const fn = tc && tc.function && typeof tc.function === "object" ? tc.function : null;
          const name = String(fn && fn.name || "").trim();
          if (!id || !name) continue;
          let args = {};
          try { args = JSON.parse(String(fn && fn.arguments || "{}")); } catch (_) { args = {}; }
          if (name === "read_file") {
            pending.set(id, {
              kind: "read",
              command: `read_file(path=${String(args.path || "").trim()})`,
              path: String(args.path || "").trim(),
            });
          } else if (name === "search_files") {
            pending.set(id, {
              kind: "search",
              command: `search_files(pattern=${String(args.pattern || "").trim()}, dir=${String(args.dir || "").trim() || "."})`,
              pattern: String(args.pattern || "").trim(),
            });
          }
        }
        continue;
      }
      if (!msg || msg.role !== "tool") continue;
      const toolCallId = String(msg.tool_call_id || "").trim();
      if (!toolCallId || !pending.has(toolCallId)) continue;
      const pendingItem = pending.get(toolCallId);
      pending.delete(toolCallId);
      const content = String(msg.content || "");
      if (!toolContentSucceeded(content)) continue;
      if (pendingItem.kind === "read") {
        const path = pendingItem.path || parseReadFileResultPath(content);
        if (path) evidence.reads.push({ command: pendingItem.command, path });
      } else if (pendingItem.kind === "search") {
        evidence.searches.push({
          command: pendingItem.command,
          pattern: pendingItem.pattern,
          hitCount: parseSearchHitCount(content),
          paths: parseSearchResultPaths(content),
        });
      }
    }
    return evidence;
  }

  function evidencePathMatches(target, candidate) {
    const targetSig = normalizeScratchEntry(target);
    const candidateSig = normalizeScratchEntry(candidate);
    if (!targetSig || !candidateSig) return false;
    return targetSig === candidateSig || targetSig.endsWith(candidateSig) || candidateSig.endsWith(targetSig);
  }

  function observationSupportsTargetPath(targetPath, observations) {
    return (observations.reads || []).some((read) => evidencePathMatches(targetPath, read.path))
      || (observations.searches || []).some((search) => (search.paths || []).some((path) => evidencePathMatches(targetPath, path)));
  }

  function mutationToolRequiresEvidence(toolName) {
    return toolName === "patch_file" || toolName === "apply_diff";
  }

  function mutationTargetPath(toolName, toolArgs) {
    if (!mutationToolRequiresEvidence(toolName)) return "";
    try {
      const args = JSON.parse(String(toolArgs || "{}"));
      return String(args.path || "").trim();
    } catch (_) {
      return "";
    }
  }

  function validateEvidenceBlock(block, toolName, toolArgs, observations, contract) {
    if (!block || !Array.isArray(block.targetFiles) || !block.targetFiles.length) return evidenceMissingTargetFilesMessage(contract);
    if (!String(block.evidence || "").trim()) return evidenceMissingEvidenceMessage(contract);
    if (!String(block.openQuestions || "").trim()) return evidenceMissingOpenQuestionsMessage(contract);
    if (!String(block.nextProbe || "").trim()) return evidenceMissingNextProbeMessage(contract);
    if (!mutationToolRequiresEvidence(toolName)) return "";
    const targetPath = mutationTargetPath(toolName, toolArgs);
    if (!targetPath) return evidenceUnresolvedPathMessage(contract);
    if (!block.targetFiles.some((path) => evidencePathMatches(path, targetPath))) {
      return evidenceTargetMismatchMessage(contract, targetPath);
    }
    if (!observationSupportsTargetPath(targetPath, observations)) {
      return evidenceMissingObservationMessage(contract, targetPath);
    }
    return "";
  }

  function buildEvidenceGatePrompt(toolName, toolArgs, observations, assumptionLedger) {
    const targetPath = mutationTargetPath(toolName, toolArgs) || "<path>";
    const lines = [
      "[Evidence Gate]",
      `You are about to mutate an existing file via ${toolName}.`,
      `Target path: ${targetPath}`,
      "",
      "Before this mutation, emit exactly:",
      "<evidence>",
      "target_files: 1) <exact target path>",
      "target_symbols: 1) <symbol or area>",
      "evidence: <what previous read/search proved>",
      "open_questions: <what is still uncertain or `none`>",
      "next_probe: <exact next action or edit target>",
      "</evidence>",
      "",
      "Rules:",
      "- `target_files` must include the actual file you are about to change.",
      "- Base the block on real prior read/search evidence from this session.",
      "- If the target is not yet supported by evidence, do NOT mutate; call one diagnostic tool instead.",
      "- After the <evidence> block, call exactly one tool.",
    ];
    if (observations && Array.isArray(observations.reads) && observations.reads.length) {
      lines.push("", "[Observed reads]");
      observations.reads.slice(-3).forEach((read) => lines.push(`- ${read.command} -> ${read.path}`));
    }
    if (observations && Array.isArray(observations.searches) && observations.searches.length) {
      lines.push("[Observed searches]");
      observations.searches.slice(-3).forEach((search) => {
        const pathSummary = Array.isArray(search.paths) && search.paths.length ? search.paths.slice(0, 3).join(", ") : "(no paths)";
        lines.push(`- ${search.command} -> hits=${Number(search.hitCount) || 0} paths=${pathSummary}`);
      });
    }
    const refuted = assumptionLedger && Array.isArray(assumptionLedger.entries)
      ? assumptionLedger.entries.filter((entry) => entry.status === "refuted").slice(0, 2)
      : [];
    if (refuted.length) {
      lines.push("[Refuted assumptions]");
      refuted.forEach((entry) => lines.push(`- ${entry.text}`));
    }
    return lines.join("\n");
  }

  function makeAssumptionLedger() {
    return { entries: [] };
  }

  function rememberUnknownAssumption(ledger, assumption) {
    const text = String(assumption || "").trim().replace(/\s+/g, " ");
    if (!text) return;
    const sig = normalizeScratchEntry(text);
    if (!sig) return;
    const existing = ledger.entries.find((entry) => normalizeScratchEntry(entry.text) === sig);
    if (existing) {
      if (existing.status === "unknown") existing.text = text;
      return;
    }
    ledger.entries.push({ text, status: "unknown", evidence: "" });
    while (ledger.entries.length > 8) ledger.entries.shift();
  }

  function syncAssumptionLedgerToPlan(ledger, plan) {
    const assumptions = parseAssumptionItems(plan && plan.assumptions);
    assumptions.forEach((assumption) => rememberUnknownAssumption(ledger, assumption));
    ledger.entries = ledger.entries.filter((entry) => {
      if (entry.status === "refuted") return true;
      const sig = normalizeScratchEntry(entry.text);
      return assumptions.some((assumption) => normalizeScratchEntry(assumption) === sig);
    });
  }

  function markRefutedAssumption(ledger, assumption, evidence) {
    const text = String(assumption || "").trim().replace(/\s+/g, " ");
    if (!text) return;
    const sig = normalizeScratchEntry(text);
    const evidenceText = String(evidence || "").trim().replace(/\s+/g, " ");
    const existing = ledger.entries.find((entry) => normalizeScratchEntry(entry.text) === sig);
    if (existing) {
      existing.status = "refuted";
      existing.text = text;
      existing.evidence = evidenceText;
      return;
    }
    ledger.entries.push({ text, status: "refuted", evidence: evidenceText });
    while (ledger.entries.length > 8) ledger.entries.shift();
  }

  function refreshAssumptionConfirmations(ledger, observations, knownGoodVerificationCommands) {
    const support = [
      ...(observations && Array.isArray(observations.reads) ? observations.reads.map((read) => `${read.path} ${read.command}`) : []),
      ...(observations && Array.isArray(observations.searches) ? observations.searches.map((search) => `${search.pattern} ${(search.paths || []).join(" ")}`) : []),
      ...(Array.isArray(knownGoodVerificationCommands) ? knownGoodVerificationCommands : []),
    ];
    ledger.entries.forEach((entry) => {
      if (entry.status !== "unknown") return;
      const assumptionSig = normalizeScratchEntry(entry.text);
      const assumptionTokens = keywordTokens(entry.text);
      let best = { score: 0, text: "" };
      support.forEach((candidate) => {
        const candidateSig = normalizeScratchEntry(candidate);
        let score = tokenOverlapScore(assumptionTokens, keywordTokens(candidate));
        if (assumptionSig && candidateSig && (candidateSig.includes(assumptionSig) || assumptionSig.includes(candidateSig))) {
          score = Math.max(score, 0.9);
        }
        if (score > best.score) best = { score, text: String(candidate || "").trim().replace(/\s+/g, " ") };
      });
      if (best.score >= 0.72) {
        entry.status = "confirmed";
        entry.evidence = best.text;
      }
    });
  }

  function buildAssumptionLedgerPrompt(ledger) {
    if (!ledger || !Array.isArray(ledger.entries) || !ledger.entries.length) return "";
    const sections = [
      ["Open assumptions", "unknown"],
      ["Confirmed assumptions", "confirmed"],
      ["Refuted assumptions", "refuted"],
    ];
    const lines = ["[Assumption Ledger]"];
    let wrote = false;
    sections.forEach(([title, status]) => {
      const items = ledger.entries.filter((entry) => entry.status === status);
      if (!items.length) return;
      wrote = true;
      lines.push(`${title}:`);
      items.forEach((entry) => {
        lines.push(`- [${entry.status}] ${entry.text}${entry.evidence ? ` — ${entry.evidence}` : ""}`);
      });
    });
    if (!wrote) return "";
    lines.push("Do not rely on refuted assumptions. Prefer probes that convert open assumptions into confirmed facts.");
    return lines.join("\n");
  }

  function refutedAssumptionConflict(ledger, think, toolName, toolArgs, contract) {
    if (!ledger || !Array.isArray(ledger.entries)) return "";
    let probe = `${String(think && think.goal || "")} ${String(think && think.next || "")}`.trim();
    if (toolName === "exec") {
      try {
        const args = JSON.parse(String(toolArgs || "{}"));
        probe += " " + String(args.command || "").trim();
      } catch (_) {}
    } else {
      const path = mutationTargetPath(toolName, toolArgs);
      if (path) probe += " " + path;
    }
    const probeSig = normalizeScratchEntry(probe);
    const probeTokens = keywordTokens(probe);
    for (const entry of ledger.entries.filter((item) => item.status === "refuted")) {
      const assumptionSig = normalizeScratchEntry(entry.text);
      const overlap = tokenOverlapScore(keywordTokens(entry.text), probeTokens);
      const execRetry = toolName === "exec" && overlap >= 0.5;
      if ((assumptionSig && probeSig && (probeSig.includes(assumptionSig) || assumptionSig.includes(probeSig)))
        || overlap >= 0.75
        || execRetry) {
        return assumptionRefutedReuseMessage(
          contract,
          entry.text,
          entry.evidence ? ` (${entry.evidence})` : "",
        );
      }
    }
    return "";
  }

  function lastValidPlanFromMessages(messages, contract, taskContract, instructionResolver) {
    const items = Array.isArray(messages) ? messages : [];
    for (let i = items.length - 1; i >= 0; i--) {
      const msg = items[i];
      if (!msg || msg.role !== "assistant") continue;
      const plan = parsePlanBlock(msg.content, contract);
      if (!plan) continue;
      if (validatePlanBlock(contract, plan)) continue;
      if (validatePlanAgainstTaskContract(plan, taskContract, contract)) continue;
      if (validatePlanAgainstInstructionResolver(plan, instructionResolver, contract)) continue;
      return plan;
    }
    return null;
  }

  function restoreKnownVerificationCommandsFromMessages(messages, contract) {
    const pending = new Map();
    const commands = [];
    const remember = (command) => {
      const value = String(command || "").trim().replace(/\s+/g, " ");
      const sig = normalizeScratchEntry(value);
      if (!sig) return;
      const idx = commands.findIndex((item) => normalizeScratchEntry(item) === sig);
      if (idx >= 0) commands.splice(idx, 1);
      commands.push(value);
      while (commands.length > 6) commands.shift();
    };
    for (const msg of Array.isArray(messages) ? messages : []) {
      if (msg && msg.role === "assistant" && Array.isArray(msg.tool_calls)) {
        msg.tool_calls.forEach((tc) => {
          const id = String(tc && tc.id || "").trim();
          const fn = tc && tc.function && typeof tc.function === "object" ? tc.function : null;
          if (!id || String(fn && fn.name || "") !== "exec") return;
          let args = {};
          try { args = JSON.parse(String(fn && fn.arguments || "{}")); } catch (_) { args = {}; }
          pending.set(id, String(args.command || "").trim());
        });
        continue;
      }
      if (!msg || msg.role !== "tool") continue;
      const toolCallId = String(msg.tool_call_id || "").trim();
      if (!toolCallId || !pending.has(toolCallId)) continue;
      const command = pending.get(toolCallId);
      pending.delete(toolCallId);
      if (!toolContentSucceeded(msg.content)) continue;
      if (isVerificationCommand(command, contract)) remember(command);
    }
    return commands;
  }

  function rebuildAssumptionLedgerFromMessages(messages, contract, taskContract, instructionResolver, knownGoodVerificationCommands) {
    const ledger = makeAssumptionLedger();
    const items = Array.isArray(messages) ? messages : [];
    items.forEach((msg) => {
      if (!msg || msg.role !== "assistant") return;
      const plan = parsePlanBlock(msg.content, contract);
      if (
        plan
        && !validatePlanBlock(contract, plan)
        && !validatePlanAgainstTaskContract(plan, taskContract, contract)
        && !validatePlanAgainstInstructionResolver(plan, instructionResolver, contract)
      ) {
        syncAssumptionLedgerToPlan(ledger, plan);
      }
      const reflect = parseReflectionBlock(msg.content, contract);
      if (reflect && String(reflect.wrongAssumption || "").trim()) {
        markRefutedAssumption(ledger, reflect.wrongAssumption, reflect.nextMinimalAction);
      }
    });
    refreshAssumptionConfirmations(ledger, collectObservationEvidence(items), knownGoodVerificationCommands);
    return ledger;
  }

  function isVerificationCommand(command, contract) {
    return Boolean(verificationLevelForCommand(command, contract));
  }

  // Renders message content with scratch blocks dimmed separately.
  function renderWithThink(text, execRes, onRun, onOpen) {
    const re = /<(think|reflect|impact|evidence)>([\s\S]*?)<\/\1>/gi;
    const parts = [];
    let last = 0;
    let k = 0;
    let m;
    while ((m = re.exec(text)) !== null) {
      if (m.index > last) {
        parts.push(...parseMarkdown(text.slice(last, m.index), execRes, onRun, onOpen));
      }
      const tag = String(m[1] || "").toLowerCase();
      const body = String(m[2] || "").trim();
      parts.push(e(
        "div",
        { key: tag + k++, className: tag === "reflect" ? "reflect-block" : (tag === "impact" ? "impact-block" : "think-block") },
        body,
      ));
      last = m.index + m[0].length;
    }
    if (last < text.length) {
      parts.push(...parseMarkdown(text.slice(last), execRes, onRun, onOpen));
    }
    return parts;
  }

  // Extract a short sequential list like "1) ...\n2) ..." as clickable choices.
  // Used to support "multi-choice clarification" flows from VIBE / LLMs.
  function extractChoices(text) {
    const s = String(text || "");
    if (!s) return [];
    const lines = s.split("\n");
    const cueRe = /(choose|pick|select|which|option|choice|選ん|どれ|どちら|択|choisissez|s[ée]lectionnez)/i;
    const itemRe = /^\s*(\d{1,2})[).:\-]\s+(.+)\s*$/;

    for (let i = 0; i < lines.length; i++) {
      const m = itemRe.exec(lines[i]);
      if (!m) continue;
      const n0 = Number(m[1]);
      if (n0 !== 1) continue;
      const ctx = lines.slice(Math.max(0, i - 4), i + 1).join("\n");
      if (!cueRe.test(ctx)) continue;

      const out = [];
      let expected = 1;
      for (let j = i; j < lines.length; j++) {
        const mj = itemRe.exec(lines[j]);
        if (!mj) break;
        const nj = Number(mj[1]);
        const tj = String(mj[2] || "").trim();
        if (!Number.isFinite(nj) || nj !== expected || !tj) break;
        out.push({ n: nj, text: tj });
        expected++;
        if (out.length >= 6) break;
      }
      return out.length >= 2 ? out : [];
    }
    return [];
  }

  function choiceReply(uiLang, n, text) {
    const l = String(uiLang || "").trim().toLowerCase();
    const t = String(text || "").trim();
    if (l === "fr") return `Je choisis ${n}: ${t}`;
    if (l === "en") return `I choose ${n}: ${t}`;
    return `選択: ${n} ${t}`;
  }

  // ── Observer Logic ────────────────────────────────────────────────────────────
  // Observer parsing helpers are split into `web/observer/logic.js` (window.SpiralCoder.observer).

  function parseMetaPromptOp(toCoderText) {
    const s = String(toCoderText || "");
    const re = /^\s*META_(SET|APPEND)_(CODER|OBSERVER)\s*:\s*(.*)\s*$/im;
    const m = re.exec(s);
    if (!m) return null;
    const op = String(m[1] || "").toLowerCase() === "append" ? "append" : "set";
    const target = String(m[2] || "").toLowerCase() === "observer" ? "observer" : "coder";
    const head = String(m[3] || "").trim();
    // Everything after the directive line is considered part of the meta prompt.
    const rest = s.slice(m.index + m[0].length).replace(/^\r?\n/, "").trim();
    const text = (head && rest) ? (head + "\n" + rest) : (head || rest);
    if (!String(text || "").trim()) return null;
    return { op, target, text };
  }

  function parsePhase(text) {
    const s = String(text || "");
    const m = /---\s*phase\s*---/i.exec(s);
    if (!m) return null;
    const tail = s.slice(m.index + m[0].length).trimStart();
    const word = tail.split(/[\r\n\s(]/)[0].toLowerCase();
    return word === "core" || word === "feature" || word === "polish" ? word : null;
  }

  const META_DIAGNOSE_KIND = "meta_diagnose";
  const OBSERVER_NEXT_ACTION_KIND = "observer_next_action";
  const META_FIX_LAYERS = Object.freeze([
    "guideline",
    "instruction",
    "skill",
    "harness",
    "tool",
    "index",
    "schema_ci",
    "repo_code",
    "no_change",
  ]);

  function extractJsonObjectText(text) {
    const raw = String(text || "").trim();
    if (!raw) return "";
    if (raw.startsWith("{") && raw.endsWith("}")) return raw;
    const fence = raw.match(/```(?:json)?\s*([\s\S]*?)```/i);
    if (fence && fence[1]) {
      const body = String(fence[1] || "").trim();
      if (body.startsWith("{") && body.endsWith("}")) return body;
    }
    const start = raw.indexOf("{");
    const end = raw.lastIndexOf("}");
    if (start >= 0 && end > start) return raw.slice(start, end + 1).trim();
    return "";
  }

  function normalizeMetaFixLayer(value) {
    const raw = String(value || "").trim().toLowerCase();
    if (!raw) return "no_change";
    if (META_FIX_LAYERS.includes(raw)) return raw;
    if (raw === "guidelines") return "guideline";
    if (raw === "repo" || raw === "repo_bug") return "repo_code";
    if (raw === "schema" || raw === "ci") return "schema_ci";
    return "no_change";
  }

  function normalizeMetaDiagnosis(parsed) {
    if (!parsed || typeof parsed !== "object") return null;
    const causes = Array.isArray(parsed.causes) ? parsed.causes.slice(0, 3) : [];
    const experiments = Array.isArray(parsed.recommended_experiments)
      ? parsed.recommended_experiments.slice(0, 4)
      : [];
    const doNotChange = Array.isArray(parsed.do_not_change)
      ? parsed.do_not_change.slice(0, 6).map((item) => String(item || "").trim()).filter(Boolean)
      : [];
    return {
      summary: String(parsed.summary || "").trim(),
      primary_failure: String(parsed.primary_failure || "").trim(),
      causes: causes.map((cause) => ({
        label: String(cause && cause.label || "").trim(),
        why: String(cause && cause.why || "").trim(),
        evidence: Array.isArray(cause && cause.evidence)
          ? cause.evidence.slice(0, 5).map((item) => String(item || "").trim()).filter(Boolean)
          : [],
        fix_layer: normalizeMetaFixLayer(cause && cause.fix_layer),
        minimal_patch: String(cause && cause.minimal_patch || "").trim(),
        confidence: Math.max(0, Math.min(1, Number(cause && cause.confidence) || 0)),
      })).filter((cause) => cause.label || cause.why || cause.minimal_patch),
      recommended_experiments: experiments.map((item) => ({
        change: String(item && item.change || "").trim(),
        verify: String(item && item.verify || "").trim(),
        expected_signal: String(item && item.expected_signal || "").trim(),
      })).filter((item) => item.change || item.verify || item.expected_signal),
      do_not_change: doNotChange,
    };
  }

  function parseMetaDiagnosisResult(text) {
    const raw = String(text || "").trim();
    const jsonText = extractJsonObjectText(raw);
    if (!jsonText) {
      return { diagnosis: null, parseError: "no_json_object_found" };
    }
    let parsed = null;
    try {
      parsed = JSON.parse(jsonText);
    } catch (err) {
      const msg = err && err.message ? String(err.message) : "json_parse_failed";
      return { diagnosis: null, parseError: `invalid_json: ${msg}` };
    }
    const diagnosis = normalizeMetaDiagnosis(parsed);
    if (!diagnosis) {
      return { diagnosis: null, parseError: "json_did_not_match_meta_diagnosis_schema" };
    }
    return { diagnosis, parseError: null };
  }

  function parseMetaDiagnosis(text) {
    return parseMetaDiagnosisResult(text).diagnosis;
  }

  function parseObserverNextAction(text) {
    const raw = String(text || "").replace(/\r\n/g, "\n").trim();
    if (!raw) return null;
    const headingRe = /^---\s*([a-z_]+)\s*---\s*$/gim;
    const matches = [];
    let match;
    while ((match = headingRe.exec(raw)) !== null) {
      matches.push({
        key: String(match[1] || "").trim().toLowerCase(),
        bodyStart: headingRe.lastIndex,
        matchIndex: match.index,
      });
    }
    if (!matches.length) return null;
    const sections = {};
    matches.forEach((item, idx) => {
      const next = matches[idx + 1];
      const end = next ? next.matchIndex : raw.length;
      sections[item.key] = raw.slice(item.bodyStart, end).trim();
    });
    const nextActions = String(sections.next_actions || "")
      .split("\n")
      .map((line) => line.match(/^\s*\d+[).:\-]\s+(.+)\s*$/))
      .filter(Boolean)
      .map((m) => String(m[1] || "").trim())
      .filter(Boolean)
      .slice(0, 3);
    const parsed = {
      blocker: String(sections.blocker || "").trim(),
      nextActions,
      quickestCheck: String(sections.quickest_check || "").trim(),
      whyThisFirst: String(sections.why_this_first || "").trim(),
      fallback: String(sections.fallback || "").trim(),
    };
    if (
      !parsed.blocker
      && !parsed.nextActions.length
      && !parsed.quickestCheck
      && !parsed.whyThisFirst
      && !parsed.fallback
    ) {
      return null;
    }
    return parsed;
  }

  // ╔══════════════════════════════════════════════════════════╗
  // ║  SECTION: App Shell (React state + event handlers)       ║
  // ╚══════════════════════════════════════════════════════════╝
  function App() {
    const [lang, setLang] = useState(() => {
      const v = (readStoredValue(localStorage, LS.lang) || "").trim();
      return v === "ja" || v === "en" || v === "fr" ? v : "ja";
    });
    const [status, setStatus] = useState(null);
    const harnessSupported = serverSupportsFeature(status, "harness_promotions");
    const mergeGateSupported = serverSupportsFeature(status, "merge_gate");
    const projectScanSupported = serverSupportsFeature(status, "project_scan");

    const [config, setConfig] = useState(() => {
      const v = safeJsonParse(readStoredValue(localStorage, LS.config) || "null", null);
      if (!v || typeof v !== "object") return { ...DEFAULT_CONFIG };
      const cfg = { ...DEFAULT_CONFIG, ...v };
      if (!cfg.chatModel && cfg.model) cfg.chatModel = cfg.model;
      if (!cfg.codeModel && cfg.model) cfg.codeModel = cfg.model;
      if (!cfg.model && (cfg.chatModel || cfg.codeModel)) cfg.model = cfg.chatModel || cfg.codeModel;
      // Coder pane should never run in Observer mode (it disables autonomy/tooling and looks "broken").
      if (String(cfg.mode || "").trim() === "Observer") cfg.mode = DEFAULT_CONFIG.mode;
      if (!cfg.observerMode) cfg.observerMode = DEFAULT_CONFIG.observerMode;
      if (!cfg.chatPersona) cfg.chatPersona = DEFAULT_CONFIG.chatPersona;
      if (!cfg.observerPersona) cfg.observerPersona = DEFAULT_CONFIG.observerPersona;
      const oi0 = String(cfg.observerIntensity || "").trim().toLowerCase();
      if (oi0 === "polite" || oi0 === "critical" || oi0 === "brutal") cfg.observerIntensity = oi0;
      else cfg.observerIntensity = DEFAULT_CONFIG.observerIntensity;
      {
        const ol0 = String(cfg.observerLang || "").trim().toLowerCase();
        if (ol0 === "ui" || ol0 === "auto" || ol0 === "ja" || ol0 === "en" || ol0 === "fr") cfg.observerLang = ol0;
        else cfg.observerLang = DEFAULT_CONFIG.observerLang;
      }
      if (typeof cfg.includeCoderContext !== "boolean") cfg.includeCoderContext = !!DEFAULT_CONFIG.includeCoderContext;
      if (typeof cfg.chatAttachRuntime !== "boolean") cfg.chatAttachRuntime = !!DEFAULT_CONFIG.chatAttachRuntime;
      if (typeof cfg.chatAutoTasks !== "boolean") cfg.chatAutoTasks = !!DEFAULT_CONFIG.chatAutoTasks;
      if (typeof cfg.requireEditApproval !== "boolean") cfg.requireEditApproval = !!DEFAULT_CONFIG.requireEditApproval;
      if (typeof cfg.requireCommandApproval !== "boolean") cfg.requireCommandApproval = !!DEFAULT_CONFIG.requireCommandApproval;
      if (typeof cfg.autoObserve !== "boolean") cfg.autoObserve = !!DEFAULT_CONFIG.autoObserve;
      if (typeof cfg.forceAgent !== "boolean") cfg.forceAgent = !!DEFAULT_CONFIG.forceAgent;
      if (typeof cfg.coderMaxIters !== "string") cfg.coderMaxIters = String(cfg.coderMaxIters || "");
      {
        const n = numOrUndef(cfg.coderMaxIters);
        if (!n) cfg.coderMaxIters = DEFAULT_CONFIG.coderMaxIters;
        else cfg.coderMaxIters = String(Math.max(1, Math.min(64, Math.round(n))));
      }
      if (typeof cfg.toolRoot !== "string") cfg.toolRoot = String(cfg.toolRoot || "");
      if (!String(cfg.toolRoot || "").trim()) cfg.toolRoot = DEFAULT_CONFIG.toolRoot;
      if (typeof cfg.mistralCliAgent !== "string") cfg.mistralCliAgent = String(cfg.mistralCliAgent || "");
      if (typeof cfg.mistralCliMaxTurns !== "string") cfg.mistralCliMaxTurns = String(cfg.mistralCliMaxTurns || "");
      if (typeof cfg.codeProvider !== "string") cfg.codeProvider = String(cfg.codeProvider || "");
      if (typeof cfg.codeBaseUrl !== "string") cfg.codeBaseUrl = String(cfg.codeBaseUrl || "");
      if (typeof cfg.observerProvider !== "string") cfg.observerProvider = String(cfg.observerProvider || "");
      if (typeof cfg.observerBaseUrl !== "string") cfg.observerBaseUrl = String(cfg.observerBaseUrl || "");
      if (typeof cfg.observerModel !== "string") cfg.observerModel = String(cfg.observerModel || "");
      const cot0 = String(cfg.cot || "").trim().toLowerCase();
      if (cot0 === "off") cfg.cot = "off";
      else if (cot0 === "structured") cfg.cot = "structured";
      else if (cot0 === "deep") cfg.cot = "deep";
      else cfg.cot = "brief";
      if (String(cfg.autonomy || "").trim().toLowerCase() === "off") cfg.autonomy = "off";
      else cfg.autonomy = "longrun";
      // One-time migration: switch legacy default Mistral config to OpenAI-compatible defaults.
      const provider0 = String(cfg.provider || "").trim();
      const baseUrl0 = String(cfg.baseUrl || "").trim().toLowerCase();
      if (provider0 === "mistral" && (!baseUrl0 || baseUrl0 === "https://api.mistral.ai/v1")) {
        cfg.provider = PRESETS.openai.provider;
        cfg.baseUrl = PRESETS.openai.baseUrl;
        cfg.model = PRESETS.openai.model;
        cfg.chatModel = PRESETS.openai.chatModel;
        cfg.codeModel = PRESETS.openai.codeModel;
        cfg.mode = PRESETS.openai.mode;
        cfg.vibe = false;
      }
      // Migrate legacy Mistral model ids that no longer exist.
      if (String(cfg.provider || "").trim() === "mistral") {
        const isDevstral2 = (x) => String(x || "").trim() === "devstral-2";
        if (isDevstral2(cfg.model) || isDevstral2(cfg.chatModel) || isDevstral2(cfg.codeModel)) {
          cfg.model = "codestral-latest";
          cfg.codeModel = "codestral-latest";
          if (!cfg.chatModel || isDevstral2(cfg.chatModel)) {
            cfg.chatModel = "mistral-small-latest";
          }
        }
      }
      return cfg;
    });
    const [chatApiKey, setChatApiKey] = useState("");
    const [codeApiKey, setCodeApiKey] = useState("");
    const [observerApiKey, setObserverApiKey] = useState("");
    const [diff, setDiff] = useState("");
    const [models, setModels] = useState([]);
    const [modelsLoading, setModelsLoading] = useState(false);
    const [modelsErr, setModelsErr] = useState("");
    const [modelsTarget, setModelsTarget] = useState("chat");
    const [pendingEdits, setPendingEdits] = useState([]);
    const [pendingCommands, setPendingCommands] = useState([]);
    const [pendingBusy, setPendingBusy] = useState(false);
    const [harnessPromotions, setHarnessPromotions] = useState(null);
    const [promotionBusy, setPromotionBusy] = useState(false);
    const [promotionGateError, setPromotionGateError] = useState("");
    const [mergeGate, setMergeGate] = useState(null);
    const [mergeGateBusy, setMergeGateBusy] = useState(false);
    const [mergeGateError, setMergeGateError] = useState("");
    const [metaBusy, setMetaBusy] = useState(false);
    const [observerSubTab, setObserverSubTab] = useState("analysis"); // "analysis" | "chat" | "meta"
    const [metaArtifacts, setMetaArtifacts] = useState([]);
    const [metaArtifactsLoading, setMetaArtifactsLoading] = useState(false);
    const [metaArtifactsError, setMetaArtifactsError] = useState("");
    const [metaArtifactThreadFilter, setMetaArtifactThreadFilter] = useState("");
    const [metaArtifactParseOnly, setMetaArtifactParseOnly] = useState(false);
    const [metaArtifactRoot, setMetaArtifactRoot] = useState("");
    const [metaArtifactSelectedName, setMetaArtifactSelectedName] = useState("");
    const [metaArtifactDetail, setMetaArtifactDetail] = useState(null);
    const [metaArtifactDetailLoading, setMetaArtifactDetailLoading] = useState(false);
    const [chatInput, setChatInput] = useState("");
    const [sendingChat, setSendingChat] = useState(false);
    const [proposalModal, setProposalModal] = useState(null);
    const [proposalModalText, setProposalModalText] = useState("");
    const [readerModal, setReaderModal] = useState(null);
    const [planningTasks, setPlanningTasks] = useState(false);
    const [projectScan, setProjectScan] = useState(null);
    const [projectScanLoading, setProjectScanLoading] = useState(false);
    const projectScanRootRef = useRef("");
    const settingsPanelRef = useRef(null);
    const promotionsPanelRef = useRef(null);
    const mergeGatePanelRef = useRef(null);
    const runtimeApprovalsRef = useRef(null);
    const [gitCheckpoint, setGitCheckpoint] = useState(null);

    const [threadState, setThreadState] = useState(() => {
      let threads = safeJsonParse(readStoredValue(localStorage, LS.threads) || "null", null);
      threads = Array.isArray(threads) ? threads : [];
      threads = threads
        .filter((t) => t && typeof t === "object" && typeof t.id === "string")
        .map((t) => ({
          id: t.id,
          title: typeof t.title === "string" && t.title.trim() ? t.title : "Untitled",
          createdAt: typeof t.createdAt === "number" ? t.createdAt : Date.now(),
          updatedAt: typeof t.updatedAt === "number" ? t.updatedAt : Date.now(),
          workdir: typeof t.workdir === "string" ? t.workdir : "",
          messages: Array.isArray(t.messages)
            ? t.messages
                .filter((m) => m && typeof m === "object")
                .map((m) => ({
                  id: typeof m.id === "string" && m.id ? m.id : uid(),
                  pane: m.pane === "observer" ? "observer" : m.pane === "chat" ? "chat" : "coder",
                  role: m.role === "assistant" ? "assistant" : "user",
                  origin: m.origin === "runtime" ? "runtime" : "user",
                  content: typeof m.content === "string" ? m.content : String(m.content || ""),
                  ts: typeof m.ts === "number" ? m.ts : Date.now(),
                  streaming: !!m.streaming,
                  metaKind: typeof m.metaKind === "string" ? m.metaKind : "",
                  metaTargetId: typeof m.metaTargetId === "string" ? m.metaTargetId : "",
                }))
            : [],
          tasks: Array.isArray(t.tasks)
            ? t.tasks
                .filter((x) => x && typeof x === "object")
                .map((x) => ({
                  id: typeof x.id === "string" && x.id ? x.id : uid(),
                  target: String(x.target || "").trim().toLowerCase() === "observer" ? "observer" : "coder",
                  title: typeof x.title === "string" ? x.title : String(x.title || ""),
                  body: typeof x.body === "string" ? x.body : String(x.body || ""),
                  phase: typeof x.phase === "string" ? x.phase : "",
                  priority: typeof x.priority === "number" ? x.priority : null,
                  status: typeof x.status === "string" && x.status ? x.status : "new",
                  createdAt: typeof x.createdAt === "number" ? x.createdAt : Date.now(),
                }))
            : [],
          coderMem: (() => {
            const m = t.coderMem && typeof t.coderMem === "object" ? t.coderMem : null;
            const rows = m && Array.isArray(m.cmdStats) ? m.cmdStats : [];
            const cmdStats = rows
              .filter((r) => r && typeof r === "object" && typeof r.k === "string")
              .map((r) => ({
                k: String(r.k || "").slice(0, 220),
                attempts: typeof r.attempts === "number" ? r.attempts : Number(r.attempts) || 0,
                fails: typeof r.fails === "number" ? r.fails : Number(r.fails) || 0,
                lastErr: typeof r.lastErr === "string" ? String(r.lastErr || "").slice(0, 220) : "",
                lastTs: typeof r.lastTs === "number" ? r.lastTs : Number(r.lastTs) || 0,
              }))
              .slice(0, 60);
            return { cmdStats };
          })(),
          coderObsEvidence: sanitizeObservationEvidence(t.coderObsEvidence),
          observerMem: (() => {
            const m = t.observerMem && typeof t.observerMem === "object" ? t.observerMem : null;
            const pc0 = (m && m.proposal_counts && typeof m.proposal_counts === "object") ? m.proposal_counts : {};
            const proposal_counts = {};
            const keys = Object.keys(pc0 || {});
            for (let i = 0; i < keys.length && i < 80; i++) {
              const k = String(keys[i] || "").slice(0, 120);
              if (!k) continue;
              const v0 = pc0[keys[i]];
              const v = typeof v0 === "number" ? v0 : Number(v0) || 0;
              if (v > 0) proposal_counts[k] = Math.max(1, Math.min(99, Math.round(v)));
            }
            return { proposal_counts };
          })(),
        }));
      if (!threads.length) threads = [makeThread("Thread 1")];
      const active0 = (readStoredValue(localStorage, LS.active) || "").trim();
      const active = threads.some((t) => t.id === active0) ? active0 : threads[0].id;
      return { threads, activeId: active };
    });

    const [coderInput, setCoderInput] = useState("");
    const [observerInput, setObserverInput] = useState("");
    const [coderFind, setCoderFind] = useState("");
    const [observerFind, setObserverFind] = useState("");
    const [sendingCoder, setSendingCoder] = useState(false);
    const [sendingObserver, setSendingObserver] = useState(false);
    const [loopInfo, setLoopInfo] = useState({ active: false, score: 0, depth: 0 });
    const [execResults, setExecResults] = useState({}); // { msgId: { blockIdx: {stdout,stderr,exit_code,running} } }
    const [expandedProposals, setExpandedProposals] = useState(new Set());
    const [expandedMsgs, setExpandedMsgs] = useState(new Set());
    const [copiedId, setCopiedId] = useState(null);
    const [toasts, setToasts] = useState([]);
    const [editingThreadId, setEditingThreadId] = useState(null);
    const [editingTitle, setEditingTitle] = useState("");
    const [confirmDeleteId, setConfirmDeleteId] = useState(null);
    const [showShortcuts, setShowShortcuts] = useState(false);
    const [splitPct, setSplitPct] = useState(() => {
      try {
        const v = Number(readStoredValue(localStorage, LS.splitPct));
        if (Number.isFinite(v) && v >= 20 && v <= 80) return v;
      } catch (_) {}
      // Default: bias toward readable Observer critiques (users can resize + it persists).
      return 40;
    });
    const arenaRef = useRef(null);
    const isDraggingRef = useRef(false);

    const showToast = (msg, type = "info") => {
      const id = Date.now() + Math.random();
      setToasts((t) => [...t, { id, msg, type }]);
      setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 3500);
    };

    const abortCoderRef = useRef(null);
    const abortObserverRef = useRef(null);
    const loopRef = useRef({ lastChecked: "", depth: 0 });
    const lastAutoObserveMsgRef = useRef(null);
    const lastObserverNextActionRef = useRef(null);
    const saveTimer = useRef(null);
    const coderBodyRef = useRef(null);
    const observerBodyRef = useRef(null);
    const abortChatRef = useRef(null);
    const chatBodyRef = useRef(null);
    const toolRootInitRef = useRef(false);
    const ensuredWorkdirRef = useRef({});

    // Drag-to-resize pane handler.
    const onSplitDragStart = useCallback((e) => {
      e.preventDefault();
      isDraggingRef.current = true;
      const onMove = (ev) => {
        if (!isDraggingRef.current || !arenaRef.current) return;
        if (ev && ev.touches && typeof ev.preventDefault === "function") ev.preventDefault();
        const rect = arenaRef.current.getBoundingClientRect();
        const x = ev.touches ? ev.touches[0].clientX : ev.clientX;
        const pct = Math.round(Math.min(80, Math.max(20, ((x - rect.left) / rect.width) * 100)));
        setSplitPct(pct);
      };
      const onUp = () => {
        isDraggingRef.current = false;
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
        document.removeEventListener("touchmove", onMove);
        document.removeEventListener("touchend", onUp);
        document.removeEventListener("touchcancel", onUp);
      };
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
      document.addEventListener("touchmove", onMove, { passive: false });
      document.addEventListener("touchend", onUp);
      document.addEventListener("touchcancel", onUp);
    }, []);

    // Global keyboard shortcuts.
    useEffect(() => {
      const onKey = (e) => {
        if (e.key === "?" && !["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName)) {
          setShowShortcuts((v) => !v);
        }
        if ((e.ctrlKey || e.metaKey) && String(e.key || "").toLowerCase() === "k") {
          e.preventDefault();
          try { if (abortCoderRef.current) abortCoderRef.current.abort(); } catch (_) {}
          abortCoderRef.current = null;
          setSendingCoder(false);
          try { if (abortObserverRef.current) abortObserverRef.current.abort(); } catch (_) {}
          abortObserverRef.current = null;
          setSendingObserver(false);
        }
        if (e.key === "Escape") {
          setShowShortcuts(false);
          setProposalModal(null);
          setReaderModal(null);
          setEditingThreadId(null);
          setConfirmDeleteId(null);
        }
      };
      window.addEventListener("keydown", onKey);
      return () => window.removeEventListener("keydown", onKey);
    }, []);

    // Persist pane split ratio.
    useEffect(() => {
      try { localStorage.setItem(LS.splitPct, String(splitPct)); } catch (_) {}
    }, [splitPct]);

    const activeThread = useMemo(() => {
      return threadState.threads.find((t) => t.id === threadState.activeId) || threadState.threads[0];
    }, [threadState]);

    useEffect(() => {
      document.documentElement.lang = lang;
      localStorage.setItem(LS.lang, lang);
    }, [lang]);

    useEffect(() => {
      const safe = { ...config };
      delete safe.apiKey;
      localStorage.setItem(LS.config, JSON.stringify(safe));
    }, [config]);

    useEffect(() => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(() => {
        try {
          localStorage.setItem(LS.threads, JSON.stringify(threadState.threads));
          localStorage.setItem(LS.active, threadState.activeId);
        } catch (_) {}
      }, 350);
      return () => saveTimer.current && clearTimeout(saveTimer.current);
    }, [threadState]);

    useEffect(() => {
      loopRef.current = { lastChecked: "", depth: 0 };
      lastAutoObserveMsgRef.current = null;
      lastObserverNextActionRef.current = null;
      setLoopInfo({ active: false, score: 0, depth: 0 });
    }, [threadState.activeId]);

    useEffect(() => {
      // Detect repeated Observer replies (e.g. review -> rewrite -> review loops) and surface it.
      const msgs = paneMessages("observer");
      const asst = (msgs || []).filter((m) => m && m.role === "assistant" && !m.streaming && String(m.content || "").trim());
      if (asst.length < 2) return;

      const last = asst[asst.length - 1];
      if (!last || !last.id) return;
      if (loopRef.current.lastChecked === last.id) return;
      loopRef.current.lastChecked = last.id;

      const window = asst.slice(0, -1).slice(-4);
      let maxSim = 0;
      for (const prev of window) {
        maxSim = Math.max(maxSim, similarity(last.content, prev.content));
      }

      const minChars = 180;
      const detected = String(last.content || "").trim().length >= minChars && maxSim >= 0.80;
      let depth = loopRef.current.depth || 0;
      if (detected) depth = Math.min(18, depth + 1);
      else depth = Math.max(0, depth - 1);
      loopRef.current.depth = depth;

      setLoopInfo({ active: detected, score: maxSim, depth });
    }, [threadState]);

    // Auto next-action assist: when the latest completed Coder reply is failure-like,
    // ask the Observer for the next concrete move instead of another broad critique.
    useEffect(() => {
      if (!activeThread) return;
      if (sendingObserver || sendingCoder || metaBusy) return;
      const target = findLatestObserverNextActionTarget();
      if (!target) return;
      const key = `${String(activeThread.id || "")}:${String(target.id || "")}`;
      if (lastObserverNextActionRef.current === key) return;
      lastObserverNextActionRef.current = key;
      lastAutoObserveMsgRef.current = key;
      const reasonHint = detectMetaFailureKind(target.content) || "stuck_or_failure";
      const timer = setTimeout(() => {
        runObserverNextActionAssist(`msg:${target.id}`, reasonHint);
      }, 500);
      return () => clearTimeout(timer);
    }, [threadState, sendingCoder, sendingObserver, metaBusy]);

    // Auto-observe: fire Observer automatically when Coder finishes a response.
    useEffect(() => {
      if (!config.autoObserve) return;
      if (sendingObserver || sendingCoder) return;
      const coderMsgs = paneMessages("coder");
      // Find latest completed (non-streaming, non-empty) Coder assistant message.
      const lastAsst = [...coderMsgs].reverse().find(
        (m) => m.role === "assistant" && !m.streaming && String(m.content || "").trim().length > 40
      );
      if (!lastAsst) return;
      const key = (activeThread && activeThread.id ? activeThread.id : "") + ":" + lastAsst.id;
      if (lastAutoObserveMsgRef.current === key) return;
      lastAutoObserveMsgRef.current = key;

      // Auto-observe prompt language:
      // - Do NOT always use UI language. If UI is English but the user is typing Japanese/French,
      //   we want Observer to follow the conversation language to avoid "Observer stuck in English".
      const guessLang = (sample) => {
        try {
          const x = String(sample || "");
          const jp = (x.match(/[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff]/g) || []).length;
          if (jp >= 1) return "ja";
          const acc = (x.match(/[\u00C0-\u017F]/g) || []).length;
          const fr = (x.match(/\b(le|la|les|des|du|de|pour|avec|sans|est|sont|pas|mais|donc|sur|dans|vous|tu|je|nous|votre)\b/gi) || []).length;
          if (acc > 0 || fr >= 2) return "fr";
        } catch (_) {}
        return "en";
      };
      const lastUserSample = (() => {
        try {
          const msgs = (activeThread && activeThread.messages) ? activeThread.messages : [];
          for (let i = msgs.length - 1; i >= 0; i--) {
            const m = msgs[i];
            if (!m || m.role !== "user") continue;
            const t = String(m.content || "").trim();
            if (t) return t;
          }
        } catch (_) {}
        return "";
      })();
      const ol0 = String(config.observerLang || "ui").trim().toLowerCase();
      const uiLang = String(lang || "ja").trim().toLowerCase();
      const promptLang =
        (ol0 === "ja" || ol0 === "en" || ol0 === "fr")
          ? ol0
          : (ol0 === "auto" ? (lastUserSample ? guessLang(lastUserSample) : uiLang) : uiLang);
      // Slight delay to let React settle after streaming ends.
      const timer = setTimeout(() => {
        sendObserver(autoObservePrompt(promptLang));
      }, 700);
      return () => clearTimeout(timer);
    }, [threadState, config.autoObserve, sendingCoder, sendingObserver]);

    useEffect(() => {
      const d = Number(loopInfo.depth) || 0;
      const hue = Math.min(360, d * 20);
      if (typeof document === "undefined" || !document.body) return;
      if (d > 0) {
        document.body.classList.add("looping");
        document.body.style.setProperty("--loop-hue", hue + "deg");
        document.body.style.setProperty("--loop-sat", String(1 + Math.min(0.6, d * 0.05)));
      } else {
        document.body.classList.remove("looping");
        document.body.style.removeProperty("--loop-hue");
        document.body.style.removeProperty("--loop-sat");
      }
      return () => {
        if (typeof document === "undefined" || !document.body) return;
        document.body.classList.remove("looping");
        document.body.style.removeProperty("--loop-hue");
        document.body.style.removeProperty("--loop-sat");
      };
    }, [loopInfo.depth]);

    useEffect(() => {
      refreshStatus();
    }, []);

    useEffect(() => {
      if (!status) return;
      refreshPendingEdits();
      refreshPendingCommands();
      refreshHarnessPromotions();
      refreshMergeGate();
      const t = setInterval(() => {
        refreshPendingEdits();
        refreshPendingCommands();
        refreshHarnessPromotions();
        refreshMergeGate();
      }, 3000);
      return () => clearInterval(t);
    }, [status && status.features]);

    // Safer default: run local commands under a scratch directory, not the Spiral-Coder repo root.
    // This prevents nested git repos (embedded repo warnings) and accidental `git add .` fallout.
    useEffect(() => {
      if (toolRootInitRef.current) return;
      if (!status || !status.features || !status.features.exec) return;
      toolRootInitRef.current = true;

      const cur = String(config.toolRoot || "").trim();
      const desired = cur || ".tmp";
      if (!cur) setConfig((c) => ({ ...c, toolRoot: desired }));

      // Best-effort: ensure the default scratch dir exists so exec doesn't fail.
      if (desired === ".tmp") {
        const cmd = isWindowsHost()
          ? "New-Item -ItemType Directory -Force -Path '.tmp' | Out-Null"
          : "mkdir -p .tmp";
        postJson("/api/exec", { command: cmd }).catch(() => {});
      }
    }, [status && status.features && status.features.exec]);

    // When using the default scratch dir (.tmp), isolate each thread under .tmp/<threadId>.
    // This prevents collisions ("already exists") and the classic "embedded git repo" failure mode.
    useEffect(() => {
      if (!status || !status.features || !status.features.exec) return;
      if (!activeThread || !activeThread.id) return;
      const root0 = String(config.toolRoot || "").trim();
      const root = resolvedThreadRoot(root0, activeThread.id);
      const wd = resolvedCwd(root0, activeThread.id, activeThread.workdir);
      if (!root0 || !root) return;

      const ensureDir = (p) => {
        if (!p) return;
        const norm = normalizePathSep(p).replace(/\/+$/g, "");
        if (ensuredWorkdirRef.current[norm]) return;
        ensuredWorkdirRef.current[norm] = true;
        const cmd = isWindowsHost()
          ? ("New-Item -ItemType Directory -Force -Path " + psSingleQuote(p) + " | Out-Null")
          : ("mkdir -p " + JSON.stringify(p));
        postJson("/api/exec", { command: cmd }).catch(() => {});
      };

      // Always ensure the per-thread root exists (e.g. .tmp/<threadId>).
      ensureDir(root);
      // Also ensure the per-thread workdir exists, if set (e.g. .tmp/<threadId>/<repo>).
      if (wd && normalizePathSep(wd).replace(/\/+$/g, "") !== normalizePathSep(root).replace(/\/+$/g, "")) {
        ensureDir(wd);
      }
    }, [status && status.features && status.features.exec, config.toolRoot, activeThread && activeThread.id, activeThread && activeThread.workdir]);

    useEffect(() => {
      const root = String(config.toolRoot || "").trim();
      if (!projectScanSupported || !root || root === projectScanRootRef.current) return;
      projectScanRootRef.current = root;
      setProjectScanLoading(true);
      fetch(`/api/project/scan?root=${encodeURIComponent(root)}`)
        .then(r => r.ok ? r.json() : null)
        .then(d => setProjectScan(d && d.root ? d : null))
        .catch(() => setProjectScan(null))
        .finally(() => setProjectScanLoading(false));
    }, [config.toolRoot, projectScanSupported]);

    const refreshStatus = () => {
      fetch("/api/status")
        .then((r) => r.json())
        .then((j) => {
          try { window.__SPIRAL_CODER_HOST_OS = j && j.host_os ? String(j.host_os) : ""; } catch (_) {}
          setStatus(j);
        })
        .catch(() => {});
    };

    const refreshPendingEdits = () => {
      if (!serverSupportsFeature(status, "pending_edits")) return;
      fetch("/api/pending_edits")
        .then((r) => r.json())
        .then((j) => setPendingEdits(j && Array.isArray(j.pending) ? j.pending : []))
        .catch(() => {});
    };

    const refreshPendingCommands = () => {
      if (!serverSupportsFeature(status, "pending_commands")) return;
      fetch("/api/pending_commands")
        .then((r) => r.json())
        .then((j) => setPendingCommands(j && Array.isArray(j.pending) ? j.pending : []))
        .catch(() => {});
    };

    const refreshHarnessPromotions = () => {
      if (!harnessSupported) {
        setHarnessPromotions(null);
        setPromotionGateError("");
        return;
      }
      fetch("/api/harness_promotions")
        .then((r) => r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`)))
        .then((j) => {
          setHarnessPromotions(j && typeof j === "object" ? j : null);
          setPromotionGateError("");
        })
        .catch((err) => {
          setPromotionGateError(String((err && err.message) || err || ""));
        });
    };

    const refreshMergeGate = () => {
      if (!mergeGateSupported) {
        setMergeGate(null);
        setMergeGateError("");
        return;
      }
      fetch("/api/merge_gate")
        .then((r) => r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`)))
        .then((j) => {
          setMergeGate(j && typeof j === "object" ? j : null);
          setMergeGateError("");
        })
        .catch((err) => {
          setMergeGateError(String((err && err.message) || err || ""));
        });
    };

    const promotionReviewCount = harnessPromotions && harnessPromotions.summary
      ? Number(harnessPromotions.summary.needs_review || 0)
      : 0;
    const promotionInboxCount = harnessPromotions && harnessPromotions.summary
      ? Number(harnessPromotions.summary.needs_review || 0) + Number(harnessPromotions.summary.approved || 0)
      : 0;
    const mergeGateActionCount = mergeGate && mergeGate.summary
      ? Number(mergeGate.summary.needs_review || 0)
        + Number(mergeGate.summary.rollback_available || 0)
        + Number(mergeGate.summary.blocked || 0)
      : 0;
    const isPendingApprovalItem = (item) => String((item && item.status) || "").trim() === "pending";
    const pendingEditCount = Array.isArray(pendingEdits)
      ? pendingEdits.filter(isPendingApprovalItem).length
      : 0;
    const pendingCommandCount = Array.isArray(pendingCommands)
      ? pendingCommands.filter(isPendingApprovalItem).length
      : 0;
    const runtimeApprovalCount =
      pendingEditCount
      + pendingCommandCount;

    const scrollToPanel = (ref, fallbackRef) => {
      const target = ref.current || (fallbackRef ? fallbackRef.current : null);
      if (!target || typeof target.scrollIntoView !== "function") return;
      try {
        target.scrollIntoView({ behavior: "smooth", block: "start" });
      } catch (_) {
        target.scrollIntoView();
      }
    };

    const jumpToHarnessReviews = () => scrollToPanel(promotionsPanelRef, settingsPanelRef);
    const jumpToMergeGateReviews = () => scrollToPanel(mergeGatePanelRef, settingsPanelRef);
    const jumpToRuntimeApprovals = () => scrollToPanel(runtimeApprovalsRef, settingsPanelRef);

    useEffect(() => {
      if (observerSubTab !== "meta") return;
      refreshMetaArtifacts(true).catch(() => {});
    }, [observerSubTab, activeThread && activeThread.id, config.toolRoot]);

    const resolvePendingEdit = async (id, approve) => {
      const eid = String(id || "").trim();
      if (!eid || pendingBusy) return;
      setPendingBusy(true);
      try {
        const resp = await postJson(approve ? "/api/approve_edit" : "/api/reject_edit", { id: eid });
        if (approve && resp && resp.item && !sendingCoder) {
          const it = resp.item;
          const action = String(it.action || "").trim();
          const path = String(it.path || "").trim();
          const result = it.result != null ? JSON.stringify(it.result, null, 2) : "";
          const preview = result && result.length > 1800 ? (result.slice(0, 1800) + "\n...truncated...") : result;
          const msg = [
            "[Spiral-Coder] Pending edit approved. Continue without redoing the approved step.",
            `id: ${eid}`,
            action ? `action: ${action}` : "",
            path ? `path: ${path}` : "",
            preview ? ("result:\n" + preview) : "",
          ].filter(Boolean).join("\n");
          // Best-effort: nudge Coder to resume after approval (Lite server pauses tool loops on approvals).
          sendCoder(msg, { origin: "runtime" });
        }
      } catch (_) {
      } finally {
        setPendingBusy(false);
        refreshPendingEdits();
      }
    };

    const resolvePendingCommand = async (id, approve) => {
      const cid = String(id || "").trim();
      if (!cid || pendingBusy) return;
      setPendingBusy(true);
      try {
        const resp = await postJson(approve ? "/api/approve_command" : "/api/reject_command", { id: cid });
        if (approve && resp && resp.item && !sendingCoder) {
          const it = resp.item;
          const command = String(it.command || "").trim();
          const cwd = it.cwd != null ? String(it.cwd || "").trim() : "";
          const result = it.result != null ? JSON.stringify(it.result, null, 2) : "";
          const preview = result && result.length > 1800 ? (result.slice(0, 1800) + "\n...truncated...") : result;
          const msg = [
            "[Spiral-Coder] Pending command approved. Continue without redoing the approved step.",
            `id: ${cid}`,
            command ? `command: ${command}` : "",
            cwd ? `cwd: ${cwd}` : "",
            preview ? ("result:\n" + preview) : "",
          ].filter(Boolean).join("\n");
          sendCoder(msg, { origin: "runtime" });
        }
      } catch (_) {
      } finally {
        setPendingBusy(false);
        refreshPendingCommands();
      }
    };

    const resolveHarnessPromotion = async (id, action) => {
      const pid = String(id || "").trim();
      const act = String(action || "").trim().toLowerCase();
      if (!harnessSupported || !pid || !act || promotionBusy) return;
      const endpoint = act === "approve"
        ? "/api/harness_promotions/approve"
        : act === "hold"
        ? "/api/harness_promotions/hold"
        : act === "apply"
        ? "/api/harness_promotions/apply"
        : "";
      if (!endpoint) return;
      setPromotionBusy(true);
      try {
        const resp = await postJson(endpoint, { id: pid });
        if (resp && resp.board) {
          setHarnessPromotions(resp.board);
          setPromotionGateError("");
          if (act === "apply") {
            governorContractCache = null;
            governorContractPromise = null;
          }
        }
      } catch (err) {
        setPromotionGateError(String((err && err.message) || err || ""));
      } finally {
        setPromotionBusy(false);
        refreshHarnessPromotions();
      }
    };

    const resolveMergeGate = async (id, action) => {
      const gid = String(id || "").trim();
      const act = String(action || "").trim().toLowerCase();
      if (!mergeGateSupported || !gid || !act || mergeGateBusy) return;
      const endpoint = act === "approve"
        ? "/api/merge_gate/approve"
        : act === "hold"
        ? "/api/merge_gate/hold"
        : "";
      if (!endpoint) return;
      setMergeGateBusy(true);
      try {
        const resp = await postJson(endpoint, { id: gid });
        if (resp && resp.board) {
          setMergeGate(resp.board);
          setMergeGateError("");
        }
      } catch (err) {
        setMergeGateError(String((err && err.message) || err || ""));
      } finally {
        setMergeGateBusy(false);
        refreshMergeGate();
      }
    };

    const keyOpenAI =
      status && status.providers && status.providers["openai-compatible"]
        ? !!status.providers["openai-compatible"].api_key_present
        : false;
    const keyMistral = status && status.providers && status.providers.mistral ? !!status.providers.mistral.api_key_present : false;
    const keyCodestral = status && status.providers && status.providers.codestral ? !!status.providers.codestral.api_key_present : false;
    const keyGemini = status && status.providers && status.providers.gemini ? !!status.providers.gemini.api_key_present : false;
    const keyAnthropic =
      status && status.providers && status.providers.anthropic ? !!status.providers.anthropic.api_key_present : false;

    const chatProvider = String(config.provider || "").trim();
    const codeProvider = String(config.codeProvider || "").trim() || chatProvider;
    const observerProvider = String(config.observerProvider || "").trim() || chatProvider;

    const chatKeyPresent = String(chatApiKey || "").trim().length > 0;
    const codeKeyPresent = String(codeApiKey || "").trim().length > 0;
    const observerKeyPresent = String(observerApiKey || "").trim().length > 0;

    const typedKeyFor = (p) => {
      const pp = String(p || "").trim();
      if (!pp) return false;
      if (chatProvider === pp && chatKeyPresent) return true;
      if (codeProvider === pp && codeKeyPresent) return true;
      if (observerProvider === pp && observerKeyPresent) return true;
      if ((chatProvider === pp || codeProvider === pp || observerProvider === pp) && (chatKeyPresent || codeKeyPresent || observerKeyPresent)) return true;
      return false;
    };

    const keyMistralOk = keyMistral || typedKeyFor("mistral");
    const keyCodestralOk = keyCodestral || typedKeyFor("codestral");
    const keyOpenAIOk = keyOpenAI || typedKeyFor("openai-compatible");
    const keyGeminiOk = keyGemini || typedKeyFor("gemini");
    const keyAnthropicOk = keyAnthropic || typedKeyFor("anthropic");

    const KeyDot = ({ ok }) => e("span", { className: "kdot " + (ok ? "ok" : "missing") });

    const serverOutdated =
      status && status.ok && (!status.workspace_root || !status.features);

    const providerLabel = (k) => {
      const p = PROVIDERS.find((x) => x.k === k);
      return (p && p.l && p.l[lang]) || k;
    };

    const modeLabel = (k) => {
      const m = MODES.find((x) => x.k === k);
      return (m && m.l && m.l[lang]) || k;
    };

    const modeUsesCode = (mode) => {
      const m0 = String(mode || "");
      return m0 === "VIBE" || m0.startsWith("diff") || m0 === "ログ解析";
    };

    const coderResolvedProvider = (mode) => (modeUsesCode(mode) ? (String(config.codeProvider || "").trim() || config.provider) : config.provider);
    const coderResolvedModel = (mode) => {
      const useCode = modeUsesCode(mode);
      const m = useCode ? (config.codeModel || config.model) : (config.chatModel || config.model);
      return String(m || "").trim();
    };

    const observerResolvedProvider = () => (String(config.observerProvider || "").trim() || config.provider);
    const observerResolvedModel = () => {
      const m0 = String(config.observerModel || "").trim();
      const m = m0 ? m0 : (config.chatModel || config.model);
      return String(m || "").trim();
    };

    const coderActiveModel = () => coderResolvedModel(config.mode);
    const observerActiveModel = () => observerResolvedModel();

    const shortModel = (m) => {
      const s = String(m || "").trim();
      if (!s) return "";
      return s.length > 28 ? s.slice(0, 28) + "..." : s;
    };

    const scrollBottom = (ref) => {
      const el = ref && ref.current;
      if (!el) return;
      el.scrollTop = el.scrollHeight;
    };

    const applyPreset = (kind) => {
      const p = PRESETS[kind];
      if (!p) return;
      setConfig({ ...DEFAULT_CONFIG, ...p });
    };

    const setProviderSafe = (provider) => {
      const p = String(provider || "").trim();
      let next = { ...config, provider: p };

      if (p === "mistral") {
        next = { ...next, ...PRESETS.vibe };
      } else if (p === "codestral") {
        next = { ...next, ...PRESETS.codestral };
      } else if (p === "mistral-cli") {
        next.provider = "mistral-cli";
        next.baseUrl = "";
        next.model = next.model || "mistral-medium-latest";
        next.chatModel = next.chatModel || next.model;
        next.codeModel = next.codeModel || next.model;
      } else if (p === "openai-compatible") {
        next = { ...next, ...PRESETS.openai };
      } else if (p === "gemini") {
        next = { ...next, ...PRESETS.gemini };
      } else if (p === "anthropic") {
        next = { ...next, ...PRESETS.anthropic };
      } else if (p === "hf") {
        next.provider = "hf";
        next.baseUrl = next.baseUrl || "http://localhost";
        next.model = next.model || "local";
        next.chatModel = next.chatModel || next.model;
        next.codeModel = next.codeModel || next.model;
      }

      setModels([]);
      setModelsErr("");
      setConfig(next);
    };

    const fetchModels = async () => {
      try {
        setModelsLoading(true);
        setModelsErr("");
        const target = String(modelsTarget || "chat");
        const isCode = target === "code";
        const isObs = target === "observer";
        const provider = isCode
          ? (String(config.codeProvider || "").trim() || config.provider)
          : isObs
            ? (String(config.observerProvider || "").trim() || config.provider)
            : config.provider;
        const baseUrl = isCode
          ? (String(config.codeBaseUrl || "").trim() || config.baseUrl)
          : isObs
            ? (String(config.observerBaseUrl || "").trim() || config.baseUrl)
            : config.baseUrl;
        const apiKey = isCode
          ? (String(codeApiKey || "").trim() || String(chatApiKey || "").trim() || String(observerApiKey || "").trim())
          : isObs
            ? (String(observerApiKey || "").trim() || String(chatApiKey || "").trim() || String(codeApiKey || "").trim())
            : (String(chatApiKey || "").trim() || String(codeApiKey || "").trim() || String(observerApiKey || "").trim());
        const j = await postJson("/api/models", {
          provider,
          base_url: strOrUndef(baseUrl),
          api_key: strOrUndef(apiKey),
        });
        const ms = j && Array.isArray(j.models) ? j.models : [];
        setModels(ms);
      } catch (err) {
        setModels([]);
        const m = (err && err.message) ? String(err.message) : String(err || "");
        setModelsErr(m === "Failed to fetch" ? tr(lang, "fetchFailed") : m);
      } finally {
        setModelsLoading(false);
      }
    };

    const createThread = () => {
      setThreadState((s) => {
        const t = makeThread(`${tr(lang, "threads")} #${s.threads.length + 1}`);
        return { threads: [t, ...s.threads], activeId: t.id };
      });
    };

    const renameThread = (id) => {
      const th = threadState.threads.find((t) => t.id === id);
      setEditingTitle(th ? th.title : "");
      setEditingThreadId(id);
    };

    const commitRename = (id) => {
      const title = editingTitle.trim();
      if (title) {
        setThreadState((s) => ({
          ...s,
          threads: s.threads.map((t) => (t.id === id ? { ...t, title, updatedAt: Date.now() } : t)),
        }));
      }
      setEditingThreadId(null);
    };

    const deleteThread = (id) => {
      setConfirmDeleteId(id);
    };

    const confirmDelete = (id) => {
      setConfirmDeleteId(null);
      setThreadState((s) => {
        const remain = s.threads.filter((t) => t.id !== id);
        const nextThreads = remain.length ? remain : [makeThread("Thread 1")];
        const nextActive = nextThreads.some((t) => t.id === s.activeId) ? s.activeId : nextThreads[0].id;
        return { threads: nextThreads, activeId: nextActive };
      });
    };

    const clearActiveThread = () => {
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => (t.id === s.activeId ? { ...t, messages: [], updatedAt: Date.now() } : t)),
      }));
    };

    const exportMd = () => {
      const md = transcriptMd(activeThread, {
        thread: activeThread.title,
        exported_at: new Date().toISOString(),
        provider: config.provider,
        chat_model: config.chatModel || config.model,
        code_model: config.codeModel || config.model,
        base_url: config.baseUrl,
        coder_mode: config.mode,
        coder_persona: config.persona,
        coder_model_selected: coderActiveModel(),
        observer_mode: config.observerMode,
        observer_persona: config.observerPersona,
        observer_intensity: config.observerIntensity,
        observer_model_selected: observerActiveModel(),
      });
      const safe = String(activeThread.title || "thread").replace(/[\\/:*?\"<>|]/g, "_");
      downloadText(`spiral-coder-${safe}.md`, md);
    };

    const copyText = async (text, id) => {
      try {
        if (navigator.clipboard && navigator.clipboard.writeText) {
          await navigator.clipboard.writeText(String(text || ""));
          if (id) { setCopiedId(id); setTimeout(() => setCopiedId(null), 1400); }
          return;
        }
      } catch (_) {}
      window.prompt(tr(lang, "copy"), String(text || ""));
    };

    const runCmd = async (msgId, blockIdx, langHint, codeText) => {
      setExecResults(prev => ({
        ...prev,
        [msgId]: { ...(prev[msgId] || {}), [blockIdx]: { stdout: "", stderr: "", exit_code: 0, running: true } },
      }));
      try {
        const cwd = resolvedCwd(config.toolRoot, activeThread && activeThread.id, activeThread && activeThread.workdir);
        const command = normalizeExecScript(langHint, codeText);
        const danger = dangerousCommandReason(command);
        if (danger && !confirmDangerous(lang, danger)) {
          setExecResults(prev => ({
            ...prev,
            [msgId]: { ...(prev[msgId] || {}), [blockIdx]: { stdout: "", stderr: "cancelled", exit_code: -2, running: false } },
          }));
          return;
        }
        const result = await postJson("/api/exec", { command, cwd });
        setExecResults(prev => ({
          ...prev,
          [msgId]: { ...(prev[msgId] || {}), [blockIdx]: {
            stdout: String(result.stdout || ""),
            stderr: String(result.stderr || ""),
            exit_code: typeof result.exit_code === "number" ? result.exit_code : -1,
            running: false,
          }},
        }));
      } catch (err) {
        setExecResults(prev => ({
          ...prev,
          [msgId]: { ...(prev[msgId] || {}), [blockIdx]: {
            stdout: "", stderr: String(err.message || "error"), exit_code: -1, running: false,
          }},
        }));
      }
    };

    const openFile = async (path) => {
      try {
        const cwd = resolvedCwd(config.toolRoot, activeThread && activeThread.id, activeThread && activeThread.workdir);
        await postJson("/api/open", { path, cwd });
      } catch (_) {}
    };

    const metaLayerClass = (layer) => "meta-layer-" + normalizeMetaFixLayer(layer);

    const renderMetaDiagnosisCard = (diag) => {
      if (!diag) return null;
      const causes = Array.isArray(diag.causes) ? diag.causes : [];
      const experiments = Array.isArray(diag.recommended_experiments) ? diag.recommended_experiments : [];
      const unchanged = Array.isArray(diag.do_not_change) ? diag.do_not_change : [];
      return e(
        "div",
        { className: "meta-diag-card" },
        e(
          "div",
          { className: "meta-diag-head" },
          e("span", { className: "pill meta-pill" }, tr(lang, "metaBadge")),
          diag.primary_failure ? e("span", { className: "meta-primary" }, diag.primary_failure) : null
        ),
        diag.summary ? e("div", { className: "meta-summary" }, diag.summary) : null,
        causes.length
          ? e(
              "div",
              { className: "meta-section" },
              causes.map((cause, idx) =>
                e(
                  "div",
                  { key: `cause-${idx}`, className: "meta-cause" },
                  e(
                    "div",
                    { className: "meta-cause-head" },
                    e("strong", null, cause.label || `cause_${idx + 1}`),
                    e("span", { className: `meta-layer ${metaLayerClass(cause.fix_layer)}` }, cause.fix_layer || "no_change"),
                    e("span", { className: "meta-confidence" }, `${Math.round((Number(cause.confidence) || 0) * 100)}%`)
                  ),
                  cause.why ? e("div", { className: "meta-cause-why" }, cause.why) : null,
                  cause.evidence && cause.evidence.length
                    ? e(
                        "ul",
                        { className: "meta-list" },
                        cause.evidence.map((item, evidenceIdx) => e("li", { key: `e-${idx}-${evidenceIdx}` }, item))
                      )
                    : null,
                  cause.minimal_patch ? e("div", { className: "meta-patch" }, cause.minimal_patch) : null
                )
              )
            )
          : null,
        experiments.length
          ? e(
              "div",
              { className: "meta-section" },
              e("div", { className: "meta-label" }, "experiments"),
              experiments.map((item, idx) =>
                e(
                  "div",
                  { key: `exp-${idx}`, className: "meta-experiment" },
                  item.change ? e("div", null, `change: ${item.change}`) : null,
                  item.verify ? e("div", null, `verify: ${item.verify}`) : null,
                  item.expected_signal ? e("div", null, `signal: ${item.expected_signal}`) : null
                )
              )
            )
          : null,
        unchanged.length
          ? e(
              "div",
              { className: "meta-section" },
              e("div", { className: "meta-label" }, "do_not_change"),
              e(
                "ul",
                { className: "meta-list" },
                unchanged.map((item, idx) => e("li", { key: `nc-${idx}` }, item))
              )
            )
          : null
      );
    };

    const renderObserverNextActionCard = (assist) => {
      if (!assist) return null;
      const nextActions = Array.isArray(assist.nextActions) ? assist.nextActions : [];
      return e(
        "div",
        { className: "next-action-card" },
        e(
          "div",
          { className: "next-action-head" },
          e("span", { className: "pill next-action-pill" }, tr(lang, "nextActionBadge"))
        ),
        assist.blocker
          ? e(
              "div",
              { className: "next-action-section" },
              e("div", { className: "meta-label" }, "blocker"),
              e("div", { className: "next-action-text" }, assist.blocker)
            )
          : null,
        nextActions.length
          ? e(
              "div",
              { className: "next-action-section" },
              e("div", { className: "meta-label" }, "next_actions"),
              e(
                "ol",
                { className: "next-action-list" },
                nextActions.map((item, idx) => e("li", { key: `next-${idx}` }, item))
              )
            )
          : null,
        assist.quickestCheck
          ? e(
              "div",
              { className: "next-action-section" },
              e("div", { className: "meta-label" }, "quickest_check"),
              e("div", { className: "next-action-text mono" }, assist.quickestCheck)
            )
          : null,
        assist.whyThisFirst
          ? e(
              "div",
              { className: "next-action-section" },
              e("div", { className: "meta-label" }, "why_this_first"),
              e("div", { className: "next-action-text" }, assist.whyThisFirst)
            )
          : null,
        assist.fallback
          ? e(
              "div",
              { className: "next-action-section" },
              e("div", { className: "meta-label" }, "fallback"),
              e("div", { className: "next-action-text" }, assist.fallback)
            )
          : null
      );
    };

    const renderMetaViewer = () => {
      const needle = String(metaArtifactThreadFilter || "").trim().toLowerCase();
      const items = metaArtifacts.filter((item) => {
        if (!item || typeof item !== "object") return false;
        if (metaArtifactParseOnly && !item.parse_ok) return false;
        if (!needle) return true;
        const hay = [item.thread_id, item.target_message_id, item.primary_failure, item.name]
          .map((x) => String(x || "").toLowerCase())
          .join("\n");
        return hay.includes(needle);
      });
      const failureCounts = Array.from(
        items.reduce((map, item) => {
          const key = String(item && item.primary_failure || "").trim();
          if (!key) return map;
          map.set(key, (map.get(key) || 0) + 1);
          return map;
        }, new Map())
      )
        .sort((a, b) => {
          if (b[1] !== a[1]) return b[1] - a[1];
          return String(a[0]).localeCompare(String(b[0]));
        })
        .slice(0, 6);
      const detail = metaArtifactDetail;
      const artifact = detail && detail.artifact && typeof detail.artifact === "object" ? detail.artifact : null;
      const diagnosis = artifact && artifact.diagnosis ? normalizeMetaDiagnosis(artifact.diagnosis) : null;
      const rawResponse = artifact ? String(artifact.raw_response || detail && detail.raw || "").trim() : String(detail && detail.raw || "").trim();
      const completedTs = artifact && artifact.ts ? Date.parse(String(artifact.ts)) : NaN;
      return e(
        "div",
        { className: "meta-viewer" },
        e(
          "div",
          { className: "meta-viewer-toolbar" },
          e("button", {
            className: "btn btn-icon",
            type: "button",
            disabled: metaArtifactsLoading || metaArtifactDetailLoading,
            onClick: () => refreshMetaArtifacts(true),
          }, tr(lang, "metaViewerRefresh")),
          e("input", {
            className: "input",
            value: metaArtifactThreadFilter,
            placeholder: tr(lang, "metaViewerThreadFilter"),
            onChange: (ev) => setMetaArtifactThreadFilter(ev.target.value),
            style: { flex: 1, minWidth: 120 },
          }),
          e("label", { className: "meta-viewer-toggle" },
            e("input", {
              type: "checkbox",
              checked: metaArtifactParseOnly,
              onChange: (ev) => setMetaArtifactParseOnly(!!ev.target.checked),
            }),
            e("span", null, tr(lang, "metaViewerParseOkOnly"))
          )
        ),
        e(
          "div",
          { className: "meta-viewer-root" },
          metaArtifactRoot || "."
        ),
        failureCounts.length
          ? e(
              "div",
              { className: "meta-viewer-counts" },
              e("span", { className: "meta-label" }, "primary_failure"),
              failureCounts.map(([label, count]) =>
                e("span", { key: label, className: "meta-viewer-count-chip" }, `${label} ×${count}`)
              )
            )
          : null,
        e(
          "div",
          { className: "meta-viewer-grid" },
          e(
            "div",
            { className: "meta-viewer-list" },
            metaArtifactsLoading
              ? e("div", { className: "pane-empty" }, e("p", { className: "pane-empty-hint" }, tr(lang, "loading")))
              : metaArtifactsError
                ? e("div", { className: "pane-empty" }, e("p", { className: "pane-empty-hint" }, metaArtifactsError))
                : items.length === 0
                  ? e("div", { className: "pane-empty" }, e("p", { className: "pane-empty-hint" }, tr(lang, "metaViewerEmpty")))
                  : items.map((item, idx) => {
                      const name = String(item && item.name || "");
                      const active = name && name === metaArtifactSelectedName;
                      const parseOk = !!(item && item.parse_ok);
                      return e(
                        "button",
                        {
                          key: name || `meta-item-${idx}`,
                          type: "button",
                          className: "meta-viewer-item" + (active ? " active" : ""),
                          onClick: () => loadMetaArtifactDetail(metaArtifactRoot || currentMetaArtifactRoot(), name),
                        },
                        e(
                          "div",
                          { className: "meta-viewer-item-head" },
                          e("span", { className: "meta-viewer-item-title" }, String(item && item.primary_failure || name || "meta")),
                          e("span", { className: "meta-viewer-item-status" + (parseOk ? " ok" : " bad") }, parseOk ? "parse_ok" : "parse_fail")
                        ),
                        e("div", { className: "meta-viewer-item-sub" }, String(item && item.thread_id || "")),
                        e("div", { className: "meta-viewer-item-sub" }, String(item && item.target_message_id || "")),
                        e("div", { className: "meta-viewer-item-sub mono" }, String(item && item.ts || ""))
                      );
                    })
          ),
          e(
            "div",
            { className: "meta-viewer-detail" },
            metaArtifactDetailLoading
              ? e("div", { className: "pane-empty" }, e("p", { className: "pane-empty-hint" }, tr(lang, "loading")))
              : !detail
                ? e("div", { className: "pane-empty" }, e("p", { className: "pane-empty-hint" }, tr(lang, "metaViewerSelect")))
                : e(React.Fragment, null,
                    e(
                      "div",
                      { className: "meta-viewer-detail-head" },
                      e("div", { className: "meta-viewer-detail-title" }, String(detail.name || "meta artifact")),
                      e("div", { className: "meta-viewer-detail-actions" },
                        e("span", { className: "meta-viewer-item-status" + (detail.parse_ok ? " ok" : " bad") }, detail.parse_ok ? "parse_ok" : "parse_fail"),
                        e("button", {
                          className: "btn btn-icon",
                          type: "button",
                          disabled: metaBusy || sendingObserver,
                          onClick: () => rerunMetaDiagnoseFromArtifact(detail),
                        }, tr(lang, "metaViewerRerun")),
                        e("button", {
                          className: "btn btn-icon",
                          type: "button",
                          onClick: () => setReaderModal({
                            title: String(detail.name || tr(lang, "metaViewer")),
                            ts: Number.isFinite(completedTs) ? completedTs : Date.now(),
                            content: "```json\n" + String(detail.raw || JSON.stringify(detail.artifact || {}, null, 2)) + "\n```",
                          }),
                        }, tr(lang, "metaViewerOpenJson"))
                      )
                    ),
                    e(
                      "div",
                      { className: "meta-viewer-kv" },
                      e("span", null, `thread: ${String(artifact && artifact.thread_id || "")}`),
                      e("span", null, `target: ${String(artifact && artifact.target_message_id || "")}`),
                      e("span", null, `provider: ${String(artifact && artifact.provider || "")}`),
                      e("span", null, `model: ${String(artifact && artifact.model || "")}`)
                    ),
                    diagnosis
                      ? e(
                          "div",
                          { className: "meta-viewer-section" },
                          e("div", { className: "meta-label" }, tr(lang, "metaViewerSummary")),
                          renderMetaDiagnosisCard(diagnosis)
                        )
                      : null,
                    !diagnosis && detail.parse_error
                      ? e("div", { className: "meta-viewer-parse-error" }, String(detail.parse_error))
                      : null,
                    artifact && artifact.packet && artifact.packet.actual_outcome
                      ? e(
                          "details",
                          { className: "ctx-details" },
                          e("summary", { className: "ctx-summary" }, "packet.actual_outcome"),
                          e("pre", { className: "ctx-pre" }, String(artifact.packet.actual_outcome || ""))
                        )
                      : null,
                    rawResponse
                      ? e(
                          "details",
                          { className: "ctx-details" },
                          e("summary", { className: "ctx-summary" }, tr(lang, "metaViewerRawResponse")),
                          e("pre", { className: "ctx-pre meta-viewer-raw" }, rawResponse)
                        )
                      : null
                  )
          )
        )
      );
    };

    const renderMessage = (m) => {
      const canExec = !!(status && status.features && status.features.exec);
      const canOpen = !!(status && status.features && status.features.open_file);
      const raw = String(m && m.content ? m.content : "");
      const pane = (m && (m.pane === "observer" || m.pane === "chat" || m.pane === "coder")) ? m.pane : "coder";
      const s =
        (!m.streaming && m.role === "assistant" && pane === "observer")
          ? stripObserverMeta(raw)
          : raw;
      // Observer critiques are often long; collapsing them makes the pane hard to read.
      // Use a higher threshold so only extreme walls of text collapse by default.
      const longCharLimit = pane === "observer" ? 5600 : 2600;
      const longLineLimit = pane === "observer" ? 90 : 40;
      const isLong = !m.streaming && (s.length > longCharLimit || (s.match(/\n/g) || []).length > longLineLimit);
      const isExpanded = expandedMsgs.has(m.id);
      const isCollapsed = isLong && !isExpanded;
      const choices = (!m.streaming && m.role === "assistant" && m.pane !== "observer") ? extractChoices(s) : [];
      const metaDiagnosis = (!m.streaming && pane === "observer" && isMetaDiagnoseMessage(m) && m.role === "assistant")
        ? parseMetaDiagnosis(s)
        : null;
      const nextActionAssist = (!m.streaming && pane === "observer" && isObserverNextActionMessage(m) && m.role === "assistant")
        ? parseObserverNextAction(s)
        : null;
      const streamingNode = e("span", null,
        s || e("span", { className: "thinking" }, tr(lang, "streaming")),
        e("span", { className: "cursor-blink" }, "▊")
      );
      // File chips: shown below completed Coder assistant messages when open_file is supported.
      const isCoderAsst = !m.streaming && m.role === "assistant" && m.pane !== "observer" && m.pane !== "chat";
      const canMetaDiagnose = isFailedCoderMessage(m);
      const canSuggestNext = isFailedCoderMessage(m);
      const observerSpecialBadge = pane === "observer" && isMetaDiagnoseMessage(m)
        ? tr(lang, "metaBadge")
        : (pane === "observer" && isObserverNextActionMessage(m) ? tr(lang, "nextActionBadge") : "");
      const fileChips = (canOpen && isCoderAsst) ? extractPathHints(s, 10) : [];
      return e(
        "div",
        { key: m.id, className: "msg msg-pane-" + pane + (m.role === "user" ? " msg-user" : " msg-assistant") + (m.pane === "chat" ? " chat-msg" : "") },
        e("div", { className: "avatar" }, avatarLabel(m)),
        e(
          "div",
          { className: "bubble " + (m.role === "user" ? "user" : "assistant") + " pane-" + pane },
          e(
            "div",
            { className: "msg-meta" },
            e("div", { className: "who" }, whoLabel(m, lang), observerSpecialBadge ? ` · ${observerSpecialBadge}` : ""),
            m.ts ? e("span", { className: "msg-ts" }, relativeTime(m.ts, lang)) : null,
            e("div", { className: "mini" },
              isLong && e("button", {
                onClick: () => setExpandedMsgs((prev) => {
                  const next = new Set(prev);
                  if (next.has(m.id)) next.delete(m.id); else next.add(m.id);
                  return next;
                }),
              }, isExpanded ? tr(lang, "less") : tr(lang, "more")),
              (!m.streaming && m.role === "assistant" && (m.pane === "observer" || isLong)) ? e("button", {
                onClick: () => setReaderModal({
                  id: m.id,
                  title: whoLabel(m, lang) + (activeThread && activeThread.title ? (" · " + activeThread.title) : ""),
                  ts: m.ts,
                  content: String(m.content || ""),
                }),
              }, tr(lang, "reader")) : null,
              canMetaDiagnose ? e("button", {
                className: "pill-btn",
                type: "button",
                disabled: metaBusy || sendingObserver,
                onClick: () => runMetaDiagnose(`msg:${m.id}`),
              }, tr(lang, "whyFail")) : null,
              canSuggestNext ? e("button", {
                className: "pill-btn",
                type: "button",
                disabled: sendingObserver,
                onClick: () => runObserverNextActionAssist(`msg:${m.id}`, detectMetaFailureKind(m.content)),
              }, tr(lang, "suggestNext")) : null,
              e("button", {
                className: copiedId === m.id ? "copied" : "",
                onClick: () => copyText(m.content || "", m.id),
              }, copiedId === m.id ? "✓" : tr(lang, "copy"))
              ,
              (!m.streaming && m.role === "assistant" && m.pane === "chat") ? e("button", {
                title: lang === "fr" ? "Mettre dans l'entrée Coder" : lang === "en" ? "Put into Coder input" : "Coder入力に入れる",
                onClick: () => {
                  const t = String(m.content || "").trim();
                  if (!t) return;
                  setCoderInput((prev) => {
                    const p = String(prev || "").trim();
                    return p ? (p + "\n\n" + t) : t;
                  });
                  showToast(lang === "fr" ? "Ajouté à l'entrée Coder." : lang === "en" ? "Added to Coder input." : "Coder入力に追加しました。", "success");
                },
              }, "→C") : null,
              (!m.streaming && m.role === "assistant" && m.pane === "chat") ? e("button", {
                title: lang === "fr" ? "Mettre dans l'entrée Observer" : lang === "en" ? "Put into Observer input" : "Observer入力に入れる",
                onClick: () => {
                  const t = String(m.content || "").trim();
                  if (!t) return;
                  setObserverSubTab("analysis");
                  setObserverInput((prev) => {
                    const p = String(prev || "").trim();
                    return p ? (p + "\n\n" + t) : t;
                  });
                  showToast(lang === "fr" ? "Ajouté à l'entrée Observer." : lang === "en" ? "Added to Observer input." : "Observer入力に追加しました。", "success");
                },
              }, "→O") : null
            )
          ),
          e(
            "div",
            { className: "content" + (isCollapsed ? " content-collapsed" : "") },
            m.streaming
              ? streamingNode
              : metaDiagnosis
                ? renderMetaDiagnosisCard(metaDiagnosis)
                : nextActionAssist
                  ? renderObserverNextActionCard(nextActionAssist)
                : renderWithThink(
                    s,
                    execResults[m.id] || {},
                    canExec ? ((blockIdx, langHint, codeText) => runCmd(m.id, blockIdx, langHint, codeText)) : null,
                    canOpen ? openFile : null
                  ),
            choices && choices.length
              ? e(
                  "div",
                  { className: "choice-row" },
                  choices.map((c) =>
                    e(
                      "button",
                      {
                        key: "ch" + String(c.n),
                        className: "choice-btn",
                        type: "button",
                        onClick: () => sendCoder(choiceReply(lang, c.n, c.text)),
                      },
                      String(c.n) + ") " + String(c.text).slice(0, 42)
                    )
                  )
                )
              : null
          ),
          fileChips.length ? e(
            "div",
            { className: "file-chips" },
            fileChips.map((p) =>
              e("button", {
                key: p,
                className: "file-chip",
                title: p,
                onClick: () => openFile(p),
              }, "📂 " + p)
            )
          ) : null
        )
      );
    };

    const appendDelta = (threadId, msgId, delta, bodyRef) => {
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => {
          if (t.id !== threadId) return t;
          return {
            ...t,
            updatedAt: Date.now(),
            messages: (t.messages || []).map((m) =>
              m.id === msgId ? { ...m, content: String(m.content || "") + String(delta || "") } : m
            ),
          };
        }),
      }));
      requestAnimationFrame(() => scrollBottom(bodyRef));
    };

    const setMsg = (threadId, msgId, content, bodyRef) => {
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => {
          if (t.id !== threadId) return t;
          return {
            ...t,
            updatedAt: Date.now(),
            messages: (t.messages || []).map((m) => (m.id === msgId ? { ...m, content, streaming: false } : m)),
          };
        }),
      }));
      requestAnimationFrame(() => scrollBottom(bodyRef));
    };

    const ensureTitle = (threadId, userText) => {
      const autoPrefix = tr(lang, "threads") + " #";
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => {
          if (t.id !== threadId) return t;
          const title = String(t.title || "");
          const isAuto = title === "Untitled" || /^Thread\\s+\\d+/.test(title) || title.indexOf(autoPrefix) === 0;
          if (!isAuto) return t;
          return { ...t, title: titleFrom(userText), updatedAt: Date.now() };
        }),
      }));
    };

    const stopCoder = () => {
      if (abortCoderRef.current) abortCoderRef.current.abort();
      abortCoderRef.current = null;
      setSendingCoder(false);
    };

    const stopObserver = () => {
      if (abortObserverRef.current) abortObserverRef.current.abort();
      abortObserverRef.current = null;
      setSendingObserver(false);
    };

    const prettyErr = (err) => {
      const m = (err && err.message) ? String(err.message) : String(err || "");
      return m === "Failed to fetch" ? tr(lang, "fetchFailed") : (m || tr(lang, "error"));
    };

    const paneMessages = (pane) => {
      const all = (activeThread && activeThread.messages) ? activeThread.messages : [];
      if (pane === "observer") return all.filter((m) => m.pane === "observer");
      if (pane === "chat") return all.filter((m) => m.pane === "chat");
      return all.filter((m) => m.pane !== "observer" && m.pane !== "chat");
    };

    const isMetaDiagnoseMessage = (m) =>
      String(m && m.metaKind ? m.metaKind : "").trim().toLowerCase() === META_DIAGNOSE_KIND;

    const isObserverNextActionMessage = (m) =>
      String(m && m.metaKind ? m.metaKind : "").trim().toLowerCase() === OBSERVER_NEXT_ACTION_KIND;

    const isObserverSpecialMessage = (m) =>
      isMetaDiagnoseMessage(m) || isObserverNextActionMessage(m);

    const observerConversationMessages = () =>
      paneMessages("observer").filter((m) => !isObserverSpecialMessage(m));

    const metaDigestText = (text, maxChars, maxLines) => {
      let out = String(text || "")
        .replace(/\r\n/g, "\n")
        .replace(/(authorization\s*:\s*bearer\s+)[^\s]+/ig, "$1[redacted]")
        .replace(/(api[_ -]?key\s*[:=]\s*)[^\s"']+/ig, "$1[redacted]")
        .replace(/(token\s*[:=]\s*)[^\s"']+/ig, "$1[redacted]")
        .replace(/(cookie\s*[:=]\s*)[^\s"']+/ig, "$1[redacted]")
        .replace(/\b(sk-[A-Za-z0-9_-]{10,}|ghp_[A-Za-z0-9]{10,}|github_pat_[A-Za-z0-9_]{10,}|hf_[A-Za-z0-9_]{10,})\b/g, "[redacted]")
        .trim();
      const limitLines = typeof maxLines === "number" ? Math.max(1, maxLines) : 8;
      out = out
        .split("\n")
        .map((line) => String(line || "").trimEnd())
        .filter((line) => line.trim())
        .slice(0, limitLines)
        .join("\n");
      const limitChars = typeof maxChars === "number" ? Math.max(32, maxChars) : 320;
      if (out.length > limitChars) out = out.slice(0, limitChars - 3) + "...";
      return out;
    };

    const metaFirstLine = (text, maxChars) => {
      const line = metaDigestText(text, maxChars || 180, 2).split("\n")[0] || "";
      return String(line || "").trim();
    };

    const metaFailurePattern = (text) =>
      /(?:\bFAILED\b|\[error\]|\[stop\]|REJECTED BY USER|sandbox breach|stderr:|error:|fatal:|traceback|exception|⚠ The command failed|invalid self-reflection|missing valid <plan>|Missing <think>|GOVERNOR BLOCK|\[goal_check\]\s+The task is NOT complete yet|Tests are failing|Build is failing)/i.test(
        String(text || "")
      );

    const isFailedCoderMessage = (m) =>
      !!(m && !m.streaming && m.role === "assistant" && m.pane === "coder" && metaFailurePattern(m.content));

    const detectMetaFailureKind = (text) => {
      const s = String(text || "").toLowerCase();
      if (!s.trim()) return "unclear";
      if (s.includes("governor block")) return "unclear";
      if (loopInfo && Number(loopInfo.depth) > 0 && (s.includes("loop") || s.includes("repeated") || s.includes("same"))) {
        return "loop";
      }
      if (s.includes("no tool call") || s.includes("[stop]") || s.includes("goal_check")) return "no_tool";
      if (
        s.includes("write_file failed") ||
        s.includes("patch_file failed") ||
        s.includes("apply_diff failed") ||
        s.includes("rejected by user") ||
        s.includes("unsafe path")
      ) return "bad_edit";
      if (s.includes("false success")) return "false_success";
      if (
        s.includes("failed") ||
        s.includes("[error]") ||
        s.includes("stderr:") ||
        s.includes("error:") ||
        s.includes("fatal:") ||
        s.includes("exception") ||
        s.includes("traceback") ||
        s.includes("sandbox breach")
      ) return "tool_error";
      if (loopInfo && Number(loopInfo.depth) > 0) return "loop";
      return "unclear";
    };

    const deriveMetaToolCallSnapshots = (msgs, cwd) => {
      const out = [];
      const toolNames = ["read_file", "write_file", "patch_file", "apply_diff", "search_files", "list_dir", "glob", "exec", "done"];
      for (let i = msgs.length - 1; i >= 0 && out.length < 4; i--) {
        const m = msgs[i];
        if (!m || m.role !== "assistant") continue;
        const content = String(m.content || "");
        const blocks = extractCodeBlocks(content, 2, 180);
        for (const body of blocks) {
          if (out.length >= 4) break;
          const first = metaFirstLine(body, 140);
          if (!first) continue;
          out.push({ name: "exec", args_preview: first, cwd: cwd || null });
        }
        for (const name of toolNames) {
          if (out.length >= 4) break;
          const re = new RegExp(`\\b${name}\\b`, "i");
          if (!re.test(content)) continue;
          out.push({ name, args_preview: metaFirstLine(content, 160), cwd: cwd || null });
        }
      }
      return out.slice(0, 4);
    };

    const deriveMetaToolResultSnapshots = (msgs) => {
      const out = [];
      for (let i = msgs.length - 1; i >= 0 && out.length < 4; i--) {
        const m = msgs[i];
        if (!m || m.role !== "assistant") continue;
        const content = String(m.content || "");
        if (!content.trim()) continue;
        const ok = !metaFailurePattern(content) && /(?:^OK\b|\[goal_check:[^\]]+\] OK|\[goal_check\] all requested stop checks passed)/i.test(content);
        if (!ok && !metaFailurePattern(content)) continue;
        out.push({
          ok,
          summary: metaFirstLine(content, 160),
          stderr_digest: ok ? null : metaDigestText(content, 220, 5),
        });
      }
      return out.slice(0, 4);
    };

    const buildMetaLoopSignals = (toolCalls, toolResults, assistantMessages) => {
      const norm = (value) => normalizeScratchEntry(value);
      const sameHeadCount = (items, keyFn) => {
        if (!items.length) return 0;
        const head = norm(keyFn(items[0]));
        if (!head) return 0;
        return items.filter((item) => norm(keyFn(item)) === head).length;
      };
      return {
        same_command_repeats: sameHeadCount(toolCalls, (item) => item && item.args_preview),
        same_error_repeats: sameHeadCount(toolResults.filter((item) => item && !item.ok), (item) => item && (item.stderr_digest || item.summary)),
        same_output_repeats: sameHeadCount(assistantMessages, (item) => item && item.content),
        ui_loop_depth: Math.max(0, Number(loopInfo && loopInfo.depth) || 0),
      };
    };

    const findMetaTargetMessage = (selector) => {
      const raw = String(selector || "").trim();
      const all = (activeThread && Array.isArray(activeThread.messages)) ? activeThread.messages : [];
      if (!all.length) return null;
      if (/^msg:/i.test(raw)) {
        const id = raw.replace(/^msg:/i, "").trim();
        const msg = all.find((m) => String(m && m.id || "") === id) || null;
        if (!msg) return null;
        if (isMetaDiagnoseMessage(msg)) return null;
        if (msg.pane !== "coder" || msg.role !== "assistant") return null;
        return msg;
      }
      const coderMsgs = paneMessages("coder");
      for (let i = coderMsgs.length - 1; i >= 0; i--) {
        if (isFailedCoderMessage(coderMsgs[i])) return coderMsgs[i];
      }
      if (loopInfo && Number(loopInfo.depth) > 0) {
        for (let i = coderMsgs.length - 1; i >= 0; i--) {
          const m = coderMsgs[i];
          if (m && m.role === "assistant" && !m.streaming) return m;
        }
      }
      return null;
    };

    const buildMetaFailurePacket = async (selector) => {
      if (!activeThread) return null;
      const governorContract = await getGovernorContract();
      const target = findMetaTargetMessage(selector);
      if (!target) return null;
      const coderMsgs = paneMessages("coder");
      const targetIdx = coderMsgs.findIndex((m) => m.id === target.id);
      const baseMsgs = targetIdx >= 0
        ? coderMsgs.slice(Math.max(0, targetIdx - 8), targetIdx + 1)
        : coderMsgs.slice(-8);
      const recentUsers = baseMsgs.filter((m) => m.role === "user").slice(-3);
      const recentAssistants = baseMsgs
        .filter((m) => m.role === "assistant" && !m.streaming && m.id !== target.id)
        .slice(-3);
      const curCwd = resolvedCwd(config.toolRoot, activeThread.id, activeThread.workdir);
      const toolCalls = deriveMetaToolCallSnapshots(baseMsgs, curCwd);
      const toolResults = deriveMetaToolResultSnapshots(baseMsgs);
      const loopSignals = buildMetaLoopSignals(toolCalls, toolResults, baseMsgs.filter((m) => m.role === "assistant" && !m.streaming).slice(-4));
      const approvalSignals = [];
      if (pendingEditCount) approvalSignals.push(`pending_edit_approvals=${pendingEditCount}`);
      if (pendingCommandCount) approvalSignals.push(`pending_command_approvals=${pendingCommandCount}`);
      if (/rejected by user/i.test(String(target.content || ""))) approvalSignals.push("user_rejected_action");
      const projectBits = [];
      if (projectScan && projectScan.stack_label) projectBits.push(String(projectScan.stack_label));
      if (String(config.toolRoot || "").trim()) projectBits.push(`tool_root=${String(config.toolRoot || "").trim()}`);
      if (String(activeThread.workdir || "").trim()) projectBits.push(`workdir=${String(activeThread.workdir || "").trim()}`);
      const recentUserDigest = recentUsers.map((m) => metaDigestText(m.content, 220, 4)).filter(Boolean);
      const recentAssistantDigest = recentAssistants.map((m) => metaDigestText(m.content, 220, 4)).filter(Boolean);
      return {
        thread_id: String(activeThread.id || ""),
        target_message_id: String(target.id || ""),
        task_summary: recentUserDigest[recentUserDigest.length - 1] || metaFirstLine(activeThread.title || "", 120),
        expected_outcome: recentUserDigest[recentUserDigest.length - 1] || "Complete the requested task without failure.",
        actual_outcome: metaDigestText(target.content, 320, 8),
        failure_kind: detectMetaFailureKind(target.content),
        coder_mode: String(config.mode || "").trim(),
        coder_provider: String(config.codeProvider || config.provider || "").trim(),
        coder_model: String(coderActiveModel() || "").trim(),
        observer_model: String(observerActiveModel() || "").trim(),
        tool_root: resolvedThreadRoot(config.toolRoot, activeThread.id) || null,
        cur_cwd: curCwd || null,
        checkpoint: gitCheckpoint || null,
        system_prompt_digest: metaDigestText([
          `coder_mode=${String(config.mode || "").trim()}`,
          `observer_mode=${String(config.observerMode || "").trim()}`,
          `edit_approval=${config.requireEditApproval ? "on" : "off"}`,
          `command_approval=${config.requireCommandApproval ? "on" : "off"}`,
          `governor_blocks=${Array.isArray(governorContract && governorContract.prompt_layout && governorContract.prompt_layout.block_order) ? governorContract.prompt_layout.block_order.join(">") : "plan>think>impact>reflect"}`,
        ].join("\n"), 320, 8),
        project_context_digest: projectBits.length ? projectBits.join(" | ") : null,
        agents_md_digest: null,
        available_tools: contractToolNames(governorContract).slice(0, 16),
        recent_user_messages: recentUserDigest,
        recent_assistant_messages: recentAssistantDigest,
        recent_tool_calls: toolCalls,
        recent_tool_results: toolResults,
        last_error_digest: metaFailurePattern(target.content) ? metaDigestText(target.content, 240, 5) : null,
        loop_signals: loopSignals,
        approval_signals: approvalSignals,
        packet_notes: [
          "observer history excluded from packet context",
          "tool snapshots derived from visible coder thread content",
          "credential-like substrings redacted in digests",
        ],
      };
    };

    const buildMetaDiagnosePrompt = (packet) => {
      const langName = lang === "fr" ? "French" : lang === "en" ? "English" : "Japanese";
      const schema = {
        summary: `${langName} summary`,
        primary_failure: "contract_ambiguity",
        causes: [
          {
            label: "contract_ambiguity",
            why: `${langName} explanation`,
            evidence: ["evidence 1", "evidence 2"],
            fix_layer: "instruction",
            minimal_patch: `${langName} minimal patch`,
            confidence: 0.84,
          },
        ],
        recommended_experiments: [
          {
            change: `${langName} experiment change`,
            verify: `${langName} verification`,
            expected_signal: `${langName} expected signal`,
          },
        ],
        do_not_change: ["repo code itself"],
      };
      return [
        "This is meta analysis, not implementation.",
        "Tool calls, code changes, and diff application are forbidden.",
        "Your task is to diagnose the immediate failure by layer, using the failure packet only.",
        `Write summary/why/evidence/minimal_patch/experiments in ${langName}.`,
        "Keep fix_layer values in English enum form.",
        "Return JSON only. No markdown. No backticks. No commentary outside JSON.",
        "",
        "Requirements:",
        "- Identify up to 3 causes.",
        "- Each cause must include evidence.",
        "- Each cause must choose exactly one fix_layer.",
        "- Keep patches minimal and rerunnable.",
        "- Distinguish repo_code vs agent/harness issues.",
        "- Do not overclaim; use confidence 0.0..1.0.",
        "",
        "fix_layer enum:",
        META_FIX_LAYERS.join(" | "),
        "",
        "Output schema:",
        JSON.stringify(schema, null, 2),
        "",
        "failure packet:",
        "<packet>",
        JSON.stringify(packet, null, 2),
        "</packet>",
      ].join("\n");
    };

    const buildObserverNextActionPrompt = (packet, reasonHint) => {
      const langName = lang === "fr" ? "French" : lang === "en" ? "English" : "Japanese";
      const reason = String(reasonHint || packet && packet.failure_kind || "stuck_or_failure").trim();
      return [
        "This is intervention mode, not critique.",
        "Tool calls, code changes, and diff application are forbidden.",
        "Your task is to help the Coder take the next concrete step only.",
        `Write explanations in ${langName}. Keep the section headers below in English exactly as written.`,
        "",
        "Required output format:",
        "--- blocker ---",
        "<1-2 sentences>",
        "--- next_actions ---",
        "1. <best next concrete action>",
        "2. <backup action>",
        "3. <last resort action>",
        "--- quickest_check ---",
        "<one command, file, or symbol to inspect first>",
        "--- why_this_first ---",
        "<one sentence>",
        "--- fallback ---",
        "<one sentence if the first action fails>",
        "",
        "Rules:",
        "- Prefer small, local, reversible actions.",
        "- Mention exact files, commands, or symbols when possible.",
        "- If the blocker is repo code, say so directly.",
        "- If the blocker is instruction/harness/tooling, say so directly.",
        "- Do not broaden into a full review.",
        "- If evidence is weak, make quickest_check purely diagnostic.",
        "",
        `reason_hint: ${reason}`,
        "stuck packet:",
        "<packet>",
        JSON.stringify(packet, null, 2),
        "</packet>",
      ].join("\n");
    };

    const metaConfigDigest = (packet, obsProvider, obsModel) => {
      const seed = JSON.stringify({
        thread_id: packet && packet.thread_id || "",
        coder_mode: packet && packet.coder_mode || "",
        observer_mode: String(config && config.observerMode || "").trim(),
        provider: String(obsProvider || "").trim(),
        model: String(obsModel || "").trim(),
        tool_root: packet && packet.tool_root || "",
        cur_cwd: packet && packet.cur_cwd || "",
        require_edit_approval: !!(config && config.requireEditApproval),
        require_command_approval: !!(config && config.requireCommandApproval),
        auto_observe: !!(config && config.autoObserve),
      });
      const h = fnv1a64(seed);
      return h.toString(16).padStart(16, "0");
    };

    const saveMetaDiagnoseArtifact = async (root, artifact) =>
      postJson("/api/meta_diagnose/save", {
        root: String(root || "").trim() || undefined,
        artifact,
      });

    const listMetaDiagnoseArtifacts = async (root, limit) =>
      postJson("/api/meta_diagnose/list", {
        root: String(root || "").trim() || undefined,
        limit: typeof limit === "number" ? limit : 120,
      });

    const readMetaDiagnoseArtifact = async (root, name) =>
      postJson("/api/meta_diagnose/read", {
        root: String(root || "").trim() || undefined,
        name: String(name || "").trim(),
      });

    const currentMetaArtifactRoot = () =>
      resolvedThreadRoot(config.toolRoot, activeThread && activeThread.id) || "";

    const findLatestObserverNextActionTarget = () => {
      const coderMsgs = paneMessages("coder");
      for (let i = coderMsgs.length - 1; i >= 0; i--) {
        const msg = coderMsgs[i];
        if (!msg || msg.role !== "assistant" || msg.streaming) continue;
        const content = String(msg.content || "").trim();
        if (!content) continue;
        return isFailedCoderMessage(msg) ? msg : null;
      }
      return null;
    };

    const loadMetaArtifactDetail = async (root, name) => {
      const artifactName = String(name || "").trim();
      if (!artifactName) {
        setMetaArtifactSelectedName("");
        setMetaArtifactDetail(null);
        return;
      }
      setMetaArtifactDetailLoading(true);
      try {
        const res = await readMetaDiagnoseArtifact(root, artifactName);
        setMetaArtifactSelectedName(artifactName);
        setMetaArtifactDetail({
          name: artifactName,
          path: String(res && res.path || "").trim(),
          artifact: res && typeof res.artifact === "object" ? res.artifact : null,
          raw: String(res && res.raw || "").trim(),
          parse_ok: !!(res && res.parse_ok),
          parse_error: res && res.parse_error ? String(res.parse_error) : "",
        });
      } catch (err) {
        setMetaArtifactDetail(null);
        showToast(`${tr(lang, "metaViewerReadFailed")}: ${prettyErr(err)}`, "error");
      } finally {
        setMetaArtifactDetailLoading(false);
      }
    };

    const refreshMetaArtifacts = async (preserveSelection) => {
      if (!activeThread) {
        setMetaArtifacts([]);
        setMetaArtifactRoot("");
        setMetaArtifactSelectedName("");
        setMetaArtifactDetail(null);
        return;
      }
      const root = currentMetaArtifactRoot();
      setMetaArtifactsLoading(true);
      setMetaArtifactsError("");
      try {
        const res = await listMetaDiagnoseArtifacts(root, 160);
        const items = Array.isArray(res && res.items) ? res.items : [];
        setMetaArtifactRoot(root);
        setMetaArtifacts(items);
        const keepName =
          preserveSelection && items.some((item) => String(item && item.name || "") === metaArtifactSelectedName)
            ? metaArtifactSelectedName
            : String(items[0] && items[0].name || "");
        if (!keepName) {
          setMetaArtifactSelectedName("");
          setMetaArtifactDetail(null);
        } else {
          await loadMetaArtifactDetail(root, keepName);
        }
      } catch (err) {
        setMetaArtifacts([]);
        setMetaArtifactDetail(null);
        setMetaArtifactsError(prettyErr(err));
        showToast(`${tr(lang, "metaViewerLoadFailed")}: ${prettyErr(err)}`, "error");
      } finally {
        setMetaArtifactsLoading(false);
      }
    };

    const runMetaDiagnoseForPacket = async (packet) => {
      if (!activeThread || !packet || typeof packet !== "object") return;
      if (sendingObserver || metaBusy) return;
      const threadId = activeThread.id;
      const obsProvider = String(config.observerProvider || "").trim() || config.provider;
      const obsBaseUrl = String(config.observerBaseUrl || "").trim() || config.baseUrl;
      const obsModel = String(config.observerModel || "").trim() || (config.chatModel || config.model);
      const obsKey = String(observerApiKey || "").trim() || String(chatApiKey || "").trim() || String(codeApiKey || "").trim();
      const prompt = buildMetaDiagnosePrompt(packet);
      const startedAt = new Date().toISOString();
      const targetLabel = `[META-DIAGNOSE] target=${packet.target_message_id} kind=${packet.failure_kind}`;
      const userMsg = {
        id: uid(),
        pane: "observer",
        role: "user",
        content: targetLabel,
        ts: Date.now(),
        metaKind: META_DIAGNOSE_KIND,
        metaTargetId: packet.target_message_id,
      };
      const asstId = uid();
      const asstMsg = {
        id: asstId,
        pane: "observer",
        role: "assistant",
        content: "",
        ts: Date.now(),
        streaming: true,
        metaKind: META_DIAGNOSE_KIND,
        metaTargetId: packet.target_message_id,
      };
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => (
          t.id === threadId
            ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), userMsg, asstMsg] }
            : t
        )),
      }));
      setObserverSubTab("analysis");
      setMetaBusy(true);
      setSendingObserver(true);
      showToast(tr(lang, "metaDiagnoseRunning"), "info");
      requestAnimationFrame(() => scrollBottom(observerBodyRef));
      const ac = new AbortController();
      abortObserverRef.current = ac;
      const obsCfg = {
        ...config,
        mode: config.observerMode,
        persona: config.observerPersona,
        cot: "off",
        autonomy: "off",
        provider: obsProvider,
        baseUrl: obsBaseUrl,
        model: obsModel,
        chatModel: obsModel,
        codeModel: obsModel,
      };
      const reqBody = buildReq(obsCfg, obsKey, [], prompt, null);
      reqBody.lang = String(lang || "ja").trim().toLowerCase();
      reqBody.force_tools = false;
      reqBody.temperature = 0.2;
      reqBody.max_tokens = 1800;
      let rawResponse = "";
      let diagnosis = null;
      let parseError = null;
      try {
        let j = await postJson("/api/chat", reqBody, ac.signal);
        rawResponse = String((j && j.content) || "").trim();
        let parsedResult = parseMetaDiagnosisResult(rawResponse);
        diagnosis = parsedResult.diagnosis;
        parseError = parsedResult.parseError;
        if (!diagnosis && !ac.signal.aborted) {
          showToast(tr(lang, "metaDiagnoseJsonRetry"), "info");
          const retryBody = buildReq(
            obsCfg,
            obsKey,
            [
              { role: "user", content: prompt },
              { role: "assistant", content: rawResponse },
            ],
            "Return ONLY valid JSON matching the requested schema. No markdown. No backticks. No commentary.",
            null
          );
          retryBody.lang = reqBody.lang;
          retryBody.force_tools = false;
          retryBody.temperature = 0.1;
          retryBody.max_tokens = 1800;
          j = await postJson("/api/chat", retryBody, ac.signal);
          rawResponse = String((j && j.content) || "").trim();
          parsedResult = parseMetaDiagnosisResult(rawResponse);
          diagnosis = parsedResult.diagnosis;
          parseError = parsedResult.parseError;
        }
        const finalText = diagnosis ? JSON.stringify(diagnosis, null, 2) : rawResponse || "{}";
        setMsg(threadId, asstId, finalText, observerBodyRef);
      } catch (err) {
        const msg = prettyErr(err);
        if (ac.signal.aborted) {
          rawResponse = `[${tr(lang, "stop")}]`;
          parseError = "aborted";
          setMsg(threadId, asstId, rawResponse, observerBodyRef);
        } else {
          rawResponse = `[${tr(lang, "error")}] ${msg}`;
          parseError = `request_failed: ${msg}`;
          setMsg(threadId, asstId, rawResponse, observerBodyRef);
        }
      } finally {
        const artifact = {
          ts: startedAt,
          thread_id: packet.thread_id,
          target_message_id: packet.target_message_id,
          packet,
          observer_prompt: prompt,
          raw_response: rawResponse,
          diagnosis: diagnosis || null,
          parse_ok: !!diagnosis,
          parse_error: diagnosis ? null : (parseError || "invalid_json_or_schema"),
          provider: String(obsProvider || "").trim(),
          model: String(obsModel || "").trim(),
          config_digest: metaConfigDigest(packet, obsProvider, obsModel),
        };
        try {
          const saveRoot = packet.tool_root || resolvedThreadRoot(config.toolRoot, threadId) || "";
          const saved = await saveMetaDiagnoseArtifact(saveRoot, artifact);
          if (saved && saved.path) {
            showToast(`${tr(lang, "metaDiagnoseSaved")}: ${String(saved.path)}`, "success");
            if (observerSubTab === "meta") await refreshMetaArtifacts(true);
          }
        } catch (saveErr) {
          showToast(`${tr(lang, "metaDiagnoseSaveFailed")}: ${prettyErr(saveErr)}`, "error");
        }
        setMetaBusy(false);
        setSendingObserver(false);
        if (abortObserverRef.current === ac) abortObserverRef.current = null;
      }
    };

    const runMetaDiagnose = async (selector) => {
      if (!activeThread) return;
      let targetSpec = String(selector || "").trim();
      if (!targetSpec || normalizeScratchEntry(targetSpec) === "last-fail") targetSpec = "last-fail";
      const packet = await buildMetaFailurePacket(targetSpec);
      if (!packet) {
        showToast(tr(lang, /^msg:/i.test(targetSpec) ? "metaDiagnoseBadTarget" : "metaDiagnoseMissingTarget"), "error");
        return;
      }
      await runMetaDiagnoseForPacket(packet);
    };

    const runObserverNextActionAssistForPacket = async (packet, reasonHint) => {
      if (!activeThread || !packet || typeof packet !== "object") return;
      if (sendingObserver || metaBusy) return;
      const threadId = activeThread.id;
      const obsProvider = String(config.observerProvider || "").trim() || config.provider;
      const obsBaseUrl = String(config.observerBaseUrl || "").trim() || config.baseUrl;
      const obsModel = String(config.observerModel || "").trim() || (config.chatModel || config.model);
      const obsKey = String(observerApiKey || "").trim() || String(chatApiKey || "").trim() || String(codeApiKey || "").trim();
      const prompt = buildObserverNextActionPrompt(packet, reasonHint);
      const targetLabel = `[NEXT-ACTION] target=${packet.target_message_id} kind=${packet.failure_kind}`;
      const userMsg = {
        id: uid(),
        pane: "observer",
        role: "user",
        content: targetLabel,
        ts: Date.now(),
        metaKind: OBSERVER_NEXT_ACTION_KIND,
        metaTargetId: packet.target_message_id,
      };
      const asstId = uid();
      const asstMsg = {
        id: asstId,
        pane: "observer",
        role: "assistant",
        content: "",
        ts: Date.now(),
        streaming: true,
        metaKind: OBSERVER_NEXT_ACTION_KIND,
        metaTargetId: packet.target_message_id,
      };
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => (
          t.id === threadId
            ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), userMsg, asstMsg] }
            : t
        )),
      }));
      setObserverSubTab("analysis");
      setSendingObserver(true);
      showToast(tr(lang, "nextActionRunning"), "info");
      requestAnimationFrame(() => scrollBottom(observerBodyRef));
      const ac = new AbortController();
      abortObserverRef.current = ac;
      const obsCfg = {
        ...config,
        mode: config.observerMode,
        persona: config.observerPersona,
        cot: "off",
        autonomy: "off",
        provider: obsProvider,
        baseUrl: obsBaseUrl,
        model: obsModel,
        chatModel: obsModel,
        codeModel: obsModel,
      };
      const reqBody = buildReq(obsCfg, obsKey, [], prompt, null);
      reqBody.lang = String(lang || "ja").trim().toLowerCase();
      reqBody.force_tools = false;
      reqBody.temperature = 0.2;
      reqBody.max_tokens = 1200;
      try {
        const j = await postJson("/api/chat", reqBody, ac.signal);
        const text = String((j && j.content) || "").trim() || "[Observer] No concrete next action.";
        setMsg(threadId, asstId, text, observerBodyRef);
      } catch (err) {
        const msg = prettyErr(err);
        setMsg(
          threadId,
          asstId,
          ac.signal.aborted ? `[${tr(lang, "stop")}]` : `[${tr(lang, "error")}] ${msg}`,
          observerBodyRef
        );
      } finally {
        setSendingObserver(false);
        if (abortObserverRef.current === ac) abortObserverRef.current = null;
      }
    };

    const runObserverNextActionAssist = async (selector, reasonHint) => {
      if (!activeThread) return;
      let targetSpec = String(selector || "").trim();
      if (!targetSpec || normalizeScratchEntry(targetSpec) === "last-fail") targetSpec = "last-fail";
      const packet = await buildMetaFailurePacket(targetSpec);
      if (!packet) {
        showToast(tr(lang, "nextActionMissingTarget"), "error");
        return;
      }
      await runObserverNextActionAssistForPacket(packet, reasonHint);
    };

    const rerunMetaDiagnoseFromArtifact = async (detail) => {
      const artifact = detail && detail.artifact && typeof detail.artifact === "object" ? detail.artifact : null;
      if (!artifact) return;
      const targetId = String(artifact.target_message_id || "").trim();
      const artifactThreadId = String(artifact.thread_id || "").trim();
      const liveSelector = targetId ? `msg:${targetId}` : "";
      const liveTarget = liveSelector
        && activeThread
        && (!artifactThreadId || artifactThreadId === String(activeThread.id || "").trim())
        ? findMetaTargetMessage(liveSelector)
        : null;
      if (liveTarget) {
        await runMetaDiagnose(liveSelector);
        return;
      }
      const packet = artifact.packet && typeof artifact.packet === "object"
        ? JSON.parse(JSON.stringify(artifact.packet))
        : null;
      if (!packet) {
        showToast(tr(lang, "metaDiagnoseBadTarget"), "error");
        return;
      }
      showToast(tr(lang, "metaViewerRerunSavedPacket"), "info");
      await runMetaDiagnoseForPacket(packet);
    };

    const finishStreaming = (threadId, msgId) => {
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => {
          if (t.id !== threadId) return t;
          return {
            ...t,
            updatedAt: Date.now(),
            messages: (t.messages || []).map((m) => (m.id === msgId ? { ...m, streaming: false } : m)),
          };
        }),
      }));
    };

    const extractCodeBlocks = (text, maxBlocks, maxCharsEach) => {
      const s = String(text || "");
      const out = [];
      const re = /```[^\n]*\n([\s\S]*?)```/g;
      let m;
      while ((m = re.exec(s)) !== null) {
        if (out.length >= maxBlocks) break;
        const body = String(m[1] || "").trim();
        if (!body) continue;
        out.push(body.length > maxCharsEach ? body.slice(0, maxCharsEach) + "..." : body);
      }
      return out;
    };

    const extractPathHints = (text, maxItems) => {
      const s = String(text || "");
      const re = /(?:[A-Za-z]:\\\\[^\s"'`]+|(?:\.\.?\/)?[A-Za-z0-9._-]+(?:\/[A-Za-z0-9._-]+)+\.[A-Za-z0-9._-]+)/g;
      const uniq = {};
      const out = [];
      let m;
      while ((m = re.exec(s)) !== null) {
        const p = String(m[0] || "").trim();
        if (!p || uniq[p]) continue;
        uniq[p] = true;
        out.push(p);
        if (out.length >= maxItems) break;
      }
      return out;
    };

    const buildCliTemplate = () => {
      return [
        "[CLI coding task template]",
        "You are a CLI coding agent.",
        "Work autonomously in long-run mode: decompose into modules, edit files, and verify.",
        "Prioritize concrete file changes over abstract discussion.",
        "Always operate under tool_root (create a new project dir; do NOT touch Spiral-Coder's repo).",
        "Before each major action, write a 3-line scratchpad: goal / risk / next (keep it short).",
        "If an action requires approval, STOP and tell the user which pending edit id(s) to approve, then continue after approval.",
        "Never run destructive cleanup commands (git reset --hard / git clean -fd / git rm ... .) unless the user explicitly asks.",
        "At the end, report: changed files / why / remaining risks.",
      ].join("\n");
    };

    const insertCliTemplate = () => {
      const t = buildCliTemplate();
      setCoderInput((prev) => {
        const p = String(prev || "").trim();
        return p ? (p + "\n\n" + t) : t;
      });
    };

    const coderContextPacket = () => {
      const msgs = paneMessages("coder");
      let lastUser = null;
      let lastAsst = null;
      const recentAsst = [];
      const recentUser = [];
      for (let i = msgs.length - 1; i >= 0; i--) {
        const m = msgs[i];
        if (m.role === "assistant") {
          if (!lastAsst) lastAsst = m;
          if (recentAsst.length < 4) recentAsst.push(m);
        } else if (m.role === "user") {
          if (!lastUser) lastUser = m;
          if (recentUser.length < 3) recentUser.push(m);
        }
      }

      const cut = (t, n) => {
        const s = String(t || "").trim();
        return s.length > n ? s.slice(0, n) + "..." : s;
      };

      const codeBlocks = [];
      recentAsst.forEach((m) => {
        extractCodeBlocks(m.content, 2, 900).forEach((b) => {
          if (codeBlocks.length < 6) codeBlocks.push(b);
        });
      });

      const pathHints = [];
      [...recentAsst, ...recentUser].forEach((m) => {
        extractPathHints(m.content, 12).forEach((p) => {
          if (pathHints.length < 12 && pathHints.indexOf(p) === -1) pathHints.push(p);
        });
      });

      const parts = [];
      parts.push("--- coder_context ---");
      parts.push("observer_view: You can inspect coder outputs and code snippets below.");
      parts.push(`coder_mode: ${String(config.mode || "")}`);
      parts.push(`coder_model: ${coderActiveModel()}`);
      const cwdNow = resolvedCwd(config.toolRoot, activeThread && activeThread.id, activeThread && activeThread.workdir);
      parts.push(`tool_root: ${cwdNow ? String(cwdNow) : ""}`);
      if (lastUser) parts.push("last_user:\n" + cut(lastUser.content, 1200));
      if (lastAsst) parts.push("last_assistant:\n" + cut(lastAsst.content, 2600));
      if (pathHints.length) parts.push("file_hints:\n- " + pathHints.join("\n- "));
      if (codeBlocks.length) {
        parts.push(
          "recent_code_blocks:\n" +
          codeBlocks.map((b, i) => `#${i + 1}\n\`\`\`\n${b}\n\`\`\``).join("\n\n")
        );
      }
      if (diff && String(diff).trim()) parts.push("diff: (present)");
      return parts.join("\n");
    };

    // Chat is not a runtime agent, but it can optionally "see" a small read-only snapshot of
    // what the runtime is doing. Keep this short to avoid token waste and role confusion.
    const lastCoderErrorDigest = (maxChars) => {
      const max = typeof maxChars === "number" ? maxChars : 900;
      const isErrLine = (line) =>
        /(?:\berror\b|fatal:|stderr:|\[error\]|exception|traceback|unauthorized|forbidden|access is denied|アクセスが拒否|認証|権限|not recognized|pathspec|cannot|failed)/i.test(
          String(line || "")
        );
      try {
        const msgs = paneMessages("coder");
        for (let i = msgs.length - 1; i >= 0; i--) {
          const m = msgs[i];
          if (!m || m.role !== "assistant" || m.streaming) continue;
          const t = String(m.content || "");
          if (!t || !isErrLine(t)) continue;
          const lines = t.split("\n");
          let idx = -1;
          for (let j = lines.length - 1; j >= 0; j--) {
            if (isErrLine(lines[j])) { idx = j; break; }
          }
          const start = Math.max(0, idx - 6);
          const end = Math.min(lines.length, idx + 12);
          let snippet = lines.slice(start, end).join("\n").trim();
          if (!snippet) snippet = t.trim();
          if (snippet.length > max) snippet = snippet.slice(0, max) + "...";
          return snippet;
        }
      } catch (_) {}
      return "";
    };

    const chatRuntimePacket = () => {
      try {
        if (!activeThread) return "";
        const threadId = activeThread.id;
        const cut = (t, n) => {
          const s = String(t || "").trim().replace(/\s+\n/g, "\n");
          return s.length > n ? s.slice(0, n) + "..." : s;
        };
        const pickLast = (msgs, role) => {
          for (let i = msgs.length - 1; i >= 0; i--) {
            const m = msgs[i];
            if (!m || m.streaming) continue;
            if (role && m.role !== role) continue;
            const t = String(m.content || "").trim();
            if (!t) continue;
            return m;
          }
          return null;
        };

        const cwdNow = resolvedCwd(config.toolRoot, threadId, activeThread.workdir);
        const coderMsgs = paneMessages("coder");
        const obsMsgs = observerConversationMessages();
        const lastCoderUser = pickLast(coderMsgs, "user");
        const lastCoderAsst = pickLast(coderMsgs, "assistant");
        const lastObsAsst = pickLast(obsMsgs, "assistant");
        const err = lastCoderErrorDigest(700);

        const tasks0 = Array.isArray(activeThread.tasks) ? activeThread.tasks : [];
        const tasks = tasks0
          .filter((t) => t && typeof t === "object" && String(t.status || "new") !== "done")
          .slice(0, 6);

        const parts = [];
        parts.push("[Runtime snapshot (read-only)]");
        parts.push(`thread: ${String(activeThread.title || "Untitled")} (id=${threadId})`);
        if (cwdNow) parts.push(`cwd: ${String(cwdNow)}`);
        parts.push(`coder: sending=${sendingCoder ? "yes" : "no"} mode=${String(config.mode || "")} model=${coderActiveModel()}`);
        if (lastCoderUser) parts.push("last_coder_user:\n" + cut(lastCoderUser.content, 500));
        if (lastCoderAsst) parts.push("last_coder_assistant:\n" + cut(lastCoderAsst.content, 900));
        if (err) parts.push("last_error_snippet:\n" + err);
        parts.push(
          `observer: sending=${sendingObserver ? "yes" : "no"} mode=${String(config.observerMode || "")} persona=${String(config.observerPersona || "")} intensity=${String(config.observerIntensity || "")} phase=${String(observerPhase || "")} loop_depth=${(loopInfo && loopInfo.depth) ? loopInfo.depth : 0}`
        );
        parts.push(`pending_approvals: edits=${pendingEditCount} commands=${pendingCommandCount}`);
        if (tasks.length) {
          parts.push(
            "tasks:\n- " +
            tasks
              .map((t) => {
                const tgt = String(t.target || "").toLowerCase() === "observer" ? "observer" : "coder";
                const st = String(t.status || "new");
                return `${tgt} (${st}): ${cut(String(t.title || ""), 90)}`;
              })
              .join("\n- ")
          );
        }
        if (lastObsAsst) parts.push("last_observer_assistant:\n" + cut(lastObsAsst.content, 700));
        parts.push("[/Runtime snapshot]");
        return parts.join("\n");
      } catch (_) {
        return "";
      }
    };

      const runCoderAgentic = async (text, threadId, asstMsgId, reqCfg, resolvedKey, history, ac, threadWorkdir, rootUserText) => {
      const autonomy = String((reqCfg && reqCfg.autonomy) || "longrun").trim().toLowerCase();
      const longrun = autonomy !== "off";
      const MAX_ITERS = (() => {
        const d = longrun ? 14 : 8;
        const n = numOrUndef(reqCfg && reqCfg.coderMaxIters);
        if (!n) return d;
        const clamped = Math.max(1, Math.min(64, Math.round(n)));
        return longrun ? clamped : Math.min(clamped, 16);
      })();
      const TRUNC_STDOUT = 2000;
      const TRUNC_STDERR = 800;
      const KEEP_TOOL_TURNS = longrun ? 6 : 3;
      let goalChecks = {
        repo: { attempts: 0, ok: false },
        tests: { attempts: 0, ok: false },
        build: { attempts: 0, ok: false },
      };
      const rootReadOnly = isRootReadOnlyObservationTask(rootUserText);
      const governorContract = await getGovernorContract();
      const taskContract = deriveTaskContract(rootUserText, rootReadOnly, governorContract);
      const instructionResolver = buildInstructionResolver(
        taskContract && taskContract.taskSummary,
        rootReadOnly,
        hasProjectRulesContext(history, governorContract),
      );

      const truncTool = (s, max) => {
        const t = String(s || "").trim();
        if (t.length <= max) return t;
        const lines = t.split("\n").length;
        return t.slice(0, max) + `\n[…truncated — ${lines} lines total, first ${max} chars shown]`;
      };
      const truncToolTail = (s, max) => {
        const t = String(s || "").trim();
        if (t.length <= max) return t;
        const lines = t.split("\n").length;
        return `[…truncated — ${lines} lines total, last ${max} chars shown]\n` + t.slice(-max);
      };

      const isPrunableToolSuccess = (content) => {
        const s = String(content || "").trimStart();
        if (s.startsWith("OK (exit_code: 0)")) return true;           // exec success
        if (/^OK: (wrote|patched) '/.test(s)) return true;            // write_file / patch_file
        if (s.startsWith("OK: applied ")) return true;               // apply_diff
        if (s.startsWith("OK write_file")) return true;              // GUI write_file wrapper
        if (/^\[.+\] \(\d+ lines?,/.test(s)) return true;             // read_file header
        if (s.startsWith("[search_files:")) return true;               // search_files header
        if (s.startsWith("[list_dir:")) return true;                 // list_dir header
        if (s.startsWith("[glob:") || s.startsWith("[glob]")) return true; // glob header
        return false;
      };

      const pruneToolMessages = (msgs) => {
        const toolIdxs = msgs.reduce((acc, m, i) => m.role === "tool" ? [...acc, i] : acc, []);
        if (toolIdxs.length <= KEEP_TOOL_TURNS) return;
        const toPrune = toolIdxs.slice(0, toolIdxs.length - KEEP_TOOL_TURNS);
        for (const idx of toPrune) {
          const content = String(msgs[idx].content || "");
          // Never prune failures: they are the most important recovery context.
          if (!isPrunableToolSuccess(content)) continue;
          const lines = content.split("\n");
          if (lines.length > 2) {
            msgs[idx] = { ...msgs[idx], content: lines[0] + ` [pruned ${lines.length}L]` };
          }
        }
      };

      // IMPORTANT: `exec` runs in a fresh process each time — `cd` does NOT persist across tool calls.
      // Track the agent's current working directory and pass it as `cwd` on every exec/write_file.
      // This prevents nested-git disasters and "why did it run in the repo root?" failures.
      const threadRoot = resolvedThreadRoot(config.toolRoot, threadId);
      const threadRootNorm = threadRoot ? normalizePathSep(String(threadRoot)).replace(/\/+$/g, "") : "";
      let curWorkdir = safeWorkdir(threadWorkdir);

      const cwdNow = () => resolvedCwd(config.toolRoot, threadId, curWorkdir);
      const cwdLabelNow = () => {
        const c = cwdNow();
        return c ? String(c) : "(workspace root)";
      };

      // Resolve a relative path under the current tool_root+workdir into a workspace-relative path.
      // This keeps file tools aligned with exec's cwd (prevents accidental repo-root edits).
      const joinUnderCwd = (relPath) => {
        const base0 = cwdNow();
        const base = base0 ? normalizePathSep(String(base0)).replace(/\/+$/g, "") : "";
        let rel = normalizePathSep(String(relPath || "")).replace(/^[\\/]+/g, "");
        rel = rel.replace(/^\.\//, "");
        if (!base) return rel;
        if (!rel || rel === ".") return base;
        const bl = base.toLowerCase();
        const rl = rel.toLowerCase();
        if (rl === bl || rl.startsWith(bl + "/")) return rel;
        return base + "/" + rel;
      };

      const rewriteToolPath = (text, fullPath, relPath) => {
        const s = String(text || "");
        const full = String(fullPath || "");
        const rel = String(relPath || "");
        if (!s || !full || !rel) return s;
        if (s.startsWith("[" + full + "]")) {
          return "[" + rel + "]" + s.slice(("[" + full + "]").length);
        }
        return s.replaceAll("'" + full + "'", "'" + rel + "'");
      };

      // Minimal read-cache for safe overwrite: the model must call read_file before overwriting an existing file.
      const fileReadSet = new Set(); // fullPath -> true

      const setWorkdir = (nextWorkdir) => {
        const safe = safeWorkdir(nextWorkdir);
        if (safe === curWorkdir) return;
        curWorkdir = safe;
        // Persist to thread state so subsequent manual runs start in the right directory.
        setThreadState((s) => ({
          ...s,
          threads: s.threads.map((t) => (
            t.id === threadId ? { ...t, updatedAt: Date.now(), workdir: safe } : t
          )),
        }));
      };

      const PWD_MARKER = "__SPIRAL_CODER_PWD__=";
      const wrapExecWithPwd = (cmd) => {
        const raw = String(cmd || "").trim();
        if (!raw) return raw;
        if (isWindowsHost()) {
          // Always emit a trailing marker so the UI can update curWorkdir even when `cd` is used.
          // Use try/finally so we still emit on most failures (helps stuck recovery).
          return [
            "$ErrorActionPreference = 'Stop'",
            "try {",
            raw,
            "} finally {",
            `Write-Output ("${PWD_MARKER}" + (Get-Location).Path)`,
            "}",
          ].join("\n");
        }
        // POSIX: keep behavior simple (do not `set -e`).
        return raw + `\necho "${PWD_MARKER}$(pwd)"`;
      };

      const stripPwdMarker = (stdoutRaw) => {
        const raw = String(stdoutRaw || "");
        if (!raw) return { stdout: "", pwd: "" };
        const lines = raw.replace(/\r\n/g, "\n").split("\n");
        let pwd = "";
        const kept = [];
        for (const l of lines) {
          if (l.startsWith(PWD_MARKER)) {
            pwd = l.slice(PWD_MARKER.length).trim();
            continue;
          }
          kept.push(l);
        }
        return { stdout: kept.join("\n").trimEnd(), pwd };
      };

      const maybeUpdateWorkdirFromPwd = (pwdAfter) => {
        const p0 = String(pwdAfter || "").trim();
        if (!p0 || !threadRootNorm) return;
        const p = normalizePathSep(p0).replace(/\/+$/g, "");
        const idx = p.toLowerCase().lastIndexOf(threadRootNorm.toLowerCase());
        if (idx === -1) return;
        const rel0 = p.slice(idx + threadRootNorm.length).replace(/^\/+/g, "");
        const safeRel = safeWorkdir(rel0);
        // If rel0 is non-empty but unsafe, ignore (avoid escaping tool_root).
        if (rel0 && !safeRel) return;
        setWorkdir(safeRel);
      };

      const sandboxBreachReason = (pwdAfter) => {
        // Detect if a command ended outside tool_root. This is a common cause of nested-git disasters.
        const p0 = String(pwdAfter || "").trim();
        if (!p0 || !threadRootNorm) return "";
        const p = normalizePathSep(p0).replace(/\/+$/g, "");
        const idx = p.toLowerCase().lastIndexOf(threadRootNorm.toLowerCase());
        if (idx === -1) return `cwd_after escaped tool_root: ${p0}`;
        return "";
      };

      // UI-side loop breaker: if the model repeats the exact same failing command,
      // block it and force a different strategy.
      const cmdStats = new Map(); // key -> { attempts, fails, lastErr, lastTs }
      const cmdKey = (cmd) => String(cmd || "").toLowerCase().replace(/\s+/g, " ").trim();

      // Failure memory (persisted per thread): prevents "forget and repeat" across runs.
      // Soft-load rule: a past failure counts as 1 fail (so we don't hard-block immediately).
      const MEM_TTL_MS = 12 * 60 * 60 * 1000;
      try {
        const t0 = (threadState && Array.isArray(threadState.threads))
          ? threadState.threads.find((t) => t && t.id === threadId)
          : null;
        const mem = t0 && t0.coderMem && typeof t0.coderMem === "object" ? t0.coderMem : null;
        const rows = mem && Array.isArray(mem.cmdStats) ? mem.cmdStats : [];
        const nowMs = Date.now();
        for (const r of rows) {
          if (!r || typeof r !== "object") continue;
          const k = String(r.k || "").trim();
          if (!k) continue;
          const lastTs = typeof r.lastTs === "number" ? r.lastTs : Number(r.lastTs) || 0;
          if (lastTs && nowMs - lastTs > MEM_TTL_MS) continue;
          const fails0 = typeof r.fails === "number" ? r.fails : Number(r.fails) || 0;
          const fails = Math.max(0, Math.min(1, Math.round(fails0)));
          if (fails <= 0) continue;
          const attempts0 = typeof r.attempts === "number" ? r.attempts : Number(r.attempts) || 0;
          const st = {
            attempts: Math.max(0, Math.round(attempts0)),
            fails,
            lastErr: typeof r.lastErr === "string" ? String(r.lastErr || "").slice(0, 220) : "",
            lastTs: lastTs || 0,
          };
          cmdStats.set(k, st);
        }
      } catch (_) {}

      // Failure signatures (noise-resistant): collapse digits, lowercase, collapse whitespace.
      // This improves loop detection vs messages that differ only by line numbers / timestamps.
      const normalizeForSig = (s) => {
        const t = String(s || "").trim();
        if (!t) return "";
        let out = "";
        for (let i = 0; i < t.length && out.length < 160; i++) {
          const ch = t[i];
          const code = ch.charCodeAt(0);
          if (code >= 48 && code <= 57) out += "#";
          else out += ch.toLowerCase();
        }
        return out.replace(/\s+/g, " ").trim();
      };

      // Coarse error classification (ported from Rust TUI) so the Coder can choose a better recovery strategy.
      // This is intentionally heuristic: it trades a few false positives for better "what to do next" guidance.
      const classifyErrorClass = (stderr, stdout) => {
        const low = (String(stderr || "") + "\n" + String(stdout || "")).toLowerCase();
        if (!low.trim()) return "unknown";

        if (
          low.includes("command not found") ||
          low.includes("is not recognized as the name") ||
          low.includes("is not recognized as an internal") ||
          low.includes("permission denied") ||
          low.includes("access is denied") ||
          low.includes("access denied") ||
          low.includes("commandnotfoundexception") ||
          low.includes("win32 error 5")
        ) return "environment";

        if (
          low.includes("syntax error") ||
          low.includes("unexpected token") ||
          low.includes("parse error") ||
          low.includes("parsererror") ||
          low.includes("invalid syntax") ||
          low.includes("missing expression") ||
          low.includes("unexpected end of")
        ) return "syntax";

        if (
          low.includes("no such file") ||
          low.includes("cannot find path") ||
          low.includes("path not found") ||
          (low.includes("does not exist") && !low.includes("package")) ||
          low.includes("could not find a part of the path")
        ) return "path";

        if (
          low.includes("modulenotfounderror") ||
          low.includes("cannot find module") ||
          low.includes("no module named") ||
          low.includes("package not found") ||
          low.includes("no such package") ||
          (low.includes("could not find") && (low.includes("package") || low.includes("crate")))
        ) return "dependency";

        if (
          low.includes("connection refused") ||
          low.includes("timed out") ||
          low.includes("network unreachable") ||
          low.includes("could not connect") ||
          low.includes("name resolution failed") ||
          low.includes("failed to connect")
        ) return "network";

        if (
          low.includes("assertion") ||
          low.includes("test failed") ||
          (low.includes("expected") && low.includes("actual"))
        ) return "logic";

        return "unknown";
      };
      const errorClassHint = (cls) => {
        if (!cls || cls === "unknown") return "";
        if (cls === "environment") return "⚠ ENVIRONMENT ERROR: a binary/permission is missing. Fix the environment first — do NOT modify source code.";
        if (cls === "syntax") return "⚠ SYNTAX ERROR: fix the exact parser error line — do NOT change unrelated code.";
        if (cls === "path") return "⚠ PATH ERROR: wrong cwd or missing file/dir. Verify `pwd` + `ls`, then correct the path.";
        if (cls === "dependency") return "⚠ DEPENDENCY ERROR: install missing packages/crates, then retry. Do NOT refactor code to 'work around' missing deps.";
        if (cls === "network") return "⚠ NETWORK ERROR: fix connectivity/auth/proxy first. Do NOT keep retrying the same request.";
        if (cls === "logic") return "⚠ LOGIC/TEST ERROR: the program ran but behavior is wrong. Read the failure, reproduce, then implement the minimal fix + test.";
        return "";
      };

      const extractErrorDigest = (stdout, stderr) => {
        const keys = [
          "error",
          "fatal",
          "exception",
          "traceback",
          "parsererror",
          "unexpected token",
          "not recognized",
          "commandnotfoundexception",
          "missing expression",
          "unable to",
          "could not",
          "access is denied",
          "permission denied",
          "does not have a commit checked out",
          "unable to index file",
          "adding embedded git repository",
          "unauthorized",
          "invalid api key",
          "too many requests",
          "rate limit",
          "unsupported parameter",
          "not a chat model",
        ];
        const seen = new Set();
        const out = [];
        const scan = (src) => {
          const lines = String(src || "").replace(/\r\n/g, "\n").split("\n");
          for (const ln0 of lines) {
            const t = String(ln0 || "").trim();
            if (!t) continue;
            const low = t.toLowerCase();
            if (!keys.some((k) => low.includes(k))) continue;
            const n = normalizeForSig(t).slice(0, 220);
            if (!n || seen.has(n)) continue;
            seen.add(n);
            out.push(t.slice(0, 320));
            if (out.length >= 8) break;
          }
        };
        scan(stderr);
        if (out.length < 8) scan(stdout);
        if (!out.length) return "";
        return "ERROR_DIGEST:\n- " + out.join("\n- ");
      };

      const commandSig = (command) => {
        const raw = String(command || "").replace(/\r\n/g, "\n");
        const first = raw.split("\n").find((l) => String(l || "").trim()) || "";
        return normalizeForSig(first.split(/\s+/).join(" "));
      };

      const pickInterestingErrorLine = (stdout, stderr) => {
        const keywords = [
          "error",
          "fatal",
          "exception",
          "traceback",
          "parsererror",
          "unexpected token",
          "not recognized",
          "commandnotfoundexception",
          "missing expression",
          "unable to",
          "could not",
          "access is denied",
          "permission denied",
        ];

        const scan = (src) => {
          const raw = String(src || "").replace(/\r\n/g, "\n");
          for (const ln of raw.split("\n")) {
            const t = String(ln || "").trim();
            if (!t) continue;
            const low = t.toLowerCase();
            if (keywords.some((k) => low.includes(k))) {
              return normalizeForSig(t);
            }
          }
          return "";
        };

        return scan(stderr) || scan(stdout) || "";
      };

      const errorSignature = (command, stdout, stderr, exitCode) => {
        const cmd = commandSig(command);
        const err = pickInterestingErrorLine(stdout, stderr);
        const out = `exit=${exitCode}|cmd=${cmd}|err=${err}`;
        return out.length > 220 ? out.slice(0, 220) : out;
      };

      const errorLineSig = (stdout, stderr) => {
        const p = pickInterestingErrorLine(stdout, stderr);
        if (p) return p;
        const s = String(stderr || "") || String(stdout || "");
        const first = (s.replace(/\r\n/g, "\n").split("\n")[0] || "").trim();
        return normalizeForSig(first).slice(0, 180);
      };
      const suspiciousSuccessReason = (stdout, stderr) => {
        // PowerShell can exit 0 even when it printed errors (non-terminating error records).
        // Cargo warnings also go to stderr; only trigger on strong error markers (and a few known critical git warnings).
        const s = (String(stderr || "") + "\n" + String(stdout || "")).toLowerCase();
        if (!s.trim()) return "";
        const strong = [
          "parsererror",
          "unexpected token",
          "missing expression",
          "commandnotfoundexception",
          "not recognized",
          "error:",
          "fatal:",
          "exception",
          "traceback",
          "access is denied",
          "permission denied",
          "does not have a commit checked out",
          "unable to index file",
          "could not find a part of the path",
        ];
        const embedded = s.includes("adding embedded git repository") || s.includes("embedded git repository");
        if (!embedded && !strong.some((k) => s.includes(k))) return "";
        const line = pickInterestingErrorLine(stdout, stderr);
        if (!line) return "exit_code was 0, but output contained error markers";
        return `exit_code was 0, but output contained error markers (e.g. \`${line}\`)`;
      };
      const blockedByRepeatFailure = (k) => {
        const st = cmdStats.get(k);
        if (!st) return "";
        if ((st.fails || 0) >= 2) {
          const why = st.lastErr ? ` last_error: ${st.lastErr}` : "";
          return `repeated failure (${st.fails}x).${why}`;
        }
        return "";
      };
      const noteCmd = (k, failed, sig) => {
        const st = cmdStats.get(k) || { attempts: 0, fails: 0, lastErr: "", lastTs: 0 };
        st.attempts++;
        if (failed) {
          st.fails++;
          if (sig) st.lastErr = sig;
          st.lastTs = Date.now();
        } else {
          st.fails = 0;
          st.lastErr = "";
          st.lastTs = Date.now();
        }
        cmdStats.set(k, st);
        return st;
      };

      const persistFailureMemory = () => {
        try {
          const rows = [];
          for (const [k, st] of cmdStats.entries()) {
            if (!k || !st) continue;
            const fails = Number(st.fails) || 0;
            if (fails <= 0) continue;
            rows.push({
              k: String(k).slice(0, 220),
              attempts: Number(st.attempts) || 0,
              fails: fails,
              lastErr: typeof st.lastErr === "string" ? String(st.lastErr || "").slice(0, 220) : "",
              lastTs: typeof st.lastTs === "number" ? st.lastTs : Date.now(),
            });
          }
          rows.sort((a, b) => (Number(b.lastTs) || 0) - (Number(a.lastTs) || 0));
          const trimmed = rows.slice(0, 60);
          setThreadState((s) => ({
            ...s,
            threads: s.threads.map((t) => (
              t.id === threadId
                ? { ...t, updatedAt: Date.now(), coderMem: { cmdStats: trimmed } }
                : t
            )),
          }));
        } catch (_) {}
      };

      const fnv1a64 = (s) => {
        // FNV-1a 64-bit (BigInt) hash. Cheap and stable for stuck detection.
        let h = 0xcbf29ce484222325n;
        const prime = 0x100000001b3n;
        const str = String(s || "");
        for (let i = 0; i < str.length; i++) {
          h ^= BigInt(str.charCodeAt(i) & 0xff);
          h = (h * prime) & 0xffffffffffffffffn;
        }
        return h;
      };

      const governor = {
        consecutiveFailures: 0,

        lastCmdSig: "",
        sameCmdRepeats: 0,

        lastErrSig: "",
        sameErrRepeats: 0,

        lastOutHash: 0n,
        sameOutRepeats: 0,

        pendingHint: "",
        pendingDiag: "",
      };

      // Lightweight state machine: the system prompt sees the current state so the model can route its behavior.
      // This is intentionally small (planning/executing/verifying/recovery/done) to keep it robust across models.
      let agentState = "planning";
      let activePlan = null;
      const knownGoodVerificationCommands = restoreKnownVerificationCommandsFromMessages(history, governorContract);
      let fileToolConsecutiveFailures = 0;
      let reflectionRequired = "";
      let impactRequired = "";
      const rememberKnownVerificationCommand = (command) => {
        const value = String(command || "").trim().replace(/\s+/g, " ");
        const sig = normalizeScratchEntry(value);
        if (!sig) return;
        const idx = knownGoodVerificationCommands.findIndex((item) => normalizeScratchEntry(item) === sig);
        if (idx >= 0) knownGoodVerificationCommands.splice(idx, 1);
        knownGoodVerificationCommands.push(value);
        while (knownGoodVerificationCommands.length > 6) knownGoodVerificationCommands.shift();
      };
      const requireReflection = (reason) => {
        reflectionRequired = String(reason || "failure or stall detected").trim();
      };
      const requireImpact = (reason) => {
        impactRequired = String(reason || "successful mutation requires impact check").trim();
      };
      let assumptionLedger = rebuildAssumptionLedgerFromMessages(
        history,
        governorContract,
        taskContract,
        instructionResolver,
        knownGoodVerificationCommands,
      );

      // Short-term memory: keep a compact summary of recent tool actions so the model can avoid repeats.
      const recentRuns = [];
      const pushRecentRun = (rec) => {
        try {
          const r = rec && typeof rec === "object" ? rec : { note: String(rec || "") };
          recentRuns.push({ ts: Date.now(), ...r });
          while (recentRuns.length > 8) recentRuns.shift();
        } catch (_) {}
      };
      const firstDigestLine = (digest) => {
        const s = String(digest || "");
        const m = s.match(/^\s*-\s+(.+)$/m);
        return m && m[1] ? String(m[1]).trim() : "";
      };
      const clip = (s, n) => {
        const t = String(s || "").trim();
        if (t.length <= n) return t;
        return t.slice(0, Math.max(0, n - 3)) + "...";
      };
      const formatRecentRuns = () => {
        try {
          const last = recentRuns.slice(-6);
          if (!last.length) return "";
          const lines = [];
          for (const r of last) {
            const kind = String(r.kind || "").trim();
            if (kind === "exec") {
              const exitCode = Number(r.exit);
              const cls = String(r.cls || "").trim();
              const cmd = clip(String(r.cmd || "").trim(), 80);
              const err = clip(String(r.err || "").trim(), 90);
              const parts = [];
              parts.push(`exit=${Number.isFinite(exitCode) ? exitCode : String(r.exit)}`);
              if (cls) parts.push(`cls=${cls}`);
              if (cmd) parts.push(`cmd=${cmd}`);
              if (err) parts.push(`err=${err}`);
              lines.push("- exec " + parts.join(" "));
              continue;
            }
            if (kind === "write_file" || kind === "patch_file" || kind === "apply_diff") {
              const status = String(r.status || "").trim() || (r.ok === false ? "FAIL" : "OK");
              const path0 = clip(String(r.path || "").trim(), 90);
              const note = clip(String(r.note || "").trim(), 90);
              lines.push(`- ${kind} ${status} ${path0}${note ? (" " + note) : ""}`.trim());
              continue;
            }
            if (kind) {
              lines.push(`- ${kind} ${clip(String(r.note || "").trim(), 120)}`.trim());
            }
          }
          const out = lines.filter(Boolean);
          if (!out.length) return "";
          return ["[Recent runs]", ...out].join("\n");
        } catch (_) {
          return "";
        }
      };

      const stableHashHex = (text) => fnv1a64(String(text || "").trimEnd()).toString(16).padStart(16, "0");
      const promptCache = {
        project: "",
        resolver: "",
        task: "",
        assumption: "",
        verify: "",
        acceptance: "",
        knownVerify: "",
        recentRuns: "",
      };
      const renderCachedPrompt = (key, full, compact) => {
        const digest = stableHashHex(full);
        const unchanged = promptCache[key] === digest;
        promptCache[key] = digest;
        return unchanged ? compact : full;
      };
      const buildProjectContextCompactPrompt = () => {
        if (!projectScan || !String(projectScan.context_text || "").trim()) return "";
        return [
          "[Project Context cache]",
          `hash: ${stableHashHex(projectScan.context_text)}`,
          `- stack: ${String(projectScan.stack_label || (Array.isArray(projectScan.stack) ? projectScan.stack.join(", ") : "unknown"))}`,
          `- git: branch=${String(projectScan.git_branch || "-")} modified=${Number(projectScan.git_modified) || 0} untracked=${Number(projectScan.git_untracked) || 0}`,
        ].join("\n");
      };
      const buildInstructionResolverCompactPrompt = () => {
        const verificationFloor = inferTaskVerificationFloor(instructionResolver && instructionResolver.taskSummary, activePlan, governorContract);
        const full = buildInstructionResolverPrompt(instructionResolver, activePlan, governorContract);
        return [
          "[Instruction Resolver cache]",
          `hash: ${stableHashHex(full)}`,
          "- order: root > system > project > user > execution",
          `- task: ${clip(String(instructionResolver && instructionResolver.taskSummary || "complete the requested task"), 120)}`,
          `- read_only: ${instructionResolver && instructionResolver.rootReadOnly ? "yes" : "no"}`,
          `- project_rules: ${instructionResolver && instructionResolver.projectRulesActive ? "yes" : "no"}`,
          `- verification_floor: ${verificationFloor}`,
        ].join("\n");
      };
      const buildTaskContractCompactPrompt = () => {
        const verificationFloor = inferTaskVerificationFloor(taskContract && taskContract.taskSummary, activePlan, governorContract);
        const full = buildTaskContractPrompt(taskContract, activePlan, governorContract);
        const firstConstraint = Array.isArray(taskContract && taskContract.hardConstraints) && taskContract.hardConstraints.length
          ? clip(String(taskContract.hardConstraints[0] || ""), 120)
          : "";
        const lines = [
          "[Task Contract cache]",
          `hash: ${stableHashHex(full)}`,
          `- task: ${clip(String(taskContract && taskContract.taskSummary || "complete the requested task"), 120)}`,
          `- hard_constraints: ${Array.isArray(taskContract && taskContract.hardConstraints) ? taskContract.hardConstraints.length : 0} non_goals: ${Array.isArray(taskContract && taskContract.nonGoals) ? taskContract.nonGoals.length : 0} output_shape: ${Array.isArray(taskContract && taskContract.outputShape) ? taskContract.outputShape.length : 0}`,
          `- verification_floor: ${verificationFloor}`,
        ];
        if (firstConstraint) lines.push(`- key_constraint: ${firstConstraint}`);
        if (activePlan && Array.isArray(activePlan.acceptanceCriteria)) {
          lines.push(`- acceptance_items: ${activePlan.acceptanceCriteria.length}`);
        }
        return lines.join("\n");
      };
      const buildAssumptionLedgerCompactPrompt = () => {
        const full = buildAssumptionLedgerPrompt(assumptionLedger);
        if (!full) return "";
        const entries = Array.isArray(assumptionLedger && assumptionLedger.entries) ? assumptionLedger.entries : [];
        const open = entries.filter((entry) => entry.status === "unknown").length;
        const confirmed = entries.filter((entry) => entry.status === "confirmed").length;
        const refuted = entries.filter((entry) => entry.status === "refuted");
        const lines = [
          "[Assumption Ledger cache]",
          `hash: ${stableHashHex(full)}`,
          `- open: ${open} confirmed: ${confirmed} refuted: ${refuted.length}`,
        ];
        refuted.slice(-2).forEach((entry) => lines.push(`- refuted: ${clip(String(entry.text || ""), 120)}`));
        return lines.join("\n");
      };
      const buildAcceptanceCompactPrompt = () => {
        if (!activePlan || !Array.isArray(activePlan.acceptanceCriteria) || !activePlan.acceptanceCriteria.length) return "";
        const full = ["[Current acceptance criteria]", ...activePlan.acceptanceCriteria.map((criterion, idx) => `- acceptance ${idx + 1}: ${criterion}`)].join("\n");
        const lines = [
          "[Current acceptance cache]",
          `hash: ${stableHashHex(full)}`,
          `- items: ${activePlan.acceptanceCriteria.length}`,
        ];
        activePlan.acceptanceCriteria.slice(0, 2).forEach((criterion, idx) => {
          lines.push(`- acceptance ${idx + 1}: ${clip(String(criterion || ""), 120)}`);
        });
        return lines.join("\n");
      };
      const buildKnownVerificationCompactPrompt = () => {
        if (!knownGoodVerificationCommands.length) return "";
        const full = ["[Known-good verification commands]", ...knownGoodVerificationCommands.map((cmd) => `- ${cmd}`)].join("\n");
        return [
          "[Known-good verification cache]",
          `hash: ${stableHashHex(full)}`,
          `- count: ${knownGoodVerificationCommands.length}`,
          ...knownGoodVerificationCommands.slice(-2).map((cmd) => `- ${clip(String(cmd || ""), 140)}`),
        ].join("\n");
      };
      const buildRecentRunsCompactPrompt = () => {
        const full = formatRecentRuns();
        if (!full) return "";
        const lines = full.split("\n").slice(1).filter(Boolean);
        return [
          "[Recent runs cache]",
          `hash: ${stableHashHex(full)}`,
          `- count: ${lines.length}`,
          ...lines.slice(-2),
        ].join("\n");
      };
      const compactSuccessToolResultForHistory = (toolName, content) => {
        const name = String(toolName || "").trim();
        const text = String(content || "");
        if (name === "read_file") return text;
        const lines = text.split("\n");
        if (text.length <= 1200 && lines.length <= 10) return text;
        const kept = [];
        const seen = new Set();
        const pushLine = (line) => {
          const trimmed = String(line || "").trim();
          if (!trimmed) return;
          const compact = clip(trimmed, 220);
          if (seen.has(compact)) return;
          seen.add(compact);
          kept.push(compact);
        };
        pushLine(lines[0] || "");
        if (name === "exec") {
          lines.slice(1, 4).forEach(pushLine);
        }
        const markers = [
          "[auto-test]",
          "[hash]",
          "PASSED (exit 0)",
          "FAILED (exit ",
          "✓ auto-verify",
          "✗ auto-verify",
          "test result:",
          "Finished ",
          "running ",
          "cwd:",
          "cwd_after:",
        ];
        for (const line of lines) {
          const trimmed = String(line || "").trim();
          if (markers.some((marker) => trimmed.includes(marker))) pushLine(trimmed);
          if (kept.length >= 10) break;
        }
        const fillFrom = /^(search_files|list_dir|glob|write_file|patch_file|apply_diff)$/.test(name) ? 1 : 2;
        for (const line of lines.slice(fillFrom)) {
          pushLine(line);
          if (kept.length >= 10) break;
        }
        if (kept.length < lines.filter((line) => String(line || "").trim()).length) {
          kept.splice(Math.min(1, kept.length), 0, `[history digest — kept ${kept.length}/${lines.length} lines, ${text.length} chars]`);
        }
        return kept.join("\n");
      };
      const rememberObservationRead = (command, path) => {
        const cmd = String(command || "").trim();
        const target = String(path || "").trim();
        if (!cmd || !target) return;
        const sig = `${normalizeScratchEntry(cmd)}|${normalizeScratchEntry(target)}`;
        const reads = Array.isArray(observationEvidence.reads) ? observationEvidence.reads : [];
        const idx = reads.findIndex((item) => `${normalizeScratchEntry(item.command)}|${normalizeScratchEntry(item.path)}` === sig);
        if (idx >= 0) reads.splice(idx, 1);
        reads.push({ command: cmd, path: target });
        while (reads.length > 8) reads.shift();
        observationEvidence.reads = reads;
        if (typeof persistObservationEvidence === "function") persistObservationEvidence();
      };
      const rememberObservationSearch = (command, pattern, hitCount, paths) => {
        const cmd = String(command || "").trim();
        const patt = String(pattern || "").trim();
        if (!cmd || !patt) return;
        const pathList = Array.isArray(paths) ? paths.map((item) => String(item || "").trim()).filter(Boolean).slice(0, 8) : [];
        const sig = `${normalizeScratchEntry(cmd)}|${normalizeScratchEntry(patt)}`;
        const searches = Array.isArray(observationEvidence.searches) ? observationEvidence.searches : [];
        const idx = searches.findIndex((item) => `${normalizeScratchEntry(item.command)}|${normalizeScratchEntry(item.pattern)}` === sig);
        if (idx >= 0) searches.splice(idx, 1);
        searches.push({ command: cmd, pattern: patt, hitCount: Number(hitCount) || 0, paths: pathList });
        while (searches.length > 8) searches.shift();
        observationEvidence.searches = searches;
        if (typeof persistObservationEvidence === "function") persistObservationEvidence();
      };
      const assistantMessageCompactable = (msg) => {
        if (!msg || msg.role !== "assistant") return false;
        const content = String(msg.content || "").trim();
        if (!content) return false;
        if (content.startsWith("[DONE]") || content.includes("[error]") || content.includes("GOVERNOR BLOCKED")) return false;
        return (Array.isArray(msg.tool_calls) && msg.tool_calls.length)
          || !!parsePlanBlock(content, governorContract)
          || !!parseThinkBlock(content, governorContract)
          || !!parseReflectionBlock(content, governorContract)
          || !!parseImpactBlock(content, governorContract)
          || !!parseEvidenceBlock(content, governorContract);
      };
      const summarizeAssistantMessage = (msg) => {
        const content = String(msg && msg.content || "").trim();
        if (!content) return "";
        const parts = [];
        const plan = parsePlanBlock(content, governorContract);
        if (plan) parts.push(`plan goal=${clip(String(plan.goal || ""), 90)} steps=${Array.isArray(plan.steps) ? plan.steps.length : 0} acceptance=${Array.isArray(plan.acceptanceCriteria) ? plan.acceptanceCriteria.length : 0}`);
        const think = parseThinkBlock(content, governorContract);
        if (think) parts.push(`think step=${Number(think.step) || 0} tool=${clip(String(think.tool || ""), 24)} next=${clip(String(think.next || ""), 90)}`);
        const reflect = parseReflectionBlock(content, governorContract);
        if (reflect) parts.push(`reflect delta=${reflect.goalDelta} strategy=${reflect.strategyChange} next=${clip(String(reflect.nextMinimalAction || ""), 90)}`);
        const impact = parseImpactBlock(content, governorContract);
        if (impact) parts.push(`impact progress=${clip(String(impact.progress || ""), 90)} gap=${clip(String(impact.remainingGap || ""), 90)}`);
        const evidence = parseEvidenceBlock(content, governorContract);
        if (evidence) parts.push(`evidence files=${Array.isArray(evidence.targetFiles) ? evidence.targetFiles.length : 0} next_probe=${clip(String(evidence.nextProbe || ""), 90)}`);
        if (Array.isArray(msg && msg.tool_calls) && msg.tool_calls.length) {
          const names = msg.tool_calls.map((tc) => String(tc && tc.function && tc.function.name || "").trim()).filter(Boolean);
          if (names.length) parts.push(`tools=${names.join(",")}`);
        }
        if (!parts.length) parts.push(clip(content, 140));
        return `[assistant-summary] ${parts.join(" | ")} [compacted]`;
      };
      const pruneAssistantMessages = (msgs) => {
        const KEEP_ASSISTANT_TURNS = longrun ? 6 : 4;
        const assistantIdxs = msgs.reduce((acc, msg, idx) => msg && msg.role === "assistant" ? [...acc, idx] : acc, []);
        if (assistantIdxs.length <= KEEP_ASSISTANT_TURNS) return;
        const toPrune = assistantIdxs.slice(0, assistantIdxs.length - KEEP_ASSISTANT_TURNS);
        toPrune.forEach((idx) => {
          if (!assistantMessageCompactable(msgs[idx])) return;
          const summary = summarizeAssistantMessage(msgs[idx]);
          if (!summary) return;
          msgs[idx] = { ...msgs[idx], content: summary };
        });
      };
      const assistantMessageHasObservationToolCall = (msg) => {
        const calls = Array.isArray(msg && msg.tool_calls) ? msg.tool_calls : [];
        return calls.some((tc) => {
          const name = String(tc && tc.function && tc.function.name || "").trim();
          return /^(read_file|search_files|list_dir|glob)$/.test(name);
        });
      };
      const toolMessageDropSafe = (msg) => {
        const content = String(msg && msg.content || "").trimStart();
        return /^OK \(exit_code: 0\)/.test(content)
          || /^OK: wrote '/.test(content)
          || /^OK: patched '/.test(content)
          || /^OK: applied /.test(content)
          || /^OK write_file/.test(content);
      };
      const pruneMessageWindow = (msgs) => {
        const MAX_CONTEXT_MESSAGES = longrun ? 48 : 32;
        const KEEP_RECENT_MESSAGE_WINDOW = longrun ? 24 : 16;
        if (msgs.length <= MAX_CONTEXT_MESSAGES) return;
        const protectedIdx = new Set();
        msgs.forEach((msg, idx) => {
          const role = String(msg && msg.role || "");
          if (role !== "assistant" && role !== "tool") protectedIdx.add(idx);
        });
        for (let idx = Math.max(0, msgs.length - KEEP_RECENT_MESSAGE_WINDOW); idx < msgs.length; idx++) {
          protectedIdx.add(idx);
        }
        const anchorChecks = [
          (content) => !!parsePlanBlock(content, governorContract),
          (content) => !!parseThinkBlock(content, governorContract),
          (content) => !!parseReflectionBlock(content, governorContract),
          (content) => !!parseImpactBlock(content, governorContract),
          (content) => !!parseEvidenceBlock(content, governorContract),
        ];
        anchorChecks.forEach((check) => {
          for (let idx = msgs.length - 1; idx >= 0; idx--) {
            const msg = msgs[idx];
            if (!msg || msg.role !== "assistant") continue;
            const content = String(msg.content || "").trim();
            if (!content || !check(content)) continue;
            protectedIdx.add(idx);
            break;
          }
        });
        const removable = [];
        msgs.forEach((msg, idx) => {
          if (protectedIdx.has(idx)) return;
          const role = String(msg && msg.role || "");
          if (role === "assistant") {
            if (!assistantMessageHasObservationToolCall(msg) && assistantMessageCompactable(msg)) removable.push(idx);
            return;
          }
          if (role === "tool" && toolMessageDropSafe(msg)) removable.push(idx);
        });
        const over = msgs.length - MAX_CONTEXT_MESSAGES;
        if (over <= 0 || !removable.length) return;
        const dropIdx = new Set(removable.slice(0, over));
        const next = [];
        msgs.forEach((msg, idx) => {
          if (!dropIdx.has(idx)) next.push(msg);
        });
        msgs.splice(0, msgs.length, ...next);
      };

      const deriveGovernorHint = (stderr, stdout) => {
        const sErr = String(stderr || "");
        const sOut = String(stdout || "");
        const s = (sErr || sOut || "").toLowerCase();
        if (!s) return "";

        // Poison proxy: github push/connect fails via 127.0.0.1:9
        if (s.includes("port 443 via 127.0.0.1") || s.includes("127.0.0.1:9")) {
          return [
            "Detected a poisoned proxy env (127.0.0.1:9).",
            "Fix (PowerShell):",
            "$env:HTTP_PROXY=''; $env:HTTPS_PROXY=''; $env:ALL_PROXY=''; $env:GIT_HTTP_PROXY=''; $env:GIT_HTTPS_PROXY=''",
            "Then retry. For GitHub push, prefer: .\\scripts\\push_ssh.ps1 (SSH over 443).",
            "Alternative: .\\scripts\\push.ps1 (with $env:GITHUB_TOKEN).",
          ].join("\n");
        }

        // WDAC-ish: msys tools like head.exe fail with Win32 error 5.
        if (s.includes("win32 error 5") && s.includes("head.exe")) {
          return [
            "This environment blocks some MSYS/Unix tools (Win32 error 5).",
            "Avoid `head`, `sed`, `nl`, pipes into MSYS tools. Use PowerShell equivalents:",
            "Get-Content file | Select-Object -First 40",
          ].join("\n");
        }

        // WDAC-ish: msys sh.exe sometimes fails with Win32 error 5 (signal pipe).
        if (s.includes("win32 error 5") && (s.includes("sh.exe") || s.includes("couldn't create signal pipe"))) {
          return [
            "This environment blocks MSYS sh.exe (Win32 error 5).",
            "Fix: avoid invoking MSYS tools. Prefer pure PowerShell commands.",
            "For GitHub push: use .\\scripts\\push_ssh.ps1 (SSH over 443) and ensure proxy env vars are cleared.",
          ].join("\n");
        }

        // Cargo exe lock (binary is running).
        if (s.includes("failed to remove file") && s.includes("spiral-coder.exe") && (s.includes("access is denied") || s.includes("アクセスが拒否"))) {
          return [
            "spiral-coder.exe is locked (running). Stop the process before rebuilding.",
            "Fix: .\\scripts\\kill-spiral-coder.ps1 ; then re-run build (or use .\\scripts\\run-tui.ps1 / run-ui.ps1).",
          ].join("\n");
        }

        // Embedded repo hint (also covered elsewhere).
        if (s.includes("embedded git repository") || s.includes("does not have a commit checked out")) {
          return [
            "You are mixing repos (nested git repo).",
            "Fix: operate inside the project directory only, or move it under tool_root (.tmp/<threadId>).",
            "Do NOT run `git add .` from the Spiral-Coder repo root.",
          ].join("\n");
        }

        return "";
      };

      const sharedToolNames = toolNamesCsv(governorContract);
      const WANTS_REPO_GOAL = textMatchesVerificationTerms(text, verificationTerms(governorContract, "goal_repo_terms"));
      const WANTS_TEST_GOAL = textMatchesVerificationTerms(text, verificationTerms(governorContract, "goal_test_terms"));
      const WANTS_BUILD_GOAL = textMatchesVerificationTerms(text, verificationTerms(governorContract, "goal_build_terms"));
      const isWindows = isWindowsHost();
      const SYSTEM_BASE = isWindows ? [
        "You are an autonomous coding agent with DIRECT access to the user's Windows machine.",
        `Working directory (tool_root): ${cwdLabelNow()}. Always create new projects under this directory. Do NOT cd to parent directories.`,
        "CRITICAL RULES — follow these without exception:",
        "0. NEVER create a git repo inside another git repo. If you see 'embedded git repository' warnings, STOP and relocate to a clean directory under tool_root.",
        `1. ALWAYS use tools to act. Available tools: ${sharedToolNames}. NEVER just show code.`,
        "   PRIORITY: 1) read_file  2) list_dir  3) search_files/glob  4) patch_file/apply_diff  5) write_file  6) exec  7) done",
        "   Use read_file before editing. Use patch_file/apply_diff for edits. Use list_dir/search_files/glob to discover structure quickly.",
        `   ${contractMessage(governorContract, "instruction_resolver_scratchpad_rule")}`,
        "   For existing-file mutation, emit <evidence> first; if evidence is weak, inspect instead of mutating.",
        "   Fallback (if tool calls are not supported): output ONE ```powershell``` code block containing ONLY commands (no `$ ` or `PS>` prompts).",
        "2. Use PowerShell syntax ONLY (cmd.exe is NOT used):",
        "   - Create directory tree: New-Item -ItemType Directory -Force -Path 'a/b/c'",
        "   - Create file with content: Set-Content -Path 'file.txt' -Value 'line1`nline2' -Encoding UTF8",
        "   - Multi-line file: @('line1','line2') | Set-Content -Path 'file.txt' -Encoding UTF8",
        "   - Append to file: Add-Content -Path 'file.txt' -Value 'more' -Encoding UTF8",
        "   - Git (new repo): New-Item -ItemType Directory -Force -Path 'MyRepo'; cd 'MyRepo'; git init; git add .; git commit -m 'init'",
        "   - NEVER use mkdir -p, touch, cat >, or any Unix syntax.",
        "3. Execute ALL steps immediately via tools. Do NOT ask for permission or confirmation.",
        "4. After each exec call, read the output and continue until the task is 100% complete.",
        "5. When the task is complete, call done with summary + acceptance coverage + verification evidence.",
      ].join("\n") : [
        "You are an autonomous coding agent with DIRECT access to the user's local machine.",
        `Working directory (tool_root): ${cwdLabelNow()}. Always create new projects under this directory. Do NOT cd to parent directories.`,
        "CRITICAL RULES — follow these without exception:",
        "0. NEVER create a git repo inside another git repo. If you see 'embedded git repository' warnings, STOP and relocate to a clean directory under tool_root.",
        `1. ALWAYS use tools to act. Available tools: ${sharedToolNames}. NEVER just show code.`,
        "   PRIORITY: 1) read_file  2) list_dir  3) search_files/glob  4) patch_file/apply_diff  5) write_file  6) exec  7) done",
        "   Use read_file before editing. Use patch_file/apply_diff for edits. Use list_dir/search_files/glob to discover structure quickly.",
        `   ${contractMessage(governorContract, "instruction_resolver_scratchpad_rule")}`,
        "   For existing-file mutation, emit <evidence> first; if evidence is weak, inspect instead of mutating.",
        "   Fallback (if tool calls are not supported): output ONE ```bash``` code block containing ONLY commands (no `$ ` prompts).",
        "2. Use Unix shell commands:",
        "   - Create directory: mkdir -p path/to/dir",
        "   - Write file: printf '%s' 'content' > file.txt   OR   python3 -c \"open('f','w').write('...')\"",
        "   - Multi-line file: use a heredoc via python3 or printf with \\n",
        "   - Git: git init, git add ., git commit -m 'init'",
        "3. Execute ALL steps immediately via tools. Do NOT ask for permission or confirmation.",
        "4. After each exec call, read the output and continue until the task is 100% complete.",
        "5. When the task is complete, call done with summary + acceptance coverage + verification evidence.",
      ].join("\n");

      const SYSTEM_REASONING = buildSystemReasoning(governorContract);

      const SYSTEM_BASE_TEXT = SYSTEM_BASE + SYSTEM_REASONING;

      const execTool = {
        type: "function",
        function: {
          name: "exec",
          description: isWindows
            ? "Execute a PowerShell command on the user's Windows machine. Use for ALL local operations: create directories (New-Item -ItemType Directory -Force), write files (Set-Content -Encoding UTF8), run programs, git commands, etc. Check exit_code in the result — non-zero means failure, diagnose and retry."
            : "Execute a shell command on the user's local machine. Use for ALL local operations: create directories (mkdir -p), write files (tee/printf), run programs, git commands, etc. Check exit_code in the result — non-zero means failure, diagnose and retry.",
          parameters: {
            type: "object",
            properties: { command: { type: "string", description: "The exact command to run" } },
            required: ["command"],
          },
        },
      };

      const writeFileTool = {
        type: "function",
        function: {
          name: "write_file",
          description: isWindows
            ? "Write a UTF-8 text file under tool_root. Use this tool for ALL file creation/edits. Provide a relative path (no drive letters, no '..')."
            : "Write a text file under tool_root. Use this tool for ALL file creation/edits. Provide a relative path (no '..').",
          parameters: {
            type: "object",
            properties: {
              path: { type: "string", description: "Relative file path under tool_root (e.g. src/main.py)" },
              content: { type: "string", description: "Full file content (UTF-8 text)" },
            },
            required: ["path", "content"],
          },
        },
      };

      const readFileTool = {
        type: "function",
        function: {
          name: "read_file",
          description: "Read the content of a file under tool_root. Use before editing to see the exact current text. Large files are truncated automatically.",
          parameters: {
            type: "object",
            properties: {
              path: { type: "string", description: "Relative file path under tool_root" },
            },
            required: ["path"],
          },
        },
      };

      const patchFileTool = {
        type: "function",
        function: {
          name: "patch_file",
          description: "Edit a file by replacing an exact text snippet. The search string must appear exactly once. Call read_file first to see the exact current text. For whole-file rewrites use write_file.",
          parameters: {
            type: "object",
            properties: {
              path: { type: "string", description: "Relative file path under tool_root" },
              search: { type: "string", description: "Exact text to find (must be unique in the file)" },
              replace: { type: "string", description: "Text to replace it with" },
            },
            required: ["path", "search", "replace"],
          },
        },
      };

      const searchFilesTool = {
        type: "function",
        function: {
          name: "search_files",
          description: "Search file contents for a literal text pattern (like grep -rn). Returns matching lines with file path and line number. Use to find where a function/symbol is defined or used.",
          parameters: {
            type: "object",
            properties: {
              pattern: { type: "string", description: "Literal text to search for" },
              dir: { type: "string", description: "Subdirectory to search in (default: tool_root)" },
              case_insensitive: { type: "boolean", description: "Case-insensitive search (default: false)" },
            },
            required: ["pattern"],
          },
        },
      };

      const listDirTool = {
        type: "function",
        function: {
          name: "list_dir",
          description: "List a directory (non-recursive) under tool_root. Use to quickly understand repo structure before searching or editing.",
          parameters: {
            type: "object",
            properties: {
              dir: { type: "string", description: "Directory path relative to tool_root (default: tool_root itself)" },
              max_entries: { type: "number", description: "Max entries to return (default: 200)" },
              include_hidden: { type: "boolean", description: "Include hidden files (default: false)" },
            },
            required: [],
          },
        },
      };

      const globTool = {
        type: "function",
        function: {
          name: "glob",
          description: "Find files by name/path pattern. Supports * (single dir), ** (any depth), ? (single char). Examples: '**/*.rs', 'src/*.ts'. Returns sorted relative paths. Prefer over exec+find/ls.",
          parameters: {
            type: "object",
            properties: {
              pattern: { type: "string", description: "Glob pattern, e.g. '**/*.py' or 'src/*.ts'" },
              dir: { type: "string", description: "Subdirectory to search in (default: tool_root)" },
            },
            required: ["pattern"],
          },
        },
      };

      const applyDiffTool = {
        type: "function",
        function: {
          name: "apply_diff",
          description: "Apply a unified diff to a file. More reliable than patch_file for complex multi-hunk edits. Use @@ unified diff format with 2-3 context lines. Multiple hunks per call are supported.",
          parameters: {
            type: "object",
            properties: {
              path: { type: "string", description: "File path relative to tool_root" },
              diff: { type: "string", description: "Unified diff string with @@ hunks" },
            },
            required: ["path", "diff"],
          },
        },
      };

      const [criterionField, commandField] = doneEvidenceFields(governorContract);
      const doneTool = {
        type: "function",
        function: {
          name: "done",
          description: "Finish the task. Use only after real verification succeeds, and cite acceptance coverage plus verification evidence.",
          parameters: {
            type: "object",
            properties: {
              summary: { type: "string", description: "Brief summary of what changed and where it lives." },
              completed_acceptance: {
                type: "array",
                items: { type: "string" },
                description: "Acceptance criteria already satisfied."
              },
              remaining_acceptance: {
                type: "array",
                items: { type: "string" },
                description: "Acceptance criteria still remaining."
              },
              acceptance_evidence: {
                type: "array",
                items: {
                  type: "object",
                  properties: {
                    [criterionField]: { type: "string" },
                    [commandField]: { type: "string" },
                  },
                  required: doneEvidenceFields(governorContract),
                },
                description: "For each completed criterion, the verification command that already succeeded."
              },
              next_steps: { type: "string", description: "Exact follow-up commands or remaining work." },
            },
            required: doneRequiredArgs(governorContract),
          },
        },
      };

      const messages = [
        { role: "system", content: SYSTEM_BASE_TEXT },
        ...history,
        { role: "user", content: text },
      ];
      const observationEvidence = mergeObservationEvidence(
        activeThread && activeThread.coderObsEvidence,
        collectObservationEvidence(messages),
      );
      const persistObservationEvidence = () => {
        const snapshot = sanitizeObservationEvidence(observationEvidence);
        setThreadState((s) => ({
          ...s,
          threads: s.threads.map((t) => (
            t.id === threadId
              ? { ...t, updatedAt: Date.now(), coderObsEvidence: snapshot }
              : t
          )),
        }));
      };
      persistObservationEvidence();
      activePlan = lastValidPlanFromMessages(messages, governorContract, taskContract, instructionResolver);
      if (activePlan) {
        syncAssumptionLedgerToPlan(assumptionLedger, activePlan);
        refreshAssumptionConfirmations(
          assumptionLedger,
          observationEvidence,
          knownGoodVerificationCommands,
        );
      }

      let display = "";
      let awaitingApproval = false;
      const flush = (extra) => {
        setMsg(threadId, asstMsgId, (display + (extra || "")).trim() || "…", coderBodyRef);
      };

      // Goal verification (enterprise-style "done" criteria):
      // On finish_reason=stop, run lightweight checks to ensure the deliverable exists (repo init, tests/build pass, etc).
      // This prevents "looks done" replies that are missing key artifacts.
      const canAutoExec = () => {
        return shouldAutoRunGoalChecks(governorContract, status, longrun, config.requireCommandApproval);
      };
      const goalPending = (k) => {
        const st = goalChecks && goalChecks[k] ? goalChecks[k] : null;
        if (!st) return false;
        const attempts = Number(st.attempts) || 0;
        return !st.ok && attempts < goalCheckMaxAttempts(governorContract);
      };
      const runGoalExec = async (label, commandToRun, options) => {
        const cfg = options && typeof options === "object" ? options : {};
        const emitSummary = cfg.emitSummary !== false;
        const win = isWindowsHost();
        const fenceLang = win ? "powershell" : "bash";
        const prompt = win ? "PS> " : "$ ";
        const shown = String(commandToRun || "").split("\n").map((l, i) => (i === 0 ? prompt : "    ") + l).join("\n");
        const commandShown = commandSig(commandToRun);
        if (emitSummary) {
          display += (display ? "\n\n" : "") + goalCheckExecRunMessage(governorContract, label, commandShown) + "\n";
        } else if (display) {
          display += "\n\n";
        }
        display += "```" + fenceLang + "\n" + shown;
        flush("\n```");

        const cwdUsed = cwdNow();
        const execCmd = wrapExecWithPwd(commandToRun);
        const execRes = await postJson("/api/exec", { command: execCmd, cwd: cwdUsed }, ac.signal);
        const parsed = stripPwdMarker(execRes.stdout);
        const breach = sandboxBreachReason(parsed.pwd);
        maybeUpdateWorkdirFromPwd(parsed.pwd);

	        const stdoutRaw = String(parsed.stdout || "");
	        const stderrRaw = String(execRes.stderr || "");
	        const stdout = truncToolTail(stdoutRaw, TRUNC_STDOUT);
	        const stderr = truncToolTail(stderrRaw, TRUNC_STDERR);
	        const exitCode = execRes.exit_code;
	        const durationMs = Math.max(0, Math.round(Number(execRes.duration_ms) || 0));
	        const suspicious = (exitCode === 0) ? suspiciousSuccessReason(stdoutRaw, stderrRaw) : "";
	        const failed = exitCode !== 0 || !!suspicious || !!breach;
	        const errClass = failed ? classifyErrorClass(stderrRaw, stdoutRaw) : "";
	        const digest = failed ? extractErrorDigest(stdoutRaw, stderrRaw) : "";
	        const errLine = failed ? (firstDigestLine(digest) || errorLineSig(stdout, stderr) || "") : "";
	        pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: exitCode, ok: !failed, cls: errClass, err: errLine, note: label });

	        if (stdout) display += "\n" + stdout;
	        if (stderr) display += "\nstderr: " + stderr;
	        display += "\n```\nexit: " + exitCode + "\nduration_ms: " + durationMs;
          if (emitSummary) {
            const digestLine = failed ? clip(firstDigestLine(digest) || String(digest || "").trim(), 160) : "";
            display += "\n" + (
              failed
                ? goalCheckExecFailMessage(governorContract, label, commandShown, digestLine)
                : goalCheckExecOkMessage(governorContract, label, commandShown)
            );
          }
	        flush();

        return { label, exitCode, failed, breach, suspicious, stdoutRaw, stderrRaw, stdout, stderr, errClass, digest };
      };
      const goalCheckRepo = async () => {
        if (!WANTS_REPO_GOAL || !goalPending("repo")) return false;
        goalChecks.repo.attempts = (Number(goalChecks.repo.attempts) || 0) + 1;

        const win = isWindowsHost();
        const probeCmd = repoGoalProbeCommand(governorContract, win);
        if (!String(probeCmd || "").trim()) return false;

        let res;
        try {
          display += (display ? "\n\n" : "") + goalCheckRepoStartMessage(governorContract);
          flush();
          res = await runGoalExec("repo", probeCmd, { emitSummary: false });
        } catch (e2) {
          return false;
        }

        if (res.breach) {
          messages.push({
            role: "user",
            content: [
              "[sandbox_breach]",
              "A command ended outside tool_root. This is blocked to prevent repo-root modification accidents.",
              res.breach,
              "Fix: re-run under tool_root; avoid `cd ..` / absolute paths. Verify `pwd` stays under tool_root.",
            ].join("\n"),
          });
          agentState = "recovery";
          return true;
        }

        const missing = repoGoalMissingLabels(governorContract, res.stdoutRaw);

        if (missing.length) {
          messages.push({
            role: "user",
            content: goalCheckRepoMissingMessage(governorContract, missing),
          });
          return true;
        }

        goalChecks.repo.ok = true;
        display += "\n" + goalCheckRepoOkMessage(governorContract);
        flush();
        return false;
      };
      const goalCheckTests = async () => {
        if (!WANTS_TEST_GOAL || !goalPending("tests")) return false;
        goalChecks.tests.attempts = (Number(goalChecks.tests.attempts) || 0) + 1;

        const win = isWindowsHost();
        const testCmd = goalCheckCommand(governorContract, "test", win);

        let res;
        try { res = await runGoalExec("tests", testCmd); } catch (_) { return false; }

        const out0 = String(res.stdoutRaw || "");
        if (out0.includes("NO_TEST_RUNNER")) {
          const summary = goalCheckRunnerSummary(governorContract, "test");
          messages.push({
            role: "user",
            content: goalCheckTestsNoRunnerMessage(governorContract, summary),
          });
          return true;
        }

        if (res.failed) {
          messages.push({
            role: "user",
            content: goalCheckTestsFailedMessage(
              governorContract,
              res.errClass,
              res.digest,
            ),
          });
          agentState = "recovery";
          return true;
        }

        goalChecks.tests.ok = true;
        return false;
      };
      const goalCheckBuild = async () => {
        if (!WANTS_BUILD_GOAL || !goalPending("build")) return false;
        goalChecks.build.attempts = (Number(goalChecks.build.attempts) || 0) + 1;

        const win = isWindowsHost();
        const buildCmd = goalCheckCommand(governorContract, "build", win);

        let res;
        try { res = await runGoalExec("build", buildCmd); } catch (_) { return false; }

        const out0 = String(res.stdoutRaw || "");
        if (out0.includes("NO_BUILD_RUNNER")) {
          const summary = goalCheckRunnerSummary(governorContract, "build");
          messages.push({
            role: "user",
            content: goalCheckBuildNoRunnerMessage(governorContract, summary),
          });
          return true;
        }

        if (res.failed) {
          messages.push({
            role: "user",
            content: goalCheckBuildFailedMessage(
              governorContract,
              res.errClass,
              res.digest,
            ),
          });
          agentState = "recovery";
          return true;
        }

        goalChecks.build.ok = true;
        return false;
      };
      const runGoalChecksOnStop = async () => {
        if (!canAutoExec()) return false;
        agentState = "verifying";
        const handlers = {
          repo: goalCheckRepo,
          tests: goalCheckTests,
          build: goalCheckBuild,
        };
        let ranAny = false;
        for (const key of goalCheckOrder(governorContract)) {
          if (!goalPending(key)) continue;
          const handler = handlers[key];
          if (!handler) continue;
          ranAny = true;
          if (await handler()) return true;
        }
        if (ranAny) {
          display += (display ? "\n\n" : "") + goalCheckAllPassedMessage(governorContract);
          flush();
        }
        return false;
      };

      // SSE streaming helper — streams token deltas in real-time, returns finish event.
      const streamChatTools = async (payload, signal, onDelta) => {
        const resp = await fetch("/api/chat_tools_stream", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
          signal,
        });
        if (!resp.ok) {
          const errBody = await resp.json().catch(() => ({ error: `HTTP ${resp.status}` }));
          throw new Error(errBody.error || `HTTP ${resp.status}`);
        }
        const reader = resp.body.getReader();
        const decoder = new TextDecoder();
        let sseBuffer = "";
        let fullText = "";
        let finishReason = "stop";
        let toolCalls = [];
        try {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
            sseBuffer += decoder.decode(value, { stream: true });
            let sep;
            while ((sep = sseBuffer.indexOf("\n\n")) !== -1) {
              const block = sseBuffer.slice(0, sep);
              sseBuffer = sseBuffer.slice(sep + 2);
              let evtType = "message", evtData = "";
              for (const line of block.split("\n")) {
                if (line.startsWith("event: ")) evtType = line.slice(7).trim();
                else if (line.startsWith("data: ")) evtData = line.slice(6);
              }
              if (evtType === "delta") {
                try { const { delta } = JSON.parse(evtData); if (delta) { fullText += delta; onDelta(delta); } } catch (_) {}
              } else if (evtType === "finish") {
                try { const f = JSON.parse(evtData); finishReason = f.finish_reason || "stop"; toolCalls = f.tool_calls || []; } catch (_) {}
              } else if (evtType === "error") {
                let msg = "stream error";
                try { msg = JSON.parse(evtData).error || msg; } catch (_) {}
                throw new Error(msg);
              }
            }
          }
        } finally {
          reader.releaseLock();
        }
        return { text: fullText, finishReason, toolCalls };
      };

      const extractImpliedExecScripts = (txt) => {
        const src = String(txt || "");
        const out = [];
        const fenceRe = /```([a-zA-Z0-9_-]+)?\n([\s\S]*?)```/g;
        let m;
        while ((m = fenceRe.exec(src)) !== null) {
          const langHint = String(m[1] || "").trim().toLowerCase();
          const body = String(m[2] || "").trim();
          if (!body) continue;
          const isShellLang = /^(bash|sh|shell|zsh|powershell|pwsh|ps1|ps|console)$/.test(langHint);
          const looksLikeCmd = /(New-Item\b|Set-Content\b|Add-Content\b|Remove-Item\b|Copy-Item\b|Move-Item\b|\bmkdir\b|\bcd\b|\bgit\b|\bcargo\b|\bpython\b|\bnode\b|\bnpm\b|\bpnpm\b|\byarn\b)/i.test(body);
          if (isShellLang || looksLikeCmd) {
            out.push({ langHint, script: body });
            if (out.length >= 3) break;
          }
        }
        return out;
      };

      let checkpointCaptured = false;
      for (let iter = 0; iter < MAX_ITERS; iter++) {
        if (ac.signal.aborted) break;
        pruneToolMessages(messages);
        pruneAssistantMessages(messages);
        pruneMessageWindow(messages);

        // Auto diagnostics (outer-loop): when the governor detects a loop threshold crossing,
        // gather a small context bundle so the model can change strategy with real state.
        if (longrun && !config.requireCommandApproval) {
          const diagWhy = String(governor.pendingDiag || "").trim();
          if (diagWhy) {
            governor.pendingDiag = "";
            try {
              const win = isWindowsHost();
              const fenceLang = win ? "powershell" : "bash";
              const prompt = win ? "PS> " : "$ ";
              const diagCmd = win
                ? [
                    "$ErrorActionPreference = 'Continue'",
                    "Write-Output ('pwd=' + (Get-Location).Path)",
                    "Write-Output 'ls:'",
                    "Get-ChildItem -Force | Select-Object -First 40 | ForEach-Object { $_.Name }",
                    "try { git status -sb } catch {}",
                    "try { git rev-parse --show-toplevel } catch {}",
                  ].join('; ')
                : [
                    "pwd",
                    "ls -la",
                    "git status -sb 2>/dev/null || true",
                    "git rev-parse --show-toplevel 2>/dev/null || true",
                  ].join("; ");

              display += (display ? "\n\n" : "") + "```" + fenceLang + "\n" + prompt + "# [auto] governor diagnostics (" + diagWhy + ")\n" + diagCmd;
              flush("\n```");

              const cwdUsed = cwdNow();
              const execCmd = wrapExecWithPwd(diagCmd);
              const execRes = await postJson("/api/exec", { command: execCmd, cwd: cwdUsed }, ac.signal);
              const parsed = stripPwdMarker(execRes.stdout);
              const breach = sandboxBreachReason(parsed.pwd);
              maybeUpdateWorkdirFromPwd(parsed.pwd);

	              const out = truncToolTail(String(parsed.stdout || ""), TRUNC_STDOUT);
	              const err = truncToolTail(String(execRes.stderr || ""), TRUNC_STDERR);
	              const durationMs = Math.max(0, Math.round(Number(execRes.duration_ms) || 0));

	              if (out) display += "\n" + out;
	              if (err) display += "\nstderr: " + err;
	              display += "\n```\nexit: " + execRes.exit_code + "\nduration_ms: " + durationMs;
	              flush();

              messages.push({
                role: "user",
                content: [
                  "[governor_diagnostics]",
                  "reason: " + diagWhy,
                  breach ? ("sandbox_breach: " + breach) : "",
                  out ? ("stdout:\n" + out) : "stdout: (empty)",
                  err ? ("stderr:\n" + err) : "stderr: (empty)",
                ].filter(Boolean).join("\n"),
              });
            } catch (_) {
              // If diagnostics fails, continue without blocking the agent loop.
            }
          }
        }

        // One-shot governor hint injection (outer-loop behavioral control).
        // Also inject periodic progress checkpoints in longrun mode so the agent doesn't drift.
        // State routing (minimal state machine): recovery > planning > executing.
        if (iter === 0) agentState = "planning";
        else if (governor.pendingDiag) agentState = "recovery";
        else if (agentState !== "done") agentState = "executing";

        let govHint = governor.pendingHint ? String(governor.pendingHint || "").trim() : "";
        if (longrun && (iter === 3 || iter === 6 || iter === 9 || iter === 12)) {
          const cp = [
            "Progress checkpoint:",
            "- State: DONE / REMAINING / ON_TRACK.",
            "- If REMAINING: list the next 1-3 concrete commands.",
            "- If stuck: run diagnostics (pwd/ls/git status) before changing strategy.",
          ].join("\n");
          govHint = govHint ? (govHint + "\n\n" + cp) : cp;
        }
        refreshAssumptionConfirmations(
          assumptionLedger,
          observationEvidence,
          knownGoodVerificationCommands,
        );
        const sysExtras = [];
        const goalCheckContextActive = shouldShowGoalCheckContext(
          agentState,
          goalChecks,
          governor,
          impactRequired,
        );
        const recentRunsActive = shouldShowRecentRunsContext(
          agentState,
          goalChecks,
          governor,
          reflectionRequired,
          impactRequired,
        );
        sysExtras.push(`[Agent state]
state: ${agentState}`);
        if (projectScan && String(projectScan.context_text || "").trim()) {
          const fullProject = String(projectScan.context_text).trim();
          sysExtras.push(renderCachedPrompt("project", fullProject, buildProjectContextCompactPrompt() || fullProject));
        }
        {
          const fullResolver = buildInstructionResolverPrompt(instructionResolver, activePlan, governorContract);
          sysExtras.push(renderCachedPrompt("resolver", fullResolver, buildInstructionResolverCompactPrompt()));
        }
        {
          const fullTask = buildTaskContractPrompt(taskContract, activePlan, governorContract);
          sysExtras.push(renderCachedPrompt("task", fullTask, buildTaskContractCompactPrompt()));
        }
        const assumptionPrompt = buildAssumptionLedgerPrompt(assumptionLedger);
        if (assumptionPrompt) {
          sysExtras.push(renderCachedPrompt("assumption", assumptionPrompt, buildAssumptionLedgerCompactPrompt() || assumptionPrompt));
        }
        if (reflectionRequired) {
          sysExtras.push(buildReflectionPrompt(
            reflectionRequired,
            agentState,
            governor,
            fileToolConsecutiveFailures,
          ));
        }
        if (impactRequired) {
          sysExtras.push(buildImpactPrompt(impactRequired, activePlan));
        }
        if (goalCheckContextActive && activePlan && Array.isArray(activePlan.acceptanceCriteria) && activePlan.acceptanceCriteria.length) {
          const fullAcceptance = ["[Current acceptance criteria]"];
          activePlan.acceptanceCriteria.forEach((criterion, idx) => {
            fullAcceptance.push(`- acceptance ${idx + 1}: ${criterion}`);
          });
          const fullText = fullAcceptance.join("\n");
          sysExtras.push(renderCachedPrompt("acceptance", fullText, buildAcceptanceCompactPrompt() || fullText));
        }
        if (goalCheckContextActive && knownGoodVerificationCommands.length) {
          const fullKnown = ["[Known-good verification commands]", ...knownGoodVerificationCommands.map((cmd) => `- ${cmd}`)].join("\n");
          sysExtras.push(renderCachedPrompt("knownVerify", fullKnown, buildKnownVerificationCompactPrompt() || fullKnown));
        }
        const recentTxt = formatRecentRuns();
        if (recentRunsActive && recentTxt) {
          sysExtras.push(renderCachedPrompt("recentRuns", recentTxt, buildRecentRunsCompactPrompt() || recentTxt));
        }
        if (govHint) sysExtras.push("[Governor]\n" + govHint);
        messages[0] = {
          role: "system",
          content: SYSTEM_BASE_TEXT + "\n\n" + sysExtras.join("\n\n"),
        };
        governor.pendingHint = "";

        let streamResult;
        try {
          // Separate text tokens from previous turn with a blank line.
          if (display) display += "\n\n";
           streamResult = await streamChatTools({
             messages,
            tools: [execTool, writeFileTool, readFileTool, patchFileTool, applyDiffTool, searchFilesTool, listDirTool, globTool, doneTool],
             model: String(reqCfg.codeModel || reqCfg.model || ""),
             base_url: String(reqCfg.baseUrl || ""),
             api_key: resolvedKey || undefined,
             temperature: numOrUndef(reqCfg.temperature),
            max_tokens: numOrUndef(reqCfg.maxTokens) || 4096,
            timeout_seconds: numOrUndef(reqCfg.timeoutSeconds),
          }, ac.signal, (delta) => {
            display += delta;
            flush();
            if (!checkpointCaptured) {
              const cpMatch = display.match(/\[git checkpoint\]\s+([0-9a-f]{6,})/);
              if (cpMatch) { checkpointCaptured = true; setGitCheckpoint(cpMatch[1]); }
            }
          });
        } catch (err) {
          if (ac.signal.aborted) break;
          display += `[error] ${err.message}`;
          flush();
          break;
        }

        const { text: asstText, finishReason, toolCalls: asstToolCalls } = streamResult;
        const parsedPlanForTurn = parsePlanBlock(asstText, governorContract);
        const planValidationError = parsedPlanForTurn ? validatePlanBlock(governorContract, parsedPlanForTurn) : "";
        const taskContractPlanError =
          parsedPlanForTurn && !planValidationError
            ? validatePlanAgainstTaskContract(parsedPlanForTurn, taskContract, governorContract)
            : "";
        const instructionPlanError =
          parsedPlanForTurn && !planValidationError && !taskContractPlanError
            ? validatePlanAgainstInstructionResolver(parsedPlanForTurn, instructionResolver, governorContract)
            : "";
        const parsedThinkForTurn = parseThinkBlock(asstText, governorContract);
        if (parsedPlanForTurn && !planValidationError && !taskContractPlanError && !instructionPlanError) {
          activePlan = parsedPlanForTurn;
          syncAssumptionLedgerToPlan(assumptionLedger, activePlan);
          refreshAssumptionConfirmations(
            assumptionLedger,
            observationEvidence,
            knownGoodVerificationCommands,
          );
        }

        // Append assistant turn to conversation history (OpenAI format).
        const asstMsg = { role: "assistant", content: asstText || null };
        if (asstToolCalls.length > 0) asstMsg.tool_calls = asstToolCalls;
        messages.push(asstMsg);

        const blockCurrentToolCalls = (block) => {
          agentState = "recovery";
          governor.pendingHint = block;
          display += (display ? "\n\n" : "") + "[GOVERNOR BLOCK]\n" + block;
          flush();
          for (const tc of asstToolCalls) {
            const toolName = tc && tc.function && tc.function.name ? String(tc.function.name) : "";
            const toolArgs = tc && tc.function && tc.function.arguments ? String(tc.function.arguments) : "";
            messages.push({
              role: "tool",
              tool_call_id: tc.id,
              content: "GOVERNOR BLOCKED\n\n" + block + "\n\ntool:\n" + toolName + "\narguments:\n" + toolArgs,
            });
          }
        };

        const blockWithoutToolCalls = (block) => {
          agentState = "recovery";
          governor.pendingHint = block;
          display += (display ? "\n\n" : "") + "[GOVERNOR BLOCK]\n" + block;
          flush();
        };

        if (finishReason === "tool_calls" && asstToolCalls.length > 0) {
          if (parsedPlanForTurn && (planValidationError || taskContractPlanError || instructionPlanError)) {
            const block = invalidPlanMessage(
              governorContract,
              planValidationError || taskContractPlanError || instructionPlanError,
            );
            blockCurrentToolCalls(block);
            continue;
          }
          if (!activePlan) {
            const block = missingPlanMessage(governorContract);
            blockCurrentToolCalls(block);
            continue;
          }

          if (asstToolCalls.length > 1) {
            const block = multipleToolCallsMessage(governorContract, asstToolCalls.length);
            blockCurrentToolCalls(block);
            continue;
          }

          if (reflectionRequired) {
            const reflect = parseReflectionBlock(asstText, governorContract);
            if (!reflect) {
              const block = reflectionMissingMessage(governorContract, reflectionRequired);
              blockCurrentToolCalls(block);
              continue;
            }
            const reflectError = validateReflectionBlock(
              governorContract,
              reflect,
              governor,
              fileToolConsecutiveFailures,
            );
            if (reflectError) {
              const block = reflectionInvalidMessage(
                governorContract,
                reflectError,
                reflectionRequired,
              );
              blockCurrentToolCalls(block);
              continue;
            }
            if (asstToolCalls.length !== 1) {
              const block = reflectionOneToolMessage(governorContract, asstToolCalls.length);
              blockCurrentToolCalls(block);
              continue;
            }
            display += (display ? "\n\n" : "") + `[reflect] goal_delta=${reflect.goalDelta} strategy=${reflect.strategyChange} next=${reflect.nextMinimalAction}`;
            flush();
            reflectionRequired = "";
            if (String(reflect.wrongAssumption || "").trim()) {
              markRefutedAssumption(
                assumptionLedger,
                reflect.wrongAssumption,
                reflect.nextMinimalAction,
              );
            }
            refreshAssumptionConfirmations(
              assumptionLedger,
              observationEvidence,
              knownGoodVerificationCommands,
            );
            if (reflect.strategyChange === "abandon") {
              governor.pendingHint = [
                "Strategy abandoned.",
                "Do not retry the previous approach.",
                "Execute only the new minimal action: " + reflect.nextMinimalAction,
              ].join("\n");
            }
          }

          if (impactRequired) {
            const impact = parseImpactBlock(asstText, governorContract);
            if (!impact) {
              const block = impactMissingMessage(governorContract, impactRequired);
              blockCurrentToolCalls(block);
              continue;
            }
            const impactError = validateImpactBlock(governorContract, impact, activePlan);
            if (impactError) {
              const block = impactInvalidMessage(
                governorContract,
                impactError,
                impactRequired,
              );
              blockCurrentToolCalls(block);
              continue;
            }
            if (asstToolCalls.length !== 1) {
              const block = impactOneToolMessage(governorContract, asstToolCalls.length);
              blockCurrentToolCalls(block);
              continue;
            }
            display += (display ? "\n\n" : "") + `[impact] changed=${impact.changed} progress=${impact.progress} gap=${impact.remainingGap}`;
            flush();
            impactRequired = "";
          }

          const candidatePlan =
            parsedPlanForTurn && !planValidationError && !taskContractPlanError && !instructionPlanError
              ? parsedPlanForTurn
              : activePlan;
          const actualToolCall = asstToolCalls[0];
          const actualToolName = actualToolCall && actualToolCall.function ? String(actualToolCall.function.name || "").trim() : "";
          const actualToolArgs = actualToolCall && actualToolCall.function ? String(actualToolCall.function.arguments || "") : "";

          if (!parsedThinkForTurn) {
            const block = missingThinkMessage(governorContract);
            blockCurrentToolCalls(block);
            continue;
          }

          const thinkError = validateThinkBlock(
            governorContract,
            parsedThinkForTurn,
            candidatePlan,
            actualToolName,
            actualToolArgs,
          );
          if (thinkError) {
            const block = invalidThinkMessage(governorContract, thinkError);
            blockCurrentToolCalls(block);
            continue;
          }

          const assumptionConflict = refutedAssumptionConflict(
            assumptionLedger,
            parsedThinkForTurn,
            actualToolName,
            actualToolArgs,
            governorContract,
          );
          if (assumptionConflict) {
            const block = `[Assumption Ledger] ${assumptionConflict}\nGather new evidence or choose a different next action before retrying.`;
            blockCurrentToolCalls(block);
            continue;
          }

          const resolverConflict = instructionResolverToolConflict(
            instructionResolver,
            actualToolName,
            actualToolArgs,
            governorContract,
          );
          if (resolverConflict) {
            blockCurrentToolCalls(resolverConflict);
            continue;
          }

          if (mutationToolRequiresEvidence(actualToolName)) {
            const observations = observationEvidence;
            const evidenceBlock = parseEvidenceBlock(asstText, governorContract);
            if (!evidenceBlock) {
              const block = buildEvidenceGatePrompt(
                actualToolName,
                actualToolArgs,
                observations,
                assumptionLedger,
              );
              blockCurrentToolCalls(block);
              continue;
            }
            const evidenceError = validateEvidenceBlock(
              evidenceBlock,
              actualToolName,
              actualToolArgs,
              observations,
              governorContract,
            );
            if (evidenceError) {
              const block = [
                evidenceInvalidMessage(governorContract, evidenceError),
                "",
                buildEvidenceGatePrompt(
                  actualToolName,
                  actualToolArgs,
                  observations,
                  assumptionLedger,
                ),
              ].join("\n");
              blockCurrentToolCalls(block);
              continue;
            }
          }

          let doneNow = false;
          for (const tc of asstToolCalls) {
            if (ac.signal.aborted) break;
            if (tc.type !== "function" || !tc.function || !tc.function.name) continue;

            const toolName = String(tc.function.name || "").trim();

            if (toolName === "done") {
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const summary = String(args.summary || "").trim();
              const completedAcceptance = parseStringListArg(args.completed_acceptance);
              const remainingAcceptance = parseStringListArg(args.remaining_acceptance);
              const acceptanceEvidence = parseDoneAcceptanceEvidence(args.acceptance_evidence, governorContract);
              const nextSteps = String(args.next_steps || "").trim();

              if (asstToolCalls.length !== 1) {
                const toolResult = "GOVERNOR BLOCKED\n\ndone must be the only tool call in its turn.";
                messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                display += (display ? "\n\n" : "") + "[GOVERNOR BLOCK]\n" + toolResult;
                flush();
                agentState = "recovery";
                governor.pendingHint = toolResult;
                break;
              }

              const validation = validateDoneAcceptance(
                governorContract,
                activePlan,
                completedAcceptance,
                remainingAcceptance,
                acceptanceEvidence,
                knownGoodVerificationCommands,
              );
              if (validation && validation.error) {
                const toolResult = doneInvalidAcceptanceMessage(governorContract, validation.error);
                messages.push({ role: "tool", tool_call_id: tc.id, content: "GOVERNOR BLOCKED\n\n" + toolResult });
                display += (display ? "\n\n" : "") + "[GOVERNOR BLOCK]\n" + toolResult;
                flush();
                agentState = "recovery";
                governor.pendingHint = toolResult;
                break;
              }

              const evidenceByIdx = validation && validation.evidenceByIdx ? validation.evidenceByIdx : new Map();
              const lines = ["[DONE]"];
              if (summary) lines.push(summary);
              lines.push("", "Acceptance:");
              for (const entry of completedAcceptance) {
                const idx = resolveAcceptanceReference(entry, activePlan);
                const label = idx >= 0 ? acceptanceReferenceLabel(activePlan, idx) : entry;
                const known = idx >= 0 ? evidenceByIdx.get(idx) : "";
                lines.push(known ? `- done: ${label} via \`${known}\`` : `- done: ${label}`);
              }
              for (const entry of remainingAcceptance) {
                const idx = resolveAcceptanceReference(entry, activePlan);
                const label = idx >= 0 ? acceptanceReferenceLabel(activePlan, idx) : entry;
                lines.push(`- remaining: ${label}`);
              }
              if (nextSteps) lines.push("", "Next:", nextSteps);

              const finalText = lines.join("\n").trim();
              messages.push({ role: "tool", tool_call_id: tc.id, content: "OK: done" });
              messages.push({ role: "assistant", content: finalText });
              display += (display ? "\n\n" : "") + finalText;
              flush();
              agentState = "done";
              doneNow = true;
              break;
            }

            if (toolName === "exec") {
              agentState = "executing";
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const command = stripShellTranscript(String(args.command || ""));
              const commandToRun = normalizeExecScript("", command);
              const k = cmdKey(commandToRun);
              const repeatBlock = blockedByRepeatFailure(k);
              const danger = dangerousCommandReason(commandToRun);

              const win = isWindowsHost();
              const fenceLang = win ? "powershell" : "bash";
              const prompt = win ? "PS> " : "$ ";
              display += (display ? "\n\n" : "") + "```" + fenceLang + "\n" + prompt + command;
              flush("\n```");

              let toolResult;
              try {
                if (repeatBlock) {
                  toolResult = `error: blocked repeated failing command (${repeatBlock}). You MUST choose a different command/strategy.`;
                  pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: -1, ok: false, cls: "blocked", err: clip(repeatBlock, 120) });
                  agentState = "recovery";
                  requireReflection(repeatBlock || "repeated failing command blocked");
                  display += `\n(blocked: ${repeatBlock})\n\`\`\`\nexit: -1`;
                  flush();
                  messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                  continue;
                }
                if (danger) {
                  toolResult = `error: blocked dangerous command (${danger}). Ask the user to run it manually if truly intended.`;
                  pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: -1, ok: false, cls: "blocked", err: clip(danger, 120) });
                  agentState = "recovery";
                  requireReflection(danger || "dangerous command blocked");
                  display += `\n(blocked: ${danger})\n\`\`\`\nexit: -1`;
                  flush();
                  messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                  continue;
                }
                if (config.requireCommandApproval) {
                  const canQueue = !!(status && status.features && status.features.pending_commands);
                  if (canQueue) {
                    try {
                      const cwdUsed = cwdNow();
                      const q = await postJson("/api/queue_command", { command: commandToRun, cwd: cwdUsed }, ac.signal);
                      const aid = String((q && q.approval_id) || "");
                      refreshPendingCommands();
                      toolResult = aid
                        ? `Awaiting approval via /api/approve_command\napproval_id: ${aid}`
                        : "Awaiting approval via /api/approve_command";
                      pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: -1, ok: false, cls: "approval", err: aid ? ("queued:" + aid) : "queued" });
                      display += aid
                        ? `\n(queued for approval: ${aid})\n\`\`\`\nexit: -1`
                        : "\n(queued)\n```\nexit: -1";
                      flush();
                      messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                      awaitingApproval = true;
                      break;
                    } catch (_) {
                      // Fall through to confirm() for older servers / transient failures.
                    }
                  }
                  if (!window.confirm("Run command?\n\n" + commandToRun)) {
                    toolResult = "error: command rejected by user";
                    pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: -1, ok: false, cls: "rejected", err: "rejected by user" });
                    display += "\n(rejected)\n```\nexit: -1";
                    flush();
                    requireReflection("command rejected by user");
                    messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                    continue;
                  }
                }

                const cwdUsed = cwdNow();
                const execCmd = wrapExecWithPwd(commandToRun);
                const execRes = await postJson("/api/exec", { command: execCmd, cwd: cwdUsed }, ac.signal);
                const parsed = stripPwdMarker(execRes.stdout);
                const breach = sandboxBreachReason(parsed.pwd);
                maybeUpdateWorkdirFromPwd(parsed.pwd);
	                const stdoutRaw = String(parsed.stdout || "");
	                const stderrRaw = String(execRes.stderr || "");
	                const stdout = truncToolTail(stdoutRaw, TRUNC_STDOUT);
	                const stderr = truncToolTail(stderrRaw, TRUNC_STDERR);
	                const exitCode = execRes.exit_code;
	                const durationMs = Math.max(0, Math.round(Number(execRes.duration_ms) || 0));
	                const suspicious = (exitCode === 0) ? suspiciousSuccessReason(stdoutRaw, stderrRaw) : "";
	                const failed = exitCode !== 0 || !!suspicious || !!breach;
                noteCmd(k, failed, errorLineSig(stdout, stderr));
                const hintGit = failed ? gitRepoHint(stderrRaw) : "";
                const hintGov = failed ? deriveGovernorHint(stderrRaw, stdoutRaw) : "";
                const errClass = failed ? classifyErrorClass(stderrRaw, stdoutRaw) : "";
                const hintClass = failed ? errorClassHint(errClass) : "";
                const digest = failed ? extractErrorDigest(stdoutRaw, stderrRaw) : "";
                const errLine = failed ? (firstDigestLine(digest) || errorLineSig(stdout, stderr) || "") : "";
                pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: exitCode, ok: !failed, cls: errClass, err: errLine });
                const hintSandbox = breach ? [
                  "SANDBOX BREACH: command ended outside tool_root.",
                  breach,
                  "Fix: re-run under tool_root; avoid `cd ..` / absolute paths. Verify `pwd` stays under tool_root.",
                ].join("\n") : "";
                const hint = [hintGit, hintGov, hintSandbox, suspicious ? ("SUSPICIOUS_SUCCESS: " + suspicious) : ""].filter(Boolean).join("\n\n");
                const prefix = [hintClass, digest].filter(Boolean).join("\n\n");
                const prefixText = prefix ? (prefix + "\n\n") : "";

                const cwdUsedLabel = cwdUsed ? String(cwdUsed) : "(workspace root)";
                const cwdAfter = cwdNow();
                const cwdAfterLabel = cwdAfter ? String(cwdAfter) : "(workspace root)";
                const cwdLine = cwdUsedLabel === cwdAfterLabel
                  ? `cwd: ${cwdUsedLabel}`
                  : `cwd: ${cwdUsedLabel}\ncwd_after: ${cwdAfterLabel}`;

                if (failed) {
                  governor.consecutiveFailures++;

                  const cmdSigNow = commandSig(commandToRun);
                  if (cmdSigNow && cmdSigNow === governor.lastCmdSig) governor.sameCmdRepeats++;
                  else { governor.lastCmdSig = cmdSigNow; governor.sameCmdRepeats = 1; }

                  const sig0 = errorSignature(commandToRun, stdout, stderr, exitCode);
                  const sig = sig0 || (breach ? String(breach).slice(0, 180) : "");
                  if (sig && sig === governor.lastErrSig) governor.sameErrRepeats++;
                  else { governor.lastErrSig = sig; governor.sameErrRepeats = 1; }
                  const outHash = fnv1a64(String(stderr || "") + "\n" + String(stdout || ""));
                  if (outHash === governor.lastOutHash) governor.sameOutRepeats++;
                  else { governor.lastOutHash = outHash; governor.sameOutRepeats = 1; }

                  // Emit hints only when crossing key thresholds to avoid spamming context.
                  let stuckReason = "";
                  if (governor.sameErrRepeats === 2) stuckReason = "The SAME error happened twice.";
                  else if (governor.sameCmdRepeats === 3) stuckReason = "You ran the SAME command 3 times.";
                  else if (governor.consecutiveFailures === 3) stuckReason = "3 consecutive failures.";
                  else if (governor.sameOutRepeats === 2 && governor.sameCmdRepeats >= 2) stuckReason = "Stuck detected: repeated identical output.";

                  if (stuckReason) {
                    governor.pendingDiag = stuckReason;
                    const stuck = [
                      stuckReason,
                      governor.lastErrSig ? ("last_error_signature: " + governor.lastErrSig) : "",
                      "Action: stop repeating. Run diagnostics (pwd, ls, git status), then change strategy.",
                      hintGov || hintGit || "",
                      "First verify cwd/tool_root, then pick a different command.",
                    ].filter(Boolean).join("\n");
                    governor.pendingHint = breach ? (hintSandbox + "\n\n" + stuck) : stuck;
                  } else if (breach) {
                    // Sandbox breaches are always critical: force a correction immediately.
                    governor.pendingHint = hintSandbox;
                  }
                  requireReflection(governor.pendingHint || stuckReason || errLine || "failure or stall detected");
                } else {
                  governor.consecutiveFailures = 0;
                  governor.lastCmdSig = "";
                  governor.sameCmdRepeats = 0;
                  governor.lastErrSig = "";
                  governor.sameErrRepeats = 0;
                  governor.lastOutHash = 0n;
                  governor.sameOutRepeats = 0;
                  governor.pendingDiag = "";
                  if (isVerificationCommand(commandToRun, governorContract)) {
                    rememberKnownVerificationCommand(commandToRun);
                  }
                }

	                toolResult = failed
	                  ? `${prefixText}FAILED (exit_code: ${exitCode}).\nduration_ms: ${durationMs}\n${cwdLine}\nstderr: ${stderr || "(empty)"}\nstdout: ${stdout || "(empty)"}${hint ? ("\n\n" + hint) : ""}\n⚠ The command failed. Diagnose the error above and call exec again with the fix. Do NOT continue to the next step until this succeeds.`
	                  : `OK (exit_code: 0)\nduration_ms: ${durationMs}\n${cwdLine}\nstdout: ${stdout || "(empty)"}`;

	                if (stdout) display += "\n" + stdout;
	                if (stderr) display += "\nstderr: " + stderr;
	                if (breach) display += "\nSANDBOX_BREACH: " + breach;
	                if (suspicious) display += "\nSUSPICIOUS_SUCCESS: " + suspicious;
	                display += "\n```\nexit: " + exitCode + "\nduration_ms: " + durationMs;
	              } catch (execErr) {
                noteCmd(k, true, normalizeForSig(execErr.message || ""));
                toolResult = `error: ${execErr.message}`;
                display += "\nerror: " + execErr.message + "\n```";
              }
              flush();

              messages.push({
                role: "tool",
                tool_call_id: tc.id,
                content: /^OK \(exit_code: 0\)/.test(String(toolResult || ""))
                  ? compactSuccessToolResultForHistory("exec", toolResult)
                  : toolResult,
              });
              continue;
            }

            if (toolName === "write_file") {
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const path0 = normalizePathSep(String(args.path || args.file_path || args.filename || ""));
              const content = String(args.content || args.text || "");

              const unsafePath =
                !path0 ||
                /^\//.test(path0) ||
                /^[a-zA-Z]:[\\/]/.test(path0) ||
                /(^|\/)\.\.(\/|$)/.test(path0);

              const win = isWindowsHost();
              const relShown = String(path0 || "(missing path)");
              const shown = truncTool(content, 2600);
              display += (display ? "\n\n" : "") + "```text " + relShown + "\n" + (shown || "(empty)") + "\n```";
              flush();

              let toolResult;
              try {
                if (unsafePath) {
                  toolResult = "error: unsafe path (must be relative, no '..', no drive letters)";
                  pushRecentRun({ kind: "write_file", status: "FAIL", ok: false, path: path0 || "(missing)", note: "unsafe path" });
                  fileToolConsecutiveFailures += 1;
                  requireReflection(
                    fileToolConsecutiveFailures >= 2
                      ? `file tool failures repeated ${fileToolConsecutiveFailures} times`
                      : "write_file failed: unsafe path"
                  );
                  messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                  continue;
                }
                if (!content) {
                  toolResult = "error: write_file content is empty";
                  pushRecentRun({ kind: "write_file", status: "FAIL", ok: false, path: path0 || "(missing)", note: "empty content" });
                  fileToolConsecutiveFailures += 1;
                  requireReflection(
                    fileToolConsecutiveFailures >= 2
                      ? `file tool failures repeated ${fileToolConsecutiveFailures} times`
                      : "write_file failed: empty content"
                  );
                  messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                  continue;
                }

                const fullPath = joinUnderCwd(path0);

                // Safe overwrite guard: require an explicit read_file before overwriting an existing file.
                try {
                  const st = await postJson("/api/stat_path", { path: fullPath }, ac.signal);
                  const exists = !!(st && st.exists);
                  if (exists && !fileReadSet.has(fullPath)) {
                    toolResult = [
                      "GOVERNOR BLOCKED",
                      "Refusing to overwrite an existing file that was not read in this session.",
                      "Required: call read_file on the target path first, then re-issue write_file.",
                      "path: " + String(path0 || fullPath),
                    ].join("\n");
                    pushRecentRun({ kind: "write_file", status: "FAIL", ok: false, path: path0 || fullPath, note: "blocked: not read before overwrite" });
                    messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                    agentState = "recovery";
                    governor.pendingHint = toolResult;
                    fileToolConsecutiveFailures += 1;
                    requireReflection(
                      fileToolConsecutiveFailures >= 2
                        ? `file tool failures repeated ${fileToolConsecutiveFailures} times`
                        : "write_file blocked before overwrite"
                    );
                    continue;
                  }
                } catch (_) {
                  // If stat fails, do not block the write; the server will still enforce path safety.
                }

                if (config.requireEditApproval) {
                  const q = await postJson("/api/queue_edit", {
                    action: "write_file",
                    path: fullPath,
                    content,
                  }, ac.signal);
                  const aid = String((q && q.approval_id) || "");
                  refreshPendingEdits();
                  toolResult = aid
                    ? `Awaiting approval via /api/approve_edit\napproval_id: ${aid}`
                    : "Awaiting approval via /api/approve_edit";
                  pushRecentRun({ kind: "write_file", status: "PENDING", path: path0 || fullPath, note: aid ? ("approval_id=" + aid) : "approval queued" });
                  messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
                  awaitingApproval = true;
                  break;
                }

                const wr = await postJson("/api/write_file", { path: fullPath, content }, ac.signal);
                toolResult = `OK write_file\nbytes_written: ${wr && wr.bytes_written != null ? wr.bytes_written : content.length}`;
                fileReadSet.add(fullPath);
                fileToolConsecutiveFailures = 0;
                requireImpact(`write_file succeeded: ${path0 || fullPath}`);
                pushRecentRun({ kind: "write_file", status: "OK", path: path0 || fullPath, note: `bytes=${wr && wr.bytes_written != null ? wr.bytes_written : content.length}` });
                messages.push({
                  role: "tool",
                  tool_call_id: tc.id,
                  content: compactSuccessToolResultForHistory("write_file", toolResult),
                });
              } catch (e2) {
                toolResult = `error: ${prettyErr(e2)}`;
                pushRecentRun({ kind: "write_file", status: "FAIL", ok: false, path: path0 || "(missing)", note: clip(prettyErr(e2), 120) });
                fileToolConsecutiveFailures += 1;
                requireReflection(
                  fileToolConsecutiveFailures >= 2
                    ? `file tool failures repeated ${fileToolConsecutiveFailures} times`
                    : firstDigestLine(toolResult) || "write_file failed"
                );
                messages.push({ role: "tool", tool_call_id: tc.id, content: toolResult });
              }
              flush();
              if (awaitingApproval) break;
              continue;
            }

            if (toolName === "read_file") {
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const path0 = String(args.path || "").trim();
              display += (display ? "\n\n" : "") + `📄 read_file: ${path0}`;
              flush();
              let toolResult;
              try {
                const fullPath = joinUnderCwd(path0);
                const res = await postJson("/api/read_file", { path: fullPath }, ac.signal);
                const raw = res && res.content ? res.content : `ERROR: empty response`;
                toolResult = rewriteToolPath(raw, fullPath, path0);
                if (res && res.content) {
                  fileReadSet.add(fullPath);
                  rememberObservationRead(`read_file(path=${path0})`, path0 || parseReadFileResultPath(toolResult));
                }
              } catch (e2) {
                toolResult = `ERROR reading '${path0}': ${prettyErr(e2)}`;
              }
              messages.push({
                role: "tool",
                tool_call_id: tc.id,
                content: /^ERROR /.test(String(toolResult || ""))
                  ? toolResult
                  : compactSuccessToolResultForHistory("read_file", toolResult),
              });
              flush();
              continue;
            }

            if (toolName === "list_dir") {
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const dir0raw = String(args.dir || "").trim();
              const dir0 = (dir0raw === "." || dir0raw === "./") ? "" : dir0raw;
              const maxEntries0 = Number(args.max_entries || args.maxEntries || 0) || 0;
              const max_entries = Math.max(0, Math.min(500, Math.round(maxEntries0)));
              const include_hidden = !!(args.include_hidden || args.includeHidden);
              const shown = dir0 ? dir0 : ".";
              display += (display ? "\n\n" : "") + `📁 list_dir: ${shown}`;
              flush();
              let toolResult;
              try {
                const fullDir = joinUnderCwd(dir0);
                const res = await postJson("/api/list_dir", { dir: fullDir, max_entries, include_hidden }, ac.signal);
                const raw = res && res.output ? res.output : `ERROR: empty response`;
                toolResult = String(raw || "")
                  .replace(`[list_dir: '${fullDir}'`, `[list_dir: '${shown}'`)
                  .trim();
              } catch (e2) {
                toolResult = `ERROR listing '${shown}': ${prettyErr(e2)}`;
              }
              messages.push({
                role: "tool",
                tool_call_id: tc.id,
                content: /^ERROR /.test(String(toolResult || ""))
                  ? toolResult
                  : compactSuccessToolResultForHistory("list_dir", toolResult),
              });
              flush();
              continue;
            }

            if (toolName === "patch_file") {
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const path0 = String(args.path || "").trim();
              const search = String(args.search || "");
              const replace = String(args.replace || "");
              display += (display ? "\n\n" : "") + `✎ patch_file: ${path0}`;
              flush();
              let toolResult;
              try {
                const fullPath = joinUnderCwd(path0);
                const res = await postJson("/api/patch_file", { path: fullPath, search, replace }, ac.signal);
                const raw = res && res.message ? res.message : "OK: patched";
                toolResult = rewriteToolPath(raw, fullPath, path0);
                fileToolConsecutiveFailures = 0;
                requireImpact(`patch_file succeeded: ${path0}`);
                pushRecentRun({ kind: "patch_file", status: "OK", path: path0, note: firstDigestLine(toolResult) || "" });
              } catch (e2) {
                toolResult = `ERROR patching '${path0}': ${prettyErr(e2)}`;
                fileToolConsecutiveFailures += 1;
                requireReflection(
                  fileToolConsecutiveFailures >= 2
                    ? `file tool failures repeated ${fileToolConsecutiveFailures} times`
                    : firstDigestLine(toolResult) || "patch_file failed"
                );
                pushRecentRun({ kind: "patch_file", status: "FAIL", ok: false, path: path0, note: clip(prettyErr(e2), 120) });
              }
              messages.push({
                role: "tool",
                tool_call_id: tc.id,
                content: /^ERROR /.test(String(toolResult || ""))
                  ? toolResult
                  : compactSuccessToolResultForHistory("patch_file", toolResult),
              });
              flush();
              continue;
            }

            if (toolName === "search_files") {
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const pattern = String(args.pattern || "").trim();
              const dir0raw = String(args.dir || "");
              const dir0 = (dir0raw === "." || dir0raw === "./") ? "" : dir0raw;
              const ci = !!args.case_insensitive;
              const shown = dir0 ? dir0 : ".";
              display += (display ? "\n\n" : "") + `🔍 search_files: ${pattern} (dir=${shown})`;
              flush();
              let toolResult;
              try {
                const dir = joinUnderCwd(dir0);
                const res = await postJson("/api/search_files", { pattern, dir, case_insensitive: ci }, ac.signal);
                toolResult = res && res.output ? res.output : `[search_files] No matches for '${pattern}'`;
                rememberObservationSearch(
                  `search_files(pattern=${pattern}, dir=${shown})`,
                  pattern,
                  parseSearchHitCount(toolResult),
                  parseSearchResultPaths(toolResult),
                );
              } catch (e2) {
                toolResult = `ERROR searching '${pattern}': ${prettyErr(e2)}`;
              }
              messages.push({
                role: "tool",
                tool_call_id: tc.id,
                content: /^ERROR /.test(String(toolResult || ""))
                  ? toolResult
                  : compactSuccessToolResultForHistory("search_files", toolResult),
              });
              flush();
              continue;
            }

            if (toolName === "apply_diff") {
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const path0 = String(args.path || "").trim();
              const diff = String(args.diff || "");
              const diffChars = diff.length;
              const hunks = (diff.match(/^@@/gm) || []).length;
              display += (display ? "\n\n" : "") + `⟁ apply_diff: ${path0} (${diffChars} chars, ${hunks} hunks)`;
              flush();
              let toolResult;
              try {
                const fullPath = joinUnderCwd(path0);
                const res = await postJson("/api/apply_diff", { path: fullPath, diff }, ac.signal);
                const raw = res && res.message ? res.message : "OK: diff applied";
                toolResult = rewriteToolPath(raw, fullPath, path0);
                fileToolConsecutiveFailures = 0;
                requireImpact(`apply_diff succeeded: ${path0}`);
                pushRecentRun({ kind: "apply_diff", status: "OK", path: path0, note: firstDigestLine(toolResult) || "" });
              } catch (e2) {
                toolResult = `ERROR applying diff to '${path0}': ${prettyErr(e2)}`;
                fileToolConsecutiveFailures += 1;
                requireReflection(
                  fileToolConsecutiveFailures >= 2
                    ? `file tool failures repeated ${fileToolConsecutiveFailures} times`
                    : firstDigestLine(toolResult) || "apply_diff failed"
                );
                pushRecentRun({ kind: "apply_diff", status: "FAIL", ok: false, path: path0, note: clip(prettyErr(e2), 120) });
              }
              messages.push({
                role: "tool",
                tool_call_id: tc.id,
                content: /^ERROR /.test(String(toolResult || ""))
                  ? toolResult
                  : compactSuccessToolResultForHistory("apply_diff", toolResult),
              });
              flush();
              continue;
            }

            if (toolName === "glob") {
              let args;
              try { args = JSON.parse(tc.function.arguments || "{}"); } catch (_) { args = {}; }
              const pattern = String(args.pattern || "").trim();
              const dir0raw = String(args.dir || "");
              const dir0 = (dir0raw === "." || dir0raw === "./") ? "" : dir0raw;
              const shown = dir0 ? dir0 : ".";
              display += (display ? "\n\n" : "") + `❖ glob: ${pattern} (dir=${shown})`;
              flush();
              let toolResult;
              try {
                const dir = joinUnderCwd(dir0);
                const res = await postJson("/api/glob_files", { pattern, dir }, ac.signal);
                toolResult = res && res.output ? res.output : `[glob] No files matching '${pattern}'`;
              } catch (e2) {
                toolResult = `ERROR glob '${pattern}': ${prettyErr(e2)}`;
              }
              messages.push({
                role: "tool",
                tool_call_id: tc.id,
                content: /^ERROR /.test(String(toolResult || ""))
                  ? toolResult
                  : compactSuccessToolResultForHistory("glob", toolResult),
              });
              flush();
              continue;
            }

            // Unknown tool — ignore, but keep the model informed.
            messages.push({ role: "tool", tool_call_id: tc.id, content: `error: unknown tool: ${toolName}` });
          }
          if (doneNow) break;
          if (awaitingApproval) break;
        } else {
          // finish_reason === "stop" — if the model didn't produce tool calls, try implied scripts.
          if (reflectionRequired) {
            const block = reflectionStopMessage(governorContract, reflectionRequired);
            blockWithoutToolCalls(block);
            continue;
          }
          if (impactRequired) {
            const block = impactStopMessage(governorContract, impactRequired);
            blockWithoutToolCalls(block);
            continue;
          }
          const implied = extractImpliedExecScripts(asstText);
          if (!implied.length) {
            // Goal delta check: the model may "stop" even though the deliverable isn't actually complete.
            // In longrun mode (and when command approval is OFF), auto-run a small set of checks (repo/tests/build).
            try {
              if (await runGoalChecksOnStop()) continue;
            } catch (_) {
              // If goal checks fail, don't hard-block completion; fall through to break.
            }
            break;
          }
          agentState = "executing";
          for (const im of implied) {
            if (ac.signal.aborted) break;
            const command = stripShellTranscript(String(im.script || ""));
            if (!command) continue;
            const commandToRun = normalizeExecScript(im.langHint, command);
            const k = cmdKey(commandToRun);
            const repeatBlock = blockedByRepeatFailure(k);
            const danger = dangerousCommandReason(commandToRun);
            const win = isWindowsHost();
            const fenceLang = win ? "powershell" : "bash";
            const prompt = win ? "PS> " : "$ ";
            const shown = command.split("\n").map((l, i) => (i === 0 ? prompt : "    ") + l).join("\n");
            display += (display ? "\n\n" : "") + "```" + fenceLang + "\n" + shown;
            flush("\n```");

            let resultText;
            try {
              if (repeatBlock) {
                resultText = `error: blocked repeated failing command (${repeatBlock}). You MUST choose a different command/strategy.`;
                pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: -1, ok: false, cls: "blocked", err: clip(repeatBlock, 120) });
                agentState = "recovery";
                display += `\n(blocked: ${repeatBlock})\n\`\`\`\nexit: -1`;
                flush();
                messages.push({ role: "user", content: `[exec blocked repeated-failure]\n${repeatBlock}\ncommand:\n${commandToRun}` });
                break;
              }
              if (danger) {
                resultText = `error: blocked dangerous command (${danger}). Ask the user to run it manually if truly intended.`;
                pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: -1, ok: false, cls: "blocked", err: clip(danger, 120) });
                agentState = "recovery";
                display += `\n(blocked: ${danger})\n\`\`\`\nexit: -1`;
                flush();
                messages.push({ role: "user", content: `[exec blocked]\nreason: ${danger}\ncommand:\n${commandToRun}` });
                break;
              }
              if (config.requireCommandApproval) {
                const canQueue = !!(status && status.features && status.features.pending_commands);
                if (canQueue) {
                  try {
                    const cwdUsed = cwdNow();
                    const q = await postJson("/api/queue_command", { command: commandToRun, cwd: cwdUsed }, ac.signal);
                    const aid = String((q && q.approval_id) || "");
                    refreshPendingCommands();
                    resultText = aid
                      ? `Awaiting approval via /api/approve_command\napproval_id: ${aid}`
                      : "Awaiting approval via /api/approve_command";
                    pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: -1, ok: false, cls: "approval", err: aid ? ("queued:" + aid) : "queued" });
                    display += aid
                      ? `\n(queued for approval: ${aid})\n\`\`\`\nexit: -1`
                      : "\n(queued)\n```\nexit: -1";
                    flush();
                    messages.push({
                      role: "user",
                      content: [
                        "[exec awaiting approval]",
                        aid ? ("approval_id: " + aid) : "",
                        "command:\n" + commandToRun,
                      ].filter(Boolean).join("\n"),
                    });
                    awaitingApproval = true;
                    break;
                  } catch (_) {
                    // Fall through to confirm() for older servers / transient failures.
                  }
                }
                if (!window.confirm("Run command?\n\n" + commandToRun)) {
                  resultText = "error: command rejected by user";
                  pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: -1, ok: false, cls: "rejected", err: "rejected by user" });
                  display += "\n(rejected)\n```\nexit: -1";
                  flush();
                  messages.push({ role: "user", content: `[exec rejected]\ncommand:\n${commandToRun}` });
                  break;
                }
              }

              const cwdUsed = cwdNow();
              const execCmd = wrapExecWithPwd(commandToRun);
              const execRes = await postJson("/api/exec", { command: execCmd, cwd: cwdUsed }, ac.signal);
              const parsed = stripPwdMarker(execRes.stdout);
              const breach = sandboxBreachReason(parsed.pwd);
              maybeUpdateWorkdirFromPwd(parsed.pwd);
	              const stdoutRaw = String(parsed.stdout || "");
	              const stderrRaw = String(execRes.stderr || "");
	              const stdout = truncToolTail(stdoutRaw, TRUNC_STDOUT);
	              const stderr = truncToolTail(stderrRaw, TRUNC_STDERR);
	              const exitCode = execRes.exit_code;
	              const durationMs = Math.max(0, Math.round(Number(execRes.duration_ms) || 0));
	              const suspicious = (exitCode === 0) ? suspiciousSuccessReason(stdoutRaw, stderrRaw) : "";
	              const failed = exitCode !== 0 || !!suspicious || !!breach;
              noteCmd(k, failed, errorLineSig(stdout, stderr));
              const hintGit = failed ? gitRepoHint(stderrRaw) : "";
              const hintGov = failed ? deriveGovernorHint(stderrRaw, stdoutRaw) : "";
              const errClass = failed ? classifyErrorClass(stderrRaw, stdoutRaw) : "";
              const hintClass = failed ? errorClassHint(errClass) : "";
              const digest = failed ? extractErrorDigest(stdoutRaw, stderrRaw) : "";
              const errLine = failed ? (firstDigestLine(digest) || errorLineSig(stdout, stderr) || "") : "";
              pushRecentRun({ kind: "exec", cmd: commandSig(commandToRun), exit: exitCode, ok: !failed, cls: errClass, err: errLine });
              const hintSandbox = breach ? [
                "SANDBOX BREACH: command ended outside tool_root.",
                breach,
                "Fix: re-run under tool_root; avoid `cd ..` / absolute paths. Verify `pwd` stays under tool_root.",
              ].join("\n") : "";
              const hint = [hintGit, hintGov, hintSandbox, suspicious ? ("SUSPICIOUS_SUCCESS: " + suspicious) : ""].filter(Boolean).join("\n\n");
              const prefix = [hintClass, digest].filter(Boolean).join("\n\n");
              const prefixText = prefix ? (prefix + "\n\n") : "";

              const cwdUsedLabel = cwdUsed ? String(cwdUsed) : "(workspace root)";
              const cwdAfter = cwdNow();
              const cwdAfterLabel = cwdAfter ? String(cwdAfter) : "(workspace root)";
              const cwdLine = cwdUsedLabel === cwdAfterLabel
                ? `cwd: ${cwdUsedLabel}`
                : `cwd: ${cwdUsedLabel}\ncwd_after: ${cwdAfterLabel}`;

              if (failed) {
                governor.consecutiveFailures++;

                const cmdSigNow = commandSig(commandToRun);
                if (cmdSigNow && cmdSigNow === governor.lastCmdSig) governor.sameCmdRepeats++;
                else { governor.lastCmdSig = cmdSigNow; governor.sameCmdRepeats = 1; }

                const sig0 = errorSignature(commandToRun, stdout, stderr, exitCode);
                const sig = sig0 || (breach ? String(breach).slice(0, 180) : "");
                if (sig && sig === governor.lastErrSig) governor.sameErrRepeats++;
                else { governor.lastErrSig = sig; governor.sameErrRepeats = 1; }
                const outHash = fnv1a64(String(stderr || "") + "\n" + String(stdout || ""));
                if (outHash === governor.lastOutHash) governor.sameOutRepeats++;
                else { governor.lastOutHash = outHash; governor.sameOutRepeats = 1; }

                // Emit hints only when crossing key thresholds to avoid spamming context.
                let stuckReason = "";
                if (governor.sameErrRepeats === 2) stuckReason = "The SAME error happened twice.";
                else if (governor.sameCmdRepeats === 3) stuckReason = "You ran the SAME command 3 times.";
                else if (governor.consecutiveFailures === 3) stuckReason = "3 consecutive failures.";
                else if (governor.sameOutRepeats === 2 && governor.sameCmdRepeats >= 2) stuckReason = "Stuck detected: repeated identical output.";

                if (stuckReason) {
                  governor.pendingDiag = stuckReason;
                  const stuck = [
                    stuckReason,
                    governor.lastErrSig ? ("last_error_signature: " + governor.lastErrSig) : "",
                    "Action: stop repeating. Run diagnostics (pwd, ls, git status), then change strategy.",
                    hintGov || hintGit || "",
                    "First verify cwd/tool_root, then pick a different command.",
                  ].filter(Boolean).join("\n");
                  governor.pendingHint = breach ? (hintSandbox + "\n\n" + stuck) : stuck;
                } else if (breach) {
                  governor.pendingHint = hintSandbox;
                }
              } else {
                governor.consecutiveFailures = 0;
                governor.lastCmdSig = "";
                governor.sameCmdRepeats = 0;
                governor.lastErrSig = "";
                governor.sameErrRepeats = 0;
                governor.lastOutHash = 0n;
                governor.sameOutRepeats = 0;
                governor.pendingDiag = "";
                if (isVerificationCommand(commandToRun, governorContract)) {
                  rememberKnownVerificationCommand(commandToRun);
                }
              }

	              resultText = failed
	                ? `${prefixText}FAILED (exit_code: ${exitCode}).\nduration_ms: ${durationMs}\n${cwdLine}\nstderr: ${stderr || "(empty)"}\nstdout: ${stdout || "(empty)"}${hint ? ("\n\n" + hint) : ""}`
	                : `OK (exit_code: 0)\nduration_ms: ${durationMs}\n${cwdLine}\nstdout: ${stdout || "(empty)"}`;

	              if (stdout) display += "\n" + stdout;
	              if (stderr) display += "\nstderr: " + stderr;
	              display += "\n```\nexit: " + exitCode + "\nduration_ms: " + durationMs;
	              flush();

              const nextInstr = failed
                ? "⚠ The command failed. Diagnose the error above and output a FIX command as ONE code block. Do NOT continue to the next step until this succeeds."
                : "Continue by outputting the NEXT command(s) as ONE code block (or use exec tool calls if supported).";
              messages.push({
                role: "user",
                content: [
                  "[exec result]",
                  "command:",
                  commandToRun,
                  resultText,
                  nextInstr,
                ].join("\n"),
              });
            } catch (execErr) {
              noteCmd(k, true, normalizeForSig(execErr.message || ""));
              resultText = `error: ${execErr.message}`;
              display += "\nerror: " + execErr.message + "\n```";
              flush();
              messages.push({ role: "user", content: `[exec error]\n${execErr.message}` });
              break;
            }
          }
          // In longrun autonomy mode, continue the agent loop so non-tool-calling models can still
          // iterate (command -> result -> next command) without user nudges.
          if (awaitingApproval) break;
          if (longrun) continue;
          break; // implied exec done — in non-longrun mode, stop to avoid notification spam
        }
      }

      // Persist per-thread failure memory once per run to avoid re-render churn.
      persistFailureMemory();
      finishStreaming(threadId, asstMsgId);
    };

      const sendCoder = async (overrideText, options = {}) => {
        if (sendingCoder) return;
        const raw = overrideText != null ? String(overrideText) : String(coderInput || "");
        const text = raw.trim();
        if (!text) return;

        // Local slash commands (UI-side). These do not call the model.
        // They are useful when using the Mistral VIBE CLI provider (mistral-cli).
        if (overrideText == null && text.startsWith("/")) {
          const parts = text.split(/\s+/);
          const cmd = String(parts[0] || "").trim().toLowerCase();
          const arg = parts.slice(1).join(" ").trim();
          if (cmd === "/agent") {
            setConfig((c) => ({ ...c, mistralCliAgent: arg }));
            const msg = { id: uid(), pane: "coder", role: "assistant", content: `[Spiral-Coder] vibe agent = ${arg || "(default)"}`, ts: Date.now() };
            setThreadState((s) => ({
              ...s,
              threads: s.threads.map((t) => (t.id === activeThread.id ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), msg] } : t)),
            }));
            setCoderInput("");
            return;
          }
          if (cmd === "/turns") {
            const n = String(arg || "").trim();
            setConfig((c) => ({ ...c, mistralCliMaxTurns: n }));
            const msg = { id: uid(), pane: "coder", role: "assistant", content: `[Spiral-Coder] vibe max_turns = ${n || "(default)"}`, ts: Date.now() };
            setThreadState((s) => ({
              ...s,
              threads: s.threads.map((t) => (t.id === activeThread.id ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), msg] } : t)),
            }));
            setCoderInput("");
            return;
          }
          if (cmd === "/scaffold") {
            const canExec = !!(status && status.features && status.features.exec);
            if (!canExec) {
              const msg = { id: uid(), pane: "coder", role: "assistant", content: "[Spiral-Coder] /scaffold requires /api/exec", ts: Date.now() };
              setThreadState((s) => ({
                ...s,
                threads: s.threads.map((t) => (t.id === activeThread.id ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), msg] } : t)),
              }));
              setCoderInput("");
              return;
            }

            const name0 = String(arg || "").trim();
            const safe = name0
              .replace(/[\\/:*?\"<>|]/g, "_")
              .replace(/\s+/g, "-")
              .replace(/-+/g, "-")
              .replace(/^\.+/g, "")
              .replace(/_+/g, "_")
              .trim();
            if (!safe) {
              const msg = { id: uid(), pane: "coder", role: "assistant", content: "[Spiral-Coder] usage: /scaffold <repo-name>", ts: Date.now() };
              setThreadState((s) => ({
                ...s,
                threads: s.threads.map((t) => (t.id === activeThread.id ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), msg] } : t)),
              }));
              setCoderInput("");
              return;
            }

            const baseCwd = resolvedThreadRoot(config.toolRoot, activeThread && activeThread.id);
            const readmeEn = [
              `# ${safe}`,
              "",
              "Generated by Spiral-Coder `/scaffold`.",
              "",
              "## What is this?",
              "A minimal repository scaffold so you can start coding immediately.",
              "",
              "## Structure",
              "- `src/`  Code goes here",
              "- `docs/` Notes, screenshots, design docs",
              "",
              "## Publish (optional)",
              "```powershell",
              "git remote add origin <YOUR_URL>",
              "git push -u origin main",
              "```",
              "",
              "## README translations",
              "- Japanese: README.ja.md",
              "- French: README.fr.md",
              "",
            ];
            const readmeJa = [
              `# ${safe}`,
              "",
              "Spiral-Coder `/scaffold` により生成されました。",
              "",
              "## これは何？",
              "すぐに開発を開始できる最小のリポジトリ雛形です。",
              "",
              "## 構成",
              "- `src/`  コードを置く",
              "- `docs/` メモ、スクショ、設計ドキュメント",
              "",
              "## 公開（任意）",
              "```powershell",
              "git remote add origin <YOUR_URL>",
              "git push -u origin main",
              "```",
              "",
              "## README翻訳",
              "- English: README.md",
              "- French: README.fr.md",
              "",
            ];
            const readmeFr = [
              `# ${safe}`,
              "",
              "Généré par Spiral-Coder `/scaffold`.",
              "",
              "## C'est quoi ?",
              "Un modèle minimal pour démarrer un projet immédiatement.",
              "",
              "## Structure",
              "- `src/`  Code",
              "- `docs/` Notes, captures, docs de conception",
              "",
              "## Publier (optionnel)",
              "```powershell",
              "git remote add origin <YOUR_URL>",
              "git push -u origin main",
              "```",
              "",
              "## Traductions du README",
              "- Anglais : README.md",
              "- Japonais : README.ja.md",
              "",
            ];
            const readmeEnPs = "@(" + readmeEn.map(psSingleQuote).join(",") + ") | Set-Content -LiteralPath 'README.md' -Encoding UTF8";
            const readmeJaPs = "@(" + readmeJa.map(psSingleQuote).join(",") + ") | Set-Content -LiteralPath 'README.ja.md' -Encoding UTF8";
            const readmeFrPs = "@(" + readmeFr.map(psSingleQuote).join(",") + ") | Set-Content -LiteralPath 'README.fr.md' -Encoding UTF8";
            const gitignoreLines = [
              "# Spiral-Coder scaffold",
              ".DS_Store",
              "node_modules/",
              "dist/",
              "target/",
              ".venv/",
              "__pycache__/",
              "*.log",
              "",
            ];
            const gitignorePs = "@(" + gitignoreLines.map(psSingleQuote).join(",") + ") | Set-Content -LiteralPath '.gitignore' -Encoding UTF8";
            const cmdPs = [
              "$ErrorActionPreference = 'Stop'",
              `New-Item -ItemType Directory -Force -Path ${psSingleQuote(safe)} | Out-Null`,
              `Set-Location ${psSingleQuote(safe)}`,
              "New-Item -ItemType Directory -Force -Path 'src' | Out-Null",
              "New-Item -ItemType Directory -Force -Path 'docs' | Out-Null",
              readmeEnPs,
              readmeJaPs,
              readmeFrPs,
              gitignorePs,
              "git init | Out-Null",
              "git branch -M main | Out-Null",
              "$n = (git config user.name); if (-not $n) { git config user.name 'Spiral-Coder' }",
              "$e = (git config user.email); if (-not $e) { git config user.email 'spiral-coder@local' }",
              "git add README.md README.ja.md README.fr.md .gitignore | Out-Null",
              "git commit -m 'Initial commit' | Out-Null",
            ].join("; ");

            try {
              const res = await postJson("/api/exec", { command: cmdPs, cwd: baseCwd });
              const ok = (res && typeof res.exit_code === "number") ? res.exit_code === 0 : false;
              const out = ok
                ? `[Spiral-Coder] scaffolded repo: ${safe} (cwd=${String(res.cwd || baseCwd || "")})`
                : `[Spiral-Coder] scaffold failed (exit_code=${res.exit_code}). stderr: ${String(res.stderr || "(empty)")}`;
              const msg = { id: uid(), pane: "coder", role: "assistant", content: out, ts: Date.now() };
              setThreadState((s) => ({
                ...s,
                threads: s.threads.map((t) => (
                  t.id === activeThread.id
                    ? { ...t, updatedAt: Date.now(), workdir: ok ? safe : (t.workdir || ""), messages: [...(t.messages || []), msg] }
                    : t
                )),
              }));
            } catch (err) {
              const msg = { id: uid(), pane: "coder", role: "assistant", content: `[Spiral-Coder] scaffold error: ${prettyErr(err)}`, ts: Date.now() };
              setThreadState((s) => ({
                ...s,
                threads: s.threads.map((t) => (t.id === activeThread.id ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), msg] } : t)),
              }));
            }
            setCoderInput("");
            return;
          }
          if (cmd === "/meta-diagnose") {
            setCoderInput("");
            setObserverSubTab("analysis");
            await runMetaDiagnose(arg || "last-fail");
            return;
          }
        }

        const coderCfg = String(config.mode || "").trim() === "Observer" ? { ...config, mode: "VIBE" } : config;
        if (String(config.mode || "").trim() === "Observer") {
          setConfig((c) => ({ ...c, mode: "VIBE" }));
        }

        const wantsMaterial = /(?:repo|repository|scaffold|bootstrap|init|setup|create|generate|implement|install|build|test|run|git|winget|bash|powershell|cmd|command|readme|license|requirements|folder|directory|depot|dépôt|referentiel|référentiel|projet|dossier|répertoire|repertoire|fichier|commande|créer|cree|crée|générer|generer|implément|implementer|exécuter|execute|installer|リポ|リポジトリ|雛形|ひな形|フォルダ|ふぉるだ|ディレクトリ|でぃれくとり|プロジェクト|実装|ファイル|作成|作ろ|作って|生成|追加|組み込|編集|更新|書いて|書き直して|コマンド|実行|インストール|セットアップ|ビルド|テスト|自分で|じぶんで|やって|やってみて)/i.test(text);
        const useCode = modeUsesCode(coderCfg.mode) || wantsMaterial;
        const resolvedProvider = useCode ? (String(coderCfg.codeProvider || "").trim() || coderCfg.provider) : coderCfg.provider;
        const resolvedBaseUrl = useCode ? (String(coderCfg.codeBaseUrl || "").trim() || coderCfg.baseUrl) : coderCfg.baseUrl;
        const resolvedModel = useCode ? (coderCfg.codeModel || coderCfg.model) : (coderCfg.chatModel || coderCfg.model);
        const resolvedKey = useCode
          ? (String(codeApiKey || "").trim() || String(chatApiKey || "").trim() || String(observerApiKey || "").trim())
          : (String(chatApiKey || "").trim() || String(codeApiKey || "").trim() || String(observerApiKey || "").trim());
        const reqCfg = {
          ...coderCfg,
          provider: resolvedProvider,
          baseUrl: resolvedBaseUrl,
          model: resolvedModel,
          chatModel: resolvedModel,
          codeModel: resolvedModel,
        };

        const threadId = activeThread.id;
        const coderHistory = paneMessages("coder");
        const origin = options.origin === "runtime" ? "runtime" : "user";
        const rootUserText = rootUserTextForRun(text, coderHistory, origin);
        const history = coderHistory.map((m) => ({ role: m.role, content: m.content }));

      const userMsg = { id: uid(), pane: "coder", role: "user", origin, content: text, ts: Date.now() };
      const asstMsg = { id: uid(), pane: "coder", role: "assistant", content: "", ts: Date.now(), streaming: true };

      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) =>
          t.id === threadId ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), userMsg, asstMsg] } : t
        ),
      }));
      ensureTitle(threadId, text);
      setCoderInput("");
      setSendingCoder(true);
      requestAnimationFrame(() => scrollBottom(coderBodyRef));

      const toolRootResolved = resolvedCwd(config.toolRoot, threadId, activeThread && activeThread.workdir);
      const reqCfg2 = toolRootResolved ? { ...reqCfg, toolRoot: toolRootResolved } : reqCfg;
      const reqBody = buildReq(reqCfg2, resolvedKey, history, text, diff);
      reqBody.lang = lang;
      reqBody.force_tools = !!(useCode || wantsMaterial);
      const ac = new AbortController();
      abortCoderRef.current = ac;

      try {
          const supportsTools = resolvedProvider === "openai-compatible" || resolvedProvider === "mistral" || resolvedProvider === "openai";
          const serverChatTools = !!(status && status.features && status.features.chat_tools);
          if ((config.forceAgent || wantsMaterial) && supportsTools && serverChatTools) {
            await runCoderAgentic(text, threadId, asstMsg.id, reqCfg, resolvedKey, history, ac, activeThread && activeThread.workdir, rootUserText);
          } else if (config.stream) {
          await streamChat(
            reqBody,
            (evt) => {
              if (!evt) return;
              if (evt.event === "delta") {
                const j = safeJsonParse(evt.data || "{}", {});
                if (j && j.delta) appendDelta(threadId, asstMsg.id, j.delta, coderBodyRef);
              } else if (evt.event === "error") {
                const j = safeJsonParse(evt.data || "{}", {});
                throw new Error(j.error || tr(lang, "error"));
              }
            },
            ac.signal
          );
          finishStreaming(threadId, asstMsg.id);
        } else {
          const j = await postJson("/api/chat", reqBody, ac.signal);
          setMsg(threadId, asstMsg.id, String((j && j.content) || ""), coderBodyRef);
        }
      } catch (err) {
        const msg = prettyErr(err);
        if (config.stream && !ac.signal.aborted) {
          try {
            const j = await postJson("/api/chat", reqBody, ac.signal);
            setMsg(threadId, asstMsg.id, String((j && j.content) || ""), coderBodyRef);
          } catch (err2) {
            const msg2 = prettyErr(err2);
            setMsg(threadId, asstMsg.id, `[${tr(lang, "error")}] ${msg2}`, coderBodyRef);
          }
        } else if (ac.signal.aborted) {
          setMsg(threadId, asstMsg.id, `[${tr(lang, "stop")}]`, coderBodyRef);
        } else {
          setMsg(threadId, asstMsg.id, `[${tr(lang, "error")}] ${msg}`, coderBodyRef);
        }
      } finally {
        setSendingCoder(false);
        abortCoderRef.current = null;
        refreshPendingEdits();
      }
    };

    const runObserverEngine = async () => {
      if (sendingObserver) return;
      if (!activeThread) return;

      const supported = !!(status && status.features && status.features.observer_engine);
      if (!supported) {
        showToast(lang === "fr" ? "Observer engine non disponible sur ce serveur." : lang === "en" ? "Observer engine not available on this server." : "このサーバではObserver engineが使えません。", "info");
        return;
      }

      const threadId = activeThread.id;
      const coderMsgs = paneMessages("coder").filter((m) => m && !m.streaming);
      if (!coderMsgs.length) {
        showToast(lang === "fr" ? "Aucune activité du Coder à analyser." : lang === "en" ? "No Coder activity to analyze." : "分析できるCoderの履歴がありません。", "info");
        return;
      }

      const recent = coderMsgs.slice(-10);
      const transcript = recent
        .map((m) => {
          const who = m.role === "user" ? "User" : "Coder";
          return `${who}:\n${String(m.content || "").trimEnd()}`;
        })
        .join("\n\n")
        .trim();

      if (!transcript) {
        showToast(lang === "fr" ? "Transcript vide." : lang === "en" ? "Empty transcript." : "Transcriptが空です。", "info");
        return;
      }

      const userText =
        lang === "fr"
          ? "[ENGINE] Analyse déterministe des actions récentes du Coder."
          : lang === "en"
            ? "[ENGINE] Deterministic analysis of the Coder's recent actions."
            : "[ENGINE] Coderの直近行動をdeterministicに解析。";

      const userMsg = { id: uid(), pane: "observer", role: "user", content: userText, ts: Date.now() };
      const asstId = uid();
      const asstMsg = { id: asstId, pane: "observer", role: "assistant", content: "", ts: Date.now(), streaming: true };

      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) =>
          t.id === threadId ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), userMsg, asstMsg] } : t
        ),
      }));
      setSendingObserver(true);
      requestAnimationFrame(() => scrollBottom(observerBodyRef));

      const ac = new AbortController();
      abortObserverRef.current = ac;
      try {
        const mem0 = (activeThread && activeThread.observerMem && typeof activeThread.observerMem === "object") ? activeThread.observerMem : { proposal_counts: {} };
        const resp = await postJson("/api/observer_engine", { transcript, memory: mem0 }, ac.signal);
        const formatted = resp && resp.formatted ? String(resp.formatted || "") : "";
        const mem1 = (resp && resp.memory && typeof resp.memory === "object") ? resp.memory : mem0;

        setThreadState((s) => ({
          ...s,
          threads: s.threads.map((t) => {
            if (t.id !== threadId) return t;
            const msgs = (t.messages || []).map((m) => (m.id === asstId ? { ...m, content: formatted || "…", streaming: false } : m));
            return { ...t, updatedAt: Date.now(), observerMem: mem1, messages: msgs };
          }),
        }));
        requestAnimationFrame(() => scrollBottom(observerBodyRef));
      } catch (err) {
        const msg = prettyErr(err);
        setMsg(threadId, asstId, `[${tr(lang, "error")}] ${msg}`, observerBodyRef);
      } finally {
        setSendingObserver(false);
        abortObserverRef.current = null;
      }
    };

    const sendObserver = async (overrideText) => {
      if (sendingObserver) return;
      const text = overrideText != null ? String(overrideText) : String(observerInput || "").trim();
      if (!text) return;

      const threadId = activeThread.id;
      const history = observerConversationMessages().map((m) => ({ role: m.role, content: m.content }));
      const prevObserverAssts = (() => {
        const out = [];
        for (let i = history.length - 1; i >= 0 && out.length < 4; i--) {
          const m = history[i];
          if (m && m.role === "assistant") out.push(String(m.content || ""));
        }
        return out;
      })();

      const userMsg = { id: uid(), pane: "observer", role: "user", content: text, ts: Date.now() };
      const asstMsg = { id: uid(), pane: "observer", role: "assistant", content: "", ts: Date.now(), streaming: true };

      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) =>
          t.id === threadId ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), userMsg, asstMsg] } : t
        ),
      }));
      if (overrideText == null) setObserverInput("");
      setSendingObserver(true);
      requestAnimationFrame(() => scrollBottom(observerBodyRef));

      // Keep Observer in-character: do not inherit coder's CoT/autonomy formatting.
      const obsProvider = String(config.observerProvider || "").trim() || config.provider;
      const obsBaseUrl = String(config.observerBaseUrl || "").trim() || config.baseUrl;
      const obsModel = String(config.observerModel || "").trim() || (config.chatModel || config.model);
      const obsKey = String(observerApiKey || "").trim() || String(chatApiKey || "").trim() || String(codeApiKey || "").trim();
      const obsCfg = {
        ...config,
        mode: config.observerMode,
        persona: config.observerPersona,
        cot: "off",
        autonomy: "off",
        provider: obsProvider,
        baseUrl: obsBaseUrl,
        model: obsModel,
        chatModel: obsModel,
        codeModel: obsModel,
      };
      const intensity0 = String(config.observerIntensity || "critical").trim().toLowerCase();
      const intensity = intensity0 === "polite" || intensity0 === "critical" || intensity0 === "brutal" ? intensity0 : "critical";
      let intensityInstr = "";
      if (intensity === "polite") {
        intensityInstr = [
          "Intensity: polite. Be constructive and encouraging.",
          "Still flag concrete issues across all five dimensions (correctness/security/reliability/performance/maintainability).",
          "Every proposal must include a specific, actionable to_coder message.",
          "Anti-loop: flag NEW issues only. If nothing new, summarise still-open items from prior critiques as [OPEN].",
        ].join("\n");
      } else if (intensity === "brutal") {
        intensityInstr = [
          "Intensity: brutal. Assume this code ships to 10,000 users at midnight. Find every failure mode.",
          "Required: identify at least TWO failure modes — one correctness/data bug and one operational risk (monitoring, rollback, config drift).",
          "Required: every proposal must include a specific to_coder message and realistic impact estimate.",
          "Forbidden: 'looks good', 'nice work', 'could consider'. Only concrete flaws with concrete fixes.",
          "Anti-loop: if a prior issue is still unresolved, escalate its score by 10 and mark [ESCALATED]. New findings only otherwise.",
        ].join("\n");
      } else {
        intensityInstr = [
          "Intensity: critical. Treat this as a pre-merge review for a production service.",
          "Required: identify at least ONE concrete bug, security risk, or architectural weakness with a specific to_coder message.",
          "Check for: missing input validation, unhandled errors, hardcoded values, and missing test coverage.",
          "Anti-loop: if you raised this issue before, mark it [UNRESOLVED] and move on. No new signal → reply exactly: [Observer] No new critique. Loop detected.",
        ].join("\n");
      }
      const loopLine =
        loopInfo && loopInfo.depth > 0
          ? `ui_loop_detected: depth=${loopInfo.depth} sim=${Math.round((loopInfo.score || 0) * 100)}%`
          : "";
      const inferLangFromText = (s) => {
        const x = String(s || "");
        const jp = (x.match(/[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff]/g) || []).length;
        if (jp >= 1) return "ja";
        const acc = (x.match(/[\u00C0-\u017F]/g) || []).length;
        const fr = (x.match(/\b(le|la|les|des|du|de|pour|avec|sans|est|sont|pas|mais|donc|sur|dans|vous|tu|je|nous|votre)\b/gi) || []).length;
        if (acc > 0 || fr >= 2) return "fr";
        const en = (x.match(/\b(the|and|or|to|of|in|for|with|is|are|you|we|i|this|that|it)\b/gi) || []).length;
        // Do NOT infer English from "latin letters" alone: code blocks are mostly ASCII.
        if (en >= 2) return "en";
        return "";
      };
      const pickLastUserSample = () => {
        try {
          const pickLastUser = (pane) => {
            const msgs = pane ? paneMessages(pane) : (activeThread && activeThread.messages) || [];
            if (!msgs || !msgs.length) return "";
            for (let i = msgs.length - 1; i >= 0; i--) {
              const m = msgs[i];
              if (!m || m.role !== "user") continue;
              const t = String(m.content || "").trim();
              if (t) return t;
            }
            return "";
          };
          return pickLastUser("coder") || pickLastUser("chat") || pickLastUser("");
        } catch (_) {
          return "";
        }
      };
      const outLang = (() => {
        const ol0 = String(config.observerLang || "ui").trim().toLowerCase();
        const isAutoObserve = overrideText != null;
        const pickLangSample = () => {
          // Prefer a *typed* Observer prompt (not auto-observe). Auto-observe prompts are often in
          // UI language (e.g. English), which would incorrectly force the Observer output language.
          const a = String(text || "").trim();
          if (a && !isAutoObserve) return a;
          return pickLastUserSample();
        };
        if (ol0 === "auto") {
          const sample = pickLangSample();
          const sampleTrim = String(sample || "").trim();
          if (!sampleTrim) return String(lang || "ja").trim().toLowerCase();
          const inferred = inferLangFromText(sampleTrim);
          return inferred || String(lang || "ja").trim().toLowerCase();
        }
        if (ol0 === "ja" || ol0 === "en" || ol0 === "fr") return ol0;

        // ol0 === "ui" (or unknown): follow UI language, but if the UI is English/French and the user is
        // clearly typing in another language, prefer the user's language to avoid "Observer stuck in English".
        const uiLang = String(lang || "ja").trim().toLowerCase();
        if (uiLang === "en" || uiLang === "fr") {
          const sample = String(pickLangSample() || "").trim();
          if (sample) {
            const inferred = inferLangFromText(sample);
            if (inferred && inferred !== uiLang) return inferred;
          }
        }
        return uiLang;
      })();
      const proposalKeysLine = "Keep proposals block keys in English (title/to_coder/severity/score/phase/impact/cost).";
      const langLine = outLang === "fr"
        ? [
            "Language: French.",
            "Langue: français.",
            "Write the critique in French.",
            "Écris la critique en français.",
            "Do not write in English.",
            "N'écris pas en anglais.",
            proposalKeysLine,
            "Garde les clés du bloc proposals en anglais (title/to_coder/severity/score/phase/impact/cost).",
          ].join("\n")
        : outLang === "en"
          ? [
              "Language: English.",
              "Write the critique in English.",
              proposalKeysLine,
            ].join("\n")
          : [
              "Language: Japanese.",
              "言語: 日本語。",
              "Write the critique in Japanese.",
              "批評は日本語で書いてください。",
              "Do not write in English.",
              "英語で書かないでください。",
              proposalKeysLine,
              "proposalsブロックのキー(title/to_coder/severity/score/phase/impact/cost)は英語のままにしてください。",
            ].join("\n");

      // Localize the intensity guidelines to reduce "Observer stuck in English" bias.
      if (outLang === "ja") {
        if (intensity === "polite") {
          intensityInstr = [
            "強度: polite（丁寧）。建設的かつ前向きに。",
            "ただし5軸（正しさ/セキュリティ/信頼性/性能/保守性）で具体的な指摘は必ず入れる。",
            "各proposalのto_coderは具体的な1アクションにする。",
            "アンチループ: 新規の指摘のみ。新規が無ければ過去の未解決事項を[OPEN]で要約。",
          ].join("\n");
        } else if (intensity === "brutal") {
          intensityInstr = [
            "強度: brutal（容赦なし）。今夜0時に1万人へ出荷される前提で潰す。",
            "必須: 失敗モードを最低2つ（正しさ/データ + 運用リスク）挙げる。",
            "必須: 各proposalはto_coder（具体）とimpact（現実的）を含める。",
            "禁止: 『良さそう』『検討』などの抽象。具体的欠陥と具体的修正のみ。",
            "アンチループ: 既出が未解決ならスコア+10し[ESCALATED]。それ以外は新規のみ。",
          ].join("\n");
        } else {
          intensityInstr = [
            "強度: critical（批評）。本番マージ前レビューとして扱う。",
            "必須: 具体的なバグ/セキュリティ/設計弱点を最低1つ、to_coderで具体修正を書く。",
            "観点: 入力検証、未処理エラー、ハードコード、テスト不足。",
            "アンチループ: 既出は[UNRESOLVED]と明記して繰り返さない。新規無しなら次の一文だけ: [Observer] No new critique. Loop detected.",
          ].join("\n");
        }
      } else if (outLang === "fr") {
        if (intensity === "polite") {
          intensityInstr = [
            "Intensité: polite. Constructif et encourageant.",
            "Mais: signale des problèmes concrets sur les 5 axes (exactitude/sécurité/fiabilité/performance/maintenabilité).",
            "Chaque proposal doit inclure un message to_coder spécifique et actionnable.",
            "Anti-boucle: nouveaux points seulement. Sinon résume les points encore ouverts en [OPEN].",
          ].join("\n");
        } else if (intensity === "brutal") {
          intensityInstr = [
            "Intensité: brutal. Ça part en prod à minuit pour 10 000 utilisateurs.",
            "Obligatoire: au moins 2 modes de panne (un bug exactitude/données + un risque opérationnel).",
            "Obligatoire: chaque proposal doit inclure to_coder concret + impact réaliste.",
            "Interdit: 'ça a l'air bien', 'on pourrait'. Seulement défauts concrets + fixes concrets.",
            "Anti-boucle: si non résolu, +10 score et tag [ESCALATED]. Sinon nouveaux points uniquement.",
          ].join("\n");
        } else {
          intensityInstr = [
            "Intensité: critical. Revue pré-merge pour un service de prod.",
            "Obligatoire: au moins 1 bug/risque sécu/faiblesse d'archi avec un to_coder concret.",
            "Vérifie: validation d'entrées, erreurs non gérées, valeurs en dur, manque de tests.",
            "Anti-boucle: si déjà mentionné, marque [UNRESOLVED] et passe à autre chose. Pas de nouveau signal → réponds exactement: [Observer] No new critique. Loop detected.",
          ].join("\n");
        }
      }

      // Lightweight proposal memory: provide the Observer with a compact list of recent proposals and approvals
      // so it can avoid repeating the same template critique and can mark items UNRESOLVED/ESCALATED accurately.
      const priorProposalsSummary = (() => {
        try {
          const normTitle = (s) => String(s || "").trim().toLowerCase().replace(/\s+/g, " ");
          const approved = new Set();
          const coderMsgs = paneMessages("coder");
          for (const m of coderMsgs || []) {
            if (!m || m.role !== "user") continue;
            const t = String(m.content || "");
            if (!t.startsWith("[Observer proposal approved]")) continue;
            const mm = t.match(/^Title:\s*(.+)$/im);
            const title = mm ? mm[1] : "";
            const k = normTitle(title);
            if (k) approved.add(k);
          }

          const obsMsgs = observerConversationMessages();
          const map = new Map(); // normTitle -> proposal
          let scanned = 0;
          for (let i = (obsMsgs || []).length - 1; i >= 0 && scanned < 12; i--) {
            const m = obsMsgs[i];
            if (!m || m.role !== "assistant" || m.streaming) continue;
            scanned++;
            const props = parseProposals(String(m.content || ""));
            for (const p of props || []) {
              const k = normTitle(p.title);
              if (!k) continue;
              const prev = map.get(k);
              // Keep the latest/highest-score version (they can differ per turn).
              if (!prev || (p.score || 0) >= (prev.score || 0)) {
                map.set(k, { ...p, _approved: approved.has(k) });
              }
            }
          }

          const arr = Array.from(map.values())
            .sort((a, b) => (b.score || 50) - (a.score || 50))
            .slice(0, 6);

          if (!arr.length) return "";
          const lines = arr.map((p) => {
            const tag = p._approved ? "[APPROVED]" : "[OPEN]";
            const sev = String(p.severity || "info");
            const sc = typeof p.score === "number" ? p.score : 50;
            const ph = String(p.phase || "any");
            return `- ${tag} ${p.title} (sev:${sev} score:${sc} phase:${ph})`;
          });
          return ["prior_proposals:", ...lines].join("\n");
        } catch (_) {
          return "";
        }
      })();
      const observerBridge = [
        "[Observer bridge]",
        langLine,
        `observer_intensity: ${intensity}`,
        loopLine,
        intensityInstr,
        "Meta ops (optional): if you propose updating Spiral-Coder runtime prompts, start to_coder with one of: META_SET_CODER:, META_APPEND_CODER:, META_SET_OBSERVER:, META_APPEND_OBSERVER:.",
        "Review the coder's artifacts below. Check each dimension: CORRECTNESS, SECURITY, RELIABILITY, PERFORMANCE, MAINTAINABILITY.",
        "Code citation: for every warn/crit proposal, add a quote: field containing an exact function name,",
        "  variable name, or ≤40-char code snippet from the coder's output. Use n/a only if no code is visible.",
        priorProposalsSummary,
        "Follow-through: scan for prior proposals in the conversation.",
        "  If addressed by the Coder: mark status: addressed.",
        "  If still unresolved: mark status: [UNRESOLVED] and add +10 to the score.",
        "After proposals, always append BOTH additional blocks:",
        "--- critical_path ---",
        "<ONE sentence: the single issue that, if unaddressed, makes all other improvements pointless. Or 'none' if no blockers.>",
        "--- health ---",
        "score: <0-100 integer: 0=won't run, 50=works-but-risky, 100=shippable>",
        "rationale: <one sentence explaining the score>",
      ].filter(Boolean).join("\n");
      const sendText = config.includeCoderContext
        ? (text + "\n\n" + observerBridge + "\n\n" + coderContextPacket())
        : (text + "\n\n" + observerBridge);
      const toolRootResolved = resolvedCwd(config.toolRoot, threadId, activeThread && activeThread.workdir);
      const obsCfg2 = toolRootResolved ? { ...obsCfg, toolRoot: toolRootResolved } : obsCfg;
      const reqBody = buildReq(obsCfg2, obsKey, history, sendText, diff);
      reqBody.lang = outLang;
      reqBody.force_tools = false;
      const ac = new AbortController();
      abortObserverRef.current = ac;

      try {
        // One-shot language retry: if the response ignores the requested language (ja/fr),
        // retry once with an explicit rewrite instruction.
         const countRe = (re, s) => (String(s || "").match(re) || []).length;
         const stripFencesForLang = (s) => {
           const raw = String(s || "").replace(/\r\n/g, "\n");
           const out = [];
           let inFence = false;
           for (const line of raw.split("\n")) {
             if (/^\s*```/.test(line)) { inFence = !inFence; continue; }
             if (inFence) continue;
             out.push(line);
           }
           return out.join("\n");
          };
          const looksJapanese = (s) => {
            const x = stripFencesForLang(stripObserverMeta(s));
            const jp = countRe(/[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff]/g, x);
            const lat = countRe(/[A-Za-z]/g, x);
            if (jp < 8) return false;
            if (lat <= 0) return true;
           // Allow some English tokens (code, keys) but avoid "mostly English with a few JP chars".
           return lat <= jp * 2;
          };
         const looksFrench = (s) => {
          const x = stripFencesForLang(stripObserverMeta(s));
           const a = countRe(/[\u00C0-\u017F]/g, x);
           const fr = countRe(/\b(le|la|les|des|du|de|pour|avec|sans|est|sont|pas|mais|donc|sur|dans|vous|tu|je|nous|votre)\b/gi, x);
           const en = countRe(/\b(the|and|you|your|should|this|that|with|for|not|are|is|was|were|will|can|cannot|do|does)\b/gi, x);
           if (a > 0 && fr >= 1) return true;
          return fr > en + 1;
        };
        const skippable = (s) => {
          const t = String(s || "").trim();
          if (!t) return true;
          if (t.startsWith("[Observer]")) return true;
          if (/^\[(error|erreur|エラー)\]/i.test(t)) return true;
          if (t.startsWith("[" + tr(lang, "error") + "]")) return true;
          if (t.startsWith("[" + tr(lang, "stop") + "]")) return true;
          return false;
        };
        const needsLangRetry = (expected, content) => {
          if (skippable(content)) return false;
          if (expected === "ja") return !looksJapanese(content);
          if (expected === "fr") return !looksFrench(content);
          return false;
        };

        let finalText = "";
        if (config.stream) {
          let acc = "";
          await streamChat(
            reqBody,
            (evt) => {
              if (!evt) return;
              if (evt.event === "delta") {
                const j = safeJsonParse(evt.data || "{}", {});
                if (j && j.delta) {
                  acc += String(j.delta);
                  appendDelta(threadId, asstMsg.id, j.delta, observerBodyRef);
                }
              } else if (evt.event === "error") {
                const j = safeJsonParse(evt.data || "{}", {});
                throw new Error(j.error || tr(lang, "error"));
              }
            },
            ac.signal
          );
          finishStreaming(threadId, asstMsg.id);
          finalText = acc;
        } else {
          const j = await postJson("/api/chat", reqBody, ac.signal);
          finalText = String((j && j.content) || "");
          setMsg(threadId, asstMsg.id, finalText, observerBodyRef);
        }

        if (!ac.signal.aborted && needsLangRetry(outLang, finalText)) {
          const msg =
            outLang === "fr"
              ? "Observer replied in the wrong language — retrying in French…"
              : outLang === "en"
                ? "Observer language mismatch — retrying…"
                : "Observerが指定言語で返していないため、再試行します…";
          showToast(msg, "info");

          const retryInstr =
            outLang === "fr"
              ? "LANGUAGE FIX: Rewrite the assistant's last message in French ONLY. Do not add new content. Output ONLY the rewritten text. Keep proposals block keys in English (title/to_coder/severity/score/phase/impact/cost)."
              : outLang === "en"
                ? "LANGUAGE FIX: Rewrite the assistant's last message in English ONLY. Do not add new content. Output ONLY the rewritten text. Keep proposals block keys in English (title/to_coder/severity/score/phase/impact/cost)."
                : "LANGUAGE FIX: Rewrite the assistant's last message in Japanese ONLY. Do not add new content. Output ONLY the rewritten text. Keep proposals block keys in English (title/to_coder/severity/score/phase/impact/cost).";
          const extHistory = [
            ...history,
            { role: "user", content: sendText },
            { role: "assistant", content: finalText },
          ];
          const retryBody = buildReq(obsCfg, obsKey, extHistory, retryInstr + "\n\n" + observerBridge, diff);
          retryBody.lang = outLang;
          retryBody.force_tools = false;
          // Make the rewrite more deterministic (language fix should not "drift").
          retryBody.temperature = 0.2;
          try {
            const j2 = await postJson("/api/chat", retryBody, ac.signal);
            const fixed = String((j2 && j2.content) || "");
            if (fixed) {
              finalText = fixed;
              setMsg(threadId, asstMsg.id, fixed, observerBodyRef);
            }
          } catch (_) {
            // If retry fails, keep the original response.
          }
        }

        // One-shot loop retry: if the Observer repeats itself with no new signal,
        // force a diff-style critique instead of reprinting the same template.
        if (!ac.signal.aborted && prevObserverAssts.length && !skippable(finalText)) {
          const maxSim = (() => {
            let s = 0;
            for (const prev of prevObserverAssts) s = Math.max(s, similarity(prev, finalText));
            return s;
          })();
          const titleJacc = (() => {
            try {
              const norm = (s) => String(s || "").trim().toLowerCase().replace(/\s+/g, " ");
              const a0 = prevObserverAssts[0] || "";
              const a = new Set(parseProposals(a0).map((p) => norm(p.title)).filter(Boolean));
              const b = new Set(parseProposals(finalText).map((p) => norm(p.title)).filter(Boolean));
              if (!a.size && !b.size) return 1;
              return jaccardSim(a, b);
            } catch (_) {
              return maxSim;
            }
          })();
          const loopish = maxSim >= 0.82 && titleJacc >= 0.75;
          if (loopish) {
            const msg =
              outLang === "fr"
                ? "Observer se répète — nouvelle tentative (diff-only)…"
                : outLang === "en"
                  ? "Observer repeated itself — retrying (diff-only)…"
                  : "Observerが同じ内容を繰り返しているため、差分批評で再試行します…";
            showToast(msg, "info");

            const loopFixInstr =
              outLang === "fr"
                ? "LOOP FIX: Ton dernier message répétait la même critique. Écris une NOUVELLE critique UNIQUEMENT à partir des informations NOUVELLES depuis ton message précédent. Ne répète pas les mêmes proposals. S'il n'y a pas de nouveau signal, réponds exactement: [Observer] No new critique. Loop detected."
                : outLang === "en"
                  ? "LOOP FIX: Your last message repeated the same critique. Write a NEW critique ONLY based on NEW information since your previous message. Do not restate the same proposals. If there is no new signal, reply exactly: [Observer] No new critique. Loop detected."
                  : "LOOP FIX: 直前の批評と内容がほぼ同一です。前回から増えた情報に基づく「新しい」批評だけを書いてください。同じ提案の焼き直しは禁止。新しい指摘が無い場合は、次の1行だけを厳密に出力: [Observer] No new critique. Loop detected.";
            const extHistory2 = [
              ...history,
              { role: "user", content: sendText },
              { role: "assistant", content: finalText },
            ];
            const retryBody2 = buildReq(obsCfg, obsKey, extHistory2, loopFixInstr + "\n\n" + observerBridge, diff);
            retryBody2.lang = outLang;
            retryBody2.force_tools = false;
            retryBody2.temperature = 0.2;
            try {
              const j3 = await postJson("/api/chat", retryBody2, ac.signal);
              const fixed2 = String((j3 && j3.content) || "");
              if (fixed2) {
                finalText = fixed2;
                setMsg(threadId, asstMsg.id, fixed2, observerBodyRef);
              }
            } catch (_) {
              // Keep original if retry fails.
            }

            // If retry still loops, hard-stop it instead of spamming templates.
            const maxSim2 = (() => {
              let s = 0;
              for (const prev of prevObserverAssts) s = Math.max(s, similarity(prev, finalText));
              return s;
            })();
            if (maxSim2 >= 0.82 && !skippable(finalText)) {
              finalText = "[Observer] No new critique. Loop detected.";
              setMsg(threadId, asstMsg.id, finalText, observerBodyRef);
            }
          }
        }
      } catch (err) {
        const msg = prettyErr(err);
        if (config.stream && !ac.signal.aborted) {
          try {
            const j = await postJson("/api/chat", reqBody, ac.signal);
            setMsg(threadId, asstMsg.id, String((j && j.content) || ""), observerBodyRef);
          } catch (err2) {
            const msg2 = prettyErr(err2);
            setMsg(threadId, asstMsg.id, `[${tr(lang, "error")}] ${msg2}`, observerBodyRef);
          }
        } else if (ac.signal.aborted) {
          setMsg(threadId, asstMsg.id, `[${tr(lang, "stop")}]`, observerBodyRef);
        } else {
          setMsg(threadId, asstMsg.id, `[${tr(lang, "error")}] ${msg}`, observerBodyRef);
        }
      } finally {
        setSendingObserver(false);
        abortObserverRef.current = null;
        refreshPendingEdits();
      }
    };

    const sendProposalToCoder = (p) => {
      if (!p) return;
      const title = String(p.title || "").trim();
      const to = String(p.toCoder || "").trim();
      if (!to) return;
      const sev = String(p.severity || "info").trim();
      const steer = [
        "[Observer proposal approved]",
        '<observer_proposal source="external_observer" priority="' + sev + '">',
        `Title: ${title}`,
        `Severity: ${sev}`,
        "",
        to,
        "",
        "In your next <plan> or visible planning text, explicitly say whether you accept, override, or defer this Observer proposal.",
        "</observer_proposal>",
        "",
      ].join("\n");
      sendCoder(steer);
    };

    const sendDiagnosticToCoder = (d) => {
      if (!d) return;
      const anchor = d.mutationAnchor && d.mutationAnchor.path ? String(d.mutationAnchor.path) : "";
      const followups = Array.isArray(d.requiredFollowups)
        ? d.requiredFollowups.map((f) => {
            const path = String((f && f.path) || "").trim();
            const lit = String((f && f.required_literal) || "").trim();
            return lit ? `${path} (literal: ${lit})` : path;
          }).filter(Boolean)
        : [];
      const action = d.nextCoderAction && typeof d.nextCoderAction === "object"
        ? JSON.stringify(d.nextCoderAction, null, 2)
        : "";
      const literals = Array.isArray(d.finalHandoffLiterals)
        ? d.finalHandoffLiterals.map((x) => String(x || "").trim()).filter(Boolean)
        : [];
      const lines = [
        "[Observer coder diagnostic approved]",
        '<observer_diagnostic source="deterministic_observer" required="true">',
        `failure_mode: ${String(d.failureMode || "needs_coder_followup")}`,
        anchor ? `mutation_anchor: ${anchor}` : "",
        followups.length ? "required_followups:\n- " + followups.join("\n- ") : "",
        d.verificationCmd ? `verification_cmd: ${d.verificationCmd}` : "",
        action ? "next_coder_action:\n" + action : "",
        literals.length ? "final_handoff_literals:\n- " + literals.join("\n- ") : "",
        "",
        "Use this packet as the next concrete Coder step. Prefer patch/verify over more broad reading.",
        "In your next <plan> or visible planning text, explicitly say whether you accept, override, or defer this Observer diagnostic.",
        "</observer_diagnostic>",
      ].filter(Boolean);
      sendCoder(lines.join("\n"));
    };

    const sendBenchmarkPlanToCoder = (p) => {
      if (!p) return;
      const checks = Array.isArray(p.requiredChecks)
        ? p.requiredChecks.map((x) => String(x || "").trim()).filter(Boolean)
        : [];
      const targets = Array.isArray(p.targetFiles)
        ? p.targetFiles.map((x) => String(x || "").trim()).filter(Boolean)
        : [];
      const criteria = Array.isArray(p.successCriteria)
        ? p.successCriteria.map((x) => String(x || "").trim()).filter(Boolean)
        : [];
      const lines = [
        "[Observer benchmark plan approved]",
        '<observer_benchmark_plan source="deterministic_observer" required="true">',
        `case_id_hint: ${String(p.caseIdHint || "")}`,
        `lane: ${String(p.lane || "none")}`,
        `objective: ${String(p.objective || "")}`,
        targets.length ? "target_files:\n- " + targets.join("\n- ") : "",
        checks.length ? "required_checks:\n- " + checks.join("\n- ") : "",
        criteria.length ? "success_criteria:\n- " + criteria.join("\n- ") : "",
        "Implement or update the smallest deterministic regression that proves this Observer finding.",
        "In your next <plan> or visible planning text, explicitly say whether you accept, override, or defer this benchmark plan.",
        "</observer_benchmark_plan>",
      ].filter(Boolean);
      sendCoder(lines.join("\n"));
    };

    const upsertTasksForThread = (threadId, items) => {
      const incoming = Array.isArray(items) ? items : [];
      if (!incoming.length) return;
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => {
          if (t.id !== threadId) return t;
          const existing = Array.isArray(t.tasks) ? t.tasks : [];
          const seen = new Set(existing.map((x) => `${String(x.target || "coder")}|${normTitle(x.title || "")}`));
          const merged = [...existing];
          for (const it of incoming) {
            const k = `${String(it.target || "coder")}|${normTitle(it.title || "")}`;
            if (!k.endsWith("|") && !seen.has(k)) {
              seen.add(k);
              merged.push({
                id: it.id || uid(),
                target: it.target === "observer" ? "observer" : "coder",
                title: String(it.title || "").trim(),
                body: String(it.body || "").trim(),
                phase: String(it.phase || "any").trim().toLowerCase(),
                priority: typeof it.priority === "number" ? it.priority : null,
                status: String(it.status || "new").trim().toLowerCase() || "new",
                createdAt: typeof it.createdAt === "number" ? it.createdAt : Date.now(),
              });
            }
          }
          return { ...t, updatedAt: Date.now(), tasks: merged };
        }),
      }));
    };

    const patchTask = (threadId, taskId, patch) => {
      if (!taskId) return;
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) => {
          if (t.id !== threadId) return t;
          const tasks = Array.isArray(t.tasks) ? t.tasks : [];
          return {
            ...t,
            updatedAt: Date.now(),
            tasks: tasks.map((x) => (x && x.id === taskId ? { ...x, ...patch } : x)),
          };
        }),
      }));
    };

    const dispatchTask = (task) => {
      if (!task || !activeThread) return;
      const threadId = activeThread.id;
      const title = String(task.title || "").trim();
      const body = String(task.body || "").trim();
      const id = String(task.id || "").trim();
      const payload = `[Task ${id}] ${title}\n\n${body}\n`;

      if (String(task.target || "") === "observer") {
        sendObserver(payload);
      } else {
        sendCoder(payload);
      }
      patchTask(threadId, id, { status: "sent" });
    };

    const markTaskDone = (task) => {
      if (!task || !activeThread) return;
      patchTask(activeThread.id, String(task.id || ""), { status: "done" });
    };

    const planTasksFromChat = async (userText, ctxText) => {
      if (!activeThread) return;
      if (planningTasks) return;

      const threadId = activeThread.id;
      const obsProvider = String(config.observerProvider || "").trim() || config.provider;
      const obsBaseUrl = String(config.observerBaseUrl || "").trim() || config.baseUrl;
      const obsModel = String(config.observerModel || "").trim() || (config.chatModel || config.model);
      const obsKey = String(observerApiKey || "").trim() || String(chatApiKey || "").trim() || String(codeApiKey || "").trim();
      if (!obsKey) return;

      setPlanningTasks(true);
      showToast(tr(lang, "planningTasks"), "info");

      const langName = (lang === "fr") ? "French" : (lang === "en") ? "English" : "Japanese";
      const routerPrompt = [
        "You are TaskRouter for Spiral-Coder (behind-the-scenes).",
        "Return ONLY valid JSON. No markdown. No commentary.",
        "Schema: {\"tasks\":[{\"target\":\"coder|observer\",\"title\":\"...\",\"body\":\"...\",\"phase\":\"core|feature|polish|any\",\"priority\":0-100}]}",
        "Rules:",
        "- Keep it minimal: 2-6 tasks total.",
        "- coder tasks MUST be concrete (file edits, commands, checks).",
        "- observer tasks are audit/validation tasks (risks, checks, acceptance criteria).",
        `Write title/body in ${langName}.`,
        "",
        "User message:",
        String(userText || ""),
        ctxText ? ("\nContext:\n" + String(ctxText || "")) : "",
      ].join("\n");

      const routerCfg = {
        ...config,
        mode: "壁打ち",
        cot: "off",
        autonomy: "off",
        provider: obsProvider,
        baseUrl: obsBaseUrl,
        model: obsModel,
        chatModel: obsModel,
        codeModel: obsModel,
        persona: "default",
      };
      const req = buildReq(routerCfg, obsKey, [], routerPrompt, null);
      req.lang = lang;
      req.force_tools = false;
      req.temperature = 0.2;
      req.max_tokens = 900;

      try {
        const j = await postJson("/api/chat", req);
        const tasks = parseTasksJson(String((j && j.content) || ""));
        if (tasks.length) upsertTasksForThread(threadId, tasks);
      } catch (err) {
        showToast(prettyErr(err), "error");
      } finally {
        setPlanningTasks(false);
      }
    };

    const sendChat = async (overrideText) => {
      if (sendingChat) return;
      const raw = overrideText != null ? String(overrideText) : String(chatInput || "");
      const text = raw.trim();
      if (!text) return;
      if (!activeThread) return;
      const threadId = activeThread.id;
      const apiKey = String(chatApiKey || "").trim() || String(codeApiKey || "").trim() || String(observerApiKey || "").trim();
      const chatCfg = { ...config, mode: "会話", cot: "off", autonomy: "off", persona: config.chatPersona || "cheerful" };
      const userMsg = { id: uid(), pane: "chat", role: "user", content: text, ts: Date.now() };
      const asstMsg = { id: uid(), pane: "chat", role: "assistant", content: "", ts: Date.now(), streaming: true };
      setThreadState((s) => ({
        ...s,
        threads: s.threads.map((t) =>
          t.id === threadId ? { ...t, updatedAt: Date.now(), messages: [...(t.messages || []), userMsg, asstMsg] } : t
        ),
      }));
      setChatInput("");
      setSendingChat(true);
      requestAnimationFrame(() => scrollBottom(chatBodyRef));
      const history = paneMessages("chat").filter((m) => !m.streaming && m.content).map((m) => ({ role: m.role, content: m.content }));
      const ctx = config.chatAttachRuntime ? chatRuntimePacket() : "";
      if (config.chatAutoTasks) planTasksFromChat(text, ctx);
      const fullInput = ctx ? `${ctx}\n\n${text}` : text;
      const reqBody = buildReq(chatCfg, apiKey, history, fullInput, null);
      reqBody.lang = lang;
      reqBody.force_tools = false;
      const ac = new AbortController();
      abortChatRef.current = ac;
      try {
        if (config.stream) {
          await streamChat(reqBody, (evt) => {
            if (!evt) return;
            if (evt.event === "delta") {
              const j = safeJsonParse(evt.data || "{}", {});
              if (j && j.delta) appendDelta(threadId, asstMsg.id, j.delta, chatBodyRef);
            } else if (evt.event === "error") {
              const j = safeJsonParse(evt.data || "{}", {});
              throw new Error(j.error || tr(lang, "error"));
            }
          }, ac.signal);
          finishStreaming(threadId, asstMsg.id);
        } else {
          const j = await postJson("/api/chat", reqBody, ac.signal);
          setMsg(threadId, asstMsg.id, String((j && j.content) || ""), chatBodyRef);
        }
      } catch (err) {
        const msg2 = prettyErr(err);
        if (config.stream && !ac.signal.aborted) {
          try {
            const j = await postJson("/api/chat", reqBody, ac.signal);
            setMsg(threadId, asstMsg.id, String((j && j.content) || ""), chatBodyRef);
          } catch (_) {
            setMsg(threadId, asstMsg.id, `[${tr(lang, "error")}] ${msg2}`, chatBodyRef);
          }
        } else if (ac.signal.aborted) {
          setMsg(threadId, asstMsg.id, `[${tr(lang, "stop")}]`, chatBodyRef);
        } else {
          setMsg(threadId, asstMsg.id, `[${tr(lang, "error")}] ${msg2}`, chatBodyRef);
        }
        finishStreaming(threadId, asstMsg.id);
      } finally {
        setSendingChat(false);
        abortChatRef.current = null;
      }
    };

    const stopChat = () => {
      if (abortChatRef.current) abortChatRef.current.abort();
      abortChatRef.current = null;
      setSendingChat(false);
    };

    const applyMetaOp = async (metaOp) => {
      if (!metaOp) return;
      const supported = !!(status && status.features && status.features.meta_prompts);
      if (!supported) return;
      if (metaBusy) return;
      setMetaBusy(true);
      try {
        const j = await postJson("/api/meta_prompts", metaOp);
        refreshPendingEdits();
        const eid = j && j.approval_id ? String(j.approval_id) : "";
        if (eid) {
          const msg =
            String(lang || "").trim().toLowerCase() === "fr"
              ? `Mise à jour du meta prompt en attente d'approbation: ${eid}`
              : String(lang || "").trim().toLowerCase() === "en"
                ? `Meta prompt update queued for approval: ${eid}`
                : `メタプロンプト更新が承認待ちです: ${eid}`;
          showToast(msg, "success");
        }
      } catch (err) {
        showToast(prettyErr(err), "error");
      } finally {
        setMetaBusy(false);
      }
    };

      const coderMsgs = paneMessages("coder");
      const observerMsgs = paneMessages("observer");
      const observerAnalysisMsgs = observerMsgs.filter((m) => !isMetaDiagnoseMessage(m));
      const lastObserverMetaMsg = (() => {
        const last = observerMsgs.length ? observerMsgs[observerMsgs.length - 1] : null;
        return last && isMetaDiagnoseMessage(last) ? last : null;
      })();
      const filterMsgs = (msgs, q) => {
        const needle = String(q || "").trim().toLowerCase();
        if (!needle) return msgs;
        return (msgs || []).filter((m) => String(m && m.content ? m.content : "").toLowerCase().indexOf(needle) !== -1);
      };
      const coderMsgsView = filterMsgs(coderMsgs, coderFind);
      const observerMsgsView = filterMsgs(observerMsgs, observerFind);

      // @file reference chips — parse @path tokens from coderInput for visual feedback.
      const atRefChips = React.useMemo(() => {
        const chips = [];
        const seen = new Set();
        for (const word of String(coderInput || "").split(/\s+/)) {
          if (!word.startsWith("@")) continue;
          let path = word.replace(/^@/, "").replace(/[,);:\].]+$/, "");
          if (!path || seen.has(path)) continue;
          seen.add(path);
          chips.push(path);
        }
        return chips;
      }, [coderInput]);
      const threadTasks = (activeThread && Array.isArray(activeThread.tasks)) ? activeThread.tasks : [];
      const sortedTasks = [...threadTasks].sort((a, b) => {
        const sa = String(a && a.status ? a.status : "new");
        const sb = String(b && b.status ? b.status : "new");
        if (sa !== sb) {
          if (sa === "done") return 1;
          if (sb === "done") return -1;
          if (sa === "sent") return 1;
          if (sb === "sent") return -1;
        }
        const pa = typeof a.priority === "number" ? a.priority : 50;
        const pb = typeof b.priority === "number" ? b.priority : 50;
        if (pa !== pb) return pb - pa;
        return (a && a.createdAt ? a.createdAt : 0) - (b && b.createdAt ? b.createdAt : 0);
      });
      const normTitle = (s) => String(s || "").trim().toLowerCase().replace(/\s+/g, " ");
      const approvedProposalTitles = (() => {
        const out = new Set();
        for (const m of coderMsgs || []) {
          if (!m || m.role !== "user") continue;
          const t = String(m.content || "");
          if (!t.startsWith("[Observer proposal approved]")) continue;
          const mm = t.match(/^Title:\s*(.+)$/im);
          const title = mm ? mm[1] : "";
          const k = normTitle(title);
          if (k) out.add(k);
        }
        return out;
      })();
      let lastObserverAsst = null;
      for (let i = observerAnalysisMsgs.length - 1; i >= 0; i--) {
        if (observerAnalysisMsgs[i].role === "assistant") {
          lastObserverAsst = observerAnalysisMsgs[i];
        break;
      }
    }
    const observerProposals = parseProposals(lastObserverAsst ? lastObserverAsst.content : "");
    const criticalPath = parseCriticalPath(lastObserverAsst ? lastObserverAsst.content : "");
    const healthScore = parseHealthScore(lastObserverAsst ? lastObserverAsst.content : "");
    const coderDiagnostic = parseCoderDiagnostic(lastObserverAsst ? lastObserverAsst.content : "");
    const benchmarkPlan = parseBenchmarkPlan(lastObserverAsst ? lastObserverAsst.content : "");

    // Detect current development phase from the latest Observer message that contains one.
    const observerPhase = (() => {
      for (let i = observerAnalysisMsgs.length - 1; i >= 0; i--) {
        const m = observerAnalysisMsgs[i];
        if (m.role === "assistant" && !m.streaming) {
          const p = parsePhase(String(m.content || ""));
          if (p) return p;
        }
      }
      return null;
    })();

    // Debug/UX: show what context is being injected to Chat/Observer without forcing users to guess.
    const chatCtxPreview = config.chatAttachRuntime ? chatRuntimePacket() : "";
    const observerCtxPreview = (() => {
      const uiLang = String(lang || "ja").trim().toLowerCase();
      const ol0 = String(config.observerLang || "ui").trim().toLowerCase();
      const inferLangFromText = (s) => {
        const x = String(s || "");
        const jp = (x.match(/[\u3040-\u30ff\u3400-\u4dbf\u4e00-\u9fff]/g) || []).length;
        if (jp >= 1) return "ja";
        const acc = (x.match(/[\u00C0-\u017F]/g) || []).length;
        const fr = (x.match(/\b(le|la|les|des|du|de|pour|avec|sans|est|sont|pas|mais|donc|sur|dans|vous|tu|je|nous|votre)\b/gi) || []).length;
        if (acc > 0 || fr >= 2) return "fr";
        const en = (x.match(/\b(the|and|or|to|of|in|for|with|is|are|you|we|i|this|that|it)\b/gi) || []).length;
        if (en >= 2) return "en";
        return "";
      };
      const pickLastUserSample = () => {
        try {
          const pickLastUser = (pane) => {
            const msgs = pane ? paneMessages(pane) : (activeThread && activeThread.messages) || [];
            if (!msgs || !msgs.length) return "";
            for (let i = msgs.length - 1; i >= 0; i--) {
              const m = msgs[i];
              if (!m || m.role !== "user") continue;
              const t = String(m.content || "").trim();
              if (t) return t;
            }
            return "";
          };
          return pickLastUser("observer") || pickLastUser("coder") || pickLastUser("chat") || pickLastUser("");
        } catch (_) {
          return "";
        }
      };
      const intensity0 = String(config.observerIntensity || "critical").trim().toLowerCase();
      const intensity = intensity0 === "polite" || intensity0 === "critical" || intensity0 === "brutal" ? intensity0 : "critical";
      const outLang = (() => {
        if (ol0 === "ja" || ol0 === "en" || ol0 === "fr") return ol0;
        if (ol0 === "ui" || !ol0) return uiLang;
        if (ol0 === "auto") {
          const sample = String(observerInput || "").trim() || pickLastUserSample();
          const inferred = inferLangFromText(sample);
          return inferred || uiLang;
        }
        return uiLang;
      })();
      const head = [
        "--- observer_injected_preview ---",
        `out_lang: ${outLang}`,
        `observer_intensity: ${intensity}`,
        `include_coder_context: ${config.includeCoderContext ? "yes" : "no"}`,
      ];
      if (!config.includeCoderContext) {
        head.push("(coder context is OFF — enable 'Include coder context' to show injected artifacts)");
        return head.join("\n");
      }
      return head.join("\n") + "\n\n" + coderContextPacket();
    })();

    // Sort proposals: phase-match first, then by score descending.
    const sortedProposals = [...observerProposals].sort((a, b) => {
      const aOk = !observerPhase || a.phase === "any" || a.phase === observerPhase;
      const bOk = !observerPhase || b.phase === "any" || b.phase === observerPhase;
      if (aOk !== bOk) return aOk ? -1 : 1;
      return (b.score || 50) - (a.score || 50);
    });

    const promotionStatusLabel = (status) => {
      const s = String(status || "").trim();
      if (s === "needs_review") return tr(lang, "needsReview");
      if (s === "approved") return tr(lang, "approved");
      if (s === "held") return tr(lang, "held");
      if (s === "applied") return tr(lang, "applied");
      if (s === "up_to_date") return tr(lang, "upToDate");
      if (s === "blocked") return tr(lang, "blocked");
      return s || tr(lang, "promotionNone");
    };

    const promotionStatusKey = (entry) => {
      const raw = String((entry && (entry.review_badge || entry.review_status)) || "").trim();
      if (
        raw === "needs_review"
        || raw === "approved"
        || raw === "held"
        || raw === "applied"
        || raw === "up_to_date"
        || raw === "blocked"
      ) {
        return raw;
      }
      return "blocked";
    };

    const groupedHarnessPromotions = (() => {
      const source = harnessPromotions && Array.isArray(harnessPromotions.entries)
        ? harnessPromotions.entries
        : [];
      const order = ["needs_review", "approved", "held", "applied", "blocked", "up_to_date"];
      const buckets = new Map(order.map((status) => [status, []]));
      source.forEach((entry) => {
        const status = promotionStatusKey(entry);
        buckets.get(status).push(entry);
      });
      return order
        .map((status) => ({ status, entries: buckets.get(status) || [] }))
        .filter((group) => group.entries.length > 0);
    })();

    const mergeGateStatusLabel = (status) => {
      const s = String(status || "").trim();
      if (s === "needs_review") return tr(lang, "needsReview");
      if (s === "approved") return tr(lang, "approved");
      if (s === "held") return tr(lang, "held");
      if (s === "rollback_available") return tr(lang, "rollbackAvailable");
      if (s === "blocked") return tr(lang, "blocked");
      if (s === "passed") return tr(lang, "passed");
      if (s === "failed") return tr(lang, "failed");
      return s || tr(lang, "mergeGateNone");
    };

    const mergeGateStatusKey = (entry) => {
      const raw = String((entry && (entry.review_badge || entry.review_status)) || "").trim();
      if (
        raw === "needs_review"
        || raw === "approved"
        || raw === "held"
        || raw === "rollback_available"
        || raw === "blocked"
      ) {
        return raw;
      }
      return "blocked";
    };

    const groupedMergeGate = (() => {
      const source = mergeGate && Array.isArray(mergeGate.entries) ? mergeGate.entries : [];
      const order = ["needs_review", "rollback_available", "blocked", "approved", "held"];
      const buckets = new Map(order.map((status) => [status, []]));
      source.forEach((entry) => {
        const status = mergeGateStatusKey(entry);
        buckets.get(status).push(entry);
      });
      return order
        .map((status) => ({ status, entries: buckets.get(status) || [] }))
        .filter((group) => group.entries.length > 0);
    })();

    const renderPromotionCard = (entry) => {
      const status = promotionStatusKey(entry);
      return e(
        "div",
        {
          key: String(entry.id || `${status}-${entry.title || ""}`),
          className: `review-card ${status}`,
        },
        e(
          "div",
          { className: "review-card-head" },
          e(
            "div",
            { className: "review-card-copy" },
            e("div", { className: "review-card-title" }, String(entry.title || entry.id || "")),
            e(
              "div",
              { className: "review-card-meta" },
              e("span", { className: "pill" }, String(entry.badge || "")),
              e("span", { className: "pill" }, promotionStatusLabel(status)),
              e("span", { className: "pill" }, `${Number(entry.green_case_ids ? entry.green_case_ids.length : 0)} ${tr(lang, "greenCases")}`)
            ),
            entry.subtitle
              ? e("div", { className: "panel-subtitle" }, String(entry.subtitle || ""))
              : null
          ),
          e(
            "div",
            { className: "review-card-actions" },
            entry.can_approve
              ? e(
                  "button",
                  {
                    className: "btn btn-primary",
                    type: "button",
                    disabled: promotionBusy || !harnessSupported,
                    onClick: () => resolveHarnessPromotion(entry.id, "approve"),
                  },
                  tr(lang, "approve")
                )
              : null,
            entry.can_hold
              ? e(
                  "button",
                  {
                    className: "btn btn-warn",
                    type: "button",
                    disabled: promotionBusy || !harnessSupported,
                    onClick: () => resolveHarnessPromotion(entry.id, "hold"),
                  },
                  tr(lang, "hold")
                )
              : null,
            entry.can_apply
              ? e(
                  "button",
                  {
                    className: "btn btn-accent",
                    type: "button",
                    disabled: promotionBusy || !harnessSupported,
                    onClick: () => resolveHarnessPromotion(entry.id, "apply"),
                  },
                  tr(lang, "applyToContract")
                )
              : null
          )
        ),
        entry.reasons && entry.reasons.length
          ? e(
              "div",
              { className: "review-card-reasons" },
              entry.reasons.slice(0, 3).map((reason, idx) =>
                e("div", { key: `${entry.id}-reason-${idx}`, className: "review-card-reason" }, "• " + String(reason || ""))
              )
            )
          : null,
        entry.patch_path
          ? e("pre", { className: "review-card-path" }, String(entry.patch_path || ""))
          : null
      );
    };

    const renderMergeGateCard = (entry) => {
      const status = mergeGateStatusKey(entry);
      return e(
        "div",
        {
          key: String(entry.id || `${status}-${entry.root || ""}`),
          className: `review-card ${status}`,
        },
        e(
          "div",
          { className: "review-card-head" },
          e(
            "div",
            { className: "review-card-copy" },
            e("div", { className: "review-card-title" }, String(entry.id || "")),
            e(
              "div",
              { className: "review-card-meta" },
              e("span", { className: "pill" }, mergeGateStatusLabel(status)),
              e("span", { className: "pill" }, mergeGateStatusLabel(entry.case_status)),
              entry.promoted_overlay_path
                ? e("span", { className: "pill" }, "overlay")
                : null
            ),
            entry.root ? e("div", { className: "panel-subtitle" }, String(entry.root || "")) : null
          ),
          e(
            "div",
            { className: "review-card-actions" },
            entry.can_approve
              ? e(
                  "button",
                  {
                    className: "btn btn-primary",
                    type: "button",
                    disabled: mergeGateBusy || !mergeGateSupported,
                    onClick: () => resolveMergeGate(entry.id, "approve"),
                  },
                  tr(lang, "approve")
                )
              : null,
            entry.can_hold
              ? e(
                  "button",
                  {
                    className: "btn btn-warn",
                    type: "button",
                    disabled: mergeGateBusy || !mergeGateSupported,
                    onClick: () => resolveMergeGate(entry.id, "hold"),
                  },
                  tr(lang, "hold")
                )
              : null
          )
        ),
        entry.rollback_command
          ? e(
              "div",
              { className: "review-card-reasons" },
              e("div", { className: "review-card-reason" }, tr(lang, "rollbackPreview") + " (not executed)")
            )
          : null,
        entry.rollback_command
          ? e("pre", { className: "review-card-path" }, String(entry.rollback_command || ""))
          : null,
        entry.promoted_overlay_path
          ? e("pre", { className: "review-card-path" }, String(entry.promoted_overlay_path || ""))
          : null
      );
    };

    const renderApprovalCard = (kind, it) => {
      const pending = String(it.status || "") === "pending";
      const preview = kind === "edit"
        ? String(it.diff || it.preview || "")
        : String(it.preview || it.command || "");
      return e(
        "div",
        {
          key: String(it.id || `${kind}-${preview}`),
          className: "approval-card",
        },
        e(
          "div",
          { className: "approval-card-head" },
          e(
            "div",
            { className: "approval-card-meta" },
            e("code", null, String(kind === "edit" ? (it.action || "") : (it.id || ""))),
            e("span", { className: "pill" }, String(it.status || "")),
            kind === "edit" && it.path
              ? e("span", { className: "approval-card-path" }, String(it.path || ""))
              : null,
            kind === "command" && it.cwd
              ? e("span", { className: "approval-card-path" }, String(it.cwd || ""))
              : null
          ),
          pending
            ? e(
                "div",
                { className: "approval-card-actions" },
                e(
                  "button",
                  {
                    className: "btn btn-primary",
                    type: "button",
                    disabled: pendingBusy,
                    onClick: () => kind === "edit" ? resolvePendingEdit(it.id, true) : resolvePendingCommand(it.id, true),
                  },
                  tr(lang, "approve")
                ),
                e(
                  "button",
                  {
                    className: "btn btn-warn",
                    type: "button",
                    disabled: pendingBusy,
                    onClick: () => kind === "edit" ? resolvePendingEdit(it.id, false) : resolvePendingCommand(it.id, false),
                  },
                  tr(lang, "reject")
                )
              )
            : null
        ),
        preview ? e("pre", { className: "approval-card-preview" }, preview) : null
      );
    };

    return e(
      "div",
      { className: "app" },
      e(
        "div",
        { className: "topbar" },
        e(
          "div",
          { className: "topbar-inner" },
          e(
            "div",
            { className: "brand" },
            e("h1", null, "Spiral-Coder"),
            e(
              "span",
              { className: "pill", title: status && status.workspace_root ? String(status.workspace_root) : "" },
              status && status.version ? `v${status.version}` : "local"
            ),
            serverOutdated
              ? e(
                  "span",
                  { className: "pill pill-warn", title: "Restart the lite server to pick up latest backend features." },
                  tr(lang, "serverOutdated")
                )
              : null,
            e(
              "span",
              { className: "pill", title: tr(lang, "keys") },
              e(KeyDot, { ok: keyCodestralOk }),
              " C  ",
              e(KeyDot, { ok: keyMistralOk }),
              " M  ",
              e(KeyDot, { ok: keyOpenAIOk }),
              " O  ",
              e(KeyDot, { ok: keyGeminiOk }),
              " G  ",
              e(KeyDot, { ok: keyAnthropicOk }),
              " A"
            )
          ),
            e(
              "div",
              { className: "top-actions" },
            e(
              "div",
              { className: "seg" },
              e("button", { className: "seg-btn " + (lang === "ja" ? "active" : ""), onClick: () => setLang("ja") }, "JA"),
              e("button", { className: "seg-btn " + (lang === "en" ? "active" : ""), onClick: () => setLang("en") }, "EN"),
              e("button", { className: "seg-btn " + (lang === "fr" ? "active" : ""), onClick: () => setLang("fr") }, "FR")
            ),
            e("button", { className: "btn", onClick: refreshStatus, type: "button" }, tr(lang, "refresh")),
            promotionInboxCount
              ? e(
                  "button",
                  {
                    className: "btn btn-review approval-jump",
                    onClick: jumpToHarnessReviews,
                    type: "button",
                    title: tr(lang, "openHarnessReviews"),
                  },
                  `${tr(lang, "harnessReviews")} ${promotionInboxCount}`
                )
              : null,
            mergeGateActionCount
              ? e(
                  "button",
                  {
                    className: "btn btn-review approval-jump",
                    onClick: jumpToMergeGateReviews,
                    type: "button",
                    title: tr(lang, "openMergeGateReviews"),
                  },
                  `${tr(lang, "mergeGateReviews")} ${mergeGateActionCount}`
                )
              : null,
            runtimeApprovalCount
              ? e(
                  "button",
                  {
                    className: "btn btn-warn approval-jump",
                    onClick: jumpToRuntimeApprovals,
                    type: "button",
                    title: tr(lang, "openRuntimeApprovals"),
                  },
                  `${tr(lang, "runtimeApprovals")} ${runtimeApprovalCount}`
                )
              : null,
            e("button", {
              className: "btn btn-icon",
              title: tr(lang, "shortcuts") + " (?)",
              onClick: () => setShowShortcuts((v) => !v),
              type: "button",
            }, "⌨")
          )
        )
      ),
      e(
        "datalist",
        { id: "models-list" },
        (models || []).map((m) => e("option", { key: m, value: m }))
      ),
      e(
        "div",
        { className: "main" },
        e(
          "div",
          { style: { display: "flex", flexDirection: "column", gap: "14px" } },
          e(
            "div",
            { className: "panel" },
            e(
              "div",
              { className: "panel-header" },
              e("h2", null, tr(lang, "threads")),
              e(
                "div",
                { style: { display: "flex", gap: "8px" } },
                e("button", { className: "btn btn-primary", onClick: createThread }, tr(lang, "newThread")),
                e("button", { className: "btn", onClick: exportMd }, tr(lang, "exportMd"))
              )
            ),
            e(
              "div",
              { className: "panel-body" },
              e(
                "div",
                { style: { display: "flex", flexDirection: "column", gap: "10px", maxHeight: "260px", overflow: "auto" } },
                threadState.threads.map((t) => {
                  const active = t.id === threadState.activeId;
                  const activeStyle = active
                    ? { borderColor: "rgba(96,165,250,0.60)", background: "rgba(96,165,250,0.10)" }
                    : null;
                  const msgs = (t.messages || []);
                  const lastMsg = msgs.length ? msgs[msgs.length - 1] : null;
                  const workdirLabel = t.workdir ? (" · wd:" + String(t.workdir)) : "";
                  const snippet = (() => {
                    if (!lastMsg || !lastMsg.content) return "";
                    const raw = String(lastMsg.content || "").replace(/\r\n/g, "\n").trim();
                    if (!raw) return "";
                    const firstLine = raw.split("\n").find((x) => String(x || "").trim()) || raw;
                    const oneLine = String(firstLine).replace(/\s+/g, " ").trim();
                    const max = 120;
                    return oneLine.length > max ? (oneLine.slice(0, max) + "...") : oneLine;
                  })();
                  const snippetPrefix = (() => {
                    if (!lastMsg) return "";
                    const p = lastMsg.pane === "observer" ? "O" : lastMsg.pane === "chat" ? "H" : "C";
                    const r = lastMsg.role === "user" ? "U" : "A";
                    return p + r + ": ";
                  })();
                  const isEditing = editingThreadId === t.id;
                  const isConfirmDel = confirmDeleteId === t.id;
                  return e(
                    "div",
                    { key: t.id, style: { display: "flex", gap: "8px", alignItems: "stretch" } },
                    isEditing
                      ? e(
                          "div",
                          { style: { flex: 1, display: "flex", gap: 6 } },
                          e("input", {
                            className: "field",
                            style: { flex: 1, padding: "6px 8px", fontSize: 13 },
                            value: editingTitle,
                            autoFocus: true,
                            onChange: (ev) => setEditingTitle(ev.target.value),
                            onKeyDown: (ev) => {
                              if (ev.key === "Enter") commitRename(t.id);
                              if (ev.key === "Escape") setEditingThreadId(null);
                            },
                          }),
                          e("button", { className: "btn btn-accent", style: { padding: "6px 10px" }, onClick: () => commitRename(t.id) }, "✓"),
                          e("button", { className: "btn", style: { padding: "6px 10px" }, onClick: () => setEditingThreadId(null) }, "✕")
                        )
                      : isConfirmDel
                      ? e(
                          "div",
                          { style: { flex: 1, display: "flex", gap: 6, alignItems: "center" } },
                          e("span", { style: { flex: 1, fontSize: 12, color: "var(--danger,#f87171)" } }, tr(lang, "delQ")),
                          e("button", { className: "btn btn-warn", style: { padding: "6px 10px" }, onClick: () => confirmDelete(t.id) }, tr(lang, "yes")),
                          e("button", { className: "btn", style: { padding: "6px 10px" }, onClick: () => setConfirmDeleteId(null) }, tr(lang, "no"))
                        )
                      : e(
                          "button",
                          {
                            className: "preset",
                            style: { flex: 1, ...(activeStyle || {}) },
                            onClick: () => setThreadState((s) => ({ ...s, activeId: t.id })),
                          },
                          e("div", { className: "preset-title" }, t.title),
                          e(
                            "div",
                            { className: "preset-sub" },
                            `${new Date(t.updatedAt).toLocaleString()} · ${msgs.length} msgs${workdirLabel}`
                          ),
                          snippet
                            ? e("div", { className: "preset-snippet" }, snippetPrefix + snippet)
                            : null
                        ),
                    !isEditing && !isConfirmDel && e("button", { className: "btn", style: { padding: "8px 10px" }, onClick: () => renameThread(t.id) }, "✎"),
                    !isEditing && !isConfirmDel && e(
                      "button",
                      { className: "btn btn-warn", style: { padding: "8px 10px" }, onClick: () => deleteThread(t.id) },
                      "×"
                    )
                  );
                })
              )
            )
          ),
          e(
            "div",
            { className: "panel panel-review", ref: promotionsPanelRef },
            e(
              "div",
              { className: "panel-header" },
              e(
                "div",
                { className: "panel-header-copy" },
                e("h2", null, tr(lang, "harnessReviews")),
                e("p", { className: "panel-subtitle" }, tr(lang, "harnessReviewHint"))
              ),
              e(
                "div",
                { className: "review-summary" },
                e("span", { className: "pill" }, `${tr(lang, "needsReview")}: ${promotionReviewCount}`),
                harnessPromotions && harnessPromotions.summary
                  ? e("span", { className: "pill" }, `${tr(lang, "approved")}: ${Number(harnessPromotions.summary.approved || 0)}`)
                  : null,
                harnessPromotions && harnessPromotions.summary
                  ? e("span", { className: "pill" }, `${tr(lang, "applied")}: ${Number(harnessPromotions.summary.applied || 0)}`)
                  : null,
                e(
                  "button",
                  {
                    className: "btn",
                    type: "button",
                    disabled: promotionBusy || !harnessSupported,
                    onClick: refreshHarnessPromotions,
                  },
                  tr(lang, "refresh")
                )
              )
            ),
            e(
              "div",
              { className: "panel-body" },
              promotionGateError
                ? e("div", { className: "hint", style: { color: "var(--warn)" } }, promotionGateError)
                : null,
              harnessPromotions && harnessPromotions.status_message
                ? e("div", { className: "hint" }, String(harnessPromotions.status_message))
                : null,
              groupedHarnessPromotions.length
                ? e(
                    "div",
                    { className: "review-groups" },
                    groupedHarnessPromotions.map((group) =>
                      e(
                        "div",
                        { key: group.status, className: "review-group" },
                        e(
                          "div",
                          { className: "review-group-header" },
                          e("div", { className: "review-group-title" }, promotionStatusLabel(group.status)),
                          e("div", { className: "review-group-count" }, `${group.entries.length} / ${Number((harnessPromotions && harnessPromotions.summary && harnessPromotions.summary.total) || 0)}`)
                        ),
                        e(
                          "div",
                          { className: "review-card-list" },
                          group.entries.map((entry) => renderPromotionCard(entry))
                        )
                      )
                    )
                  )
                : e("div", { className: "hint" }, tr(lang, status && !harnessSupported ? "requiresRustServer" : "promotionNone"))
            )
          ),
          e(
            "div",
            { className: "panel panel-review", ref: mergeGatePanelRef },
            e(
              "div",
              { className: "panel-header" },
              e(
                "div",
                { className: "panel-header-copy" },
                e("h2", null, tr(lang, "mergeGateReviews")),
                e("p", { className: "panel-subtitle" }, tr(lang, "mergeGateHint"))
              ),
              e(
                "div",
                { className: "review-summary" },
                mergeGate && mergeGate.summary
                  ? e("span", { className: "pill" }, `${tr(lang, "needsReview")}: ${Number(mergeGate.summary.needs_review || 0)}`)
                  : null,
                mergeGate && mergeGate.summary
                  ? e("span", { className: "pill" }, `${tr(lang, "approved")}: ${Number(mergeGate.summary.approved || 0)}`)
                  : null,
                mergeGate && mergeGate.summary
                  ? e("span", { className: "pill" }, `${tr(lang, "rollbackAvailable")}: ${Number(mergeGate.summary.rollback_available || 0)}`)
                  : null,
                mergeGate && mergeGate.summary
                  ? e("span", { className: "pill" }, `${tr(lang, "blocked")}: ${Number(mergeGate.summary.blocked || 0)}`)
                  : null,
                e(
                  "button",
                  {
                    className: "btn",
                    type: "button",
                    disabled: mergeGateBusy || !mergeGateSupported,
                    onClick: refreshMergeGate,
                  },
                  tr(lang, "refresh")
                )
              )
            ),
            e(
              "div",
              { className: "panel-body" },
              mergeGateError
                ? e("div", { className: "hint", style: { color: "var(--warn)" } }, mergeGateError)
                : null,
              mergeGate && mergeGate.status_message
                ? e("div", { className: "hint" }, String(mergeGate.status_message))
                : null,
              groupedMergeGate.length
                ? e(
                    "div",
                    { className: "review-groups" },
                    groupedMergeGate.map((group) =>
                      e(
                        "div",
                        { key: group.status, className: "review-group" },
                        e(
                          "div",
                          { className: "review-group-header" },
                          e("div", { className: "review-group-title" }, mergeGateStatusLabel(group.status)),
                          e("div", { className: "review-group-count" }, `${group.entries.length} / ${Number((mergeGate && mergeGate.summary && mergeGate.summary.total) || 0)}`)
                        ),
                        e(
                          "div",
                          { className: "review-card-list" },
                          group.entries.map((entry) => renderMergeGateCard(entry))
                        )
                      )
                    )
                  )
                : e("div", { className: "hint" }, tr(lang, status && !mergeGateSupported ? "requiresRustServer" : "mergeGateNone"))
            )
          ),
          e(
            "div",
            { className: "panel", ref: runtimeApprovalsRef },
            e(
              "div",
              { className: "panel-header" },
              e(
                "div",
                { className: "panel-header-copy" },
                e("h2", null, tr(lang, "runtimeApprovals")),
                e("p", { className: "panel-subtitle" }, tr(lang, "runtimeApprovalHint"))
              ),
              e(
                "div",
                { className: "review-summary" },
                e("span", { className: "pill" }, `${tr(lang, "pendingEdits")}: ${pendingEditCount}`),
                e("span", { className: "pill" }, `${tr(lang, "pendingCommands")}: ${pendingCommandCount}`)
              )
            ),
            e(
              "div",
              { className: "panel-body" },
              e(
                "div",
                { className: "approval-board" },
                e(
                  "div",
                  { className: "approval-group" },
                  e(
                    "div",
                    { className: "review-group-header" },
                    e("div", { className: "review-group-title" }, tr(lang, "pendingEdits")),
                    e("div", { className: "review-group-count" }, String(pendingEditCount))
                  ),
                  e(
                    "div",
                    { className: "approval-group-body" },
                    pendingEdits && pendingEdits.length
                      ? pendingEdits.map((it) => renderApprovalCard("edit", it))
                      : e("div", { className: "hint" }, tr(lang, "nothingPending"))
                  )
                ),
                e(
                  "div",
                  { className: "approval-group" },
                  e(
                    "div",
                    { className: "review-group-header" },
                    e("div", { className: "review-group-title" }, tr(lang, "pendingCommands")),
                    e("div", { className: "review-group-count" }, String(pendingCommandCount))
                  ),
                  e(
                    "div",
                    { className: "approval-group-body" },
                    pendingCommands && pendingCommands.length
                      ? pendingCommands.map((it) => renderApprovalCard("command", it))
                      : e("div", { className: "hint" }, tr(lang, "nothingPending"))
                  )
                )
              )
            )
          ),
          e(
            "div",
            { className: "panel", ref: settingsPanelRef },
            e(
              "div",
              { className: "panel-header" },
              e(
                "div",
                { className: "panel-header-copy" },
                e("h2", null, tr(lang, "settings")),
                e("p", { className: "panel-subtitle" }, tr(lang, "settingsHint"))
              )
            ),
            e(
              "div",
              { className: "panel-body" },
              e("div", { className: "section-title" }, tr(lang, "presets")),
              e(
                "div",
                { className: "preset-row" },
                e(
                  "button",
                  { className: "preset preset-vibe", onClick: () => applyPreset("vibe") },
                  e("div", { className: "preset-title" }, "VIBE"),
                  e("div", { className: "preset-sub" }, "Mistral · codestral-latest")
                ),
                e(
                  "button",
                  { className: "preset", onClick: () => applyPreset("codestral") },
                  e("div", { className: "preset-title" }, "Codestral"),
                  e("div", { className: "preset-sub" }, "codestral.mistral.ai · /v1")
                ),
                e(
                  "button",
                  { className: "preset", onClick: () => applyPreset("openai") },
                  e("div", { className: "preset-title" }, "OpenAI"),
                  e("div", { className: "preset-sub" }, "OpenAI-compatible · /v1")
                ),
                e(
                  "button",
                  { className: "preset", onClick: () => applyPreset("anthropic") },
                  e("div", { className: "preset-title" }, "Anthropic"),
                  e("div", { className: "preset-sub" }, "/v1/messages")
                )
              ),
              e("div", { className: "hr" }),
              e(
                "div",
                { className: "grid2" },
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "provider")),
                  e(
                    "select",
                    {
                      className: "select",
                      value: config.provider,
                      onChange: (ev) => setProviderSafe(ev.target.value),
                    },
                    PROVIDERS.map((p) =>
                      e("option", { key: p.k, value: p.k }, (p.l && p.l[lang]) || p.k)
                    )
                  )
                ),
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "chatModel")),
                  e("input", {
                    className: "input",
                    value: config.chatModel || "",
                    list: models && models.length ? "models-list" : undefined,
                    onChange: (ev) => setConfig({ ...config, chatModel: ev.target.value }),
                  })
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "codeModel")),
                e("input", {
                  className: "input",
                  value: config.codeModel || "",
                  list: models && models.length ? "models-list" : undefined,
                  onChange: (ev) => setConfig({ ...config, codeModel: ev.target.value }),
                })
              ),
              e(
                "div",
                { style: { display: "flex", gap: 10, alignItems: "center", flexWrap: "wrap" } },
                e(
                  "div",
                  { className: "seg" },
                  e(
                    "button",
                    {
                      className: "seg-btn " + (modelsTarget === "chat" ? "active" : ""),
                      onClick: () => setModelsTarget("chat"),
                      type: "button",
                    },
                    "Chat"
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (modelsTarget === "code" ? "active" : ""),
                      onClick: () => setModelsTarget("code"),
                      type: "button",
                    },
                    "Code"
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (modelsTarget === "observer" ? "active" : ""),
                      onClick: () => setModelsTarget("observer"),
                      type: "button",
                    },
                    "Obs"
                  )
                ),
                e(
                  "button",
                  { className: "btn", onClick: fetchModels, disabled: modelsLoading },
                  modelsLoading ? "Loading..." : tr(lang, "fetchModels")
                ),
                models && models.length
                  ? e("span", { className: "pill" }, `${models.length} models`)
                  : null
              ),
              modelsErr ? e("div", { className: "hint", style: { color: "var(--warn)", marginTop: "8px" } }, modelsErr) : null,
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "baseUrl")),
                e("input", {
                  className: "input",
                  value: config.baseUrl,
                  onChange: (ev) => setConfig({ ...config, baseUrl: ev.target.value }),
                })
              ),
              e(
                "div",
                { className: "grid2", style: { marginTop: "10px" } },
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "codeProvider")),
                  e(
                    "select",
                    {
                      className: "select",
                      value: String(config.codeProvider || ""),
                      onChange: (ev) => setConfig({ ...config, codeProvider: ev.target.value }),
                    },
                    e("option", { value: "" }, tr(lang, "sameAsChat")),
                    PROVIDERS.map((p) => e("option", { key: p.k, value: p.k }, (p.l && p.l[lang]) || p.k))
                  )
                ),
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "codeBaseUrl")),
                  e("input", {
                    className: "input",
                    value: String(config.codeBaseUrl || ""),
                    onChange: (ev) => setConfig({ ...config, codeBaseUrl: ev.target.value }),
                    placeholder: tr(lang, "sameAsChat"),
                  })
                )
              ),
              e(
                "div",
                { className: "grid2", style: { marginTop: "10px" } },
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "mode")),
                  e(
                    "select",
                    {
                      className: "select",
                      value: config.mode,
                      onChange: (ev) => setConfig({ ...config, mode: ev.target.value }),
                    },
                    CODER_MODES.map((m) => e("option", { key: m.k, value: m.k }, (m.l && m.l[lang]) || m.k))
                  )
                ),
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "persona")),
                  e(
                    "select",
                    {
                      className: "select",
                      value: config.persona,
                      onChange: (ev) => setConfig({ ...config, persona: ev.target.value }),
                    },
                    PERSONAS.map((p) => e("option", { key: p, value: p }, p))
                  )
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "observerIntensity")),
                e(
                  "div",
                  { className: "seg seg-4" },
                  e(
                    "button",
                    {
                      className: "seg-btn " + (String(config.observerIntensity || "critical") === "polite" ? "active" : ""),
                      onClick: () => setConfig({ ...config, observerIntensity: "polite" }),
                      type: "button",
                    },
                    tr(lang, "polite")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (String(config.observerIntensity || "critical") === "critical" ? "active" : ""),
                      onClick: () => setConfig({ ...config, observerIntensity: "critical" }),
                      type: "button",
                    },
                    tr(lang, "critical")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (String(config.observerIntensity || "critical") === "brutal" ? "active" : ""),
                      onClick: () => setConfig({ ...config, observerIntensity: "brutal" }),
                      type: "button",
                    },
                    tr(lang, "brutal")
                  )
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "observerLang")),
                e(
                  "select",
                  {
                    className: "select",
                    value: String(config.observerLang || "ui"),
                    onChange: (ev) => setConfig({ ...config, observerLang: ev.target.value }),
                  },
                  e("option", { value: "ui" }, "UI"),
                  e("option", { value: "auto" }, "Auto"),
                  e("option", { value: "ja" }, "JA"),
                  e("option", { value: "en" }, "EN"),
                  e("option", { value: "fr" }, "FR"),
                )
              ),
              e(
                "div",
                { className: "grid2", style: { marginTop: "10px" } },
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "observer") + " · " + tr(lang, "mode")),
                  e(
                    "select",
                    {
                      className: "select",
                      value: config.observerMode,
                      onChange: (ev) => setConfig({ ...config, observerMode: ev.target.value }),
                    },
                    OBSERVER_MODES.map((m) => e("option", { key: m.k, value: m.k }, (m.l && m.l[lang]) || m.k))
                  )
                ),
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "observer") + " · " + tr(lang, "persona")),
                  e(
                    "select",
                    {
                      className: "select",
                      value: config.observerPersona,
                      onChange: (ev) => setConfig({ ...config, observerPersona: ev.target.value }),
                    },
                    PERSONAS.map((p) => e("option", { key: p, value: p }, p))
                  )
                )
              ),
              e(
                "div",
                { className: "grid2", style: { marginTop: "10px" } },
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "observerProvider")),
                  e(
                    "select",
                    {
                      className: "select",
                      value: String(config.observerProvider || ""),
                      onChange: (ev) => setConfig({ ...config, observerProvider: ev.target.value }),
                    },
                    e("option", { value: "" }, tr(lang, "sameAsChat")),
                    PROVIDERS.map((p) => e("option", { key: p.k, value: p.k }, (p.l && p.l[lang]) || p.k))
                  )
                ),
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "observerModel")),
                  e("input", {
                    className: "input",
                    value: String(config.observerModel || ""),
                    list: models && models.length ? "models-list" : undefined,
                    onChange: (ev) => setConfig({ ...config, observerModel: ev.target.value }),
                    placeholder: tr(lang, "sameAsChat"),
                  })
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "observerBaseUrl")),
                e("input", {
                  className: "input",
                  value: String(config.observerBaseUrl || ""),
                  onChange: (ev) => setConfig({ ...config, observerBaseUrl: ev.target.value }),
                  placeholder: tr(lang, "sameAsChat"),
                })
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "cot")),
                e(
                  "div",
                  { className: "seg" },
                  e(
                    "button",
                    {
                      className: "seg-btn " + (String(config.cot || "brief") === "brief" ? "active" : ""),
                      onClick: () => setConfig({ ...config, cot: "brief" }),
                      type: "button",
                    },
                    tr(lang, "brief")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (String(config.cot || "brief") === "structured" ? "active" : ""),
                      onClick: () => setConfig({ ...config, cot: "structured" }),
                      type: "button",
                    },
                    tr(lang, "structured")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (String(config.cot || "brief") === "deep" ? "active" : ""),
                      onClick: () => setConfig({ ...config, cot: "deep" }),
                      type: "button",
                    },
                    tr(lang, "deep")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (String(config.cot || "brief") === "off" ? "active" : ""),
                      onClick: () => setConfig({ ...config, cot: "off" }),
                      type: "button",
                    },
                    tr(lang, "off")
                  )
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "editApproval")),
                e(
                  "div",
                  { className: "seg" },
                  e(
                    "button",
                    {
                      className: "seg-btn " + (config.requireEditApproval ? "active" : ""),
                      onClick: () => setConfig({ ...config, requireEditApproval: true }),
                      type: "button",
                    },
                    tr(lang, "on")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (!config.requireEditApproval ? "active" : ""),
                      onClick: () => setConfig({ ...config, requireEditApproval: false }),
                      type: "button",
                    },
                    tr(lang, "off")
                  )
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "commandApproval")),
                e(
                  "div",
                  { className: "seg" },
                  e(
                    "button",
                    {
                      className: "seg-btn " + (config.requireCommandApproval ? "active" : ""),
                      onClick: () => setConfig({ ...config, requireCommandApproval: true }),
                      type: "button",
                    },
                    tr(lang, "on")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (!config.requireCommandApproval ? "active" : ""),
                      onClick: () => setConfig({ ...config, requireCommandApproval: false }),
                      type: "button",
                    },
                    tr(lang, "off")
                  )
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "autoObserve")),
                e(
                  "div",
                  { className: "seg" },
                  e(
                    "button",
                    {
                      className: "seg-btn " + (config.autoObserve ? "active" : ""),
                      onClick: () => setConfig({ ...config, autoObserve: true }),
                      type: "button",
                    },
                    tr(lang, "on")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (!config.autoObserve ? "active" : ""),
                      onClick: () => setConfig({ ...config, autoObserve: false }),
                      type: "button",
                    },
                    tr(lang, "off")
                  )
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "forceAgent")),
                e(
                  "div",
                  { className: "seg" },
                  e(
                    "button",
                    {
                      className: "seg-btn " + (config.forceAgent ? "active" : ""),
                      onClick: () => setConfig({ ...config, forceAgent: true }),
                      type: "button",
                    },
                    tr(lang, "on")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (!config.forceAgent ? "active" : ""),
                      onClick: () => setConfig({ ...config, forceAgent: false }),
                      type: "button",
                    },
                    tr(lang, "off")
                  )
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "coderMaxIters")),
                e("input", {
                  className: "input",
                  value: String(config.coderMaxIters || ""),
                  onChange: (ev) => setConfig({ ...config, coderMaxIters: ev.target.value }),
                  placeholder: String(DEFAULT_CONFIG.coderMaxIters || "14"),
                  inputMode: "numeric",
                })
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "toolRoot")),
                e("input", {
                  className: "input",
                  value: String(config.toolRoot || ""),
                  onChange: (ev) => setConfig({ ...config, toolRoot: ev.target.value }),
                  placeholder: "(optional) subdir (e.g. myrepo)",
                }),
                (projectScanLoading || (projectScan && projectScan.stack_label))
                  ? e("div", { style: { marginTop: "4px", fontSize: "0.78rem",
                                         color: "var(--accent,#2dd4bf)", fontFamily: "monospace" } },
                      projectScanLoading
                        ? "⟳ scanning…"
                        : e("span", null,
                            e("span", { style: { opacity: 0.6 } }, (config.toolRoot || "") + "  ●  "),
                            projectScan.stack_label)
                    )
                  : null
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "workdir")),
                e("input", {
                  className: "input",
                  value: String((activeThread && activeThread.workdir) || ""),
                  onChange: (ev) => {
                    const v = String(ev.target.value || "");
                    setThreadState((s) => ({
                      ...s,
                      threads: s.threads.map((t) => (t.id === (activeThread && activeThread.id) ? { ...t, updatedAt: Date.now(), workdir: v } : t)),
                    }));
                  },
                  placeholder: "(optional) subdir inside tool_root (e.g. myrepo)",
                })
              ),
              (String(config.provider || "").trim() === "mistral-cli" ||
                String(config.codeProvider || "").trim() === "mistral-cli" ||
                String(config.observerProvider || "").trim() === "mistral-cli")
                ? e(
                    "div",
                    { className: "grid2", style: { marginTop: "10px" } },
                    e(
                      "div",
                      { className: "field" },
                      e("label", null, tr(lang, "vibeAgent")),
                      e("input", {
                        className: "input",
                        value: String(config.mistralCliAgent || ""),
                        onChange: (ev) => setConfig({ ...config, mistralCliAgent: ev.target.value }),
                        placeholder: "accept-edits / plan / ...",
                      })
                    ),
                    e(
                      "div",
                      { className: "field" },
                      e("label", null, tr(lang, "vibeMaxTurns")),
                      e("input", {
                        className: "input",
                        value: String(config.mistralCliMaxTurns || ""),
                        onChange: (ev) => setConfig({ ...config, mistralCliMaxTurns: ev.target.value }),
                        placeholder: "8",
                        inputMode: "numeric",
                      })
                    )
                  )
                : null,
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "includeCoderContext")),
                e(
                  "div",
                  { className: "seg" },
                  e(
                    "button",
                    {
                      className: "seg-btn " + (config.includeCoderContext ? "active" : ""),
                      onClick: () => setConfig({ ...config, includeCoderContext: true }),
                      type: "button",
                    },
                    tr(lang, "on")
                  ),
                  e(
                    "button",
                    {
                      className: "seg-btn " + (!config.includeCoderContext ? "active" : ""),
                      onClick: () => setConfig({ ...config, includeCoderContext: false }),
                      type: "button",
                    },
                    tr(lang, "off")
                  )
                )
              ),
              e(
                "div",
                { className: "grid2", style: { marginTop: "10px" } },
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "temperature")),
                  e("input", {
                    className: "input",
                    value: config.temperature,
                    onChange: (ev) => setConfig({ ...config, temperature: ev.target.value }),
                    inputMode: "decimal",
                  })
                ),
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "maxTokens")),
                  e("input", {
                    className: "input",
                    value: config.maxTokens,
                    onChange: (ev) => setConfig({ ...config, maxTokens: ev.target.value }),
                    inputMode: "numeric",
                  })
                )
              ),
              e(
                "div",
                { className: "grid2", style: { marginTop: "10px" } },
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "timeoutSeconds")),
                  e("input", {
                    className: "input",
                    value: config.timeoutSeconds,
                    onChange: (ev) => setConfig({ ...config, timeoutSeconds: ev.target.value }),
                    inputMode: "numeric",
                  })
                ),
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "stream")),
                  e(
                    "div",
                    { className: "seg" },
                    e(
                      "button",
                      {
                        className: "seg-btn " + (config.stream ? "active" : ""),
                        onClick: () => setConfig({ ...config, stream: true }),
                      },
                      tr(lang, "on")
                    ),
                    e(
                      "button",
                      {
                        className: "seg-btn " + (!config.stream ? "active" : ""),
                        onClick: () => setConfig({ ...config, stream: false }),
                      },
                      tr(lang, "off")
                    )
                  )
                )
              ),
              e(
                "div",
                { className: "grid2", style: { marginTop: "10px" } },
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "apiKeyChat")),
                  e("input", {
                    className: "input",
                    type: "password",
                    value: chatApiKey,
                    onChange: (ev) => setChatApiKey(ev.target.value),
                    placeholder: "env",
                  })
                ),
                e(
                  "div",
                  { className: "field" },
                  e("label", null, tr(lang, "apiKeyCode")),
                  e("input", {
                    className: "input",
                    type: "password",
                    value: codeApiKey,
                    onChange: (ev) => setCodeApiKey(ev.target.value),
                    placeholder: "env",
                  })
                )
              ),
              e(
                "div",
                { className: "field", style: { marginTop: "10px" } },
                e("label", null, tr(lang, "apiKeyObserver")),
                e("input", {
                  className: "input",
                  type: "password",
                  value: observerApiKey,
                  onChange: (ev) => setObserverApiKey(ev.target.value),
                  placeholder: "env",
                })
              ),
              e(
                "div",
                {
                  className: "diffbox",
                  onDragOver: (ev) => { ev.preventDefault(); ev.currentTarget.style.borderColor = "rgba(45,212,191,0.7)"; ev.currentTarget.style.background = "rgba(45,212,191,0.07)"; },
                  onDragLeave: (ev) => { ev.currentTarget.style.borderColor = ""; ev.currentTarget.style.background = ""; },
                  onDrop: async (ev) => {
                    ev.preventDefault();
                    ev.currentTarget.style.borderColor = "";
                    ev.currentTarget.style.background = "";
                    const f = ev.dataTransfer && ev.dataTransfer.files && ev.dataTransfer.files[0];
                    if (!f) return;
                    const txt = await readFileText(f);
                    setDiff(txt);
                    setConfig((c) => ({ ...c, mode: "diff批評" }));
                  },
                },
                e("div", { className: "section-title" }, tr(lang, "diff") + "  (⎘ drag & drop)"),
                e(
                  "div",
                  { className: "diff-actions" },
                  e("input", {
                    className: "file",
                    type: "file",
                    accept: ".diff,.patch,.txt",
                    onChange: async (ev) => {
                      const f = ev.target.files && ev.target.files[0];
                      if (!f) return;
                      const txt = await readFileText(f);
                      setDiff(txt);
                      ev.target.value = "";
                    },
                  }),
                  e("button", { className: "btn", onClick: () => setDiff("") }, tr(lang, "clear"))
                ),
                e("textarea", {
                  className: "textarea",
                  value: diff,
                  onChange: (ev) => setDiff(ev.target.value),
                  placeholder: tr(lang, "diffHint"),
                })
              )
            )
          )
        ),
        e(
          "div",
          { className: "arena", ref: arenaRef, style: { display: "flex", gap: 0 } },
          // Coder pane
          e(
            "div",
            { className: "panel chat", style: { flex: splitPct, minWidth: 0 } },
            e(
              "div",
              { className: "panel-header" },
              e("h2", null, tr(lang, "coder") + ": " + (activeThread ? activeThread.title : "")),
              e(
                "div",
                { style: { display: "flex", gap: 8, alignItems: "center" } },
                e("span", {
                  style: {
                    width: 8, height: 8, borderRadius: "50%", display: "inline-block",
                    background: PROVIDER_COLORS[coderResolvedProvider(config.mode)] || "rgba(255,255,255,0.4)",
                    boxShadow: "0 0 8px " + (PROVIDER_COLORS[coderResolvedProvider(config.mode)] || "transparent") + "88",
                    transition: "background 400ms ease, box-shadow 400ms ease",
                  },
                }),
                e(
                  "span",
                  { className: "pill", title: coderActiveModel() },
                  providerLabel(coderResolvedProvider(config.mode)) + " · " + modeLabel(config.mode) + " · " + shortModel(coderActiveModel())
                ),
                e("button", {
                  className: "btn btn-icon",
                  type: "button",
                  title: tr(lang, "focusCoder"),
                  onClick: () => setSplitPct(70),
                }, "◀"),
                gitCheckpoint
                  ? e("button", {
                      className: "btn btn-warn",
                      type: "button",
                      title: `Rollback to git checkpoint ${gitCheckpoint}`,
                      style: { fontSize: 11, padding: "3px 8px" },
                      disabled: sendingCoder,
                      onClick: async () => {
                        if (!confirm(`Reset all files to checkpoint ${gitCheckpoint}? (git reset --hard)`)) return;
                        try {
                          const r = await fetch("/api/rollback", {
                            method: "POST",
                            headers: { "Content-Type": "application/json" },
                            body: JSON.stringify({ checkpoint: gitCheckpoint }),
                          });
                          const j = await r.json().catch(() => ({}));
                          const ok = !!(j && j.ok);
                          const msgText = ok
                            ? String(j.message || `rolled back to ${String(gitCheckpoint || "").slice(0, 8)}`)
                            : String(j.error || `HTTP ${r.status}`);
                          const content = ok ? `[rollback] ✅ ${msgText}` : `[rollback] ❌ ${msgText}`;
                          const msg = { id: uid(), pane: "coder", role: "assistant", content, ts: Date.now() };
                          setThreadState((s) => ({
                            ...s,
                            threads: s.threads.map((t) =>
                              t.id === activeThread.id ? { ...t, messages: [...(t.messages || []), msg] } : t
                            ),
                          }));
                          if (j.ok) setGitCheckpoint(null);
                        } catch (err) {
                          alert("Rollback failed: " + err.message);
                        }
                      },
                    }, `⟳ ${gitCheckpoint}`)
                  : null
              )
            ),
            e(
              "div",
              { className: "chat-find" },
              e("input", {
                className: "input",
                value: coderFind,
                placeholder: tr(lang, "findInThread"),
                onChange: (ev) => setCoderFind(ev.target.value),
                onKeyDown: (ev) => { if (ev.key === "Escape") setCoderFind(""); },
                style: { flex: 1, padding: "8px 10px", fontSize: 12, fontFamily: "var(--mono)" },
              }),
              coderFind
                ? e("button", { className: "btn btn-icon", type: "button", title: tr(lang, "clear"), onClick: () => setCoderFind("") }, "✕")
                : null,
              coderFind
                ? e("span", { className: "msg-ts", style: { marginLeft: 6 } }, `${coderMsgsView.length}/${coderMsgs.length}`)
                : null
            ),
            e("div", { className: "chat-body", ref: coderBodyRef },
              coderMsgs.length === 0
                ? e("div", { className: "pane-empty" },
                    e("div", { className: "pane-empty-icon" }, "⚡"),
                    e("p", { className: "pane-empty-hint" }, tr(lang, "placeholder"))
                  )
                : coderMsgsView.length === 0
                  ? e("div", { className: "pane-empty" },
                      e("div", { className: "pane-empty-icon" }, "🔎"),
                      e("p", { className: "pane-empty-hint" }, tr(lang, "noMatches"))
                    )
                  : coderMsgsView.map(renderMessage)
            ),
            e(
              "div",
              { className: "composer" },
              e("textarea", {
                className: "textarea",
                value: coderInput,
                rows: Math.max(2, Math.min(8, (coderInput.match(/\n/g) || []).length + 1)),
                style: { resize: "none" },
                placeholder: tr(lang, "placeholder"),
                onChange: (ev) => setCoderInput(ev.target.value),
                onKeyDown: (ev) => {
                  if ((ev.metaKey || ev.ctrlKey) && ev.key === "Enter") {
                    ev.preventDefault();
                    sendCoder();
                    return;
                  }
                  if (ev.key === "Enter" && !ev.shiftKey) {
                    ev.preventDefault();
                    sendCoder();
                  }
                },
              }),
              atRefChips.length > 0
                ? e(
                    "div",
                    { style: { display: "flex", flexWrap: "wrap", gap: "4px", padding: "4px 0" } },
                    ...atRefChips.map((path) =>
                      e(
                        "span",
                        {
                          key: path,
                          style: {
                            background: "rgba(45,212,191,0.12)",
                            color: "var(--accent,#2dd4bf)",
                            border: "1px solid rgba(45,212,191,0.3)",
                            borderRadius: "4px",
                            padding: "1px 7px",
                            fontSize: "0.76rem",
                            fontFamily: "var(--mono)",
                          },
                        },
                        "📎 @" + path
                      )
                    )
                  )
                : null,
              e(
                "div",
                { style: { display: "flex", gap: 8, alignItems: "center" } },
                e(
                  "button",
                  { className: "btn", type: "button", onClick: insertCliTemplate, disabled: sendingCoder },
                  tr(lang, "insertCliTemplate")
                ),
                sendingCoder
                  ? e("button", { className: "btn btn-warn", onClick: stopCoder }, tr(lang, "stop"))
                  : e("button", { className: "btn btn-primary", onClick: () => sendCoder() }, tr(lang, "send"))
              )
            ),
            e(
              "div",
              { className: "statusline" },
              e("span", {
                className: "dot" + (sendingCoder ? " streaming" : ""),
                style: {
                  background: sendingCoder
                    ? (PROVIDER_COLORS[coderResolvedProvider(config.mode)] || "rgba(45,212,191,0.85)")
                    : "rgba(45,212,191,0.85)",
                  transition: "background 400ms ease",
                },
              }),
              e("span", null, sendingCoder ? (config.stream ? tr(lang, "streaming") : tr(lang, "sending")) : tr(lang, "ready")),
              coderMsgs.length > 0 && e(
                "span",
                {
                  style: { marginLeft: "auto", color: "var(--faint)", fontSize: 11, fontFamily: "var(--mono)" },
                },
                "~" + coderMsgs.reduce((s, m) => s + estimateTokens(m.content), 0) + " tokens · " + coderMsgs.length + " msgs"
              )
            )
          ),

          // Drag handle
          e("div", {
            className: "drag-handle",
            title: tr(lang, "splitHint"),
            onMouseDown: onSplitDragStart,
            onTouchStart: onSplitDragStart,
            onDoubleClick: () => setSplitPct(40),
          }),

          // Observer pane
          e(
            "div",
            { className: "panel chat", style: { flex: 100 - splitPct, minWidth: 0 } },
            e(
              "div",
              { className: "panel-header" },
              e("h2", null, tr(lang, "observer")),
              e(
                "div",
                { style: { display: "flex", gap: 8, alignItems: "center" } },
                e("span", {
                  style: {
                    width: 8, height: 8, borderRadius: "50%", display: "inline-block",
                    background: PROVIDER_COLORS[observerResolvedProvider()] || "rgba(251, 191, 36, 0.85)",
                    boxShadow: "0 0 8px " + (PROVIDER_COLORS[observerResolvedProvider()] || "rgba(251, 191, 36, 0.45)") + "88",
                    transition: "background 400ms ease, box-shadow 400ms ease",
                  },
                }),
                e(
                  "span",
                  { className: "pill", title: observerActiveModel() },
                  providerLabel(observerResolvedProvider()) + " · " + modeLabel(config.observerMode) + " · " + shortModel(observerActiveModel())
                ),
                loopInfo && loopInfo.depth > 0
                  ? e(
                      "span",
                      {
                        className: "pill pill-warn",
                        title: `${tr(lang, "loopDetected")} (sim=${Math.round((loopInfo.score || 0) * 100)}%)`,
                      },
                      tr(lang, "loopDetected") + " ×" + String(loopInfo.depth)
                    )
                  : null,
                lastObserverMetaMsg
                  ? e(
                      "span",
                      {
                        className: "pill meta-pill",
                        title: lastObserverMetaMsg.metaTargetId
                          ? `[META-DIAGNOSE] target=${lastObserverMetaMsg.metaTargetId}`
                          : "[META-DIAGNOSE]",
                      },
                      tr(lang, "metaBadge")
                    )
                  : null,
                observerPhase && e("span", { className: "phase-indicator phase-" + observerPhase }, observerPhase),
                config.autoObserve && e("span", { className: "pill auto-badge" }, "AUTO"),
                e("button", {
                  className: "btn btn-icon",
                  type: "button",
                  title: tr(lang, "focusObserver"),
                  onClick: () => setSplitPct(30),
                }, "▶")
              )
            ),
            e("div", { className: "obs-subtab-bar" },
              e("button", {
                className: "obs-subtab" + (observerSubTab === "analysis" ? " active" : ""),
                onClick: () => setObserverSubTab("analysis"),
              }, tr(lang, "observer")),
              e("button", {
                className: "obs-subtab" + (observerSubTab === "meta" ? " active" : ""),
                onClick: () => setObserverSubTab("meta"),
              }, tr(lang, "metaViewer")),
              e("button", {
                className: "obs-subtab" + (observerSubTab === "chat" ? " active" : ""),
                onClick: () => setObserverSubTab("chat"),
              }, tr(lang, "chat"))
            ),
            observerSubTab === "chat"
              ? e(React.Fragment, { key: "chat" },
                  e("div", { className: "obs-scroll-zone chat-scroll-zone" },
                    e("div", { className: "chat-body", ref: chatBodyRef },
                      paneMessages("chat").length === 0
                        ? e("div", { className: "pane-empty chat-empty" },
                            e("div", { className: "pane-empty-icon" }, "💬"),
                            e("div", { className: "chat-empty-title" },
                              lang === "en" ? "Free chat"
                              : lang === "fr" ? "Discussion libre"
                              : "会話モード"
                            ),
                            e("p", { className: "pane-empty-hint" },
                              lang === "en" ? "Ask anything — code questions, design decisions, or just think out loud"
                              : lang === "fr" ? "Posez n'importe quelle question — code, design, ou simple réflexion"
                              : "なんでも聞いてください — 実装の疑問、設計の相談、ゴム鴨モード"
                            )
                          )
                        : paneMessages("chat").map(renderMessage)
                    )
                  ),
                  e("div", { className: "chat-persona-bar" },
                    CHAT_PERSONAS.map((p) =>
                      e("button", {
                        key: p.key,
                        className: "persona-chip" + (config.chatPersona === p.key ? " active" : ""),
                        onClick: () => setConfig({ ...config, chatPersona: p.key }),
                        title: lang === "en" ? p.en : lang === "fr" ? p.fr : p.ja,
                      }, p.icon + "\u00a0" + (lang === "en" ? p.en : lang === "fr" ? p.fr : p.ja))
                    )
                  ),
                  e("div", { className: "chat-quick-bar" },
                    e("button", {
                      type: "button",
                      className: "pill pill-btn" + (config.chatAttachRuntime ? " active" : ""),
                      title: tr(lang, "chatAttachRuntime"),
                      onClick: () => setConfig({ ...config, chatAttachRuntime: !config.chatAttachRuntime }),
                    }, tr(lang, "chatAttachRuntime")),
                    e("button", {
                      type: "button",
                      className: "pill pill-btn" + (config.chatAutoTasks ? " active" : ""),
                      title: tr(lang, "chatAutoTasks"),
                      onClick: () => setConfig({ ...config, chatAutoTasks: !config.chatAutoTasks }),
                    }, tr(lang, "chatAutoTasks")),
                    (sendingCoder || sendingObserver)
                      ? e("span", { className: "chat-runtime-badge" },
                          (sendingCoder ? (lang === "fr" ? "Codeur: en cours" : lang === "en" ? "Coder: running" : "Coder: 実行中") : null),
                          (sendingCoder && sendingObserver) ? " / " : null,
                          (sendingObserver ? (lang === "fr" ? "Observer: en cours" : lang === "en" ? "Observer: running" : "Observer: 実行中") : null)
                        )
                      : null,
                    e("span", { className: "chat-quick-spacer" }),
                    e("button", {
                      type: "button",
                      className: "pill pill-btn",
                      title: tr(lang, "chatExplainLastError"),
                      onClick: () => {
                        const err = lastCoderErrorDigest(600);
                        if (!err) {
                          showToast(lang === "fr" ? "Aucune erreur récente détectée." : lang === "en" ? "No recent error detected." : "直近のエラーが見つかりませんでした。", "info");
                          return;
                        }
                        const prompt =
                          lang === "fr"
                            ? "Analyse le dernier échec (voir snapshot runtime) et propose la prochaine action concrète (commande/fichier)."
                            : lang === "en"
                              ? "Diagnose the latest failure (see runtime snapshot) and propose the next concrete action (command/file)."
                              : "直近の失敗（runtime snapshot参照）を原因分析して、次の具体アクション（コマンド/ファイル）を1つ提案して。";
                        sendChat(prompt);
                      },
                    }, tr(lang, "chatExplainLastError")),
                    e("button", {
                      type: "button",
                      className: "pill pill-btn",
                      title: tr(lang, "chatWhatsHappening"),
                      onClick: () => {
                        const prompt =
                          lang === "fr"
                            ? "Résume ce que fait le runtime (Coder/Observer) en ce moment (snapshot) et quel est le prochain pas probable."
                            : lang === "en"
                              ? "Summarize what the runtime (Coder/Observer) is doing right now (snapshot) and the most likely next step."
                              : "いまランタイム（Coder/Observer）が何をしているか（snapshot参照）を要約して、次に起きそうな一手も教えて。";
                        sendChat(prompt);
                      },
                    }, tr(lang, "chatWhatsHappening"))
                  ),
                  config.chatAttachRuntime
                    ? e("details", { className: "ctx-details" },
                        e("summary", { className: "ctx-summary" }, tr(lang, "contextPreview") + ": " + tr(lang, "chatAttachRuntime")),
                        e("div", { className: "ctx-actions" },
                          e("button", {
                            className: "btn btn-icon",
                            type: "button",
                            title: tr(lang, "copy"),
                            onClick: (ev) => { ev.preventDefault(); ev.stopPropagation(); copyToClipboard(chatCtxPreview || ""); },
                          }, tr(lang, "copy"))
                        ),
                        e("pre", { className: "ctx-pre" }, chatCtxPreview || "")
                      )
                    : null,
                  e("div", { className: "composer chat-composer" },
                    e("textarea", {
                      className: "textarea",
                      value: chatInput,
                      placeholder: lang === "en" ? "Chat…" : lang === "fr" ? "Discuter…" : "話しかける…",
                      rows: Math.max(2, Math.min(8, (chatInput.match(/\n/g) || []).length + 1)),
                      style: { resize: "none" },
                      onChange: (ev) => setChatInput(ev.target.value),
                      onKeyDown: (ev) => {
                        if ((ev.metaKey || ev.ctrlKey) && ev.key === "Enter") { ev.preventDefault(); sendChat(); return; }
                        if (ev.key === "Enter" && !ev.shiftKey) { ev.preventDefault(); sendChat(); }
                      },
                    }),
                    sendingChat
                      ? e("button", { className: "btn btn-warn", onClick: stopChat }, tr(lang, "stop"))
                      : e("button", { className: "btn btn-primary", onClick: () => sendChat() }, tr(lang, "send"))
                  ),
                  e("div", { className: "statusline" },
                    e("span", { className: "dot" + (sendingChat ? " streaming" : "") }),
                    e("span", null, sendingChat ? (config.stream ? tr(lang, "streaming") : tr(lang, "sending")) : tr(lang, "ready")),
                    paneMessages("chat").length > 0 && e("span", {
                      style: { marginLeft: "auto", color: "var(--faint)", fontSize: 11, fontFamily: "var(--mono)" },
                    }, paneMessages("chat").length + " msgs")
                  )
                )
              : observerSubTab === "meta"
                ? e(React.Fragment, { key: "meta" },
                    e("div", { className: "obs-scroll-zone" }, renderMetaViewer())
                  )
                : e(React.Fragment, { key: "analysis" },
                  e("div", { className: "obs-scroll-zone" },
                  e(
                    "div",
                    { className: "chat-find" },
                    e("input", {
                      className: "input",
                      value: observerFind,
                      placeholder: tr(lang, "findInThread"),
                      onChange: (ev) => setObserverFind(ev.target.value),
                      onKeyDown: (ev) => { if (ev.key === "Escape") setObserverFind(""); },
                      style: { flex: 1, padding: "8px 10px", fontSize: 12, fontFamily: "var(--mono)" },
                    }),
                    observerFind
                      ? e("button", { className: "btn btn-icon", type: "button", title: tr(lang, "clear"), onClick: () => setObserverFind("") }, "✕")
                      : null,
                    e("button", {
                      className: "btn btn-icon",
                      type: "button",
                      title: lang === "fr" ? "Lancer l'Observer engine" : lang === "en" ? "Run Observer engine" : "Observer engineを実行",
                      disabled: sendingObserver || coderMsgs.length === 0,
                      onClick: () => runObserverEngine(),
                      style: { marginLeft: 6 },
                    }, "⚙"),
                    observerFind
                      ? e("span", { className: "msg-ts", style: { marginLeft: 6 } }, `${observerMsgsView.length}/${observerMsgs.length}`)
                      : null
                  ),
                  e("details", { className: "ctx-details" },
                    e("summary", { className: "ctx-summary" }, tr(lang, "contextPreview") + ": " + tr(lang, "observer")),
                    e("div", { className: "ctx-actions" },
                      e("button", {
                        className: "btn btn-icon",
                        type: "button",
                        title: tr(lang, "copy"),
                        onClick: (ev) => { ev.preventDefault(); ev.stopPropagation(); copyToClipboard(observerCtxPreview || ""); },
                      }, tr(lang, "copy"))
                    ),
                    e("pre", { className: "ctx-pre" }, observerCtxPreview || "")
                  ),
                  e("div", { className: "chat-body", ref: observerBodyRef },
                    observerMsgs.length === 0
                      ? e("div", { className: "pane-empty pane-empty-obs" },
                          e("div", { className: "pane-empty-icon" }, "👁"),
                          e("p", { className: "pane-empty-hint" }, tr(lang, "observerHint")),
                          !config.autoObserve && coderMsgs.length > 0 && e("div", {
                            style: { display: "flex", gap: 10, flexWrap: "wrap", justifyContent: "center" },
                          },
                            e("button", {
                              className: "btn btn-accent obs-quick-trigger",
                              disabled: sendingObserver,
                              onClick: () => sendObserver(lang === "en"
                                ? "Please review the Coder's latest output."
                                : lang === "fr"
                                  ? "Veuillez examiner la dernière sortie du Coder."
                                  : "Coderの最新の出力をレビューしてください。"),
                            }, lang === "en" ? "▶ Observe (LLM)" : lang === "fr" ? "▶ Observer (LLM)" : "▶ LLMで観察"),
                            e("button", {
                              className: "btn obs-quick-trigger",
                              disabled: sendingObserver,
                              onClick: () => runObserverEngine(),
                            }, lang === "en" ? "⚙ Observe (engine)" : lang === "fr" ? "⚙ Observer (engine)" : "⚙ エンジン観察")
                          )
                        )
                      : observerMsgsView.length === 0
                        ? e("div", { className: "pane-empty" },
                            e("div", { className: "pane-empty-icon" }, "🔎"),
                            e("p", { className: "pane-empty-hint" }, tr(lang, "noMatches"))
                          )
                        : observerMsgsView.map(renderMessage)
                  ),
                  coderDiagnostic
                    ? e("div", { className: "diagnosticbox" },
                        e("div", { className: "diagnostic-head" },
                          e("div", null,
                            e("div", { className: "section-title", style: { margin: 0 } },
                              lang === "en" ? "Coder diagnostic" : lang === "fr" ? "Diagnostic Coder" : "Coder診断"
                            ),
                            e("div", { className: "diagnostic-sub" },
                              String(coderDiagnostic.failureMode || "needs_coder_followup") + " · " + String(coderDiagnostic.confidence || 0) + "%"
                            )
                          ),
                          e("button", {
                            className: "btn btn-primary",
                            disabled: sendingCoder,
                            onClick: () => sendDiagnosticToCoder(coderDiagnostic),
                          }, tr(lang, "sendToCoder"))
                        ),
                        coderDiagnostic.mutationAnchor && coderDiagnostic.mutationAnchor.path
                          ? e("div", { className: "diagnostic-row" },
                              e("span", { className: "diagnostic-label" }, "anchor"),
                              e("code", null, String(coderDiagnostic.mutationAnchor.path || ""))
                            )
                          : null,
                        coderDiagnostic.requiredFollowups && coderDiagnostic.requiredFollowups.length
                          ? e("div", { className: "diagnostic-row" },
                              e("span", { className: "diagnostic-label" }, "follow-up"),
                              e("span", null, coderDiagnostic.requiredFollowups.map((f) => String((f && f.path) || "")).filter(Boolean).join(", "))
                            )
                          : null,
                        coderDiagnostic.verificationCmd
                          ? e("div", { className: "diagnostic-row" },
                              e("span", { className: "diagnostic-label" }, "verify"),
                              e("code", null, String(coderDiagnostic.verificationCmd || ""))
                            )
                          : null,
                        coderDiagnostic.nextCoderAction
                          ? e("pre", { className: "diagnostic-action" }, JSON.stringify(coderDiagnostic.nextCoderAction, null, 2))
                          : null
                      )
                    : null,
                  benchmarkPlan
                    ? e("div", { className: "diagnosticbox benchmarkbox" },
                        e("div", { className: "diagnostic-head" },
                          e("div", null,
                            e("div", { className: "section-title", style: { margin: 0 } },
                              lang === "en" ? "Benchmark plan" : lang === "fr" ? "Plan benchmark" : "ベンチ計画"
                            ),
                            e("div", { className: "diagnostic-sub benchmark-sub" },
                              String(benchmarkPlan.lane || "none") + " · " + String(benchmarkPlan.confidence || 0) + "%"
                            )
                          ),
                          e("button", {
                            className: "btn btn-primary",
                            disabled: sendingCoder,
                            onClick: () => sendBenchmarkPlanToCoder(benchmarkPlan),
                          }, tr(lang, "sendToCoder"))
                        ),
                        benchmarkPlan.caseIdHint
                          ? e("div", { className: "diagnostic-row" },
                              e("span", { className: "diagnostic-label" }, "case"),
                              e("code", null, String(benchmarkPlan.caseIdHint || ""))
                            )
                          : null,
                        benchmarkPlan.objective
                          ? e("div", { className: "diagnostic-row" },
                              e("span", { className: "diagnostic-label" }, "objective"),
                              e("span", null, String(benchmarkPlan.objective || ""))
                            )
                          : null,
                        benchmarkPlan.requiredChecks && benchmarkPlan.requiredChecks.length
                          ? e("div", { className: "diagnostic-row" },
                              e("span", { className: "diagnostic-label" }, "checks"),
                              e("span", null, benchmarkPlan.requiredChecks.map((x) => String(x || "")).filter(Boolean).join(", "))
                            )
                          : null,
                        benchmarkPlan.successCriteria && benchmarkPlan.successCriteria.length
                          ? e("pre", { className: "diagnostic-action benchmark-action" }, benchmarkPlan.successCriteria.map((x) => "- " + String(x || "")).join("\n"))
                          : null
                      )
                    : null,
                  criticalPath
                    ? e("div", { className: "critical-path-banner" },
                        e("span", { className: "critical-path-icon" }, "⚠"),
                        e("span", { className: "critical-path-text" }, criticalPath)
                      )
                    : null,
                  sortedTasks && sortedTasks.length
                    ? e(
                        "div",
                        { className: "taskbox" },
                        e("div", { className: "section-title", style: { margin: 0, display: "flex", alignItems: "center", gap: 10 } },
                          tr(lang, "tasks"),
                          planningTasks && e("span", { className: "pill", style: { marginLeft: "auto" } }, tr(lang, "planningTasks"))
                        ),
                        e(
                          "div",
                          { className: "task-list" },
                          sortedTasks.map((t) => {
                            const tgt = String(t.target || "coder") === "observer" ? "O" : "C";
                            const sendLabel = tgt === "O" ? tr(lang, "sendToObserver") : tr(lang, "sendToCoder");
                            const done = String(t.status || "") === "done";
                            return e(
                              "div",
                              { key: t.id, className: "task task-" + String(t.status || "new") },
                              e("div", { className: "task-head" },
                                e("span", { className: "task-target", title: String(t.target || "coder") }, tgt),
                                e("div", { className: "task-title", title: String(t.title || "") }, String(t.title || "(untitled)")),
                                t.phase && t.phase !== "any" && e("span", { className: "task-phase" }, String(t.phase)),
                                typeof t.priority === "number" && e("span", { className: "task-prio" }, String(t.priority) + "pt"),
                                e("span", { className: "task-status" }, String(t.status || "new")),
                                e("div", { className: "task-actions" },
                                  e("button", {
                                    className: "btn btn-primary",
                                    disabled: done,
                                    onClick: () => dispatchTask(t),
                                  }, sendLabel),
                                  e("button", {
                                    className: "btn",
                                    disabled: done,
                                    onClick: () => markTaskDone(t),
                                  }, tr(lang, "done"))
                                )
                              ),
                              t.body ? e("pre", { className: "task-body" }, String(t.body || "").trim()) : null
                            );
                          })
                        )
                      )
                    : null,
                  sortedProposals && sortedProposals.length
                    ? e(
                  "div",
                  { className: "proposalbox" },
                  e("div", { className: "section-title", style: { margin: 0 } }, tr(lang, "proposals")),
                  e(
                    "div",
                    { className: "proposal-list" },
                    sortedProposals.map((p) => {
                      const score = typeof p.score === "number" ? p.score : 50;
                      const phaseMismatch = observerPhase && p.phase && p.phase !== "any" && p.phase !== observerPhase;
                      const alreadyApproved = approvedProposalTitles && approvedProposalTitles.has(normTitle(p.title));
                      const scoreColor = score >= 70 ? "var(--accent)" : score >= 40 ? "var(--accent2)" : "var(--warn)";
                      const phaseLabel = p.phase && p.phase !== "any" ? p.phase : null;
                      const metaOp =
                        status && status.features && status.features.meta_prompts
                          ? parseMetaPromptOp(String(p.toCoder || ""))
                          : null;
                      return e(
                        "div",
                        { key: p.id, className: "proposal sev-" + p.severity + (phaseMismatch ? " phase-mismatch" : "") },
                        e("div", { className: "proposal-head" },
                          e("div", { style: { display: "flex", flexDirection: "column", gap: 4, flex: 1, minWidth: 0 } },
                            e("div", { style: { display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" } },
                              e("div", { className: "proposal-title" }, p.title),
                              phaseLabel && e("span", {
                                className: "proposal-phase-badge " + (phaseMismatch ? "phase-miss" : "phase-match"),
                                title: phaseMismatch ? `${tr(lang, "phaseMismatch")} (${observerPhase} -> ${p.phase})` : p.phase,
                              }, p.phase),
                              p.cost && e("span", { className: "cost-badge" }, p.cost),
                              p.status && p.status !== "new" && e("span", {
                                className: "status-badge status-" + (
                                  p.status.includes("UNRESOLVED") ? "unresolved" :
                                  p.status.includes("ESCALATED") ? "escalated" :
                                  p.status === "addressed" ? "addressed" : "info"
                                ),
                              }, p.status),
                              alreadyApproved && e("span", { className: "status-badge status-approved" }, tr(lang, "approved")),
                            ),
                            e("div", { className: "proposal-score-bar" },
                              e("div", { className: "proposal-score-fill", style: { width: score + "%", background: `linear-gradient(90deg, ${scoreColor}88, ${scoreColor})` } })
                            ),
                            e("div", { style: { display: "flex", gap: 8, alignItems: "center" } },
                              e("span", { className: "proposal-score-text" }, score + "pt"),
                              p.impact && e("span", { style: { fontSize: 11, color: "var(--muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, p.impact),
                            ),
                            p.quote && p.quote !== "n/a" && e("div", {
                              style: { fontSize: 11, fontFamily: "var(--mono)", color: "rgba(45,212,191,0.7)",
                                       overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", marginTop: 2 },
                              title: p.quote,
                            }, "❝ " + p.quote),
                          ),
                          e("div", { className: "proposal-actions" },
                            score < 30 && e("span", { style: { fontSize: 10, color: "var(--faint)", fontFamily: "var(--mono)", whiteSpace: "nowrap" } }, "低優先"),
                            metaOp && e("button", {
                              className: "btn",
                              disabled: metaBusy,
                              onClick: () => applyMetaOp(metaOp),
                              title: "Apply to runtime prompt (requires approval)",
                            }, tr(lang, "applyMeta")),
                            e("button", {
                              className: "btn btn-primary",
                              disabled: alreadyApproved || sendingCoder || !String(p.toCoder || "").trim(),
                              onClick: () => {
                                if (alreadyApproved) { showToast(tr(lang, "alreadyApproved"), "info"); return; }
                                if (phaseMismatch && !confirmPhaseMismatch(lang, observerPhase, p.phase)) return;
                                setProposalModal(p);
                                setProposalModalText(String(p.toCoder || "").trim());
                              },
                              title: alreadyApproved
                                ? tr(lang, "alreadyApproved")
                                : (phaseMismatch ? `${tr(lang, "phaseMismatch")} (${observerPhase} -> ${p.phase})` : ""),
                            }, tr(lang, "sendToCoder"))
                          )
                        ),
                        p.toCoder ? e(
                          "div",
                          null,
                          e("button", {
                            className: "proposal-toggle",
                            onClick: () => setExpandedProposals((prev) => {
                              const next = new Set(prev);
                              if (next.has(p.id)) next.delete(p.id); else next.add(p.id);
                              return next;
                            }),
                          }, (expandedProposals.has(p.id) ? "▼ " + tr(lang, "hide") : "▶ " + tr(lang, "details"))),
                          expandedProposals.has(p.id)
                            ? e("pre", { className: "proposal-body" }, String(p.toCoder || "").trim())
                            : null
                        ) : null
                      );
                    })
                  )
                )
                  : null,
                  ), // end obs-scroll-zone
                  e(
                    "div",
                    { className: "composer" },
                    e("textarea", {
                      className: "textarea",
                      value: observerInput,
                      placeholder: tr(lang, "placeholder"),
                      rows: Math.max(2, Math.min(8, (observerInput.match(/\n/g) || []).length + 1)),
                      style: { resize: "none" },
                      onChange: (ev) => setObserverInput(ev.target.value),
                      onKeyDown: (ev) => {
                        if ((ev.metaKey || ev.ctrlKey) && ev.key === "Enter") {
                          ev.preventDefault();
                          sendObserver();
                          return;
                        }
                        if (ev.key === "Enter" && !ev.shiftKey) {
                          ev.preventDefault();
                          sendObserver();
                        }
                      },
                    }),
                    sendingObserver
                      ? e("button", { className: "btn btn-warn", onClick: stopObserver }, tr(lang, "stop"))
                      : e("button", { className: "btn btn-primary", onClick: sendObserver }, tr(lang, "send"))
                  ),
                  e(
                    "div",
                    { className: "statusline" },
                    e("span", {
                      className: "dot" + (sendingObserver ? " streaming" : ""),
                      style: {
                        background: PROVIDER_COLORS[observerResolvedProvider()] || (sendingObserver ? "rgba(251, 191, 36, 0.95)" : "rgba(251, 191, 36, 0.85)"),
                        transition: "background 400ms ease",
                      },
                    }),
                    e("span", null, sendingObserver ? (config.stream ? tr(lang, "streaming") : tr(lang, "sending")) : tr(lang, "ready")),
                    healthScore && e("span", {
                      className: "pill health-badge",
                      style: {
                        background: healthScore.score >= 70 ? "rgba(45,212,191,0.15)" : healthScore.score >= 40 ? "rgba(251,191,36,0.15)" : "rgba(251,113,133,0.15)",
                        borderColor: healthScore.score >= 70 ? "rgba(45,212,191,0.5)" : healthScore.score >= 40 ? "rgba(251,191,36,0.5)" : "rgba(251,113,133,0.5)",
                        color: healthScore.score >= 70 ? "var(--accent)" : healthScore.score >= 40 ? "#fbbf24" : "var(--warn)",
                      },
                      title: healthScore.rationale,
                    }, "❤ " + healthScore.score),
                    observerMsgs.length > 0 && e(
                      "span",
                      { style: { marginLeft: "auto", color: "var(--faint)", fontSize: 11, fontFamily: "var(--mono)" } },
                      "~" + observerMsgs.reduce((s, m) => s + estimateTokens(m.content), 0) + " tokens · " + observerMsgs.length + " msgs"
                    )
                  )
                ) // analysis Fragment
          )
        )
      ),
      e("div", { className: "toast-container" },
        toasts.map((t) => e("div", { key: t.id, className: "toast toast-" + t.type }, t.msg))
      ),

      // Proposal send modal
      proposalModal && e(
        "div",
        { className: "modal-overlay", onClick: () => setProposalModal(null) },
        e(
          "div",
          { className: "modal-box proposal-send-modal", onClick: (ev) => ev.stopPropagation() },
          e("div", { className: "modal-header" },
            e("div", { style: { display: "flex", flexDirection: "column", gap: 3 } },
              e("h3", null, String(proposalModal.title || "")),
              e("span", { style: { fontSize: 11, fontFamily: "var(--mono)", color: "var(--muted)" } },
                `[${String(proposalModal.severity || "info")}]` +
                (proposalModal.score != null ? ` · ${proposalModal.score}pt` : "")
              )
            ),
            e("button", { className: "btn btn-icon", title: tr(lang, "close"), onClick: () => setProposalModal(null) }, "×")
          ),
          e("textarea", {
            className: "textarea proposal-send-textarea",
            value: proposalModalText,
            onChange: (ev) => setProposalModalText(ev.target.value),
            spellCheck: false,
          }),
          e("div", { className: "modal-footer" },
            e("button", { className: "btn", onClick: () => setProposalModal(null) }, tr(lang, "close")),
            e("button", {
              className: "btn btn-primary",
              disabled: !proposalModalText.trim() || sendingCoder,
              onClick: () => {
                const to = proposalModalText.trim();
                if (!to) return;
                sendProposalToCoder({ ...proposalModal, toCoder: to });
                setProposalModal(null);
              },
            }, tr(lang, "sendToCoder"))
          )
        )
      ),

      // Reader modal (for readable long messages, especially Observer critiques)
      readerModal && e(
        "div",
        { className: "modal-overlay", onClick: () => setReaderModal(null) },
        e(
          "div",
          { className: "modal-box reader-modal", onClick: (ev) => ev.stopPropagation() },
          e("div", { className: "modal-header" },
            e("div", { style: { display: "flex", flexDirection: "column", gap: 3 } },
              e("h3", null, String(readerModal.title || tr(lang, "observer"))),
              readerModal.ts ? e("span", { style: { fontSize: 11, fontFamily: "var(--mono)", color: "var(--muted)" } }, relativeTime(readerModal.ts, lang)) : null
            ),
            e("button", { className: "btn btn-icon", title: tr(lang, "close"), onClick: () => setReaderModal(null) }, "×")
          ),
          e("div", { className: "reader-body" },
            renderWithThink(String(readerModal.content || ""), {}, null, null)
          ),
          e("div", { className: "modal-footer" },
            e("button", { className: "btn", onClick: () => setReaderModal(null) }, tr(lang, "close")),
            e("button", { className: "btn btn-primary", onClick: () => copyText(String(readerModal.content || "")) }, tr(lang, "copy"))
          )
        )
      ),

      // Keyboard shortcuts modal
      showShortcuts && e(
        "div",
        { className: "modal-overlay", onClick: () => setShowShortcuts(false) },
        e(
          "div",
          { className: "modal-box shortcuts-modal", onClick: (e) => e.stopPropagation() },
          e("div", { className: "modal-header" },
            e("h3", null, tr(lang, "shortcuts")),
            e("button", { className: "btn btn-icon", title: tr(lang, "close"), onClick: () => setShowShortcuts(false) }, "×")
          ),
          e(
            "table",
            { className: "shortcuts-table" },
            e("tbody", null,
              [
                ["Enter", tr(lang, "sendMsg")],
                ["Shift+Enter", tr(lang, "newline")],
                ["?", tr(lang, "toggleHelp")],
                ["Esc", tr(lang, "closeModals")],
                ["Ctrl+K", tr(lang, "stopStreamingCoder")],
              ].map(([key, desc]) =>
                e("tr", { key },
                  e("td", null, e("kbd", null, key)),
                  e("td", null, desc)
                )
              )
            )
          )
        )
      )
    );
  }

  // ╔══════════════════════════════════════════════════════════╗
  // ║  SECTION: Render                                         ║
  // ╚══════════════════════════════════════════════════════════╝
  try {
    if (ReactDOM.createRoot) {
      ReactDOM.createRoot(root).render(e(App));
    } else {
      ReactDOM.render(e(App), root);
    }
  } catch (err) {
    root.textContent = String(err && err.message ? err.message : err);
  }
})();
