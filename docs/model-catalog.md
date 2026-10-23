# TUI model catalog

Checked against official provider documentation on **2026-10-23**. Type `/provider`
to choose an endpoint preset, then `/model`, Up/Down and Enter to select a model.
The catalog is curated for text and tool use. It is not an account entitlement
check; `other` or `/model <exact-id>` still accepts a custom deployment or snapshot.

| Preset | Added models | Reference |
| --- | --- | --- |
| OpenAI | `gpt-6-luna`, `gpt-6-sol`, `gpt-5.4-mini`, `gpt-5.4` | [GPT-6 API migration](https://developers.openai.com/api/docs/guides/latest-model#update-api-and-model-parameters), [GPT-5.4 mini](https://developers.openai.com/api/docs/models/gpt-5.4-mini), [GPT-5.4](https://developers.openai.com/api/docs/models/gpt-5.4) |
| Gemini | `gemini-3.8-flash`, `gemini-3.5-flash-lite`, `gemini-4-argon` | [Models](https://ai.google.dev/gemini-api/docs/models), [OpenAI compatibility](https://ai.google.dev/gemini-api/docs/openai) |
| Anthropic / Anthropic-compatible | `claude-sonnet-5-5`, `claude-haiku-5-5`, `claude-opus-5-5`, `claude-fable-5-1` | [Model IDs](https://platform.claude.com/docs/en/models/overview), [compatibility](https://platform.claude.com/docs/en/cli-sdks-libraries/libraries/openai-sdk) |
| Mistral | `mistral-small-2603`, `mistral-medium-3-5`, `mistral-large-4` | [Small 4](https://docs.mistral.ai/models/mistral-small-4-0-26-03), [Medium 3.5](https://docs.mistral.ai/models/mistral-medium-3-5-26-04), [Large 4](https://docs.mistral.ai/models/mistral-large-4-0) |

Mistral Large 4 is a **public preview**. Existing Mistral aliases remain available.
The custom OpenAI-compatible preset offers `other` because gateway deployment
names need not match upstream IDs. HF local retains `local` and `other`.

## Transport constraints

The current Coder transport uses Chat Completions with function calls. GPT-6 Luna
and Sol require `reasoning_effort: "none"` on this route. Coder, Observer and Chat
all use that setting with `max_completion_tokens` and omit explicit temperature.
GPT-5 candidates also use `max_completion_tokens` on the first request.
This does not test their reasoning-enabled behavior. GPT-6 Astra and GPT-6.1 Sol
need a Responses API tool transport; they are deliberately absent from this
catalog until that transport is implemented and tested.

Claude 5 sampling parameters are omitted on both native Messages and compatible
Chat Completions paths, including the non-streaming Observer. See the
[Sonnet 5.5 parameter constraints](https://platform.claude.com/docs/en/models/sonnet-5-5/overview#good-to-know)
and [Haiku 5.5 constraints](https://platform.claude.com/docs/en/models/haiku-5-5/overview#good-to-know).

Gemini's new preset default is `gemini-3.5-flash-lite`; Google restricts 2.5 models
to previous users. Saved preferences and explicit model arguments are retained,
and 2.5 Flash is still offered for existing users. Other preset defaults stay as
before. No evaluation manifest or weak-model baseline changes with this catalog.

## Ownership and verification

`src/config/model_catalog.rs` owns the picker lists and request compatibility.
`RunConfig` still owns the effective model; `.spiral-coder/tui_prefs.json` stores
pane choices. Choosing a model remains pane-scoped. To evaluate Coder + Observer
as one unit, select the same model in both panes (the coupled runner already
uses one shared configuration).

Regression checks cover selecting and persisting every catalog entry in all
three panes, scrolling at small terminal heights, custom IDs, and captured HTTP
payloads from Coder/Observer streaming and non-streaming routes. These checks
establish request wiring, not live model quality or universal account access.
