# PR #86：S17–S19の境界条件監査

監査日：2026-11-14（日本時間）。対象PR先頭：`5177c3a3497c85c129488a386224a2314ec69a12`。

## 結論と到達範囲

parent link、ledger task ID、完了状態、dangling pending-linkの4条件は、元の課題文に明示され、凍結済み独立評価器と参照実装の期待も一致している。保存済みの失敗を、評価器が誤った入力を拒否させた結果と解釈する根拠は見つからなかった。

ただし、PR #86が変更しているのはCoder／Observerの実行基盤であり、実験中に生成された`src/coupled_status.rs`の採用ではない。公開ツリーにはS16–S19の実装・作成テスト・候補patchの実データがなく、ハッシュと集計結果がある。今回の新規実行は、公開された正しい参照実装と明示した合成欠陥に対する事後診断である。S17–S19の原候補再実行、原候補への修正適用、新しいモデル実行の一般化は未実施として保持する。

全作業はモデル／provider呼び出し0回。GitHubへのcommit、push、コメント、PR状態変更は行っていない。

## 保存済み結果の読み直し

| 条件 | 元の独立24ケース | protected path検査 | 作成テストの4故障検出 | 元の終了状態 |
| --- | ---: | --- | ---: | --- |
| S16 | 24/24 | 通過 | 0/4 | native完了、未qualified |
| S17 | 23/24 | 通過 | 2/4 | native完了、先頭parent不正を受理、未qualified |
| S18 | 24/24 | dangling pending-linkの3件で失敗 | 1/4 | iteration limit、最終format後は6チェックの証拠がstale／pending、未qualified |
| S19 | 22/24 | 通過 | 2/4 | native完了、completed＋activeとledger task IDで失敗、未qualified |
| S16へのテスト設計修正 | 同じ保存済み実装で24/24 | 通過 | 4/4 | 補助付きのテスト専用継続。新規unseeded runではない |

S18の「6チェックがcurrentでない」は、6コマンドを実行してすべて失敗したという意味ではない。最後のformat変更で以前の成功証拠が失効し、その後の再検証を終えていない。これをS17／S19の6/6完了と同じ列で成功扱いしない。

また、S19の後続grouped replayで最終修正を戻した際の「missing pending componentの3失敗」と、S18の「dangling pending-linkの3失敗」は別の記録である。S19の最終候補のpath検査は通過している。

出典：[元の評価概要](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/docs/evals/2026-11-12-authored-tests/README.md)、[S16–S18記録](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/docs/evals/2026-11-12-authored-tests/closeout-results.json)、[S19記録](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/docs/evals/2026-11-12-authored-tests/observer-continuation-results.json)。

## 不変条件と検出経路

| 境界 | 維持すべき不変条件 | 元の評価ID | 通常チェック／作成テストが通してしまう経路 |
| --- | --- | --- | --- |
| parent link | 先頭finalized rowのparentはnull。後続rowは直前task IDと一致 | `StatusOracle.test_wrong_parent`。正常な2行chainは`test_two_rounds` | 先頭行だけ比較を省略しても正常fixtureは通る。S17に記録された4つの作成テスト名にはparent負例がなく、parent故障も未検出 |
| ledger task ID | finalized rowとそのledgerのtask IDが一致。count一致だけでは不足 | `StatusOracle.test_wrong_task_id` | 別taskの同数requestを集計できてしまう。4故障監査にはこのfault familyがない |
| 完了状態 | completedならpendingなし、stop reasonがcompleted、最終rowもcompleted。rowのterminationとも整合 | `StatusOracle.test_completion_conflict` | completedのbooleanを優先して出力するとpendingとの矛盾を見落とす。4故障監査にも含まれていない |
| dangling pending-link | 本当の不存在はunknown。存在するsymlinkは各componentで拒否し、リンク先の有無で変えない | `StatusPaths.test_symlink_components_even_dangling_are_rejected` | 最終pathの`exists()`を先に使うとdangling linkを不存在扱いできる。通常24ケースのsymlink検査はfinalized ledgerの生存linkであり、この境界を含まない |

