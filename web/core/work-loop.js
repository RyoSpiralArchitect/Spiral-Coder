(() => {
  "use strict";
  const cache = new Map();
  const clone = value => JSON.parse(JSON.stringify(value));
  const key = id => "spiral-coder.work.v1." + id;
  function read(storage, threadId) {
    if (cache.has(threadId)) return cache.get(threadId);
    let work = null;
    try { work = JSON.parse(storage.getItem(key(threadId)) || "null"); } catch (_) {}
    if (!work || work.version !== 1 || !Array.isArray(work.messages) || !Array.isArray(work.requests)) return null;
    cache.set(threadId, work);
    return work;
  }
  function persist(storage, threadId, work) {
    if (cache.get(threadId) !== work) return false;
    // Contains native messages/receipts only, never request bodies or API keys.
    storage.setItem(key(threadId), JSON.stringify(work));
    return true;
  }
  function open(storage, threadId, root, limit, renew) {
    let work = read(storage, threadId);
    if (renew) {
      work = { version: 1, taskId: "work-" + Date.now() + "-" + Math.random().toString(36).slice(2), root,
        limit: Math.max(1, Math.min(64, Number(limit) || 14)), startedAt: Date.now(), revision: 0,
        requests: [], messages: [], assists: [], completed: null };
      cache.set(threadId, work);
      persist(storage, threadId, work);
    }
    if (!work || work.root !== root) throw new Error("No native task is available to resume; send the full request.");
    return work;
  }
  function reserve(storage, threadId, work, role, messages) {
    if (cache.get(threadId) !== work) throw new Error("This task was replaced.");
    if (work.requests.length >= work.limit) throw new Error("Shared task budget exhausted. Work remains incomplete.");
    const receipt = { id: work.requests.length, role, estimatedInputTokens: Math.ceil(JSON.stringify(messages || []).length / 4),
      startedAt: Date.now(), durationMs: null, providerUsage: null, status: "in_flight_or_interrupted" };
    work.requests.push(receipt);
    persist(storage, threadId, work);
    return receipt;
  }
  function finish(storage, threadId, work, receipt, usage, status) {
    receipt.durationMs = Date.now() - receipt.startedAt;
    receipt.providerUsage = usage || null;
    receipt.status = status;
    persist(storage, threadId, work);
  }
  function snapshot(storage, threadId, work, messages, completed = null) {
    if (cache.get(threadId) !== work) return;
    work.messages = clone(messages);
    work.completed = completed;
    work.revision++;
    persist(storage, threadId, work);
  }
  function isCurrent(storage, threadId, taskId, revision) {
    const work = read(storage, threadId);
    return !!work && work.taskId === taskId && work.revision === revision;
  }
  function claimAdvice(storage, threadId, work, assistId) {
    if (work.assists.some(a => a.assistId === assistId)) return false;
    work.assists.push({ assistId, requested: true, executed: false, succeeded: null, completed: null });
    persist(storage, threadId, work);
    return true;
  }

  function updateAssists(storage, threadId, work, evidence) {
    if (!evidence || !isCurrent(storage, threadId, work.taskId, work.revision)) return;
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
    persist(storage, threadId, work);
    return true;
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

  window.SpiralCoderWork = { read, open, reserve, finish, snapshot, persist, isCurrent, claimAdvice, resolveApproval, updateAssists, streamChatTools };
})();
