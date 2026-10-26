# Spiral-Coder

**CoderとObserverで、書いて、確かめて、次の一手へ。**

失敗するテスト、小さなバグ、気になっていたリファクタ。
Coderがプロジェクトを読み、編集し、Observerが別のコンテキストから
結果を見直して次の手を提案する。設計を相談したいときは、同じ画面のChatへ。

[English](README.md) · [日本語](README.ja.md) · [Français](README.fr.md)

[![npm](https://img.shields.io/npm/v/spiral-coder?color=8b8cf5)](https://www.npmjs.com/package/spiral-coder)
[![License](https://img.shields.io/badge/license-Apache_2.0-7aa2f7)](LICENSE)

## いつものプロジェクトで始める

必要なのは **Node.js 20以降** とプロバイダのAPIキー。
npmパッケージに実行ファイルを同梱しているので、Rustのインストールは不要。

```sh
npm install -g spiral-coder@latest
cd your-project
export GEMINI_API_KEY="your-key"
spiral --provider gemini
```

上はmacOS/Linuxの例。[セットアップガイド](docs/getting-started.md)には
PowerShell、OpenAI、Mistralでの始め方もある。TUIの `/provider` と `/model` で
選んだ設定はプロジェクトごとに保存され、次からは `spiral` だけで起動できる。
自分のアカウントで使えるモデルを選び、`/keys` でキーの設定状況を確認しよう。

対応環境はmacOS（Apple Silicon / Intel）、Linux（arm64 / x64、glibc 2.35以降）、Windows x64。
[インストールの補足・更新・その他の環境 →](docs/npm-distribution.md)

## まず、小さな依頼をひとつ

**Coder**欄で、たとえばこんなふうに頼んでみる。

> 失敗しているテストを再現し、原因のソースだけを修正して、もう一度テストを実行して。
> 何を変えたかと、まだ検証できていない点も教えて。

> @src/main.rs を読んで、リクエストが処理される流れを説明して。
> 変更する前に、小さく切り出せるリファクタをひとつ提案して。

> このパーサーに空入力のケースを追加して。
> 公開APIは変えず、既存のテストが通るところまで確認して。

パスやテストコマンドは、自分のプロジェクトに合わせて指定する。
結果を確かめられる大きさから始めて、次の変更へ進もう。

## 作業と見直しを、同じ場所で

- **Coder＋Observerでひとつのループ。** Coderが実行し、Observerが記録された結果を
  見直す。`Ctrl+O` でレビューを頼める。`/autofix` を有効にすると提案をCoderへ戻せる。
- **モデルを選べる。** コーディングにはOpenAI・Gemini・Mistralを利用できる。
  初期設定ではCoderとObserverが同じモデルを使い、役割ごとの設定で別の組み合わせにもできる。
- **続きから進める。** セッションを残し、ツールの結果や差分を確認しながら作業を再開できる。
- **考える時間も持てる。** Chatに切り替えて、実装の方針や設計を相談できる。

レビューの提案も判断材料のひとつ。最後は差分と検証結果を確認しよう。

| TUIでの操作 | できること |
| --- | --- |
| `/model` · `/provider` | モデル・プロバイダを選ぶ |
| `Ctrl+O` | 直前に完了したCoderの応答をObserverに見てもらう |
| `/autofix` | ObserverからCoderへの提案転送を切り替える |
| `Ctrl+K` | フォーカス中の応答を止める |
| `/lang ja` · `/lang en` · `/lang fr` | UIの言語を切り替える |
| `/help` | コマンド一覧を見る |

TUIではツール操作が自動実行される。コマンドや編集の前に承認したい場合は、
[ヘッドレスCLI](docs/usage.md#headless-coding-and-resume)を使える。

## ブラウザでも使える

```sh
spiral serve
```

**http://127.0.0.1:8080/** を開き、Settingsでプロジェクトのフォルダ、
プロバイダ、モデルを設定する。ローカルのWeb UIでもCoder・Observer・Chatを使える。

![CoderとObserverを並べたSpiral-CoderのローカルWeb UI](docs/screenshots/spiral-coder-overview.png)

*0.2.0のWeb UIをローカルで撮影。画像の撮影時にはモデルを呼び出していない。*

## さらに使う・中身を知る

- [セットアップ](docs/getting-started.md) — キー設定から最初の依頼とレビューまで。
- [使い方](docs/usage.md) — セッション、CLI、Web設定、プロジェクト設定。
- [ドキュメント入口](docs/README.md) — 目的別に探す。
- [研究・開発](docs/research/README.md) — 設計、評価、自己dogfooding、実験の記録。
- [ソースからビルド・開発に参加](docs/usage.md#build-and-contribute) — 開発中の変更を試す。
- [Observistralからの移行](docs/migration-spiral-coder.md) — コマンド名と既存データ。

[Apache License 2.0](LICENSE)