原要件IDは`behavior-3`（parent、ID、完了）、`behavior-4`（pendingとunknown）、`behavior-6`と`native-tests`（各path component／danglingと不存在の区別）。[課題文](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/benchmarks/self-dogfood-v2/coupled-status/prompt.txt)、[元の24ケース](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/benchmarks/self-dogfood-v2/coupled-status/status_oracle.py)、[元のpath検査](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/benchmarks/self-dogfood-v2/coupled-status/status_paths.py)を照合した。

S18の公開集計には失敗した3subtestのラベルやログ本文が含まれない。active pendingのdirectory／state／ledgerで`exists()`による取り違えが起きる説明は合成実験で調べられるが、S18の実際のソースがその形だったと断定しない。

### 完了状態の評価を細分化する理由

元の`test_completion_conflict`はcompleted fixtureにactive iterationを加える。この1フィールド変更は「completed＋pending禁止」と「active＋stop reason禁止」の2条件に触れる。拒否の期待は正しいが、どちらの検査で拒否したかはこのケースだけで決まらない。

追加診断では、completed＋active、completed＋interrupted、manifest／stop reason不一致、row／termination不一致、completedな最終rowの欠落、非末尾rowの完了を分ける。合成`completed_active`欠陥は上記2ガードを同時に無効化したものだと明示する。

### dangling linkを他のエラーから切り離す

interruptedの既知countを残したままdangling linkを置くと、リンクを誤ってunknown扱いしても、後段のcount／null不一致が拒否を起こし得る。追加診断では正常なunknown-interruptedから始め、null countを維持してdangling linkだけを置く。これにより「別の検査が拒否したのでpath検査も正しい」という読み違いを防ぐ。

## 4/4のテスト検出力が保証していないもの

凍結監査のfault familyは`missing_pending`、`symlink`、`duplicate_request`、`parent_link`の4つである。作成テストを正しいreferenceへ移し、そのreferenceに故障を注入する。candidateの実装そのものを評価しているわけではない。

したがって4/4は、この4種類の注入を作成テストが区別できたという結果である。ledger task ID、完了状態、すべてのpending path componentを網羅する値ではない。また、S16の修正前duplicate-IDテストはrequest数まで変えていたため、count検査が本来のduplicate-ID欠陥を隠していた。修正後は既存requestのIDだけを変えている。[凍結sensitivity実装](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/benchmarks/self-dogfood-v2/coupled-status/test_sensitivity.py)

この監査では元の4故障の集合・分母・合否を変更しない。新しい診断のfault familyや結果は別の記録にする。

## 現PRの要件保持

最新HEADのauthored-test補助ビューでは、元の9要件IDがリンクされたまま、4条件を含む`behavior-3/4/6`と`native-tests`の説明が保持される。元の課題文で説明の保持量は3,883／4,096 bytes。`behavior-7`の説明はこの補助ビューの予算外だが、そのIDと上位の全要件情報は残る。今回の4失敗を、その説明省略で説明する根拠はない。

追加Rust回帰テストは元のpromptを読み、baseline失敗、6つのartifact編集、6つの成功checkという相関付き記録を作る。完了後にも両役へ原IDと完全な対象説明が渡ることを確認し、sourceを再編集すると6チェックが再びpendingになり、説明は残ることを確認する。実行時の意味検証やモデルの実装正確性をこのテストで証明するものではない。

S17は`acbb302`、S18は`55febcb`、S19は`c247d10`のexecutor条件である。HEAD `5177c3a`のtest-source review機構はS19後に加わっており、今回のHEAD検証をS19の実測へ遡及させない。[現在の要件保持実装](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/src/tui/agent/requirement_proof/authored_tests.rs)

