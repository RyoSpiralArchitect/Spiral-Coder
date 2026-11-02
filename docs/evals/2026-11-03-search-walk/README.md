# A self-authored repair to the native search tool

On the first previously unseen [search walk task](../../../benchmarks/self-dogfood-v2/search-walk-links),
the Luna Coder/Observer pair changed only three lines in src/file_tools.rs.
The code checks the directory entry's file type before walking or reading it,
skipping discovered symlinks and errors reading the entry type. Independent
checks through the actual native server passed the visible file link, two
held-out cases (including a directory alias) and the neighboring ordinary
search control. The separately built executor never loaded candidate code.
The original bug, reference, fixtures and budgets were [frozen first](plan.json).

The native session accepted completion after both final commands passed. The
independent verifier nevertheless rejected the candidate: its method chain
failed the existing rustfmt check and its otherwise accurate counts were
embedded in sentences after the requested outcome fields. The pair used 20
Coder and 6 Observer requests in 184 seconds. The diagnostic suggestion had
one contract, passed validation and led to the exact native Coder read. Scope,
protected files and patch replay passed. The first transfer candidate remains
unqualified and was not integrated. [Result and authored report](results.json)
· [Exact patch](candidate.patch) · [Handoff](trace-handoffs.json) ·
[Plain replay](plain-replay.json) · [Raw hashes](receipts.json).

The first **preparation**, before any models ran, also caught rustfmt in the
operator's independent reference. It made zero provider requests. Only reference
layout was corrected for the fresh passing preflight; the gate was unchanged.
[First preflight](first-preflight.json) · [Frozen passing preflight](preflight.json).
[H2 results](h2/README.md) show what happened when formatting became an explicit
native check. It was selected eleven times without execution.
