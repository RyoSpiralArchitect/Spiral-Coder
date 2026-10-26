# Spiral-Coder

**Coder, vérifier, puis avancer—avec Coder + Observer.**

Un test en échec, un petit bug, une refactorisation à entreprendre : Coder lit
et modifie votre projet, tandis qu’Observer examine les résultats dans un
contexte séparé et propose la suite. Chat reste disponible pour réfléchir à la
conception ensemble.

[English](README.md) · [日本語](README.ja.md) · [Français](README.fr.md)

[![npm](https://img.shields.io/npm/v/spiral-coder?color=8b8cf5)](https://www.npmjs.com/package/spiral-coder)
[![License](https://img.shields.io/badge/license-Apache_2.0-7aa2f7)](LICENSE)

## Commencer dans votre projet

Il vous faut **Node.js 20+** et une clé API de votre fournisseur.
Le paquet npm contient l’exécutable natif : Rust n’est pas nécessaire.

```sh
npm install -g spiral-coder@latest
cd your-project
export GEMINI_API_KEY="your-key"
spiral --provider gemini
```

Cet exemple utilise la syntaxe shell de macOS/Linux. Le [guide de démarrage](docs/getting-started.md)
couvre aussi PowerShell, OpenAI et Mistral. Choisissez `/provider` et `/model`
dans le TUI pour enregistrer les préférences du projet, puis démarrez avec `spiral`.
Choisissez un modèle disponible sur votre compte ; `/keys` affiche l’état de la configuration.

Compatible avec macOS Apple Silicon/Intel, Linux arm64/x64 (glibc 2.35+) et Windows x64.
[Aide à l’installation, mises à jour et autres plateformes →](docs/npm-distribution.md)

## Confier une petite tâche concrète

Dans le panneau **Coder**, essayez par exemple :

> Reproduis le test en échec, corrige la partie du code concernée, puis relance
> les tests. Explique les changements et ce qui reste à vérifier.

> Lis @src/main.rs et explique le parcours d’une requête. Propose une petite
> refactorisation avant de modifier le code.

> Ajoute un cas d’entrée vide à ce parseur, conserve son API publique et vérifie
> que les tests existants passent toujours.

Adaptez les chemins et la commande de test à votre projet. Commencez par un
résultat que vous pouvez examiner, puis poursuivez.

## Garder le travail et sa revue ensemble

- **Coder + Observer forment une seule boucle.** Coder exécute ; Observer examine
  les résultats enregistrés. `Ctrl+O` demande une revue ; `/autofix` permet de
  transmettre les suggestions à Coder.
- **Choisissez vos modèles.** OpenAI, Gemini et Mistral sont disponibles pour le
  code. Les deux rôles utilisent le même modèle par défaut ; vous pouvez aussi
  les configurer séparément.
- **Reprenez le travail.** Conservez les sessions, consultez les résultats des
  outils et les différences, puis continuez avec une nouvelle modification.
- **Prenez le temps de réfléchir.** Passez à Chat pour discuter de la conception
  tout en gardant le travail de code à portée de vue.

Une revue apporte un avis supplémentaire ; examinez aussi le diff et les tests.

| Dans le TUI | Action |
| --- | --- |
| `/model` · `/provider` | Choisir un modèle ou un fournisseur |
| `Ctrl+O` | Faire examiner la dernière réponse terminée de Coder |
| `/autofix` | Activer ou désactiver le retour Observer → Coder |
| `Ctrl+K` | Arrêter le flux du panneau actif |
| `/lang ja` · `/lang en` · `/lang fr` | Changer la langue de l’interface |
| `/help` | Afficher toutes les commandes |

Le TUI exécute les outils automatiquement. Pour approuver chaque commande ou
modification, utilisez le [CLI sans interface](docs/usage.md#headless-coding-and-resume).

## Vous préférez le navigateur ?

```sh
spiral serve
```

Ouvrez **http://127.0.0.1:18080/**, puis choisissez le dossier du projet, le
fournisseur et le modèle dans Settings. Coder, Observer et Chat sont aussi
accessibles dans cette interface Web locale.

![Interface Web locale de Spiral-Coder, avec Coder et Observer côte à côte](docs/screenshots/spiral-coder-overview.png)

*Capture locale de l’interface 0.2.0, sans requête à un modèle.*

## Pour aller plus loin

- [Démarrage](docs/getting-started.md) — clés, première tâche et première revue.
- [Guide d’utilisation](docs/usage.md) — sessions, CLI, Web et configuration du projet.
- [Documentation](docs/README.md) — trouver le bon guide.
- [Recherche et développement](docs/research/README.md) — architecture, évaluations,
  auto-dogfooding et résultats expérimentaux.
- [Compiler et contribuer](docs/usage.md#build-and-contribute) — essayer la version de développement.
- [Migration depuis Observistral](docs/migration-spiral-coder.md) — commandes et données existantes.

[Apache License 2.0](LICENSE)