## 今回追加した差分

| ファイル | 変更内容 |
| --- | --- |
| `src/tui/agent/requirement_proof/authored_tests.rs` | 上記の現prompt・最終closeout・再変更をつなぐ回帰テスト1件のみ。runtimeロジック変更なし |
| `test_boundaries.py` | 公開CLIへの5テスト、17の拒否ケース。正常controlを先に通し、countを壊さないID変異とpendingのabsence／danglingを区別 |
| `replay_reference.py` | 元baselineと公開overlayから外部workspaceを作り、reference／合成欠陥をnativeで検証。原test ID、件数、失敗箇所、timeout、skipを検査 |
| `provenance.json` | 元のS16–S19結果、patch／receipt hash、評価ID、原ファイル22件のhash、原候補の不足データを記録 |
| `results.json`、`receipt-files.json` | 今回の実測、検証ログ原文、合成欠陥と修正例。元の3評価JSONへ追記しない |

Python診断は各CLI呼び出しの前後で、file内容、mode、mtime／ctime、link自体とtargetを含むlstat inventoryを比較する。拒否は「正の終了コード、stdoutなし、stderrあり」の全条件を検査し、timeoutやsignalによる終了を検出成功に数えない。

参照replayが出す`repair-exemplars/*.repair.patch`は、明示した合成欠陥から正しいreferenceへ戻す修正例である。S17–S19の原候補に適用できるpatchとしては提示しない。実装上の修正原則は、先頭を含む全parent比較、row／ledger ID一致、完了状態の相互整合、optional不存在判定より先のcomponent別`symlink_metadata`である。[正しいreference](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/5177c3a3497c85c129488a386224a2314ec69a12/benchmarks/self-dogfood-v2/coupled-status/reference.rs)

## 新規のモデル不要実行結果

実測は[results.json](results.json)、完全なログと合成ソース／修正patchは[receipt-files.json](receipt-files.json)に保存した。後者は`files`の各keyを相対path、`text`をUTF-8本文としてそのまま復元でき、SHA-256も収録している。

| 実行 | 実測 |
| --- | --- |
| 最新HEADのauthored-test module | 既存3件＋新規1件、4/4成功 |
| 元promptのcontext compaction回帰 | 1/1成功 |
| Rust formatting | `cargo fmt --all -- --check`成功 |
| 正しいreference：元の独立oracle | 元IDのまま24/24成功 |
| 正しいreference：元のpath検査 | 3/3メソッド、26subcase通過 |
| 正しいreference：追加診断 | 5/5メソッド。正常control25回と拒否17ケース、計42回のCLI実行を通過 |
| 先頭parentチェックを外す合成欠陥 | 元の`test_wrong_parent`だけが失敗。追加診断でも先頭parentだけを検出 |
| ledger ID一致を外す合成欠陥 | 元の`test_wrong_task_id`だけが失敗。追加診断でもID不一致を検出 |
| completed＋activeを許す合成欠陥 | 元の`test_completion_conflict`だけが失敗。追加診断でもactive矛盾を検出 |
| optional pathへ早期`exists()`を置く合成欠陥 | 元のpath検査でrunningの3componentが失敗。追加診断ではunknown-interruptedも含め6ケースで検出 |
| request ID重複を許す合成欠陥 | count／chargeを保存した追加診断が検出 |
| 合成欠陥からの修正例5件 | `git apply --check`と適用に成功し、5件とも凍結referenceとbyte単位で一致 |
| 原ファイル保持 | 22件のhash一致。元の評価器／実測結果／fixture／promptに変更なし |

5つの合成欠陥はすべて追加診断で検出された。これは今回の事後診断の5条件に対する結果であり、元の4故障監査の分母やS16–S19の数値を書き換えるものではない。全suiteで想定したmethod ID／件数を確認し、errors・skips・timeouts・意図しない失敗は0だった。

