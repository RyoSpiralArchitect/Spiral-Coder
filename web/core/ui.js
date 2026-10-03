(() => {
  "use strict";

  function isComposingKeyEvent(event) {
    const native = event.nativeEvent || event;
    // Safari may end composition before emitting the confirming Enter keydown.
    return !!(event.isComposing || native.isComposing || event.keyCode === 229 || native.keyCode === 229);
  }

  function submitComposer(event, send) {
    if (isComposingKeyEvent(event) || event.key !== "Enter") return;
    if (event.shiftKey && !event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    send();
  }

  function isAutomaticThreadTitle(title, localizedPrefix) {
    const text = String(title || "");
    return text === "Untitled" || /^Thread\s+\d+$/.test(text) || text.startsWith(localizedPrefix);
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
    return lines.join("\n");
  }

  function focusDialog(dialog) {
    if (!dialog) return undefined;
    const doc = dialog.ownerDocument;
    const previous = doc.activeElement;
    const overlay = dialog.parentElement;
    const background = Array.from(overlay.parentElement.children)
      .filter((node) => node !== overlay)
      .map((node) => [node, node.inert]);
    for (const [node] of background) node.inert = true;
    const previousOverflow = doc.body.style.overflow;
    doc.body.style.overflow = "hidden";

    const controls = () => Array.from(dialog.querySelectorAll(
      'button, input, select, textarea, a[href], [tabindex]'
    )).filter((node) => !node.disabled && node.tabIndex >= 0 && node.getClientRects().length);
    const focusFirst = () => (controls()[0] || dialog).focus();
    const onKeyDown = (event) => {
      if (event.key !== "Tab" || isComposingKeyEvent(event)) return;
      const nodes = controls();
      const index = nodes.indexOf(doc.activeElement);
      if (!nodes.length) {
        event.preventDefault();
        dialog.focus();
      } else if (index < 0 || (!event.shiftKey && index === nodes.length - 1) || (event.shiftKey && index === 0)) {
        event.preventDefault();
        (event.shiftKey ? nodes[nodes.length - 1] : nodes[0]).focus();
      }
    };
    const onFocus = (event) => {
      if (!dialog.contains(event.target)) focusFirst();
    };
    doc.addEventListener("keydown", onKeyDown, true);
    doc.addEventListener("focusin", onFocus);
    focusFirst();
    return () => {
      doc.removeEventListener("keydown", onKeyDown, true);
      doc.removeEventListener("focusin", onFocus);
      for (const [node, inert] of background) node.inert = inert;
      doc.body.style.overflow = previousOverflow;
      if (previous && previous.isConnected && typeof previous.focus === "function") previous.focus();
    };
  }

  window.SpiralCoderUI = { isComposingKeyEvent, submitComposer, isAutomaticThreadTitle, focusDialog, transcriptMd };
})();
