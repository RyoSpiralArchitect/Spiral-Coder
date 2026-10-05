(() => {
  "use strict";

  const g = (typeof window !== "undefined") ? window : globalThis;
  const SpiralCoder = g.SpiralCoder || (g.SpiralCoder = {});
  if (SpiralCoder.observer) return;

  function normalizeForSim(s) {
    let t = String(s || "");
    // Remove fenced code blocks and inline code to reduce false positives.
    t = t.replace(/```[\s\S]*?```/g, " ");
    t = t.replace(/`[^`]*`/g, " ");
    t = t.toLowerCase();
    t = t.replace(/https?:\/\/\S+/g, " ");
    // Keep latin, extended latin, numbers, and common CJK ranges.
    t = t.replace(/[^a-z0-9\u00c0-\u024f\u3040-\u30ff\u3400-\u9fff]+/g, " ");
    t = t.replace(/\s+/g, " ").trim();
    return t;
  }

  function tokenSetForSim(s) {
    const t = normalizeForSim(s);
    const out = new Set();
    if (!t) return out;

    // Word-ish tokens.
    t.split(" ").forEach((w) => {
      const x = String(w || "").trim();
      if (x.length >= 2) out.add(x);
    });

    // For CJK-heavy text, add bigrams so we can detect repetition even without spaces.
    const cjk = t.replace(/[a-z0-9\u00c0-\u024f ]+/g, "");
    if (cjk.length >= 8) {
      for (let i = 0; i < cjk.length - 1 && out.size < 2400; i++) {
        out.add(cjk.slice(i, i + 2));
      }
    }
    return out;
  }

  function jaccardSim(aSet, bSet) {
    if (!aSet || !bSet || !aSet.size || !bSet.size) return 0;
    let inter = 0;
    for (const x of aSet) if (bSet.has(x)) inter++;
    const union = aSet.size + bSet.size - inter;
    return union ? inter / union : 0;
  }

  function similarity(a, b) {
    return jaccardSim(tokenSetForSim(a), tokenSetForSim(b));
  }

  function parseProposals(text) {
    const s = String(text || "");
    const m = /---\s*proposals\s*---/i.exec(s);
    if (!m) return [];

    const tail = s.slice(m.index + m[0].length);
    const lines = tail.split(/\r?\n/);

    const out = [];
    let cur = null;
    let lastKey = "";

    const finish = () => {
      if (!cur) return;
      const title = String(cur.title || "").trim();
      const toCoder = String(cur.toCoder || "").trim();
      const severity = String(cur.severity || "info").trim().toLowerCase();
      if (!title && !toCoder) {
        cur = null;
        lastKey = "";
        return;
      }
      const rawScore = parseInt(String(cur.score || ""), 10);
      out.push({
        id: `${out.length + 1}:${title || toCoder}`.slice(0, 80),
        title: title || "(untitled)",
        toCoder,
        severity: severity === "crit" || severity === "warn" ? severity : "info",
        score: Number.isFinite(rawScore) ? Math.min(100, Math.max(0, rawScore)) : 50,
        phase: String(cur.phase || "any").trim().toLowerCase(),
        impact: String(cur.impact || "").trim(),
        cost: String(cur.cost || "").trim().toLowerCase(),
        status: String(cur.status || "new").trim(),
        quote: String(cur.quote || "").trim(),
      });
      cur = null;
      lastKey = "";
    };

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      // Stop if we hit another --- block
      if (/^\s*---/.test(line) && out.length === 0 && !cur) continue;
      if (/^\s*---/.test(line) && (out.length > 0 || cur)) {
        finish();
        break;
      }

      const start = /^\s*(\d+)\)\s*title\s*:\s*(.*)\s*$/.exec(line);
      if (start) {
        finish();
        cur = { title: start[2], toCoder: "", severity: "info" };
        lastKey = "title";
        continue;
      }

      // Also accept proposals that start with bare "title:" (no number prefix)
      const startBare = !cur && /^\s*title\s*:\s*(.+)\s*$/.exec(line);
      if (startBare) {
        finish();
        cur = { title: startBare[1], toCoder: "", severity: "info" };
        lastKey = "title";
        continue;
      }

      if (!cur) continue;

      const to = /^\s*to_coder\s*:\s*(.*)\s*$/.exec(line);
      if (to) {
        cur.toCoder = to[1];
        lastKey = "to_coder";
        continue;
      }

      const sev = /^\s*severity\s*:\s*(info|warn|crit)\b/i.exec(line);
      if (sev) {
        cur.severity = sev[1].toLowerCase();
        lastKey = "severity";
        continue;
      }

      const sc = /^\s*score\s*:\s*(\d+)/.exec(line);
      if (sc) {
        cur.score = sc[1];
        lastKey = "score";
        continue;
      }

      const ph = /^\s*phase\s*:\s*(\w+)/.exec(line);
      if (ph) {
        cur.phase = ph[1];
        lastKey = "phase";
        continue;
      }

      const imp = /^\s*impact\s*:\s*(.+)$/.exec(line);
      if (imp) {
        cur.impact = imp[1];
        lastKey = "impact";
        continue;
      }

      const co = /^\s*cost\s*:\s*(\w+)/.exec(line);
      if (co) {
        cur.cost = co[1];
        lastKey = "cost";
        continue;
      }

      const st = /^\s*status\s*:\s*(.+)$/.exec(line);
      if (st) {
        cur.status = st[1].trim();
        lastKey = "status";
        continue;
      }

      const qt = /^\s*quote\s*:\s*(.+)$/.exec(line);
      if (qt) {
        cur.quote = qt[1].trim();
        lastKey = "quote";
        continue;
      }

      // Continuation lines (indented)
      if (/^\s+/.test(line)) {
        const cont = line.trim();
        if (!cont) continue;
        if (lastKey === "to_coder") cur.toCoder = String(cur.toCoder || "") + "\n" + cont;
        if (lastKey === "title") cur.title = String(cur.title || "") + " " + cont;
        if (lastKey === "impact") cur.impact = String(cur.impact || "") + " " + cont;
      }
    }

    finish();
    return out;
  }

  function parseCriticalPath(text) {
    const s = String(text || "");
    const m = /---\s*critical_path\s*---/i.exec(s);
    if (!m) return "";
    const after = s.slice(m.index + m[0].length).trimStart();
    const line = after.split(/\r?\n/)[0].trim();
    return line.toLowerCase() === "none" || !line ? "" : line;
  }

  function parseHealthScore(text) {
    const s = String(text || "");
    const m = /---\s*health\s*---/i.exec(s);
    if (!m) return null;
    const block = s.slice(m.index + m[0].length);
    const sc = /score\s*:\s*(\d+)/.exec(block);
    const ra = /rationale\s*:\s*(.+)/.exec(block);
    if (!sc) return null;
    return {
      score: Math.min(100, Math.max(0, parseInt(sc[1], 10))),
      rationale: ra ? ra[1].trim() : "",
    };
  }

  function parseJsonSection(text, markerName) {
    const s = String(text || "");
    const re = new RegExp("---\\s*" + String(markerName || "").replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "\\s*---", "i");
    const m = re.exec(s);
    if (!m) return null;
    const after = s.slice(m.index + m[0].length).trimStart();
    const next = /\n\s*---\s*[-_\w]+\s*---/i.exec(after);
    const raw = (next ? after.slice(0, next.index) : after).trim();
    if (!raw || raw === "(unavailable)") return null;
    try {
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === "object" ? parsed : null;
    } catch (_) {
      return null;
    }
  }

  function parseCoderDiagnostic(text) {
    const d = parseJsonSection(text, "coder_diagnostic");
    if (!d) return null;
    const confidence = Number(d.confidence);
    const followups = Array.isArray(d.required_followups) ? d.required_followups : [];
    const literals = Array.isArray(d.final_handoff_literals) ? d.final_handoff_literals : [];
    const evidence = Array.isArray(d.evidence) ? d.evidence : [];
    return {
      failureMode: String(d.failure_mode || "needs_coder_followup"),
      confidence: Number.isFinite(confidence) ? Math.max(0, Math.min(100, confidence)) : 50,
      mutationAnchor: d.mutation_anchor && typeof d.mutation_anchor === "object" ? d.mutation_anchor : null,
      requiredFollowups: followups,
      verificationCmd: typeof d.verification_cmd === "string" ? d.verification_cmd : "",
      finalHandoffLiterals: literals,
      nextCoderAction: d.next_coder_action && typeof d.next_coder_action === "object" ? d.next_coder_action : null,
      evidence,
      raw: d,
    };
  }

  function parseBenchmarkPlan(text) {
    const d = parseJsonSection(text, "benchmark_plan");
    if (!d) return null;
    const confidence = Number(d.confidence);
    return {
      caseIdHint: String(d.case_id_hint || ""),
      lane: String(d.lane || "none"),
      objective: String(d.objective || ""),
      reason: String(d.reason || ""),
      confidence: Number.isFinite(confidence) ? Math.max(0, Math.min(100, confidence)) : 50,
      targetFiles: Array.isArray(d.target_files) ? d.target_files : [],
      requiredChecks: Array.isArray(d.required_checks) ? d.required_checks : [],
      successCriteria: Array.isArray(d.success_criteria) ? d.success_criteria : [],
      triggeredBy: Array.isArray(d.triggered_by) ? d.triggered_by : [],
      raw: d,
    };
  }

  function stripObserverMeta(text) {
    const s = String(text || "");
    const markers = [
      /---\s*phase\s*---/i,
      /---\s*proposals\s*---/i,
      /---\s*coder_diagnostic\s*---/i,
      /---\s*benchmark_plan\s*---/i,
      /---\s*critical_path\s*---/i,
      /---\s*health\s*---/i,
    ];
    let cut = s.length;
    for (const re of markers) {
      const m = re.exec(s);
      if (m && typeof m.index === "number") cut = Math.min(cut, m.index);
    }
    return cut < s.length ? s.slice(0, cut).trimEnd() : s;
  }

  // Persisted assistant entries record attempted assistance, including failures
  // and interrupted requests. Reloading must not silently retry a paid request;
  // the explicit next-action button remains available for a human retry.
  function hasNextActionAttempt(messages, targetId) {
    if (!targetId || !Array.isArray(messages)) return false;
    return messages.some((message) => message
      && message.pane === "observer"
      && message.role === "assistant"
      && message.metaKind === "observer_next_action"
      && message.metaTargetId === targetId);
  }

  SpiralCoder.observer = {
    normalizeForSim,
    tokenSetForSim,
    jaccardSim,
    similarity,
    parseProposals,
    parseCriticalPath,
    parseHealthScore,
    parseCoderDiagnostic,
    parseBenchmarkPlan,
    stripObserverMeta,
    hasNextActionAttempt,
  };
})();
