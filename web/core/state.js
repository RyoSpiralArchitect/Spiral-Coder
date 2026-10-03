(() => {
  "use strict";

  // Copy on first read. Existing Spiral-Coder values always win, including empty
  // values. Keep the legacy key so a downgrade cannot discard browser state.
  function readStoredValue(storage, key) {
    try {
      const current = storage.getItem(key);
      if (current !== null) return current;
      if (!key.startsWith("spiral-coder.")) return null;
      const legacy = storage.getItem("obstral." + key.slice("spiral-coder.".length));
      if (legacy === null) return null;
      try { storage.setItem(key, legacy); } catch (_) { /* Full or read-only storage. */ }
      return legacy;
    } catch (_) {
      return null;
    }
  }

  // Human steering starts a new run contract. Runtime approval messages resume
  // the latest human task; their text must not replace that task's constraints.
  function rootUserTextForRun(text, history, origin = "user") {
    if (origin !== "runtime") return String(text || "").trim();
    const messages = Array.isArray(history) ? history : [];
    for (let i = messages.length - 1; i >= 0; i--) {
      const msg = messages[i];
      if (!msg || msg.role !== "user" || msg.origin === "runtime") continue;
      const content = String(msg.content || "").trim();
      if (content) return content;
    }
    return String(text || "").trim();
  }

  function serverSupportsFeature(status, name) {
    return !!(status && status.ok && status.features && status.features[name] === true);
  }

  function configWithoutSecrets(config) {
    const safe = { ...config };
    for (const key of ["apiKey", "chatApiKey", "codeApiKey", "observerApiKey"]) delete safe[key];
    return safe;
  }

  function credentialRoute(config, pane) {
    const prefix = pane === "code" ? "code" : pane === "observer" ? "observer" : "";
    const value = (key) => String(config[key] || "").trim();
    return JSON.stringify([
      (prefix && value(prefix + "Provider")) || value("provider"),
      ((prefix && value(prefix + "BaseUrl")) || value("baseUrl")).replace(/\/+$/, ""),
    ]);
  }

  // An empty pane key may use another pane's key only when both credentials
  // target the same provider and endpoint. Provider identity alone is not enough
  // for custom OpenAI-compatible endpoints.
  function paneApiKey(config, keys, pane) {
    const route = credentialRoute(config, pane);
    const order = pane === "code" ? ["code", "chat", "observer"]
      : pane === "observer" ? ["observer", "chat", "code"] : ["chat", "code", "observer"];
    for (const candidate of order) {
      if (credentialRoute(config, candidate) !== route) continue;
      const key = String(keys[candidate] || "").trim();
      if (key) return key;
    }
    return "";
  }

  window.SpiralCoderState = {
    readStoredValue, rootUserTextForRun, serverSupportsFeature,
    configWithoutSecrets, credentialRoute, paneApiKey,
  };
})();
