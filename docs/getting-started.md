# Your first Spiral-Coder session

[Documentation](README.md) · [Usage guide](usage.md)

## Install

Install Node.js 20 or later, then run:

```sh
npm install -g spiral-coder@latest
cd your-project
```

The package includes the native runtime. Check the installation with
`spiral --version`. See [installation help](npm-distribution.md) for supported
platforms, PATH issues, updates and uninstalling.

## Choose a provider

Set the key in the same terminal that starts Spiral-Coder. Choose one provider:

| Provider | Environment variable | Start command |
| --- | --- | --- |
| Gemini | `GEMINI_API_KEY` or `GOOGLE_API_KEY` | `spiral --provider gemini` |
| OpenAI | `OPENAI_API_KEY` | `spiral --provider openai` |
| Mistral | `MISTRAL_API_KEY` | `spiral --provider mistral` |

macOS/Linux example:

```sh
export GEMINI_API_KEY="your-key"
spiral --provider gemini
```

PowerShell example:

```powershell
$env:GEMINI_API_KEY = "your-key"
spiral --provider gemini
```

Replace `your-key` locally. These commands set it for the current shell session.
For OpenAI or Mistral, use the corresponding variable and provider from the table.
`/keys` displays key status and setup help; it does not enter a key for you.

Use `/model` to select a model your provider account can access. The picker also
supports manual model names. `/provider` changes the provider for the active
pane. See the dated [model catalog](model-catalog.md) for transport constraints
and the development catalog; the published npm release can contain an older list.

## Ask for one inspectable change

Start in a project whose files and test command you know. In the Coder pane,
include the behavior you want and how to check it:

> Fix the empty-input crash in @src/parser.rs. Keep the public API stable,
> add a regression test, and run `cargo test`. Explain the changed files and
> any checks you could not complete.

Use paths and commands that exist in your own project. `@path/to/file` can supply
file context directly. You can also ask Coder to inspect and propose a change
before editing.

The TUI executes tool actions automatically. If you want approval prompts before
commands and edits, use the [headless agent](usage.md#headless-coding-and-resume).

## Review and continue

After Coder finishes a response, press `Ctrl+O` to ask Observer to review it.
Review the suggested next step alongside the diff and actual check output.
`/autofix` toggles forwarding Observer suggestions back to Coder. Coder and
Observer are a working pair; separate contexts do not guarantee correct reviews.

`Ctrl+K` stops the focused stream. `Ctrl+C` exits the TUI. `/help` lists the
available controls, and `/lang ja`, `/lang en` or `/lang fr` changes the language.

## Use the local Web UI

```sh
spiral serve
```

Open `http://127.0.0.1:8080/`. In Settings, choose Provider/Model/Base URL,
configure the API key, and set `toolRoot` to your project folder. The Web UI keeps
keys in browser storage and sends them to your local server for provider calls.
Commands and edits appear as pending approvals in Settings; the header badge
opens the pending list.

For session files, project configuration and source builds, continue to the
[usage guide](usage.md).
