# S2: connection loss before the response

S2 made 20 actual wire requests (14 Coder, six Observer) and stopped in its
first iteration after 470.086 seconds. The command absence was reproduced;
the pair edited src/main.rs and created src/coupled_status.rs. Its latest
compiler failure was a missing Args derive macro after an import repair.
No final report, guide or npm route was delivered. The independently replayed
candidate fails to build and is unqualified. [Frozen results](results.json)

The last Coder send failed after establishing a connection, before HTTP
response headers: `client error (SendRequest): connection closed before message
completed`. The existing retry classifier accepted timeout/connect errors but
missed reqwest Request errors. The Observer had recovered from an earlier
send failure; that does not establish a role-specific retry policy.

The native command replay and sealed path/protected-file checks remain intact.
Dependent binary checks were not executed after the failed build. S2 is retained
as a provider-interrupted incomplete candidate, not restarted in its output
directory. [Original candidate](candidate.patch) · [Context measurements](context.json)

A local TCP reproducer independently closes a fully read POST before response
headers. It fails with the same error chain on the old code. The next executor
admits that native request-stage failure into the existing bounded retry loop,
marks the failed wire as send_failed and charges a fresh ordinary task request
for each retry. Shared-budget exhaustion and the existing retry cap still stop
execution; no transport-repair credit or partial tool response is reused.
