# Exemples de pipelines

Chaque fichier de cette page est un `.ferrisgit-ci.yml` complet, accepté tel quel par FerrisGit. Adaptez les noms de commandes et de scripts à votre projet.

Trois points à garder en tête en les lisant :

- Les exemples qui compilent ou testent du code supposent des **runners Docker** : c'est le seul moteur qui clone le dépôt dans le job. Avec Kubernetes, le Pod ne contient pas le code (voir [Moteur Kubernetes](/docs/ci-cd/kubernetes#limites)).
- Les jobs ne partagent rien : pas d'artefacts, pas de dossier commun. Chaque job refait ses installations. C'est pour cela que les dépendances sont mises en cache (sur Kubernetes seulement : avec les runners Docker, la clé `cache` est sans effet mais inoffensive).
- Une étape est une barrière : un job attend que tous les jobs des étapes précédentes aient réussi, sauf s'il déclare `needs`, auquel cas il n'attend que les jobs listés. Voir [la référence](/docs/ci-cd/reference-yaml#ordre-de-lancement).

Dans un script YAML, mettez entre guillemets simples toute ligne qui contient un deux-points suivi d'un espace (`'curl -H "Accept: text/html" …'`), sinon YAML la lit comme une table.

## Projet Rust : format, Clippy, tests

L'image officielle `rust` n'inclut ni `rustfmt` ni Clippy : on les ajoute avec `rustup`. Les jobs `format` et `clippy` tournent en parallèle ; `test` attend les deux. Le registre de Cargo et le dossier `target` sont placés dans des caches.

```yaml ferrisgit-ci
stages: [lint, test]

jobs:
  format:
    stage: lint
    image: rust:1
    script:
      - rustup component add rustfmt
      - cargo fmt --all -- --check

  clippy:
    stage: lint
    image: rust:1
    cache: [cargo-home, cargo-target]
    variables:
      CARGO_HOME: /ferrisgit-cache/cargo-home
      CARGO_TARGET_DIR: /ferrisgit-cache/cargo-target
    script:
      - rustup component add clippy
      - cargo clippy --all-targets -- -D warnings

  test:
    stage: test
    image: rust:1
    needs: [format, clippy]
    cache: [cargo-home, cargo-target]
    variables:
      CARGO_HOME: /ferrisgit-cache/cargo-home
      CARGO_TARGET_DIR: /ferrisgit-cache/cargo-target
    script:
      - cargo test --all-targets
```

`clippy` et `test` partagent les mêmes caches mais ne s'exécutent jamais en même temps : `test` est dans l'étape suivante et déclare `needs`. Voir [Caches](/docs/ci-cd/caches#portée).

## Projet Node ou Angular : installation, lint, tests, build

Chaque job fait son propre `npm ci` (rien n'est transmis d'un job à l'autre). Le cache npm évite de retélécharger les paquets sur Kubernetes. `lint` et `unit-tests` sont indépendants ; `build` attend les deux.

```yaml ferrisgit-ci
stages: [check, build]

jobs:
  lint:
    stage: check
    image: node:22
    cache: [npm]
    variables:
      npm_config_cache: /ferrisgit-cache/npm
    script:
      - npm ci
      - npm run lint

  unit-tests:
    stage: check
    image: node:22
    cache: [npm]
    variables:
      npm_config_cache: /ferrisgit-cache/npm
    script:
      - npm ci
      - npm test

  build:
    stage: build
    image: node:22
    needs: [lint, unit-tests]
    cache: [npm]
    variables:
      npm_config_cache: /ferrisgit-cache/npm
    script:
      - npm ci
      - npm run build
```

Les scripts `lint`, `test` et `build` sont ceux de votre `package.json`. Les tests doivent pouvoir s'exécuter sans interface graphique dans un conteneur ; s'ils ont besoin d'un navigateur, utilisez une image qui en contient un.

## Construction d'une image Docker

> **Attention** : le runner ne donne pas au job l'accès au démon Docker de sa machine (pas de socket monté, pas de mode privilégié). Un `docker build` ne peut donc fonctionner qu'avec un démon Docker **distant**, que vous exploitez vous-même, joignable depuis le runner et protégé (TLS ou réseau privé). FerrisGit ne fournit pas de service de construction.

Dans cet exemple, l'image `docker:27-cli` fournit le client ; le démon est désigné par une variable CI du dépôt `DOCKER_HOST` (par exemple `tcp://builder.example.com:2376`), et la connexion au registre par `REGISTRY_USER` et `REGISTRY_PASSWORD`. Créez ces trois variables dans **Réglages > Variables CI/CD** (voir [Variables et secrets](/docs/ci-cd/variables-et-secrets)). Si le démon est protégé par TLS, ajoutez aussi les variables `DOCKER_TLS_VERIFY` et `DOCKER_CERT_PATH` que le client Docker attend, avec des certificats accessibles dans le job.

```yaml ferrisgit-ci
stages: [build]

jobs:
  image:
    stage: build
    image: docker:27-cli
    variables:
      IMAGE_NAME: registry.example.com/equipe/application
    script:
      - echo "$REGISTRY_PASSWORD" | docker login registry.example.com -u "$REGISTRY_USER" --password-stdin
      - docker build -t "$IMAGE_NAME:latest" .
      - docker push "$IMAGE_NAME:latest"
```

FerrisGit n'injecte ni SHA de commit ni nom de branche dans le job : vous ne pouvez pas, aujourd'hui, étiqueter l'image avec le commit. Le dépôt est cloné dans `/workspace`, qui est le dossier courant ; son contexte de construction est donc `.`.

## Pipeline en plusieurs étapes avec needs

Un enchaînement en éventail : une préparation, trois vérifications en parallèle, un rapport final qui attend tout. C'est la forme du fichier de test du dépôt FerrisGit lui-même.

```yaml ferrisgit-ci
stages: [prepare, check, report]

jobs:
  prepare:
    stage: prepare
    image: alpine:3.20
    script:
      - echo "préparation"

  lint:
    stage: check
    image: alpine:3.20
    needs: [prepare]
    script:
      - echo "analyse statique"
      - sleep 5

  unit:
    stage: check
    image: alpine:3.20
    needs: [prepare]
    script:
      - echo "tests unitaires"
      - sleep 5

  integration:
    stage: check
    image: alpine:3.20
    needs: [prepare]
    script:
      - echo "tests d'intégration"
      - sleep 10

  summary:
    stage: report
    image: alpine:3.20
    needs: [lint, unit, integration]
    script:
      - echo "tout est passé"
```

Ce qui se passe : `prepare` démarre seul ; quand il réussit, les trois jobs de `check` deviennent disponibles et partent en parallèle (autant qu'il y a de runners libres) ; `summary` ne part que lorsque les trois ont réussi. Si `lint` échoue, `summary` passe à **Ignoré** sans s'exécuter, tandis que `unit` et `integration` vont jusqu'au bout ; le pipeline se termine alors **Échoué**.

## Un job avec des variables

Variables du fichier de pipeline, script de plusieurs lignes et variables CI du dépôt (avec les runners Docker) :

```yaml ferrisgit-ci
stages: [report]

jobs:
  environment:
    stage: report
    image: alpine:3.20
    variables:
      CI_GREETING: "bonjour depuis FerrisGit"
      TARGET: staging
      RETRIES: 3
    script:
      - echo "$CI_GREETING"
      - echo "cible = $TARGET, essais = $RETRIES"
      - |
        if [ -n "$API_TOKEN" ]; then
          echo "API_TOKEN est défini"
        else
          echo "API_TOKEN est absent"
        fi
```

`RETRIES: 3` est lu comme la chaîne `"3"`. `API_TOKEN` n'est pas dans le fichier : c'est une variable CI du dépôt, présente avec les runners Docker. Si vous créez une variable CI du même nom que `TARGET`, elle l'emporte sur la valeur du fichier. Le bloc `|` compte pour une seule commande dans l'enchaînement par `&&`.

## Déploiement par appel HTTP

Les tests d'abord, puis un déploiement qui appelle un point d'entrée de votre plateforme. Le job `deploy` est réservé, grâce à `tags`, à un runner qui porte l'étiquette `deploy` (un runner dédié, dont l'accès réseau est celui de votre plateforme). `DEPLOY_URL` et `DEPLOY_TOKEN` sont des variables CI du dépôt.

```yaml ferrisgit-ci
stages: [test, deploy]

jobs:
  tests:
    stage: test
    image: node:22
    script:
      - npm ci
      - npm test

  deploy:
    stage: deploy
    image: alpine:3.20
    needs: [tests]
    tags: [deploy]
    script:
      - apk add --no-cache curl
      - 'curl -fsS -X POST -H "Authorization: Bearer $DEPLOY_TOKEN" "$DEPLOY_URL"'
```

Rappels importants :

- Un pipeline est créé après **chaque** push, sur le commit de `HEAD` : ce `deploy` repart donc à chaque push réussi du dépôt. FerrisGit n'a pas de condition par branche ni de déclenchement manuel. Si vous ne voulez pas déployer à chaque fois, faites porter le déploiement par un dépôt ou un runner que seuls les Mainteneurs contrôlent.
- Les valeurs des variables CI sont lisibles par quiconque peut modifier le fichier de pipeline. Voir [Variables et secrets](/docs/ci-cd/variables-et-secrets#masquage-dans-les-journaux).
- Si aucun runner ne porte l'étiquette `deploy`, le job reste **En attente** : voir [Dépannage](/docs/ci-cd/depannage).
