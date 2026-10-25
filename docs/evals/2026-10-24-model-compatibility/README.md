# Model request compatibility, 2026-10-24

Four single-request protocol probes; no tools execute, retries or model substitutions.
These are transport receipts, not coding episodes or quality scores.

| Exact model | Initial request | Compatible request |
| --- | --- | --- |
| `gpt-5.6-luna` | HTTP 400: Chat Completions function tools with default reasoning rejected; API asks for Responses or reasoning none | HTTP 200 with `reasoning_effort: none`, `max_completion_tokens`, no temperature; native read_file returned |
| `mistral-large-4` | HTTP 400: greedy sampling requires top_p 1 | HTTP 200 with temperature 0 and top_p 1; native read_file returned |

[Initial Luna](initial-openai.json), [compatible Luna](compatible-openai.json),
[initial Large 4](initial-mistral.json), [compatible Large 4](compatible-mistral.json)
retain the request bodies, response model IDs, status, usage when supplied and raw
SSE SHA-256. They contain no authorization headers. Opaque provider data remains
local. A passing probe does not test a complete coding loop. Luna reasoning-enabled
capacity is not measured; the model ID is kept exactly as selected.

Source change: `fbfc4a6`. Validation: 787 Rust tests passed, one ignored, one
integration passed; TUI replay 3/3; captured first-request tests cover Coder and Observer for both
new IDs. Older weak-profile parameters are unchanged. See
[model catalog](../../model-catalog.md) for primary documentation links.
