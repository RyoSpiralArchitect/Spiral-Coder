# Spiral-Coder

![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)
![License](https://img.shields.io/badge/license-Apache%202.0-blue)
![UI](https://img.shields.io/badge/UI-web%20%2B%20TUI-2dd4bf)

> **Une seule boite de dialogue ne suffit pas.**
> Spiral-Coder donne a votre IA un deuxieme cerveau — et les fait se disputer.

Languages: [English](README.md) | [日本語](README.ja.md) | [Français](README.fr.md)

---

Tous les outils de code IA ont le meme probleme : le modele qui ecrit votre code est aussi celui qui le relit.

Ce n'est pas une revue de code. C'est un monologue de defense.

Spiral-Coder resout ca en faisant tourner Coder et Observer dans des **contextes entierement separes**. L'Observer ne partage ni le contexte de travail en direct du Coder ni son scratchpad cache. Il critique les sorties, les transcripts et, si necessaire, des snapshots de `git diff` apres coup. C'est ce qui le rend honnete.

---

## UI Preview

Ces captures proviennent des smoke tests Playwright qui parcourent l'UI Web de bout en bout.

| Overview | Harness review gate |
|---|---|
| ![Spiral-Coder web overview](docs/screenshots/web-overview.png) | ![Harness review flow](docs/screenshots/harness-review-flow.png) |

| Runtime approvals |
|---|
| ![Runtime approvals flow](docs/screenshots/runtime-approval-flow.png) |

---

## Benchmarks Dogfood

Spiral-Coder conserve maintenant de petites fiches de benchmark pour les repos generes, sans ajouter ces repos eux-memes au git du projet.

- [Benchmark Examples](docs/examples/README.md)
- [Existing Repo Bugfix (Rust)](docs/examples/existing-repo-bugfix-rust.md)
- [Maze Game (Rust)](docs/examples/maze-game-rust.md)
- [Maze Game (pygame)](docs/examples/maze-game-pygame.md)

Chaque page note la forme du prompt d'eval, le provider/modele, la commande de rerun, un snapshot runtime, et une estimation des tokens issue d'un rapport `runtime_eval` vert.

---

## Milestone de Benchmark Actuel

Le milestone actuel est simple mais important : Spiral-Coder peut maintenant couvrir a la fois des scaffolds de repos frais et un bugfix dans un repo existant depuis `runtime_eval`, garder l'etat genere hors du git du projet, et documenter le resultat de facon reproductible.

- `maze-game-rust-repo` montre que la scaffold lane peut creer un repo Rust frais, garder la logique de jeu dans `src/lib.rs`, garder `src/main.rs` executable, puis conclure avec un vrai `cargo test`.
- `maze-game-pygame-repo` montre que le meme chemin de closeout fonctionne aussi pour un repo non Rust, y compris avec une verification `pygame` headless.
- `resume-session-bridge-fix` montre que le runtime peut reprendre un bugfix Rust dans un repo existant en combinant une memoire de session seedee et une memoire repo-locale `.spiral-coder/progress.json`, puis pousser l'agent vers le plus petit patch sur.
- Le stretch case medium+ de self-dogfood `self-fix-observer-repo-rules-review-panel` passe maintenant end-to-end : le runtime patche `src/observer/repo_rules.rs`, porte les follow-ups requis sur `docs/runtime-architecture.md` et `.spiral-coder/tui_replay.json`, puis conclut avec la commande exacte de verification reussie dans le handoff final.
- Le cas self-dogfood PR-ready `self-fix-pr-ready-runtime-followup` passe aussi end-to-end : le runtime patche `src/tui/agent/followup_requirements.rs`, porte les follow-ups requis sur `docs/state-schema.md` et `.spiral-coder/runtime_eval.json`, puis repare le handoff final si un chemin d'artefact verifie risque d'etre omis. Dernier run vert : `.tmp/runtime_eval_1777229175/report.json` (`tools=7`, `messages=22`, environ `3.85k` tokens transcript), avec la readiness merge dans `.tmp/runtime_eval_1777229175/merge_gate.json`.
- Le closeout `runtime_eval` ecrit maintenant un `merge_gate.json` genere a cote de `report.json`; `spiral-coder merge-gate`, l'onglet Merge du TUI et le panneau merge-gate de la Web GUI peuvent lire la readiness, approuver les cas passants, mettre des cas en attente, et copier les previews rollback sans executer de rollback destructif.
- Les rapports de benchmark portent maintenant le provider/modele ainsi qu'une telemetrie approximative des tokens du transcript, ce qui aide a comparer les runs sans pretendre fournir une facturation exacte.

L'interet est la : on passe de « l'agent a produit un truc sympa une fois » a « le runtime peut reproduire un cas milestone et expliquer comment ».

---

## Prochaines Marges de Progression

La suite utile consiste moins a produire de plus jolies demos qu'a rendre le runtime fiable sur des travaux plus longs.

- **Robustesse long-running** : la reprise de session est deja bien meilleure, et les snapshots repo-locaux de progression sont en place, mais le vrai travail sur plusieurs heures veut encore une compaction de contexte plus forte, des replans plus agressifs en cas de drift, et une recovery mieux branchee sur les checkpoints.
- **Benchmarks plus varies** : les scaffolds de repo frais plus un bugfix sur repo existant sont un bon debut ; les prochains cas utiles sont de petites applis web, des refactors moyens, et des taches docs/config ou le bon niveau de verification est plus subtil.
- **Boucles self-dogfood merge-ready** : le travail PR-ready en fixture a maintenant une securite branch/checkpoint, un merge gate genere, un reader CLI, et un etat de revue humain partage TUI/GUI. La prochaine etape utile est de promouvoir l'etat approuve via replay/eval vers un workflow de merge PR-ready.
- **Verification plus dure** : un test qui passe, c'est bien ; des boucles evaluateur adversarial, du rollback/checkpoint restore, et des checks orientes qualite releveraient nettement le niveau minimal.
- **Telemetrie plus riche** : les estimations de tokens du transcript sont utiles pour comparer, mais une mesure provider-native et un historique d'evolution des benchmarks rendraient l'optimisation bien plus concrete.
- **Evolution du harness mesuree** : overlay, candidate de promotion, et apply avec gate humain existent deja ; l'etape suivante est de mesurer quelles regles promues ameliorent vraiment les runs futurs et lesquelles ne font qu'alourdir le prompt.

---

## Pourquoi Spiral-Coder existe

La plupart des outils LLM optimisent la conversation.
Spiral-Coder optimise des boucles d'execution controlees : roles separes, garde-fous d'approbation, et critique cumulative au lieu de repartir de zero a chaque tour.

Ce n'est pas un client de chat.
C'est un moteur de controle du processus de developpement.

---

## Trois roles. Trois contextes. Zero conflit.

| Role | Ce qu'il fait | Ce qu'il ne fait jamais |
|---|---|---|
| **Coder** | Agit — fichiers, commandes shell, boucle agentique (12 etapes max), 5 outils integres | Relire ou remettre en question son propre travail |
| **Observer** | Critique — score chaque proposition, escalade ce que vous ignorez | Ecrire du code ou partager le contexte de travail en direct du Coder |
| **Chat** | Reflechit avec vous — conception, canard en plastique, compromis | Interrompre la boucle d'execution |

Roles distincts. Modeles distincts si vous le souhaitez. Contextes toujours distincts.

---

## Ce qu'Spiral-Coder sait avant que vous parliez

Quand vous definissez `tool_root`, Spiral-Coder analyse automatiquement le projet :

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

Ce contexte est injecte dans le message systeme du Coder **avant votre premier prompt**. Quand vous commencez a taper, le Coder connait deja le stack, la branche courante, les fichiers modifies, l'arborescence, et une petite recette d'exploration adaptee au stack.

Dans le TUI, un badge s'affiche en temps reel dans l'entete : `▸ Rust · React · git:main`
Dans l'UI Web, le label du stack apparait sous le champ toolRoot dans les parametres.

**Detection du stack** — Spiral-Coder cherche les fichiers manifestes :
- `Cargo.toml` → Rust
- `package.json` → Node / React / TypeScript (inspecte les deps)
- `pyproject.toml` / `requirements.txt` → Python
- `go.mod` → Go
- `pom.xml` → Java
- `build.gradle*`, `Gemfile`, `composer.json`, `mix.exs`, `Package.swift`, `build.zig`, `*.tf`, `CMakeLists.txt`, `*.sln` / `*.csproj`, `deno.json*` → detection additionnelle pour JVM / Ruby / PHP / Elixir / Swift / Zig / Terraform / C/C++ / .NET / Deno

L'analyse s'execute une fois par session, prend moins de 200 ms et ignore silencieusement ce qu'elle ne peut pas lire.

---

## Ce qui rend Spiral-Coder different

### L'Observer n'a rien a defendre

Autres outils : meme modele ecrit le code → meme modele le relit → modele defend ses propres choix.

Spiral-Coder : contexte vierge a chaque execution de l'Observer. Il ne sait pas ce qu'il *aurait* ecrit et ne partage pas le scratchpad vivant du Coder. Il juge uniquement les sorties, les transcripts et, si besoin, les snapshots de diff.

Resultat : feedbacks plus tranchants, evaluation de risques honnete, pas de demi-mesures.

### Les propositions ne disparaissent pas

Quand l'Observer signale un probleme que vous ignorez, la proposition monte en grade :

```
new  →  [UNRESOLVED] +10pts  →  [ESCALATED] +20pts, epinglee en haut
```

L'Observer se souvient de ce qu'il a dit. Si vous ignorez un avertissement `critical` deux fois, il devient la carte la plus visible du tableau.

### Classification d'erreur, pas juste des codes de sortie

Quand une commande echoue, Spiral-Coder ne donne pas au modele un brut `exit_code: 1` en esperant le mieux. Il classe l'erreur d'abord :

| Type d'erreur | Hint injecte |
|---|---|
| `ENVIRONMENT` | Corrigez l'environnement. Ne touchez pas au code source. |
| `SYNTAX` | Corrigez le fichier exact. Ne changez pas d'autre code. |
| `PATH` | Verifiez les chemins d'abord. Ne creez pas avant de confirmer. |
| `DEPENDENCY` | Installez le package d'abord. Puis reessayez. |
| `NETWORK` | Verifiez le service et les variables proxy. |
| `LOGIC` | Relisez la logique. Ne relancez pas juste pour relancer. |

Note PowerShell : `exit_code` peut etre `0` meme si des erreurs ont ete imprimees (erreurs non bloquantes).
Spiral-Coder le signale comme `SUSPICIOUS_SUCCESS` et le traite comme un echec pour eviter les faux progres.

### Le Coder dispose de cinq outils

Le Coder n'est pas limite aux commandes shell. Il dispose de cinq outils dedies :

| Outil | Quand l'utiliser |
|---|---|
| `exec(command, cwd?)` | Build, tests, git, installation de packages — tout ce qui est shell |
| `read_file(path)` | Lire le contenu exact d'un fichier sans problemes de guillemets shell |
| `write_file(path, content)` | Creer ou ecraser un fichier de maniere atomique (repertoires parents auto-crees) |
| `patch_file(path, search, replace)` | Remplacer un extrait exact — echoue bruyamment en cas d'ambiguite |
| `apply_diff(path, diff)` | Appliquer un diff unifie `@@` (plusieurs hunks) — ideal quand `patch_file` est trop petit |

`write_file`, `patch_file` et `apply_diff` utilisent un schema fichier temporaire → renommage, donc un crash en cours d'ecriture ne laisse jamais de fichier corrompu.

`patch_file` exige que la chaine de recherche apparaisse **exactement une fois**. Zero occurrence → apercu du fichier pour auto-correction. Plusieurs occurrences → compte exact retourne en erreur. L'ambiguite est une erreur, pas une supposition.

**Marqueurs visuels dans le TUI** pour identifier l'outil utilise d'un coup d'oeil :
- `📄 READ` (bleu-vert) — fichier lu
- `✎ WRITE` (bleu) — fichier cree ou ecrase
- `⟳ PATCH` (magenta) — extrait remplace
- `✓` (vert) / `✗` (rouge) — succes / erreur

### Le Coder se remet en question

Avant le premier vrai appel d'outil, le Coder emet un `<plan>` avec des criteres d'acceptation explicites. Ensuite, avant chaque appel d'outil, il emet un bloc `<think>` structure :

```
<plan>
goal:        ce que \"done\" veut dire
steps:       1) ... 2) ... 3) ...
acceptance:  1) check concret de fin 2) check concret de fin
risks:       modes d'echec probables
assumptions: ce qui est suppose
</plan>
```

```
<think>
goal:   ce qui doit reussir maintenant
step:   a quelle etape du plan cela correspond
tool:   exec|read_file|write_file|patch_file|apply_diff|search_files|list_dir|glob|done
risk:   le mode d'echec le plus probable
doubt:  une raison pour laquelle cette approche pourrait etre fausse   ← le champ inhabituel
next:   prochaine action exacte / prefixe de commande
verify: comment confirmer que ca a fonctionne
</think>
```

Le runtime verifie aussi, dans le TUI comme dans la Web GUI, que `step` et `tool` correspondent au plan courant et a l'appel d'outil reel. Les criteres `acceptance:` alimentent aussi l'exigence de verification : un plan docs-only peut s'arreter a build/check/lint, tandis qu'un plan qui change le comportement est pousse vers de vrais tests. Le champ `doubt:` force le modele a formuler un doute avant d'agir.

Le protocole scratchpad / governor lui-meme vient maintenant d'un contrat partage unique : `shared/governor_contract.json`. Le TUI le lit directement, et la Web GUI recupere le meme contrat via `/api/governor_contract` tout en chargeant aussi le meme contrat de fallback depuis `/assets/governor_contract.js` avant l'execution de `app.js`. Le mapping des champs de bloc, les alias de champs, la normalisation des enums, les exigences du schema `done`, les sections de prompt partagees `Done/Error Protocol`, les erreurs detaillees des validateurs `plan` / `think` / `reflect` / `impact` / `done`, les messages d'erreur des validateurs / gates, ainsi que les heuristiques de verification elles-memes (termes d'intention, criteres d'acceptation, classes de chemins et signatures de commandes de verification), le catalogue des runners `goal_check` utilises pour les probes test/build, les exigences du probe repo-goal (`.git`, `HEAD`, `README.md`), les messages de relance `goal_check` reinjectes dans la boucle, les lignes de log / statut `goal_check` affichees dans le TUI et la Web GUI, et la politique d'execution `goal_check` elle-meme (conditions d'auto-run, limite de retry, ordre des probes) derivent aussi de ce contrat, ce qui reduit la derive entre prompts.

### Boucle a machine d'etats (Planning → Executing → Verifying → Recovery)

Beaucoup de "agent loops" ne sont qu'un timer. Spiral-Coder route le Coder via une petite machine d'etats :

- `planning`  — reformuler l'objectif et choisir la prochaine etape concrete
- `executing` — executer des outils (fichiers/commandes)
- `verifying` — lancer des probes `goal_check` avant de declarer "done"
- `recovery`  — detection de blocage -> diagnostics + changement de strategie

`git status` reste un diagnostic uniquement. La verification de fin doit etre une vraie commande de test/build/check/lint ou la commande de test configuree.

La verification est aussi \"acceptance-aware\" : un travail docs-only peut s'arreter apres un vrai build/check/lint, mais un changement de code ou de comportement doit passer par une verification comportementale (`cargo test`, `pytest`, `npm test`, etc.) avant `done`.

Cela aide les runs longs a converger au lieu de tourner en rond.

Spiral-Coder injecte aussi une memoire compacte `[Recent runs]` (commandes + `write_file` / `patch_file` / `apply_diff`) pour que le Coder n'oublie pas ce qu'il vient de faire et ne repete pas les memes actions.

Il reconstruit aussi une `[Working Memory]` compacte a partir des messages de session : faits confirmes, etapes terminees et commandes de verification deja valides. Un resume repart donc d'un contexte verifie, pas seulement d'une memoire des echecs.

Pour les edits de code, le Coder applique maintenant trois garde-fous supplementaires : un `[Evidence Gate]` qui impose un court bloc `<evidence>` avant `patch_file` / `apply_diff` sur un fichier existant, un `[Task Contract]` derive de la demande racine pour garder le plan aligne sur la vraie tache et le niveau minimal de verification, et un `[Assumption Ledger]` qui suit les hypotheses ouvertes / confirmees / refutees et bloque la reutilisation d'une hypothese refutee sans nouvelle preuve. Le TUI et la Web GUI appliquent maintenant ces trois couches de la meme maniere.

Les runtimes Coder du TUI et de la Web GUI ajoutent aussi un `[Instruction Resolver]` qui rend explicite la chaine de priorite : `root/runtime safety > system/task contract > project rules > user request > execution scratchpad`. Les blocs `<plan>` / `<think>` / `<evidence>` / `<reflect>` / `<impact>` sont traites comme des notes d'execution, jamais comme une autorite. L'ordre d'autorite, l'ossature du prompt, les messages de conflit read-only et la classification des commandes diagnostiques viennent maintenant du governor contract partage dans les deux runtimes.

Apres chaque mutation reussie, le Coder doit aussi emettre un court bloc `<impact>` avant le prochain appel d'outil, pour dire ce qui a reellement change et quel critere d'acceptation a avance. Le runtime verifie maintenant que `progress:` pointe vers une vraie etape du plan ou un vrai critere d'acceptation courant.
Le TUI et la Web GUI appliquent maintenant les memes gates runtime `reflect` / `impact` : apres un echec ou un blocage, l'agent doit expliciter sa correction avant le prochain appel d'outil.

L'appel final `done` devient lui aussi \"acceptance-aware\" : il doit maintenant declarer quels criteres d'acceptation du plan courant sont deja satisfaits, lesquels restent ouverts, et quelle commande de verification reussie prouve chaque critere complete. Le runtime verifie la couverture complete et que chaque commande citee est bien reelle.

### Verification des objectifs a l'arret (pas de faux "Done")

Quand le modele renvoie `finish_reason=stop` sans tool calls, Spiral-Coder peut executer automatiquement des checks legers (repo init, tests, build) et injecter un message `[goal_check]` dans la boucle si quelque chose manque ou echoue. Ce chemin de stop suit maintenant la meme politique partagee et le meme format de log goal-check dans le TUI et la Web GUI.

La Web GUI a maintenant aussi un MVP `/meta-diagnose` : lancez `/meta-diagnose`, `/meta-diagnose last-fail` ou `/meta-diagnose msg:<message-id>` dans le composeur Coder pour envoyer le dernier echec a l'Observer sous forme de diagnostic meta JSON-only. Les messages Coder en echec affichent aussi un bouton `Why did this fail?`. Chaque execution est enregistree dans `.spiral-coder/meta-diagnose/` avec le failure packet, le prompt Observer, la reponse brute, le diagnostic parse et l'etat du parse. Le panneau Observer inclut aussi un onglet `Meta` leger pour lister les artifacts sauvegardes, afficher un petit comptage par `primary_failure`, ouvrir leurs details / JSON brut, et relancer un diagnostic depuis la cible ou le packet sauvegarde. Le TUI prend aussi en charge `/meta-diagnose`, `/meta-diagnose last-fail` et `/meta-diagnose msg:coder-<index>` depuis la zone Coder ou Observer, en sauvegardant les memes artifacts localement.

### References @fichier : sautez le tour de lecture

Tapez `@chemin` n'importe ou dans votre message pour injecter le contenu du fichier comme contexte avant que votre prompt atteigne le Coder :

```
@src/main.rs que fait run_chat ?
@Cargo.toml @package.json montre-moi les versions de dependances cote a cote
corrige le bug dans @src/server.rs ligne 400
```

Le TUI affiche une notification pour chaque fichier injecte :
```
📎 injected: [src/main.rs] (276 lines, 8192 bytes)
```

L'UI Web affiche des chips dans le compositeur pendant la saisie :
```
📎 @src/main.rs   📎 @Cargo.toml
```

Le Coder voit le contenu du fichier immediatement — pas de tour `read_file` supplementaire. Avec un budget de 12 iterations, economiser un tour de lecture peut faire la difference entre succes et timeout.

### Phase gating : taire le bon bruit

Dites a l'Observer la phase dans laquelle vous etes (`core` / `feature` / `polish`). Les propositions qui ne correspondent pas sont automatiquement estompees. Les retouches CSS ne vous interrompent pas quand votre auth est cassee.

### Sante en un coup d'oeil

Chaque reponse de l'Observer se termine par un score :

```
--- health ---
score: 74  rationale: auth is solid, tests cover happy path only
```

❤ **74** → vert (zone production). Le badge se met a jour en direct.

### Points de controle de progression

Aux iterations 3, 6 et 9, le Coder impose une auto-reflexion **avant le prochain tool call** :

```
<reflect>
last_outcome: success|failure|partial
goal_delta: closer|same|farther
wrong_assumption: <one short sentence>
strategy_change: keep|adjust|abandon
next_minimal_action: <one short sentence>
</reflect>
```

C'est la difference entre un agent qui tourne en rond et un qui sait quand il est perdu.

### Multi-OS (Windows / macOS / Linux)

Spiral-Coder tourne sur Windows, macOS et Linux.

Il a d'abord ete construit sur Windows (donc les pieges Windows sont traites serieusement), mais le runtime est agnostique et le repo fournit des points d'entree PowerShell et bash :

- Windows : `scripts/*.ps1` (`run-ui.ps1`, `run-tui.ps1`, …)
- macOS / Linux : `scripts/*.sh` (`run-ui.sh`, `run-tui.sh`, …)

Durcissement cote Windows (utile meme si vous developpez surtout sur macOS/Linux) :
- Binaires bloques par WDAC → serveur de secours Python Lite (stdlib pure)
- Traduction automatique bash → PowerShell pour des transcripts melanges
- L'enfer des proxies d'entreprise
- `sh.exe` Win32 error 5 sur les invites git interactives

### Registre de plugins

Etendez Spiral-Coder sans le forker :

```js
registerObserverPlugin({ name: "mon-plugin", onProposal, onHealth, onPhase })
registerPhase("security-review", { label: "Revue securite", color: "#f97316" })
registerValidator(propositions => propositions.filter(p => p.score > 20))
```

Chargez votre plugin via `<script>` avant `app.js`. C'est tout.

---

## Le contrat de sortie de l'Observer

L'Observer n'ecrit pas librement. Il parle un format structure que l'UI transforme en cartes :

```
--- phase ---
core

--- proposals ---
title: Validation des entrees manquante
toCoder: Validez la longueur et le type avant de traiter l'entree utilisateur.
severity: critical
score: 88
phase: core
cost: low
impact: empeche le crash sur entree malformee
quote: user_input = input()

--- critical_path ---
Corrigez la validation des entrees avant d'ajouter de nouvelles fonctionnalites.

--- health ---
score: 41  rationale: la logique centrale fonctionne mais la surface d'injection est grande ouverte
```

Chaque champ est intentionnel. `quote` epingle la ligne exacte incriminee sur la carte. `cost` dit si la correction est facile avant de lire les details. `phase` controle la visibilite.

---

## Demarrage rapide

### 0) Definir la cle API (TUI/CLI)

- OpenAI-compatible : `OPENAI_API_KEY` ou `SPIRAL_CODER_API_KEY`
- Mistral : `MISTRAL_API_KEY` (ou `SPIRAL_CODER_API_KEY`)
- Anthropic (Chat/Observer uniquement) : `ANTHROPIC_API_KEY`

```powershell
$env:OPENAI_API_KEY = "..."
# ou: $env:MISTRAL_API_KEY = "..."
```

```bash
export OPENAI_API_KEY="..."
# ou: export MISTRAL_API_KEY="..."
```

UI Web : collez les cles dans Settings (stocke dans le navigateur et envoye uniquement a votre serveur local).

**UI Web (recommande)**
```powershell
# Windows (PowerShell)
.\scripts\run-ui.ps1
# → http://127.0.0.1:18080/
```

```bash
# macOS / Linux (bash)
bash ./scripts/run-ui.sh
# → http://127.0.0.1:18080/
```

Dans l'UI Web : Settings → choisir Provider/Model/Base URL → coller la cle API → definir `toolRoot` sur le chemin de votre projet.

**TUI (terminal)**
```powershell
# Windows (PowerShell)
.\scripts\run-tui.ps1
```

```bash
# macOS / Linux (bash)
bash ./scripts/run-tui.sh
```

Comportement par defaut du TUI :
- La langue de l'UI demarre en anglais (`/lang ja|en|fr` pour changer en cours de session).
- Le panneau de droite s'ouvre sur `Chat`; utilisez `Ctrl+R` ou `/tab observer|chat|tasks|promotions|merge` pour changer d'onglet.
- L'onglet `Merge` lit le dernier `merge_gate.json` de runtime eval ; `A` approuve un cas passant, `H` met un cas en attente, `R` rafraichit, et `Ctrl+Y` copie la preview rollback sans l'executer.
- Lancez `/keys` pour voir quelle variable d'environnement ou option CLI chaque panneau attend pour la cle API.
- Taper `/` dans le composeur affiche un petit picker de commandes slash.
- Taper exactement `/provider` ou `/model` ouvre un picker au clavier (`↑/↓`, `Enter`). `/provider` propose des presets fournisseur (`openai`, `gemini`, `anthropic-compat`, `mistral`, `anthropic`, `hf`) et `/model` suit le preset courant, avec `other` pour une saisie manuelle.
- Si un panneau n'a pas la cle API requise ou n'a pas de modele, l'envoi est bloque et le TUI affiche un avertissement.

**Coder headless (CLI)**
Installer `spiral-coder` (optionnel) :
- Windows (PowerShell) : `.\scripts\install.ps1`
- macOS / Linux (bash) : `bash ./scripts/install.sh`

Puis :
```bash
# (optionnel) generer le template .spiral-coder.md (stack + test_cmd)
spiral-coder init -C .

# lancer l'agent de code dans votre projet
spiral-coder agent "fix the failing test" -C . --vibe --session
# reprendre plus tard (sans prompt -> "continue" auto)
spiral-coder agent -C . --vibe --session

# artefacts (trace JSONL + snapshot JSON final + graphe d'execution)
spiral-coder agent "fix the failing test" -C . --vibe --trace-out .tmp/spiral_coder_trace.jsonl --json-out .tmp/spiral_coder_final.json --graph-out .tmp/spiral_coder_graph.json

# lire la readiness du dernier runtime eval merge gate
spiral-coder merge-gate -C .
spiral-coder merge-gate -C . --json
spiral-coder merge-gate -C . --ci
# la meme gate est visible dans le TUI `/tab merge`, ou dans Web GUI Settings -> Merge gate
```

**Python Lite (WDAC / pas de binaire Rust)**
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

## Concepts cles

### tool_root

Chaque action de l'agent s'execute dans un repertoire de travail.

Par defaut :
- **UI Web** : `.tmp/<thread-id>` (isole par thread)
- **TUI** : `.tmp/tui_<epoch>` (isole par session)
- **CLI** : repertoire courant

Pour travailler sur votre projet reel, definissez `tool_root` sur le chemin du projet :
- **TUI** : option `-C .` / `--tool-root .`, ou commande slash `/root <chemin>` en cours de session
- **UI Web** : Parametres → champ toolRoot

Quand `tool_root` est defini, Spiral-Coder l'analyse a la premiere utilisation pour construire le bloc de contexte projet (stack, git, arborescence). Les envois suivants dans la meme session sautent l'analyse.

La traversee de chemins est bloquee : les chemins avec des composantes `..` sont rejetes a chaque frontiere d'outil (jamais silencieusement).

### Langue

- **Langue de l'UI** : TUI `/lang ja|en|fr` (affecte aussi les prompts).
- **Langue de l'Observer (UI Web)** : `auto` (par defaut) suit la langue de votre dernier message utilisateur meme si l'UI est en anglais ; `ui` suit l'UI ; ou forcez `ja`/`en`/`fr`.

### Sessions (CLI)

`spiral-coder agent` peut sauvegarder et reprendre la conversation complete (y compris les tool calls) avec `--session[=<chemin>]`.

- Chemin par defaut : `.tmp/spiral_coder_session.json`
- Si `-C/--root` est defini, les chemins relatifs de sortie sont resolus sous `tool_root`
- Auto-sauvegarde pendant l'execution (apres les tool calls)
- Reprendre sans prompt : relancez `spiral-coder agent -C . --session`
- Recommencer : ajoutez `--new-session` (ecrase le fichier)

Artefacts associes :
- `--trace-out <chemin>` / `--trace_out` : trace JSONL (tool calls, checkpoints, errors, done)
- `--json-out <chemin>` / `--json_out` : snapshot JSON final (messages + tool calls + tool results)
- `--graph-out <chemin>` / `--graph_out` : graphe d'execution JSON (noeuds + aretes) derive des messages finaux
- Si `-C/--root` est defini, les chemins relatifs sont resolus sous `tool_root`

Le JSON de session/trace peut contenir du code et des sorties d'outils; traitez-le comme sensible.

### Approbations

- **Edit approval** : les appels `write_file` sont mis en file d'attente comme pending edits. Vous approuvez ou rejetez chacun.
- **Command approval** : les appels `exec` peuvent etre gates de la meme maniere (optionnel). Le Coder attend votre decision puis reprend.

Aucun mode ne vous force a vous arreter — ils se mettent en file d'attente silencieusement.

### Providers

Spiral-Coder supporte ces providers aujourd'hui :

| Provider | `--provider` | `base_url` par defaut | Cle env | Coder avec outils |
|---|---|---|---|---|
| OpenAI-compatible | `openai-compatible` | `https://api.openai.com/v1` | `SPIRAL_CODER_API_KEY` ou `OPENAI_API_KEY` | ✅ |
| Mistral | `mistral` | `https://api.mistral.ai/v1` | `MISTRAL_API_KEY` (ou `SPIRAL_CODER_API_KEY`) | ✅ |
| Anthropic | `anthropic` | `https://api.anthropic.com/v1` | `ANTHROPIC_API_KEY` | ❌ (Chat/Observer uniquement) |
| HF local (subprocess) | `hf` | `http://localhost` | *(aucune)* | ❌ (Chat/Observer uniquement) |

Notes :
- La boucle agentique du Coder (`spiral-coder agent`, TUI Coder, mode agentique Web) requiert une API **Chat Completions OpenAI-compatible** avec tool calling (`tools` / `tool_calls`) → utilisez `openai-compatible` ou `mistral`.
- `openai-compatible` veut dire API OpenAI Chat Completions (`/v1/chat/completions`) avec Bearer auth. Pointez `--base-url` / `SPIRAL_CODER_BASE_URL` vers votre endpoint (inclure le `/v1`).
- Valeurs integrees : `spiral-coder list providers` / `spiral-coder list modes` / `spiral-coder list personas`.

Definissez un modele different par role : modele rapide pour les iterations du Coder, modele puissant pour l'analyse de l'Observer. Dans la TUI, vous pouvez aussi separer les providers par panneau (le Coder doit rester `openai-compatible`/`mistral`) : `spiral-coder tui --observer-provider anthropic --observer-model claude-3-5-sonnet-latest`. Erreurs courantes : `401` (cle incorrecte), `429` (rate limit), mismatch `max_tokens` vs `max_completion_tokens`.

### Personas Chat

Cinq chips au-dessus du compositeur Chat — changez a tout moment, independant du Coder/Observer :

| Chip | Style |
|---|---|
| 😊 Enjoue (cheerful) | Enthousiaste et encourageant |
| 🤔 Reflechi (thoughtful) | Verifie les premisses, repond avec soin |
| 🧙 Sensei | Guide par les questions, pas les reponses |
| 😏 Cynique (cynical) | Va droit a la verite qui derange |
| 🦆 Canard (duck) | Ne repond jamais — pose juste « Pourquoi ? » |

### Chat = Compagnon (pas un agent)

Le Chat n'execute jamais d'outils. Il est la pour vous garder dans le flux pendant que le runtime (Coder/Observer) tourne.

Dans l'UI Web, deux aides optionnelles :
- **Joindre snapshot runtime** : injecte un petit resume en lecture seule (cwd, dernier extrait d'erreur, approbations en attente, taches ouvertes) pour demander "que se passe-t-il ?" sans quitter l'onglet Chat.
- **Tâches auto** : un TaskRouter en coulisses transforme la discussion en taches concretes pour Coder/Observer (visibles dans **Tâches**). Vous choisissez toujours quoi envoyer.

### Commandes slash (TUI)

| Commande | Effet |
|---|---|
| `/model <nom>` | Changer de modele en cours de session |
| `/provider <nom>` | Changer le provider en cours (ou afficher ; exact `/provider` ouvre le picker) |
| `/base_url <url>` | Changer le base_url en cours (ou afficher ; `default` pour reinitialiser) |
| `/persona <cle>` | Changer le persona du Coder |
| `/temp <0.0–2.0>` | Ajuster la temperature |
| `/mode <nom>` | Changer le mode du panneau courant |
| `/root <chemin>` | Modifier le tool_root pour les envois suivants |
| `/lang ja\|en\|fr` | Changer la langue de l'UI et des prompts |
| `/tab <observer\|chat\|tasks\|next>` | Changer explicitement le panneau de droite |
| `/keys` | Afficher l'etat des cles API par panneau et l'aide de configuration |
| `/find <requete>` | Filtrer les messages dans le panneau courant |
| `/meta-diagnose [last-fail\|msg:coder-<index>]` | Envoyer un echec Coder cible a l'Observer pour un diagnostic JSON-only |
| `/help` | Afficher toutes les commandes |

---

## Securite

`127.0.0.1` uniquement par defaut. L'execution shell est reelle — gardez les approbations activees.

Les chemins des outils fichiers sont valides par rapport a `tool_root` a chaque appel : les chemins absolus hors `tool_root` et tout composant `..` sont rejetes en erreur (jamais silencieusement).

Si vous l'exposez sur un reseau, ajoutez une authentification et durcissez l'execution d'outils.

---

## Depannage

**"Failed to connect to github.com via 127.0.0.1"** — proxy mort dans les variables d'environnement :
```powershell
Remove-Item Env:HTTP_PROXY,Env:HTTPS_PROXY,Env:ALL_PROXY,Env:GIT_HTTP_PROXY,Env:GIT_HTTPS_PROXY -ErrorAction SilentlyContinue
```

**Push sans invite interactive** (WDAC / sh.exe Win32 error 5) :
```powershell
$env:GITHUB_TOKEN = "ghp_..."
.\scripts\push.ps1
```

**Push via SSH sur le port 443** (reseau d'entreprise) :
```powershell
.\scripts\push_ssh.ps1
```

**"access denied" sur spiral-coder.exe** — binaire encore en cours d'execution :
```powershell
.\scripts\kill-spiral-coder.ps1
```

---

## Licence

Apache License 2.0
