# S1 rejected before provider calls

The first declared six-iteration invocation was rejected by the executor's
existing four-iteration CLI limit. It exited 2 in 0.394 seconds, made zero
provider calls and executed no native task actions. No iteration report exists;
this is a failed launch, not a failed model solution or an assessed candidate.
[Plan](plan.json) · [Preflight](preflight.json) · [Retained outcome](results.json)

The operator should have checked the CLI limit before declaring six rounds.
The next condition keeps the same task and requested six-round envelope, uses
a new output directory, and changes the executor to support an explicit upper
bound of eight. CLI boundary tests and the native continuation/no-progress test
cover that change before the next run. The original outcome remains sealed.
