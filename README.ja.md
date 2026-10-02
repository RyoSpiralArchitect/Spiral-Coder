# Spiral-Coder

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)
![License](https://img.shields.io/badge/license-Apache%202.0-blue)
![UI](https://img.shields.io/badge/UI-web%20%2B%20TUI-2dd4bf)

> **ひとつのプロンプト窓では足りない。**
> Spiral-CoderはAIに「第二の脳」を与え、その二つを対立させる。

Languages: [English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md)

---

すべてのAIコーディングツールには同じ問題がある。コードを書いたモデルが、そのコードをレビューする。

それはレビューじゃない。自己弁護だ。

Spiral-CoderはCoderとObserverを**完全に別のコンテキスト**で動かすことでこれを解決する。ObserverはCoderのライブな作業コンテキストや隠れたスクラッチパッドを共有しない。見るのは、出力された結果・トランスクリプト・必要に応じたgit diffの断面だけだ。だから正直な批評ができる。

---

## UI Preview

以下の画像は、Web UI を end-to-end で流す Playwright smoke から取ったものです。

| Overview | Harness review gate |
|---|---|
| ![Spiral-Coder web overview](docs/screenshots/web-overview.png) | ![Harness review flow](docs/screenshots/harness-review-flow.png) |

| Runtime approvals |
|---|
| ![Runtime approvals flow](docs/screenshots/runtime-approval-flow.png) |

---

## Dogfood Benchmarks

Spiral-Coder では、生成した milestone repo を git に含めずに、軽量な benchmark writeup として残すようにしています。

- [Benchmark Examples](docs/examples/README.md)
- [Existing Repo Bugfix (Rust)](docs/examples/existing-repo-bugfix-rust.md)
- [Maze Game (Rust)](docs/examples/maze-game-rust.md)
- [Maze Game (pygame)](docs/examples/maze-game-pygame.md)

各ページには、eval の prompt 形状、provider/model、再実行コマンド、runtime snapshot、そして green な `runtime_eval` report から取った概算 token 使用量を載せています。

---

## 現在のBenchmark Milestone

いまの milestone は素朴ですが大事です。Spiral-Coder は `runtime_eval` から、fresh repo scaffold だけでなく既存 repo の bugfix も扱い、生成状態を git に含めず、それでも再現可能な形で結果を記録できるところまで来ています。

- `maze-game-rust-repo` は、scaffold lane が fresh な Rust repo を作り、ゲームロジックを `src/lib.rs` に置き、`src/main.rs` を runnable に保ち、最後に本物の `cargo test` で締められることを示します。
- `maze-game-pygame-repo` は、同じ closeout path が非 Rust repo でも動き、headless な `pygame` の verification command まで運べることを示します。
- `resume-session-bridge-fix` は、seed された session memory と repo-local な `.spiral-coder/progress.json` を使って既存 Rust repo の bugfix を resume し、最小の safe patch へ戻せることを示します。
- medium+ の self-dogfood stretch case である `self-fix-observer-repo-rules-review-panel` は、いま end-to-end で green です。runtime は `src/observer/repo_rules.rs` を patch し、required な `docs/runtime-architecture.md` と `.spiral-coder/tui_replay.json` の follow-up も運び、そのまま正確な verification command を final handoff に載せて閉じられます。
- PR-ready self-dogfood case の `self-fix-pr-ready-runtime-followup` も end-to-end で green です。runtime は `src/tui/agent/followup_requirements.rs` を patch し、required な `docs/state-schema.md` と `.spiral-coder/runtime_eval.json` の follow-up を運び、verified artifact path が final handoff から落ちそうな時は補完します。最新 green run は `.tmp/runtime_eval_1777229175/report.json` で、`tools=7`、`messages=22`、概算 transcript token は `3.85k` です。merge readiness は `.tmp/runtime_eval_1777229175/merge_gate.json` に出ます。
- runtime eval closeout は `report.json` の隣に generated な `merge_gate.json` を書くようになり、`spiral-coder merge-gate`、TUI の Merge tab、Web GUI の merge-gate panel から readiness を読み、passing case を approve し、各 case を hold し、destructive rollback を実行せずに rollback preview をコピーできます。
- benchmark report には provider/model に加えて概算 transcript token telemetry も入るようになり、billing 精度を装わずに run 同士を比較しやすくなりました。

これは「たまたま一度うまく作れた」から、「runtime が milestone case を再現し、そのやり方まで説明できる」への前進です。

---

## 次の伸び代

次に効くのは、もっと派手な demo を足すことより、runtime を長い仕事でも信頼できるものにしていくことです。

- **long-running の耐久性**: session resume はかなり良くなり、repo-local progress snapshot も入りましたが、数時間単位の仕事には context compaction、drift に応じた replan、checkpoint-aware な recovery がまだ欲しいです。
- **benchmark の幅**: fresh repo scaffold と既存 repo bugfix は良い入口で、次は小さな web app、観測ループに入りやすい中規模 refactor、docs/config task が効きます。
- **merge-ready な self-dogfood loop**: PR-ready fixture work には branch/checkpoint safety、generated merge gate、CLI reader、TUI/GUI 共有の human review state が入りました。次に効くのは、approved gate state を replay/eval で昇格判定して PR-ready merge workflow へつなぐ段です。
- **verification の圧力**: ひとつ test command が通るだけでも良いですが、adversarial evaluator loop、rollback/checkpoint restore、quality oriented な check を入れるとかなり強くなります。
- **telemetry の深さ**: 今の transcript token estimate は比較には役立ちますが、provider-native な usage accounting や benchmark trend history があると最適化がもっと具体的になります。
- **harness evolution の計測**: overlay、promotion candidate、人間ゲートつき apply flow は揃ってきたので、次は「どの promoted rule が未来の run を本当に良くしたか」を測る段です。

---

## なぜSpiral-Coderか

多くのLLMツールは「会話」を最適化している。
Spiral-Coderは「制御された実行ループ」を最適化する。役割分離、承認ゲート、そして毎ターンリセットされない累積批評でドリフトを止める。

これはチャットクライアントではない。
開発プロセスの制御エンジンだ。

---

## 3つの役割。3つのコンテキスト。干渉なし。

| 役割 | やること | やらないこと |
|---|---|---|
| **Coder** | 実行 — ファイル操作、シェルコマンド、エージェントループ（最大12回）、5つのツール | 自分のコードを見直すこと |
| **Observer** | 批評 — 提案をスコアリング、スルーした問題をエスカレート | コードを書くこと、Coderのライブな作業コンテキストを共有すること |
| **Chat** | 壁打ち — 設計、ゴム鴨、トレードオフ | 実行ループを邪魔すること |

別の役割。望めば別のモデル。コンテキストは常に別。

---

## Spiral-Coderはあなたが何か言う前から知っている

`tool_root` を設定すると、Spiral-Coderはプロジェクトを自動スキャンする:

```
[Project Context — auto-detected]
stack: Rust, React (no bundler)
explore:
  - Rust: read Cargo.toml first, then src/lib.rs or src/main.rs, then tests/ or examples/ before editing.
git:   branch=main  modified=2  untracked=1
recent: "fix(observe): require all 4 blocks" · "feat(agent): error classifier"
tree:
  src/          12 files  (Rust source)
  web/           4 files  (JS/CSS)
  scripts/       8 files  (PowerShell)
key:  Cargo.toml · web/app.js · README.md
```

このコンテキストは**最初のプロンプトより前に** Coderのシステムメッセージに注入される。あなたが入力を始める時点で、Coderはすでにスタック、現在のブランチ、変更ファイル、ディレクトリ構造に加えて、スタックに応じた最初の探索レシピも把握している。

TUIヘッダーにはリアルタイムバッジが表示される: `▸ Rust · React · git:main`
Web UIでは、SettingsのtoolRootフィールドの下にスタックラベルが表示される。

**スタック検出** — Spiral-Coderはマニフェストファイルを探す:
- `Cargo.toml` → Rust
- `package.json` → Node / React / TypeScript（depsを確認）
- `pyproject.toml` / `requirements.txt` → Python
- `go.mod` → Go
- `pom.xml` → Java
- `build.gradle*`, `Gemfile`, `composer.json`, `mix.exs`, `Package.swift`, `build.zig`, `*.tf`, `CMakeLists.txt`, `*.sln` / `*.csproj`, `deno.json*` → JVM / Ruby / PHP / Elixir / Swift / Zig / Terraform / C/C++ / .NET / Deno も追加で検出

スキャンはセッションごとに1回だけ実行され、200ms以内で完了し、読めないファイルは黙ってスキップする。

---

## Spiral-Coderが他と違うところ

### Observerには「後ろめたさ」がない

他のツール: 同じモデルがコードを書く → 同じモデルがレビューする → モデルは自分の選択を守る。

Spiral-Coder: Observerは毎回フレッシュなコンテキストで動く。「自分ならこう書いた」という記憶がなく、Coderのライブなスクラッチパッドも共有しない。見るのは出力・トランスクリプト・必要に応じたdiffだけ。

結果: より鋭い指摘、正直なリスク評価、言い訳のないフィードバック。

### 提案は消えない

Observerが指摘した問題をスルーすると、提案がエスカレートする:

```
new  →  [UNRESOLVED] +10pt  →  [ESCALATED] +20pt、先頭に固定表示
```

Observerは言ったことを覚えている。`critical` 警告を2回無視すると、それはボードの最上位カードになる。

### exit コードじゃなく、エラーの「種類」を見る

コマンドが失敗したとき、Spiral-Coderはモデルに生の `exit_code: 1` を渡して終わらない。まず種別を判定する:

| エラー種別 | 注入されるヒント |
|---|---|
| `ENVIRONMENT` | 環境を直せ。ソースコードを触るな。 |
| `SYNTAX` | その1ファイルだけ直せ。関係ないコードを変えるな。 |
| `PATH` | まずパスを確認しろ。確認前に作るな。 |
| `DEPENDENCY` | パッケージをインストールしてからリトライ。 |
| `NETWORK` | サービスとプロキシ変数を確認。 |
| `LOGIC` | ロジックを再読しろ。リランは意味がない。 |

PowerShell注意: エラーを出力しても `exit_code=0` になるケースがある（非終端エラー）。
Spiral-Coderはこれを `SUSPICIOUS_SUCCESS` として失敗扱いし、偽の前進ドリフトを止める。

### Coderには5つのツールがある

Coderはシェルコマンドだけに限定されない。5つの専用ツールを持っている:

| ツール | 使いどころ |
|---|---|
| `exec(command, cwd?)` | ビルド、テスト、git、パッケージインストール — シェル系全般 |
| `read_file(path)` | シェルのクォーティング問題なしにファイルの正確な内容を読む |
| `write_file(path, content)` | ファイルをアトミックに作成・上書き（親ディレクトリも自動生成） |
| `patch_file(path, search, replace)` | 正確なスニペットを置換 — 曖昧な場合はエラーで止まる |
| `apply_diff(path, diff)` | 統一形式の `@@` diff（複数hunk）を適用 — `patch_file` では小さすぎる編集向け |

`write_file` / `patch_file` / `apply_diff` はテンポラリファイル → リネームのパターンを使うため、書き込み途中でクラッシュしても破損ファイルが残らない。

`patch_file` は検索文字列が**ちょうど1回**だけ存在することを要求する。0回なら修正のためのファイルプレビューを返す。2回以上なら件数をエラーで返す。曖昧さはエラーであり、推測ではない。

**TUIのビジュアルマーカー**でどのツールが動いたか一目でわかる:
- `📄 READ` (teal) — ファイルを読み込み
- `✎ WRITE` (blue) — ファイルを作成・上書き
- `⟳ PATCH` (magenta) — スニペットを置換
- `✓` (緑) / `✗` (赤) — 成功 / エラー

### Coderは自分を疑う

最初の実作業前に Coder は acceptance criteria 付きの `<plan>` を出し、その後はツールを呼ぶたびに構造化された `<think>` を出す:

```
<plan>
goal:        完了条件
steps:       1) ... 2) ... 3) ...
acceptance:  1) 具体的な done-check 2) 具体的な done-check
risks:       ありそうな失敗モード
assumptions: 前提として置くもの
</plan>
```

```
<think>
goal:   今、何が成功すれば前進するか
step:   どの plan step に対応するか
tool:   exec|read_file|write_file|patch_file|apply_diff|search_files|list_dir|glob|done
risk:   一番ありそうな失敗パターン
doubt:  このアプローチが間違っている可能性   ← 新しいフィールド
next:   次の具体アクション / コマンド先頭
verify: 成功を確認する方法
</think>
```

runtime 側で `step` と `tool` が現在の plan と実際の tool call に一致するかも、TUI / Web GUI の両方で検証する。`acceptance:` は verification requirement にも使われるので、docs-only plan は build/check/lint、挙動変更を含む plan は実テストまで要求できる。`doubt:` は行動前の自己懐疑を強制する。

scratchpad / governor protocol 自体も `shared/governor_contract.json` を単一の正本にして、TUI はそれを直接読み、Web GUI は `/api/governor_contract` から同じ契約を取得しつつ、`app.js` 実行前に `/assets/governor_contract.js` から同じ fallback contract も読み込むようにしました。block field mapping、field alias、enum の正規化、`done` schema の必須要件、共通の `Done/Error Protocol` prompt section、`plan` / `think` / `reflect` / `impact` / `done` validator の詳細エラー、validator / gate のエラーメッセージに加えて、verification の判定語彙（intent / acceptance / path class）、verification command signature、test/build probe に使う `goal_check` runner catalog、repo goal の probe 要件（`.git` / `HEAD` / `README.md`）、loop に差し戻す `goal_check` メッセージ、TUI / Web に出す `goal_check` の実行ログ / ステータス行、さらに `goal_check` 実行ポリシー自体（auto-run 条件、retry 上限、probe 順序）もこの契約を基準に組み立てるので、prompt の仕様ドリフトを減らせます。

### ステートマシン・ループ（Planning → Executing → Verifying → Recovery）

多くの「エージェントループ」は最大反復回数のタイマーになりがちです。Spiral-CoderはCoderを小さなステートマシンでルーティングします。

- `planning`  — ゴールと次の具体ステップを言語化する
- `executing` — ツール実行（ファイル/コマンド）
- `verifying` — `goal_check` で「本当に終わったか」を検証してから止まる
- `recovery`  — stuck検出で診断（pwd/ls/git status）と戦略変更を強制する

`git status` は診断専用で、完了確認には数えない。完了前の verification は、実際の test/build/check/lint か設定済みの test command を使う。

さらに verification は acceptance-aware になっていて、docs-only なら build/check/lint で止まれる一方、コードや挙動を変えたタスクは `cargo test` / `pytest` / `npm test` のような behavioral verification が `done` 前に必要になる。

これにより、長距離ランがREADME整形ループに沈まず、収束しやすくなります。

さらに、直近のツール実行（コマンド + `write_file` / `patch_file` / `apply_diff`）を `[Recent runs]` としてコンパクトに注入し、「やったことを忘れて同じ作業を繰り返す」ループを減らします。

加えて、session messages から `[Working Memory]` も再構築します。中身は「確認済み事実」「終わった step」「効いた verification command」で、resume 時も failure memory だけでなく前進した情報を持ち越せます。

さらに、コード編集系には 3 つの追加ガードを入れています。既存ファイルへの `patch_file` / `apply_diff` の前に `<evidence>` block を必須化する `[Evidence Gate]`、root request から task の軸と verification floor を固定する `[Task Contract]`、そして open / confirmed / refuted assumption を持ち、refuted assumption の再利用を新しい証拠なしでは通さない `[Assumption Ledger]` です。TUI と Web GUI の両方で、この 3 層を同じように強制します。

さらに TUI と Web GUI の Coder runtime には `[Instruction Resolver]` も入りました。優先順位は `root/runtime safety > system/task contract > project rules > user request > execution scratchpad` で、`<plan>` / `<think>` / `<evidence>` / `<reflect>` / `<impact>` は「実行メモ」であって権威ではない、という前提を明示します。優先順位、prompt の骨格、read-only conflict の文言、diagnostic exec の判定も shared governor contract から供給されます。

さらに、成功した mutation の直後は次の tool call 前に短い `<impact>` block も必須になります。ここで「何を変えたか」と「どの acceptance criterion が前進したか」を明示させ、runtime 側でも `progress:` が実在する plan step / acceptance criterion を指しているか検証します。
TUI と Web GUI の両方で、`reflect` / `impact` の runtime gate を同じ基準で強制するようになりました。失敗や停滞のあとは、次の tool call 前に明示的な自己修正が必要です。

最後の `done` call も acceptance-aware になっていて、現在の plan の acceptance criteria について「満たしたもの」「まだ残っているもの」に加えて、「各 completed criterion をどの verification command で満たしたか」まで明示しないと通りません。runtime 側でも、coverage が漏れていないことと、引用した command が本当に成功済みかを検証します。

### stop時のゴール検証（偽の「Done」を防ぐ）

モデルが `finish_reason=stop` でツール呼び出しなしに止まった場合でも、Spiral-Coderは軽いチェック（repo init / tests / build など）を自動実行し、足りない・失敗している場合は `[goal_check]` を差し戻してループを継続させられます。この stop-path も、いまは TUI / Web GUI の両方で同じ shared policy と同じ goal-check ログ形式を使います。

Web GUI には `/meta-diagnose` のMVPも入りました。Coder の composer で `/meta-diagnose`、`/meta-diagnose last-fail`、`/meta-diagnose msg:<message-id>` を実行すると、直近失敗を Observer に JSON-only のメタ診断として送れます。失敗した Coder メッセージには `Why did this fail?` ボタンも出ます。各実行は `.spiral-coder/meta-diagnose/` に保存され、failure packet、observer prompt、raw response、parsed diagnosis、parse status を後から追えます。Observer ペインには軽量な `Meta` タブもあり、保存済み artifact の一覧、`primary_failure` の簡易件数、詳細/生JSONの確認に加えて、保存済み target / packet からの再診断もできます。TUI でも Coder / Observer の入力欄から `/meta-diagnose`、`/meta-diagnose last-fail`、`/meta-diagnose msg:coder-<index>` を実行でき、同じ artifact 群をローカル保存します。

### @ファイル参照：読み込みターンをスキップ

メッセージに `@path` と書くだけで、そのファイルの内容がプロンプトより前にコンテキストとして注入される:

```
@src/main.rs run_chatは何をしている？
@Cargo.toml @package.json 依存バージョンを並べて比較して
@src/server.rs の400行目のバグを直して
```

TUIはファイルごとに通知を表示する:
```
📎 injected: [src/main.rs] (276 lines, 8192 bytes)
```

Web UIではコンポーザーにチップが表示される（入力中にリアルタイム）:
```
📎 @src/main.rs   📎 @Cargo.toml
```

Coderはファイル内容を即座に参照できる — `read_file` の往復ターンが不要。12イテレーション上限の中で1ターンを節約することが、成功とタイムアウトの差になることがある。

### フェーズゲート: 正しいノイズを黙らせる

`core` / `feature` / `polish` のどのフェーズにいるかをObserverに伝える。マッチしない提案は自動的に暗転する。認証が壊れているときに、CSSの調整で割り込まれなくなる。

### 一目でわかるヘルス

Observerの応答は毎回スコアで終わる:

```
--- health ---
score: 74  rationale: auth is solid, tests cover happy path only
```

❤ **74** → 緑（本番相当ゾーン）。バッジはリアルタイムで更新される。

### 進捗チェックポイント

イテレーション3、6、9で、次の tool call の前に **強制自己内省** が入る:

```
<reflect>
last_outcome: success|failure|partial
goal_delta: closer|same|farther
wrong_assumption: <one short sentence>
strategy_change: keep|adjust|abandon
next_minimal_action: <one short sentence>
</reflect>
```

「ループで迷走するエージェント」と「迷子になったとき気づくエージェント」の差がここにある。

### クロスプラットフォーム（Windows / macOS / Linux）

Spiral-Coderは Windows / macOS / Linux で動く。

もともとはWindowsで作られた（Windows特有の面倒な罠を最初から潰している）が、コアのランタイムはOS非依存で、`scripts/` には PowerShell / bash の両方の入口が入っている:

- Windows: `scripts/*.ps1`（`run-ui.ps1`, `run-tui.ps1`, …）
- macOS / Linux: `scripts/*.sh`（`run-ui.sh`, `run-tui.sh`, …）

Windows側の“硬さ”（macOS/Linuxで開発していても効く）:
- WDACでEXEがブロックされる → Python Liteフォールバックサーバー（標準ライブラリのみ）
- PowerShell構文の自動変換（bash → PS）で混ざったトランスクリプトを吸収
- 企業プロキシ環境への対処
- `sh.exe` Win32 error 5 でgit対話プロンプトが壊れる環境

### プラグインレジストリ

フォークせずにSpiral-Coderを拡張:

```js
registerObserverPlugin({ name: "my-plugin", onProposal, onHealth, onPhase })
registerPhase("security-review", { label: "セキュリティレビュー", color: "#f97316" })
registerValidator(proposals => proposals.filter(p => p.score > 20))
```

`app.js` の前に `<script>` で読み込むだけ。

---

## Observerの出力フォーマット

Observerは自由書きしない。UIがカードにパースする構造化フォーマットで話す:

```
--- phase ---
core

--- proposals ---
title: 入力バリデーション未実装
toCoder: ユーザー入力を受け取る前に長さと文字種をバリデートしてください。
severity: critical
score: 88
phase: core
cost: low
impact: 不正入力によるクラッシュを防止
quote: user_input = input()

--- critical_path ---
入力バリデーションを修正してから次の機能に進んでください。

--- health ---
score: 41  rationale: コアロジックは動くが、インジェクション面が広く開いている
```

すべてのフィールドに意図がある。`quote` は問題の行をカードに固定する。`cost` は詳細を読む前にフィックスの難易度を示す。`phase` は表示制御に使われる。

---

## クイックスタート

### 0) APIキーをセット（TUI/CLI）

- OpenAI互換: `OPENAI_API_KEY` または `SPIRAL_CODER_API_KEY`
- Mistral: `MISTRAL_API_KEY`（または `SPIRAL_CODER_API_KEY`）
- Anthropic（Chat/Observerのみ）: `ANTHROPIC_API_KEY`

```powershell
$env:OPENAI_API_KEY = "..."
# または: $env:MISTRAL_API_KEY = "..."
```

```bash
export OPENAI_API_KEY="..."
# または: export MISTRAL_API_KEY="..."
```

Web UIは Settings でキーを貼り付け（ブラウザに保存され、ローカルのサーバにだけ送られる）。

**Web UI（推奨）**
```powershell
# Windows（PowerShell）
.\scripts\run-ui.ps1
# → http://127.0.0.1:18080/
```

```bash
# macOS / Linux（bash）
bash ./scripts/run-ui.sh
# → http://127.0.0.1:18080/
```

Web UI側: Settings → Provider/Model/Base URL を選択 → API key を貼り付け → `toolRoot` をプロジェクトパスに設定。

**TUI（ターミナル）**
```powershell
# Windows（PowerShell）
.\scripts\run-tui.ps1
```

```bash
# macOS / Linux（bash）
bash ./scripts/run-tui.sh
```

TUI のデフォルト:
- UI言語は英語で起動（`/lang ja|en|fr` で実行中に切り替え）。
- 右ペインは `Chat` で開く。`Ctrl+R` または `/tab observer|chat|tasks|promotions|merge` で切り替え。
- `Merge` tab は最新の runtime eval `merge_gate.json` を読む。`A` で passing case を approve、`H` で hold、`R` で refresh、`Ctrl+Y` で rollback preview を実行せずにコピーできる。
- `/keys` で、各ペインが必要とする API キーの env var / CLI flag を確認できる。
- composer で `/` を打つと、軽い slash command picker が出る。
- exact `/provider` と `/model` では、`↑/↓` と `Enter` で選ぶ picker が開く。`/provider` は `openai` / `gemini` / `anthropic-compat` / `mistral` / `anthropic` / `hf` の vendor preset を出し、`/model` は選択中 preset に応じた代表モデルと `other` を出す。
- 必須の API キーや model が欠けているペインでは、送信は走らず警告だけ出る。

**Headless Coder（CLI）**
（任意）`spiral-coder` をインストール:
- Windows（PowerShell）: `.\scripts\install.ps1`
- macOS / Linux（bash）: `bash ./scripts/install.sh`

その後:
```bash
#（任意）.spiral-coder.md テンプレを生成（stack + test_cmd）
spiral-coder init -C .

# プロジェクト内でコーディングエージェントを実行
spiral-coder agent "fix the failing test" -C . --vibe

# セッションを保存して再開できるようにする（デフォルト: .tmp/spiral_coder_session.json）
spiral-coder agent "fix the failing test" -C . --vibe --session
# 後で再開（プロンプト省略 -> 自動で「続けて」）
spiral-coder agent -C . --vibe --session

# 機械可読な成果物を出力（trace + 最終JSONスナップショット + 実行グラフ）
spiral-coder agent "fix the failing test" -C . --vibe --trace-out .tmp/spiral_coder_trace.jsonl --json-out .tmp/spiral_coder_final.json --graph-out .tmp/spiral_coder_graph.json

# 自動修正ループ（Coder → Observer差分レビュー → Coder）
spiral-coder agent "fix the failing test" -C . --vibe --autofix
spiral-coder agent "fix the failing test" -C . --vibe --autofix 3

# ツール実行を自動承認（プロンプトなし）
spiral-coder agent "fix the failing test" -C . --vibe -y

# 現在のgit diffをObserverでレビュー
spiral-coder review -C .

# チェックポイント以降の差分をレビュー（`spiral-coder agent`が出すhashを指定）
spiral-coder review -C . --base <checkpoint_hash>

# 最新 runtime eval merge gate の readiness を確認
spiral-coder merge-gate -C .
spiral-coder merge-gate -C . --json
spiral-coder merge-gate -C . --ci
# 同じ gate は TUI `/tab merge`、または Web GUI Settings -> Merge gate からも review できる
```

**Python Lite（WDAC / Rustバイナリ不可）**
```powershell
# Windows
python .\scripts\serve_lite.py
# → http://127.0.0.1:18080/
```

```bash
# macOS / Linux
python3 ./scripts/serve_lite.py
# → http://127.0.0.1:18080/
```

---

## 重要な概念

### tool_root

エージェントの全アクションはワーキングディレクトリ内で実行される。

デフォルト:
- **Web UI**: `.tmp/<thread-id>`（スレッドごとに分離）
- **TUI**: `.tmp/tui_<epoch>`（セッションごとに分離）
- **CLI**: カレントディレクトリ

実際のプロジェクトで作業するには `tool_root` をプロジェクトパスに設定する:
- **TUI**: `-C .` / `--tool-root .` フラグ、または実行中に `/root <path>` スラッシュコマンド
- **Web UI**: Settings → toolRoot フィールド
- **CLI**: `spiral-coder agent "<prompt>" -C .`

`tool_root` が設定されると、Spiral-Coderは初回使用時にスキャンしてプロジェクトコンテキストを構築する（スタック、git、ツリー）。同セッション内の後続の送信ではスキャンをスキップする。

パストラバーサルはすべてのツール境界でブロックされる: `..` コンポーネントを含むパスはエラーとして拒否される（サイレントではない）。

### 言語

- **UI言語**: TUIは `/lang ja|en|fr`（プロンプトも同時に切り替わる）。
- **Observer言語（Web UI）**: `auto`（デフォルト）は直近のユーザー入力の言語に追従（UIが英語でも日本語批評にできる）。`ui` はUIに追従。`ja`/`en`/`fr` で固定も可能。

### セッション（CLI）

`spiral-coder agent` は `--session[=<path>]` で会話全体（tool call含む）をJSONに保存し、再開できる。

- デフォルトパス: `.tmp/spiral_coder_session.json`
- `-C/--root` 指定時、相対パスの `--session` は `tool_root` からの相対として解釈される
- 実行中も（tool call後などに）自動保存
- プロンプト省略で再開: `spiral-coder agent -C . --session` をもう一度実行
- 最初からやり直し: `--new-session` を付ける（ファイルは上書き）

関連する成果物出力:
- `--trace-out <path>` / `--trace_out`: JSONL trace（tool call / checkpoint / error / done）
- `--json-out <path>` / `--json_out`: 最終セッションスナップショットJSON（messages + tool calls + tool results）
- `--graph-out <path>` / `--graph_out`: 最終メッセージから導出した実行グラフJSON（nodes + edges）
- `-C/--root` 指定時、相対パスは `tool_root` 配下で解決される

セッションJSONにはコードやツール出力が入るので、取り扱い注意。

### 承認（Approvals）

- **Web UI**: edit/command をPendingとしてキューできる。ブラウザから承認・却下。
- **CLI（`spiral-coder agent`）**: `exec` とファイル編集（`write_file` / `patch_file` / `apply_diff`）の前に承認プロンプトが出る。`-y/--yes` または `--no-approvals` でプロンプトを省略。
- **TUI**: 現状はツールを自動承認。

### プロバイダ

Spiral-Coderが今サポートしているプロバイダはこれ:

| Provider | `--provider` | デフォルト `base_url` | キーenv | ツール実行Coder |
|---|---|---|---|---|
| OpenAI互換 | `openai-compatible` | `https://api.openai.com/v1` | `SPIRAL_CODER_API_KEY` / `OPENAI_API_KEY` | ✅ |
| Mistral | `mistral` | `https://api.mistral.ai/v1` | `MISTRAL_API_KEY`（または `SPIRAL_CODER_API_KEY`） | ✅ |
| Anthropic | `anthropic` | `https://api.anthropic.com/v1` | `ANTHROPIC_API_KEY` | ❌（Chat/Observerのみ） |
| HF local（subprocess） | `hf` | `http://localhost` | *(なし)* | ❌（Chat/Observerのみ） |

補足:
- Coderのエージェントループ（`spiral-coder agent` / TUI Coder / Web agentic）は、OpenAI互換の **Chat Completions + tool calling**（`tools` / `tool_calls`）が必須 → `openai-compatible` か `mistral` を使う。
- `openai-compatible` は OpenAI互換の Chat Completions API（`/v1/chat/completions`）＋ Bearer認証を想定。`--base-url` / `SPIRAL_CODER_BASE_URL` にエンドポイント（`/v1` まで）を設定する。
- `spiral-coder list providers` / `spiral-coder list modes` / `spiral-coder list personas` で内蔵値を確認できる。

役割ごとに別モデルを設定できる: Coderのイテレーションには速いモデル、Observerの分析には強力なモデル。よくある実戦エラー: `401`（キー不正）、`429`（レート制限）、`max_tokens` / `max_completion_tokens` のパラメータ差異。
TUIではペインごとにプロバイダも分けられる（Coderは `openai-compatible` / `mistral` 必須）: `spiral-coder tui --observer-provider anthropic --observer-model claude-3-5-sonnet-latest`

### Chatペルソナ

Chat コンポーザーの上に5つのチップ。セッション中いつでも切り替え可能で、Coder / Observer のペルソナとは完全独立:

| チップ | スタイル |
|---|---|
| 😊 陽気 (cheerful) | 明るく前向きに応答 |
| 🤔 思慮深い (thoughtful) | 前提を確認しながら丁寧に |
| 🧙 師匠 (sensei) | 問いかけで気づかせるスタイル |
| 😏 皮肉屋 (cynical) | 核心を鋭く指摘 |
| 🦆 ゴム鴨 (duck) | 答えを出さず「なぜ？」で思考整理 |

### Chatは相棒（エージェントではない）

Chatはツールを実行しない。実装ランタイム（Coder/Observer）が動いている間の「同居する壁打ち相手」。

Web UIのChatにはオプションが2つある:
- **ランタイム状況を付与**: cwd / 直近エラー / 承認待ち / オープンタスクの要約を小さく注入して、「いま何してる？」がChatだけで成立する。
- **自動タスク化**: 裏でTaskRouterが会話をCoder/Observer向けの具体タスクに変換（**Tasks**に出る）。送るかどうかはユーザーが決める。

### スラッシュコマンド（TUI）

| コマンド | 効果 |
|---|---|
| `/model <name>` | セッション中にモデルを切り替え |
| `/provider <name>` | セッション中にプロバイダを切り替え（または表示。exact `/provider` で picker） |
| `/base_url <url>` | セッション中にbase_urlを切り替え（または表示、`default` で初期値へ） |
| `/persona <key>` | Coderのペルソナを切り替え |
| `/temp <0.0–2.0>` | temperatureを調整 |
| `/mode <name>` | 現在のペインのmodeを切り替え |
| `/root <path>` | 以降の送信のtool_rootを変更 |
| `/lang ja\|en\|fr` | UI・プロンプト言語を切り替え |
| `/tab <observer\|chat\|tasks\|next>` | 右側ペインを明示的に切り替え |
| `/keys` | ペインごとの API キー状態と設定方法を表示 |
| `/find <query>` | 現在のペインでメッセージをフィルタ |
| `/meta-diagnose [last-fail\|msg:coder-<index>]` | 指定したCoder失敗をObserverへJSON-only診断として送る |
| `/help` | 全コマンドを表示 |

---

## セキュリティ

デフォルトは `127.0.0.1` のみ。シェル実行は本物 — 承認は有効にしておくこと。

ファイルツールのパスはすべての呼び出しで `tool_root` に対して検証される: `tool_root` の外への絶対パスや `..` コンポーネントはエラーとして拒否される（サイレントではない）。

ネットワークに公開するなら、認証とツール実行の更なるハードニングが必須。

---

## トラブルシュート

**127.0.0.1経由でgithub.comへの接続が失敗する** — 環境変数に死んだプロキシが残っている:
```powershell
Remove-Item Env:HTTP_PROXY,Env:HTTPS_PROXY,Env:ALL_PROXY,Env:GIT_HTTP_PROXY,Env:GIT_HTTPS_PROXY -ErrorAction SilentlyContinue
```

**対話プロンプトなしでpush** (WDAC / sh.exe Win32 error 5):
```powershell
$env:GITHUB_TOKEN = "ghp_..."
.\scripts\push.ps1
```

**SSH over 443 でpush**（企業ネットワーク）:
```powershell
.\scripts\push_ssh.ps1
```

**spiral-coder.exe のアクセス拒否** — バイナリが実行中:
```powershell
.\scripts\kill-spiral-coder.ps1
```

---

## License

Apache License 2.0
