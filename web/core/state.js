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

  window.SpiralCoderState = { readStoredValue, rootUserTextForRun, serverSupportsFeature };
})();
