# Référence de .ferrisgit-ci.yml

Cette page décrit tout ce que FerrisGit accepte dans un fichier de pipeline : les clés, leurs types, leurs valeurs par défaut, les erreurs de validation et la façon dont les jobs sont ordonnancés. Pour une prise en main, commencez par [Premiers pas](/docs/ci-cd/premiers-pas).

## Structure du fichier

Le fichier est un document YAML (un seul document). Il a deux clés de premier niveau, toutes deux obligatoires : `stages` et `jobs`.

```yaml ferrisgit-ci
stages: [build, test]

jobs:
  compile:
    stage: build
    image: rust:1
    script:
      - cargo build --release

  unit-tests:
    stage: test
    image: rust:1
    needs: [compile]
    variables:
      RUST_LOG: debug
    tags: [docker]
    cache: [cargo-target]
    script:
      - cargo test
```

> **Attention** : FerrisGit ignore sans message toute clé qu'il ne connaît pas, au premier niveau comme dans un job. Une faute de frappe (`scripts:`, `need:`) n'est pas signalée : la clé obligatoire manquante déclenche une erreur, mais une clé facultative mal orthographiée est simplement sans effet. Seule la table [`rules`](#rules) est stricte : une clé inconnue y est une erreur, car une condition mal orthographiée ferait tourner le job partout.

## Clés de premier niveau

| Clé | Type | Obligatoire | Description |
|---|---|---|---|
| `stages` | liste de chaînes | oui | Les étapes de la pipeline, dans l'ordre. |
| `jobs` | table `nom du job` vers définition | oui | Les jobs. Le nom du job est la clé de la table. |

`stages` peut être vide et `jobs` peut être vide. Sans aucun job à lancer, voir [Quand aucun job ne reste](#quand-aucun-job-ne-reste).

## Clés d'un job

| Clé | Type | Obligatoire | Valeur par défaut | Description |
|---|---|---|---|---|
| `stage` | chaîne | oui | aucune | Étape du job. Doit figurer dans `stages`. |
| `image` | chaîne | oui | aucune | Image de conteneur dans laquelle le script s'exécute. |
| `script` | liste de chaînes | oui | aucune | Commandes à exécuter. |
| `variables` | table chaîne vers chaîne | non | table vide | Variables d'environnement du job. |
| `needs` | liste de noms de jobs | non | liste vide | Jobs qui doivent réussir avant celui-ci. |
| `tags` | liste de chaînes | non | liste vide | Étiquettes qu'un runner doit porter pour prendre le job (runners Docker seulement). |
| `cache` | liste de clés de cache | non | liste vide | Volumes de cache à monter (Kubernetes seulement). |
| `rules` | table de conditions | non | aucune | Les pipelines où le job existe : voir [rules](#rules). Sans `rules`, il est de toutes. |

Il n'y a pas d'autre clé de job. En particulier, FerrisGit n'a ni `before_script`, `after_script`, `artifacts`, `services`, `only`, `except`, `when`, `allow_failure`, `retry`, `timeout`, `extends`, `include`, ni valeurs globales par défaut : ces clés, si vous les écrivez, sont ignorées.

### stages

Liste ordonnée des noms d'étapes. L'ordre sert à trois choses : afficher les étapes dans l'interface, valider les dépendances `needs` (une dépendance ne peut pas appartenir à une étape située après celle du job), et ordonnancer les jobs : une étape est une barrière. Les jobs d'une étape ne démarrent que lorsque **tous** les jobs de **toutes** les étapes précédentes ont réussi, sauf ceux qui déclarent `needs` (voir [Ordre de lancement](#ordre-de-lancement)). Une étape sans job est acceptée et n'a aucun effet.

### stage

Nom de l'étape du job. Si le nom n'est pas dans `stages`, le fichier est refusé (voir [Erreurs de validation](#erreurs-de-validation)).

### image

Nom de l'image de conteneur, par exemple `rust:1` ou `alpine:3.24`. FerrisGit ne vérifie pas son contenu : une image introuvable fait échouer le job au moment de son exécution.

- Runners Docker : le job est lancé par `docker run` ; le runner doit pouvoir télécharger l'image. Le point d'entrée (`ENTRYPOINT`) de l'image est conservé : le script est passé en arguments sous la forme `sh -c <script>`. Une image dont le point d'entrée n'est pas un shell ou un lanceur transparent ne conviendra pas.
- Kubernetes : la commande du conteneur est remplacée par `/bin/sh -c <script>`. L'image doit donc contenir `/bin/sh`.

### script

Liste de commandes. Chaque élément est une chaîne ; FerrisGit les joint avec ` && ` en une seule commande, exécutée par `sh -c`. Conséquences :

- la première commande qui échoue arrête le script et fait échouer le job ;
- aucune commande n'est exécutée après un échec ;
- un élément ne doit pas se terminer par `&` ou `;` (`cmd & && suite` est une erreur de syntaxe du shell) ;
- un élément peut contenir plusieurs lignes (bloc YAML `|`) ; il compte pour une seule commande dans l'enchaînement.

Le script s'exécute avec le shell `sh` de l'image : pas de `bash` garanti. Une liste vide (`script: []`) est acceptée et réussit immédiatement.

### variables

Table de variables d'environnement propres au job. Les noms et les valeurs sont des chaînes ; une valeur YAML écrite sans guillemets comme `8080`, `true` ou `1.0` est lue comme la chaîne `"8080"`, `"true"` ou `"1.0"`. FerrisGit ne fait aucune substitution dans les valeurs : `B: "$A-suite"` donne à `B` la valeur littérale `$A-suite`.

Les variables CI du dépôt viennent s'ajouter, avec les runners Docker. Voir [Variables et secrets](/docs/ci-cd/variables-et-secrets) pour la précédence.

### needs

Liste de noms de jobs de la même pipeline. Un job n'est lancé que lorsque tous les jobs de sa liste sont **Réussis**. Quand `needs` est présent, il remplace la barrière d'étape pour ce job : il n'attend que les jobs listés, pas toute l'étape précédente. Si l'un d'eux échoue ou est annulé, le job est **Ignoré** sans être exécuté, ainsi que tous les jobs qui en dépendent à leur tour.

### tags

Liste d'étiquettes. Avec les runners Docker, un job n'est donné qu'à un runner qui porte **toutes** ses étiquettes (comparaison exacte, sensible à la casse). Un job sans étiquette peut être pris par n'importe quel runner ; un runner sans étiquette ne prend que des jobs sans étiquette. Avec Kubernetes, `tags` est ignoré. Voir [Runners Docker](/docs/ci-cd/runners-docker#étiquettes).

### cache

Liste de clés de cache. Chaque clé doit être non vide et ne contenir que des lettres minuscules ASCII, des chiffres et des tirets (`^[a-z0-9-]+$`). Seul le moteur Kubernetes monte des caches ; avec les runners Docker, les clés sont validées puis ignorées. Voir [Caches](/docs/ci-cd/caches).

### rules

Décide dans quelles pipelines le job existe. Un job que ses règles écartent n'est pas créé : il n'apparaît pas dans la pipeline et n'a aucun statut, ce qui n'est pas la même chose qu'un job **Ignoré**.

| Clé | Type | Effet |
|---|---|---|
| `branches` | liste de motifs | Le job tourne dans les pipelines d'une branche dont le nom correspond. |
| `tags` | liste de motifs | Le job tourne dans les pipelines d'un tag dont le nom correspond. Il s'agit de tags Git, à ne pas confondre avec la clé [`tags`](#tags) du job, qui désigne des runners. |
| `merge_requests` | `true` | Le job tourne dans les pipelines de demande de fusion. |
| `changes` | liste de motifs de chemins | Le job ne tourne que si l'un des fichiers modifiés correspond. |

`branches`, `tags` et `merge_requests` nomment les pipelines du job et s'additionnent : il suffit d'une correspondance. Sans aucune de ces trois clés, le job appartient à toutes les pipelines. `changes` s'ajoute à ces conditions : le job doit appartenir à la pipeline **et** un fichier modifié doit correspondre.

```yaml ferrisgit-ci
stages: [check, test, deploy]

jobs:
  lint:
    stage: check
    image: rust:1
    script:
      - cargo clippy

  site:
    stage: check
    image: node:24
    rules:
      changes: ["site/**", "*.md"]
    script:
      - npm --prefix site run build

  unit-tests:
    stage: test
    image: rust:1
    rules:
      branches: [main, "release/*"]
      merge_requests: true
    script:
      - cargo test

  publish:
    stage: deploy
    image: alpine:3.24
    rules:
      tags: ["v*"]
    script:
      - echo "publication"
```

Dans cet exemple, `lint` tourne partout. `site` tourne dans toute pipeline qui modifie le site ou un fichier Markdown à la racine. `unit-tests` tourne sur `main`, sur les branches `release/…` et dans les demandes de fusion. `publish` ne tourne que pour un tag `v…`.

Une pipeline de demande de fusion n'est pas une pipeline de branche : `branches: [main]` ne retient pas le job dans une demande de fusion qui vise `main`. La pipeline qui suit la fusion, elle, est une pipeline de la branche cible.

#### Motifs

Dans `branches` et `tags`, un motif est un nom, où `*` remplace n'importe quelle suite de caractères, `/` compris : `main`, `release/*`, `v*`, `*`. C'est la même syntaxe que les [branches protégées](/docs/utilisation/cloner-et-pousser#branches-protégées). Écrivez le nom court, sans `refs/heads/` ni `refs/tags/`.

Dans `changes`, un motif est un chemin depuis la racine du dépôt, comparé dossier par dossier en respectant la casse :

| Motif | Correspond à | Ne correspond pas à |
|---|---|---|
| `Cargo.lock` | `Cargo.lock` | `crates/api/Cargo.lock` |
| `*.md` | `README.md` | `docs/guide.md` |
| `src/*` | `src/main.rs` | `src/a/b.rs` |
| `src/**` | `src/main.rs`, `src/a/b.rs` | `srcs/a.rs` |
| `**/*.rs` | `main.rs`, `a/b/main.rs` | `a/b/main.ts` |

`*` reste à l'intérieur d'un nom ; `**`, seul entre deux barres obliques, remplace un nombre quelconque de dossiers, aucun compris. Il n'y a ni exclusion (`!`), ni `?`, ni `[…]` : ces caractères, sauf `!` en tête qui est refusé, se comparent tels quels.

#### Ce que `changes` compare

| Pipeline | Fichiers modifiés |
|---|---|
| Push sur une branche existante | Ce qui diffère entre l'ancienne et la nouvelle pointe de la branche, tous les commits du push compris. |
| Demande de fusion | Ce qui diffère entre la pointe de la branche cible et le commit de fusion : ce que la fusion apporterait, pas ce que la cible a fait de son côté. |
| Après la fusion d'une demande | Ce qui diffère entre la branche cible avant et après la fusion. |
| Nouvelle branche, ou tag | Rien à comparer : `changes` est considéré comme satisfait, et le job tourne. |

Un fichier supprimé compte comme modifié ; un fichier renommé compte sous son ancien et son nouveau nom. Un push qui ne change aucun fichier (`git commit --allow-empty`) ne satisfait aucun `changes`. Si la comparaison est impossible, par exemple parce que l'ancien commit n'existe plus, `changes` est considéré comme satisfait : mieux vaut lancer un job pour rien que d'en sauter un qui comptait.

#### Quand un job écarté est attendu par un autre

Un job qui déclare `needs` n'est lancé que lorsque tous les jobs de sa liste ont réussi. Si l'un d'eux est écarté par ses règles, cette promesse ne peut pas être tenue : **le job qui l'attend est écarté à son tour**, de proche en proche, même si ses propres règles le retiennent.

```yaml ferrisgit-ci
stages: [test, deploy]

jobs:
  unit-tests:
    stage: test
    image: rust:1
    rules:
      changes: ["src/**"]
    script:
      - cargo test

  deploy:
    stage: deploy
    image: alpine:3.24
    needs: [unit-tests]
    rules:
      branches: [main]
    script:
      - echo "déploiement"
```

Ici, un push sur `main` qui ne touche pas `src/` ne déploie rien : `unit-tests` est écarté, donc `deploy` aussi. Un déploiement ne part jamais sans les tests qu'il réclame. Pour qu'il parte quand même, retirez `needs` : `deploy` attend alors la barrière de son étape, qui ne porte que sur les jobs présents dans la pipeline, et démarre après ceux qui ont tourné, ou aussitôt s'il n'y en a aucun.

#### Quand aucun job ne reste

Les règles peuvent ne laisser aucun job :

- pour un **push** ou une **fusion**, aucune pipeline n'est créée, comme quand le fichier de pipeline est absent ;
- pour une **demande de fusion**, FerrisGit enregistre une pipeline **Réussie** dès sa création, sans job. Elle apparaît dans la demande et dans sa chronologie, et satisfait la vérification `pipeline` : une demande qui ne touche que la documentation n'attend pas indéfiniment une pipeline exigée.

Une vérification requise qui nomme un job, comme `pipeline/unit-tests`, reste bloquante quand ce job a été écarté : elle exige que ce job ait tourné et réussi. Voir [Demandes de fusion](/docs/utilisation/merge-requests).

#### Règles refusées

Le fichier est refusé si `rules` contient une clé inconnue, n'est pas une table, ou est vide (`rules: {}`) ; si une liste est vide (`branches: []` ne pourrait jamais correspondre) ; si `merge_requests` vaut `false` (pour écarter les demandes de fusion, listez `branches` ou `tags`) ; si un motif de `branches` ou de `tags` ne peut pas être un nom (`refs/heads/main`, espaces…) ; ou si un motif de `changes` commence par `/`, `./` ou `!`, ou se termine par `/`. `rules: ~` revient à ne pas écrire `rules`.

## Erreurs de validation

FerrisGit valide le fichier au moment de créer la pipeline. Les contrôles s'arrêtent à la première erreur. Les jobs sont examinés dans l'ordre alphabétique de leur nom ; pour chaque job, dans l'ordre : son étape, puis chaque entrée de `needs`, puis chaque clé de `cache`, puis `rules`.

| Cause | Message |
|---|---|
| Fichier non encodé en UTF-8 | `pipeline file is not valid UTF-8: <détail>` |
| YAML mal formé, clé obligatoire absente, mauvais type | `invalid YAML: <détail>` |
| Étape d'un job absente de `stages` | `job '<job>' references stage '<étape>' which is not declared in `stages`` |
| Entrée de `needs` qui n'est pas un job | `job '<job>' has a `needs` entry '<dépendance>' which is not a declared job` |
| Dépendance située dans une étape ultérieure | `job '<job>' needs '<dépendance>', but that job's stage does not come before '<job>''s own stage` |
| Clé de cache invalide | `job '<job>' declares cache key '<clé>', which is not valid: cache keys must be lowercase alphanumeric characters and hyphens only (matching `^[a-z0-9-]+$`)` |
| Règles qui ne peuvent rien vouloir dire (voir [Règles refusées](#règles-refusées)) | `job '<job>' has invalid `rules`: <raison>` |
| Cycle dans les `needs` | ``needs` form a cycle: <job> -> <job> -> <job>` |

Pour un cycle, le message cite les jobs du cycle dans l'ordre des dépendances, en reprenant le premier à la fin : `` `needs` form a cycle: a -> b -> a ``. Un job qui se déclare lui-même donne `` `needs` form a cycle: a -> a ``. La détection vient en dernier : elle n'a lieu que si l'étape, les `needs`, les clés de cache et les `rules` de tous les jobs sont valides.

Le `<détail>` de `invalid YAML` vient de l'analyseur YAML. Il indique le chemin de la clé fautive et, en général, la ligne et la colonne. Quelques exemples de forme :

- une clé obligatoire manque : `invalid YAML: jobs.compile: missing field `image` at line 3 column 5`, ou `missing field `jobs`` quand c'est la racine ;
- un `script` donné comme une simple chaîne au lieu d'une liste : `invalid type: string "…", expected a sequence` ;
- une table à la place d'une chaîne, ou une liste à la place d'une table ;
- une clé inconnue dans `rules` : `invalid YAML: jobs.compile.rules: unknown field `branch`, expected one of `branches`, `tags`, `merge_requests`, `changes``.

Si un job dépend d'un job dont l'étape n'est pas déclarée, l'erreur « references stage » est attribuée au job dépendance, pas au job qui l'exige.

Le push réussit toujours. Si le fichier est présent mais invalide, FerrisGit crée quand même une pipeline, **Échouée** dès sa création, sans aucun job, qui porte le message d'erreur : l'interface l'affiche dans l'encadré « Fichier de pipeline invalide » (liste et détail de la pipeline), et l'API le renvoie dans le champ `error`. La personne qui a poussé est notifiée comme pour toute pipeline échouée. Un fichier **absent** ne crée aucune pipeline. Voir [Dépannage](/docs/ci-cd/depannage#le-fichier-est-invalide).

## Règles sur le graphe de dépendances

- Une dépendance doit être dans la même étape que le job ou dans une étape antérieure de `stages`. Une dépendance dans la même étape est acceptée.
- Les cycles sont refusés à la validation : deux jobs de la même étape qui se déclarent mutuellement dans `needs`, un cycle plus long, ou un job qui se déclare lui-même (voir [Erreurs de validation](#erreurs-de-validation)).
- Les entrées dupliquées dans `needs` sont sans conséquence.
- Un job ne peut dépendre que de jobs de la même pipeline, désignés par leur nom exact.

## Ordre de lancement

1. À la création de la pipeline, FerrisGit crée les jobs que leurs [`rules`](#rules) retiennent à l'état **En attente**, dans l'ordre des étapes puis, dans une étape, dans l'ordre alphabétique des noms.
2. Un job devient disponible selon la première de ces règles qui s'applique à lui :
   - il déclare `needs` : dès que tous les jobs listés sont **Réussis** ;
   - sinon, il n'est pas dans la première étape : dès que **tous les jobs de toutes les étapes précédentes** présents dans la pipeline sont **Réussis** (une étape est une barrière) ;
   - sinon (première étape, sans `needs`) : immédiatement.
3. Les jobs d'une même étape qui n'ont pas de dépendance entre eux sont parallèles : ils sont disponibles en même temps.
4. Avec les runners Docker, chaque runner prend, à chaque interrogation, le plus ancien job disponible dont les étiquettes conviennent (ordre de création : donc d'abord les pipelines les plus anciennes, puis l'ordre des étapes, puis l'ordre alphabétique). Un runner exécute un seul job à la fois ; le parallélisme est donné par le nombre de runners.
5. Avec Kubernetes, tous les jobs disponibles sont lancés ensemble, chacun dans son Pod, dès qu'ils le deviennent.

Les deux moteurs appliquent exactement la même règle : c'est le serveur qui décide quels jobs sont disponibles.

```yaml ferrisgit-ci
stages: [build, test, deploy]

jobs:
  compile:
    stage: build
    image: rust:1
    script:
      - cargo build --release

  lint:
    stage: build
    image: rust:1
    script:
      - cargo clippy

  unit-tests:
    stage: test
    image: rust:1
    script:
      - cargo test

  smoke:
    stage: test
    image: rust:1
    needs: [compile]
    script:
      - ./target/release/app --check

  publish:
    stage: deploy
    image: alpine:3.24
    script:
      - echo "publication"
```

Dans cet exemple, `compile` et `lint` démarrent ensemble. `smoke` n'attend que `compile` ; `unit-tests`, qui n'a pas de `needs`, attend la fin de `compile` **et** de `lint`. `publish` attend la fin de tous les jobs de `build` et de `test`.

Les jobs d'une même pipeline ne partagent rien : chacun repart d'un dépôt fraîchement cloné (Docker) ou d'un conteneur vide (Kubernetes). Il n'y a pas d'artefacts pour passer des fichiers d'un job à l'autre.

## Statuts

Un job a l'un de ces statuts : **En attente**, **En cours**, **Réussi**, **Échoué**, **Annulé**, **Ignoré**. Un job est **Ignoré** quand il ne peut plus jamais démarrer parce qu'un job dont il dépend (par `needs`, ou par la barrière d'étape) a échoué, a été annulé ou a lui-même été ignoré. Il ne s'exécute pas, n'a ni heure de début ni journal.

Une pipeline a l'un de ces statuts : **En attente**, **En cours**, **Réussie**, **Échouée**, **Annulée**.

- Il est **En attente** de sa création jusqu'à ce qu'un premier job démarre.
- Il passe à **En cours** dès qu'un de ses jobs démarre.
- Il se termine quand tous ses jobs sont terminés (réussi, échoué, annulé ou ignoré), comme décrit dans la section suivante.

## Comportement à l'échec

- Un job échoue quand son script retourne un code non nul, quand l'image ne peut pas être lancée, ou (Docker) quand le clonage du dépôt échoue.
- Les jobs qui dépendent d'un job échoué ou annulé, par `needs` ou par la barrière d'étape, directement ou non, sont **Ignorés** : ils ne démarrent jamais. La pipeline ne reste donc pas en attente.
- Les jobs qui ne dépendent pas du job échoué continuent jusqu'au bout : ceux de la même étape, et ceux qui déclarent des `needs` satisfaits.
- Quand tous les jobs sont terminés, la pipeline est **Échouée** si l'un d'eux a échoué (ou a été ignoré), sinon **Annulée** si l'un d'eux a été annulé, sinon **Réussie**. À l'échec, la personne qui a déclenché la pipeline est notifiée.
- Il n'y a pas de nouvelle tentative automatique ni de délai maximal : voir [Limites](#limites).

## Limites

- Aucun délai maximal d'exécution d'un job : un script qui ne rend jamais la main laisse le job **En cours** jusqu'à l'annulation de la pipeline.
- Aucune limite de nombre de jobs, de lignes de script ou de variables n'est appliquée, ni de taille de journal.
- Un même push crée au plus 20 pipelines : les branches et les tags au-delà, dans l'ordre où le push les envoie, n'en reçoivent pas.
- Un job en attente d'un runner (étiquettes qu'aucun runner ne porte, aucun runner en ligne) reste en attente sans limite de temps.
- Le nombre de jobs simultanés des runners Docker peut être plafonné pour toute l'instance par un administrateur (réglage **Jobs simultanés (runners Docker)** dans **Admin > Réglages > Exécution**). Il ne s'applique pas au moteur Kubernetes.
- La taille d'un push est limitée par le réglage de l'instance (voir [Réglages de l'instance](/docs/administration/reglages)), pas par le fichier de pipeline.
