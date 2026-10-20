(() => {
  "use strict";
  const caches = new WeakMap();
  const clone = value => JSON.parse(JSON.stringify(value));
  const key = id => "spiral-coder.work.v1." + id;
  function baselineMutationIssue(obligations, tool, args, execKind) {
    const checks = obligations?.task_requirements?.pending_baselines || [];
    const mutates = ["write_file", "patch_file", "apply_diff"].includes(tool)
      || (tool === "exec" && execKind === "action" && !(obligations?.required_exact_checks || []).includes(String(args.command || "").trim()));
    return checks.length && mutates
      ? "[Requirement Gate] Before editing, run these exact baseline checks on the unchanged workspace:\n" + checks.join("\n") + "\nA failing baseline is expected. Keep the check unchanged and then implement the declared artifacts. No edit was executed."
      : null;
  }

  function flatDoneEvidence(baseUrl, provider = "") {
    return String(baseUrl || "").includes("generativelanguage.googleapis.com") ||
      (!String(baseUrl || "").trim() && provider === "gemini");
  }
  function doneEvidenceItems(baseUrl, provider, fields = ["criterion", "command"]) {
    if (flatDoneEvidence(baseUrl, provider)) return {
      type: "string", description: "One row per completed criterion: '<criterion reference> | <verification command>'.",
    };
    return { type: "object", properties: Object.fromEntries(fields.map(key => [key, { type: "string" }])), required: fields };
  }
  function parseDoneEvidence(value, fields = ["criterion", "command"]) {
    if (!Array.isArray(value)) return [];
    return value.flatMap(row => {
      let criterion, command;
      if (typeof row === "string") {
        const split = row.indexOf("|");
        if (split < 0) return [];
        criterion = row.slice(0, split); command = row.slice(split + 1);
      } else if (row && typeof row === "object" && !Array.isArray(row)) {
        criterion = row[fields[0]]; command = row[fields[1]];
      }
      if (typeof criterion !== "string" || typeof command !== "string" ||
          !criterion.trim() || !command.trim()) return [];
      // Shell quoting, case and internal whitespace are executable data.
      return [{ criterion: criterion.trim(), command: command.trim() }];
    });
  }
  function doneEvidenceHint(baseUrl, provider, plan, commands) {
    const row = flatDoneEvidence(baseUrl, provider)
      ? "<criterion reference> | <exact recorded command>"
      : { criterion: "<criterion reference>", command: "<exact recorded command>" };
    return "\n[Authored acceptance mapping]\nUse exactly one evidence row per completed criterion. Keep unproven criteria in remaining_acceptance. Do not rerun checks only to repair report formatting.\n" +
      JSON.stringify({ criteria: (plan?.acceptanceCriteria || []).map((criterion, i) => ({ reference: `acceptance ${i + 1}`, criterion })),
        current_commands: commands, evidence_row_shape: row });
  }
  function cacheFor(storage) {
    if (!caches.has(storage)) caches.set(storage, new Map());
    return caches.get(storage);
  }
  function read(storage, threadId) {
    const cache = cacheFor(storage);
    let raw;
    try { raw = storage.getItem(key(threadId)) || null; }
    catch (_) { cache.delete(threadId); return null; }
    const cached = cache.get(threadId);
    // Every read revalidates durable state, even if this tab already has a
    // handle. A changed task or storage revision invalidates that old handle.
    if (cached && cached.raw === raw) return cached.work;
    let work = null;
    try { work = JSON.parse(raw); } catch (_) {}
    if (!work || work.version !== 1 || !Array.isArray(work.messages) || !Array.isArray(work.requests)) work = null;
    cache.set(threadId, { work, raw });
    return work;
  }
  function persist(storage, threadId, work) {
    const cache = cacheFor(storage);
    const cached = cache.get(threadId);
    if (!cached || cached.work !== work) return false;
    if ((storage.getItem(key(threadId)) || null) !== cached.raw) {
      read(storage, threadId);
      return false;
    }
    // Contains native messages/receipts only, never request bodies or API keys.
    work.persistenceRevision = (Number(work.persistenceRevision) || 0) + 1;
    const raw = JSON.stringify(work);
    storage.setItem(key(threadId), raw);
    cached.raw = raw;
    return true;
  }
  function open(storage, threadId, root, limit, renew) {
    let work = read(storage, threadId);
    if (renew) {
      work = { version: 1, taskId: "work-" + Date.now() + "-" + Math.random().toString(36).slice(2), root,
        limit: Math.max(1, Math.min(64, Number(limit) || 14)), startedAt: Date.now(), revision: 0,
        requests: [], messages: [], assists: [], completed: null };
      const cached = cacheFor(storage).get(threadId);
      if (!cached) throw new Error("Task storage is unavailable.");
      cached.work = work;
      if (!persist(storage, threadId, work)) throw new Error("This task was replaced in another tab.");
    }
    if (!work || work.root !== root) throw new Error("No native task is available to resume; send the full request.");
    return work;
  }
  function reserve(storage, threadId, work, role, messages) {
    if (!isActive(storage, threadId, work)) throw new Error("This task was replaced or updated in another tab.");
    if (work.requests.length >= work.limit) throw new Error("Shared task budget exhausted. Work remains incomplete.");
    const receipt = { id: work.requests.length, role, estimatedInputTokens: Math.ceil(JSON.stringify(messages || []).length / 4),
      startedAt: Date.now(), durationMs: null, providerUsage: null, status: "in_flight_or_interrupted" };
    work.requests.push(receipt);
    if (!persist(storage, threadId, work)) throw new Error("This task was replaced or updated in another tab.");
    return receipt;
  }
  function finish(storage, threadId, work, receipt, usage, status) {
    if (!isActive(storage, threadId, work)) return false;
    receipt.durationMs = Date.now() - receipt.startedAt;
    receipt.providerUsage = usage || null;
    receipt.status = status;
    return persist(storage, threadId, work);
  }
  function snapshot(storage, threadId, work, messages, completed = null) {
    if (!isActive(storage, threadId, work)) return false;
    work.messages = clone(messages);
    work.completed = completed;
    work.revision++;
    return persist(storage, threadId, work);
  }
  function isActive(storage, threadId, work) {
    return !!work && read(storage, threadId) === work;
  }
  async function runCurrent(storage, threadId, work, operation) {
    const check = () => { if (!isActive(storage, threadId, work)) throw new Error("This task was replaced or updated in another tab."); };
    check();
    const result = await operation();
    check();
    return result;
  }
  function isCurrent(storage, threadId, taskId, revision) {
    const work = read(storage, threadId);
    return !!work && work.taskId === taskId && work.revision === revision;
  }
  function claimAdvice(storage, threadId, work, assistId) {
    if (!isActive(storage, threadId, work)) return false;
    if (work.assists.some(a => a.assistId === assistId)) return false;
    work.assists.push({ assistId, requested: true, executed: false, succeeded: null, completed: null });
    return persist(storage, threadId, work);
  }

  function updateAssists(storage, threadId, work, evidence) {
    if (!evidence || !isActive(storage, threadId, work)) return;
    for (const receipt of work.assists) {
      const actions = evidence.actions.filter(a => (receipt.callIds || []).includes(a.call_id));
      if (!actions.length) continue;
      Object.assign(receipt, { executed: actions.some(a => a.executed === true), succeeded: actions.every(a => a.succeeded !== null) ? actions.every(a => a.succeeded) : null, completed: work.completed, remainingChecks: evidence.pending_exact_checks });
    }
    persist(storage, threadId, work);
  }
  function resolveApproval(storage, threadId, id, item, approved) {
    const work = read(storage, threadId);
    if (!work || !item || String(item.id) !== String(id)) return false;
    const pending = work.messages.find(m => m.role === "tool" && String(m.content || "").startsWith("Awaiting approval") && String(m.content).split("\n").includes("approval_id: " + id));
    if (!pending) return false;
    const call = work.messages.flatMap(m => m.tool_calls || []).find(c => c.id === pending.tool_call_id);
    if (!call) return false;
    let args; try { args = JSON.parse(call.function.arguments); } catch (_) { return false; }
    const result = item.result;
    let content;
    if (call.function.name === "exec") {
      if (String(args.command).trim() !== String(item.command).trim()) return false;
      if (approved && (!result || typeof result.exit_code !== "number")) return false;
      content = approved ? `${result.exit_code === 0 ? "OK" : "ERROR"} (exit_code: ${result.exit_code})\ncwd: ${item.cwd || ""}\nstdout:\n${result.stdout || ""}\nstderr:\n${result.stderr || ""}` : "REJECTED BY USER";
    } else {
      if (!["write_file", "patch_file", "apply_diff"].includes(call.function.name) || item.action !== call.function.name) return false;
      if (!String(pending.content).split("\n").includes("approval_path: " + item.path)) return false;
      if (approved && (!result || result.ok !== true || typeof result.message !== "string")) return false;
      content = approved ? result.message : "REJECTED BY USER";
    }
    pending.content = content;
    work.revision++;
    return persist(storage, threadId, work);
  }
  function nativeArgumentsIssue(call) {
    const fn = call && call.function;
    let args;
    try { args = JSON.parse(fn && fn.arguments); } catch (_) {}
    if (args && typeof args === "object" && !Array.isArray(args)) return null;
    return "[Tool arguments] " + String(fn && fn.name || "unknown") + ": arguments must be a complete JSON object. The response may have ended at the output limit.\nNo tool was executed and no workspace failure was observed. Retry one native tool call matching its schema. For edits, prefer a small patch_file search/replace and split large edits across calls. Do not send a raw shell command as exec arguments. Preserve the existing task, checks and report obligations.";
  }

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
        let usage = null;
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
              } else if (evtType === "usage") {
                try { usage = JSON.parse(evtData); } catch (_) {}
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
        return { text: fullText, finishReason, toolCalls, usage };
      };

  window.SpiralCoderWork = { read, open, reserve, finish, snapshot, persist, isCurrent, isActive, runCurrent, claimAdvice, resolveApproval, updateAssists, nativeArgumentsIssue, streamChatTools, baselineMutationIssue, flatDoneEvidence, doneEvidenceItems, parseDoneEvidence, doneEvidenceHint };
})();