修正例のstandalone mutantを5本別々にcompileしたわけではない。nativeで実行したのは、凍結referenceに5条件の明示的switchを加えて1回buildしたバイナリである。各faultを単独で有効化した状態と、全faultを無効にしたreference状態を比較した。standalone修正例について確認したのはpatch適用とreferenceへの完全復元であり、原候補修正とは分けている。

環境はLinux x86_64、Python 3.12.14、Rust／Cargo 1.100.0。native reference binary SHA-256は`f37654958c904f21b5a74f605705dfaa514a39b0e561bf264c8258043efb0f92`。最新runtime全体のRust test suite、`check-local.sh`全体、実npm package、元候補のstock identity一覧との照合は今回の限定検証に含めていない。

## 再現手順

Rust／CargoとPythonが利用できる環境で、PR先頭へこの追加差分を適用して実行する。

```bash
cargo test --locked --bin spiral-coder authored_tests::tests::
cargo test --locked --bin spiral-coder status_validation_rules_and_required_test_survive_shared_projection
cargo fmt --all -- --check
python3 -B docs/evals/2026-11-14-status-boundary-audit/replay_reference.py \
  --out /tmp/pr86-status-boundary-audit
```

`--out`はまだ存在しないrepo外directoryを指定する。replayは凍結referenceと宣言済みoverlayをコピーし、外部のコピーだけへfault switchを加えて1回buildする。`status_paths`のsetup内buildはその1回へまとめるが、元の3テストの本体・名前・fixtureは変更しない。このsetup差分を結果へ明示する。npm、stock全体、原候補のqualificationをこの実行の合否へ混ぜない。

保存済み候補のnative binaryを別途得た場合は、追加診断だけを直接実行できる。

```bash
python3 -S docs/evals/2026-11-14-status-boundary-audit/test_boundaries.py \
  --binary /absolute/path/to/spiral-coder -v
```

## 原候補への修正に必要な保存物

共通baselineは`7ad8be4dca4bea603d717a3801837cb689f560ab`。候補patchは次のSHA-256との一致を確認する。

| 候補 | candidate patch SHA-256 |
| --- | --- |
| S16 | `376af06a08e66c168c0330399d55acf8f3ada0fa4e668f5e8445c4218e3abe34` |
| S17 | `8383fc2a8ffc14125123c0291a227bccaa7329ce9ff12e780f68090b18e29b69` |
| S18 | `b2b98a9e6ae217971a073b65f2a1c85c5e4998daf7268b9b21f6c6147c021e03` |
| S19 | `a0a636caa2aa0c7a410abe81d8cbeff5811b5c5ddd25f941d6513a2c10cf00c4` |

最小限、原patchと実際のprepared baseline／overlayが必要である。凍結評価を再実行する場合はさらに元の`predeclared.json`、plan、`inputs/`、`driver/`、sealed receipt、candidate inventory、Rust test identity一覧、評価ログを揃える。baseline commitだけではtest-only overlayまで再現したことにならない。

元のRust identity一覧の既知hashは`d38da826fbaed71abb0d11222b389cd00b27d9b34cacfbb30942e051803cb880`。保存済みテストの同一性は件数だけで代用しない。S16継続の初回評価でこの一覧が欠けた記録も、その後のreadback成功も保持する。

原データを得た後は、hash照合 → コピーへの原patch replay → 今回の境界テストで失敗確認 → 最小修正 → 同じ境界と隣接正常系 → 凍結評価の再検証という順で進める。保存候補を人手で修正した結果は、新しいunseeded runの一般化結果と別に記録する。

## 保存条件と反映状況

元のbenchmark、独立評価器、参照実装、prompt、S16–S19評価JSON、既存テストIDを変更していない。原ファイル22件のhashを監査前後で照合し、一致を確認した。終了前のGitHub再読取でもPR先頭は`5177c3a`、draft／openのままだった。GitHub反映はこの追加差分のレビューと承認後に行う。
