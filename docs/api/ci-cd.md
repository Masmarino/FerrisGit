# Pipelines et runners

Cette page décrit les routes des pipelines et de leurs jobs, des variables CI d'un dépôt, des runners (côté administration) et l'API que `ferrisgit-runner` appelle sur le serveur. Pour la configuration d'une pipeline, voir [Référence de .ferrisgit-ci.yml](/docs/ci-cd/reference-yaml). Pour installer un runner, voir [Runners Docker](/docs/ci-cd/runners-docker).

Les routes de pipeline, de variables et d'administration demandent un JWT (voir [Authentification](/docs/api/authentification)). Un rôle insuffisant sur un dépôt répond 404, jamais 403. Les routes `/api/runner/jobs/*` s'authentifient avec un jeton de runner (voir plus bas).

Il n'y a pas de route pour créer une pipeline à la main : une pipeline démarre quand un `git push` réussit, et après la fusion d'une demande de fusion.

## Pipelines et jobs

Les statuts d'une pipeline sont `pending`, `running`, `success`, `failed` et `canceled`. Ceux d'un job sont les mêmes, plus `skipped` : un job qui n'a jamais démarré parce qu'un job dont il dépend (par `needs` ou par une étape précédente) a échoué ou a été annulé. Une pipeline est `pending` jusqu'à ce que son premier job démarre, puis `running` jusqu'à ce que tous ses jobs soient terminés.

### `GET /api/repositories/{repository_id}/pipelines`

Liste les pipelines du dépôt, de la plus récente à la plus ancienne. Rôle minimal : Lecteur.

Réponse 200 :

```json
[
  {
    "id": "8f1a…",
    "commitSha": "9fceb02d…",
    "status": "success",
    "createdAt": "2026-03-01T10:00:00Z",
    "finishedAt": "2026-03-01T10:03:12Z",
    "triggeredBy": { "id": "0b6c…", "username": "alice" },
    "commitMessage": "Corrige le menu",
    "error": null
  }
]
```

`finishedAt` est `null` tant que la pipeline n'est pas terminée. `triggeredBy` est `null` si le compte a été supprimé. `commitMessage` n'est résolu que pour les 50 pipelines les plus récentes ; pour les autres, il vaut `null`.

`error` vaut `null` pour une pipeline normale. Quand le fichier de pipeline d'un push était présent mais invalide, FerrisGit crée tout de même une pipeline : `status` vaut `failed`, `finishedAt` est renseigné, elle n'a aucun job, et `error` contient le message de l'analyseur, par exemple :

```json
{
  "id": "c07d…",
  "commitSha": "41b8e3a0…",
  "status": "failed",
  "createdAt": "2026-03-01T11:00:00Z",
  "finishedAt": "2026-03-01T11:00:00Z",
  "triggeredBy": { "id": "0b6c…", "username": "alice" },
  "commitMessage": "Ajoute une étape",
  "error": "`needs` form a cycle: a -> b -> a"
}
```

Les messages possibles sont ceux de [Erreurs de validation](/docs/ci-cd/reference-yaml#erreurs-de-validation). Un fichier absent ne crée aucune pipeline.

### `GET /api/pipelines/{id}`

Renvoie une pipeline avec ses jobs et leurs journaux. Rôle minimal : Lecteur sur le dépôt de la pipeline.

Réponse 200 : les mêmes champs que dans la liste (`commitMessage` est résolu tant que le commit existe, `error` est renseigné pour un fichier de pipeline invalide), plus un tableau `jobs` (vide dans ce dernier cas) :

```json
{
  "id": "8f1a…",
  "commitSha": "9fceb02d…",
  "status": "running",
  "createdAt": "2026-03-01T10:00:00Z",
  "finishedAt": null,
  "triggeredBy": { "id": "0b6c…", "username": "alice" },
  "commitMessage": "Corrige le menu",
  "error": null,
  "jobs": [
    {
      "id": "a1b2…",
      "stage": "build",
      "name": "compile",
      "status": "success",
      "needs": [],
      "tags": ["docker"],
      "logs": "Compiling…\n",
      "createdAt": "2026-03-01T10:00:00Z",
      "startedAt": "2026-03-01T10:00:05Z",
      "finishedAt": "2026-03-01T10:02:00Z"
    }
  ]
}
```

Les journaux de chaque job sont dans le champ `logs`, en un seul texte. Il n'y a pas de route séparée pour les journaux : relisez la pipeline pour suivre un job en cours. Quand la rétention des journaux de l'instance (`logRetentionDays`, voir [API d'administration](/docs/api/administration)) a effacé le journal d'un job terminé, `logs` est vide et `logsPurgedAt` donne la date de l'effacement ; il vaut `null` tant que le journal est intact. `needs` liste les jobs dont celui-ci dépend, `tags` les étiquettes de runner exigées. Erreur : 404 `pipeline`.

### `POST /api/pipelines/{id}/cancel`

Annule une pipeline en cours : les jobs `pending` et `running` passent à `canceled`. Rôle minimal : Contributeur.

Réponse 204. Sur une pipeline déjà terminée, la route répond 204 sans rien changer : le résultat réel n'est pas écrasé. Un runner qui exécutait un job continue jusqu'au bout et son résultat est ignoré. Erreur : 404 `pipeline`.

## Lire et écrire un fichier de pipeline

Ces deux routes sont celles de l'[éditeur visuel](/docs/ci-cd/editeur-visuel) : elles font passer un fichier `.ferrisgit-ci.yml` à une description JSON et inversement, avec l'analyseur du serveur. Elles ne touchent à aucun dépôt et n'enregistrent rien ; tout utilisateur connecté peut les appeler. Le corps est limité à 256 Kio.

Une description a la forme `{ "stages": ["build", "test"], "jobs": { "compile": { "stage": "build", "image": "rust:1", "script": ["cargo build"], "variables": {}, "needs": [], "tags": [], "cache": [] } } }`. Les champs vides d'un job peuvent être omis.

Un problème est un objet `{ "code", "message", … }` avec, selon le cas, `job`, `stage`, `dependency`, `key` ou `jobs`. Ce sont les erreurs de [validation](/docs/ci-cd/reference-yaml#erreurs-de-validation), mais toutes sont rapportées d'un coup, pas seulement la première. Les codes sont `invalid_yaml`, `unknown_stage`, `unknown_dependency`, `needs_must_precede_own_stage`, `needs_cycle` et `invalid_cache_key`. Un avertissement est un objet `{ "code", "job"?, "stage"? }` qui n'empêche pas le fichier d'être accepté : `empty_image`, `empty_script`, `duplicate_stage`.

### `POST /api/pipeline-definitions/parse`

Lit un fichier de pipeline. Corps : `{ "yaml": "…" }`. Un fichier qui imbrique listes et dictionnaires entre crochets ou accolades sur plus de 64 niveaux est refusé sans être lu (problème `invalid_yaml`) : aucune pipeline n'en a besoin, et l'analyseur mettrait plusieurs secondes à le lire.

Réponse 200 :

| Champ | Description |
|---|---|
| `definition` | La description lue, ou `null` si le YAML n'a pas pu être lu. |
| `problems` | Les problèmes que le serveur reprocherait au fichier. |
| `warnings` | Les avertissements. |
| `ignoredFields` | Les clés que FerrisGit ne connaît pas (`include`, `jobs.release.when`…) : elles sont ignorées à l'exécution et disparaîtraient d'une réécriture. |
| `hasComments` | `true` si le fichier contient des commentaires, qu'une réécriture ne conserve pas. |

### `POST /api/pipeline-definitions/render`

Écrit une description en fichier de pipeline. Corps : `{ "definition": { … } }`.

Réponse 200 : `{ "yaml": "…", "problems": [], "warnings": [] }`. Le YAML est toujours produit, même s'il y a des problèmes : c'est à l'appelant de ne pas l'enregistrer tant que `problems` n'est pas vide. Les jobs sont écrits dans l'ordre des étapes, puis par nom.

### `GET /api/repositories/{repository_id}/pipeline-definition`

Renvoie le fichier de pipeline tel que la branche par défaut l'a, depuis l'endroit où le dépôt le range (le chemin des [réglages du dépôt](/docs/api/depots), `.ferrisgit-ci.yml` par défaut). C'est le point de départ de l'éditeur. Rôle minimal : Contributeur.

Réponse 200 : `{ "path": ".ferrisgit-ci.yml", "branch": "main", "baseSha": "…", "yaml": "…" }`. `yaml` vaut `null` quand la branche n'a pas ce fichier ; `branch`, `baseSha` et `yaml` valent `null` pour un dépôt sans commit. `baseSha` est la pointe de la branche au moment de la lecture : elle sert à `proposal`. Erreur : 400 si le fichier n'est pas du texte UTF-8.

### `POST /api/repositories/{repository_id}/pipeline-definition/proposal`

Enregistre un fichier de pipeline comme n'importe quelle modification : sur une **nouvelle branche** `pipeline-editor/<8 caractères>`, avec une merge request vers la branche par défaut. La branche par défaut elle-même n'est jamais écrite. Rôle minimal : Contributeur. Le commit est signé du nom et de l'adresse de l'appelant et bâti sur la pointe actuelle de la branche par défaut.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `yaml` | texte | oui | Le fichier complet. |
| `baseSha` | texte | oui | Le `baseSha` reçu à l'ouverture de l'éditeur. |
| `title` | texte | non | Titre de la merge request et message du commit. `Update the pipeline` si vide. |
| `description` | texte | non | Description de la merge request. |

Réponse 200 : `{ "branch": "pipeline-editor/3f9a1c2b", "commitSha": "…", "mergeRequestId": "…" }`. Si la merge request ne peut pas être ouverte, la branche créée pour elle est supprimée et l'erreur est renvoyée telle quelle.

Erreurs :

- 400 si le serveur refuserait le fichier (YAML mal formé, étape ou dépendance inconnue, clé de cache invalide, cycle : le message est celui de la [validation](/docs/ci-cd/reference-yaml#erreurs-de-validation)), si le fichier est identique à celui du dépôt, ou si le dépôt n'a encore aucun commit ;
- 409 si le fichier a changé sur la branche par défaut depuis `baseSha` : il faut rouvrir l'éditeur. D'autres fichiers qui ont avancé n'y changent rien ;
- 404 si le dépôt est inconnu ou si le rôle est insuffisant.

## Variables CI du dépôt

Ces variables sont injectées dans l'environnement des jobs du dépôt exécutés par un runner Docker, pas par le moteur Kubernetes. Leur valeur n'est jamais renvoyée par l'API. Voir [Variables et secrets](/docs/ci-cd/variables-et-secrets).

### `GET /api/repositories/{repository_id}/ci-variables`

Liste les variables, par clé. Rôle minimal : Mainteneur.

Réponse 200 : `[ { "id": "…", "key": "REGISTRY_TOKEN", "masked": true } ]`. Les valeurs ne sont pas incluses.

### `POST /api/repositories/{repository_id}/ci-variables`

Crée une variable, ou remplace la valeur d'une variable qui a déjà cette clé. Rôle minimal : Mainteneur.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `key` | texte | oui | Nom de la variable. |
| `value` | texte | oui | Valeur, stockée chiffrée. |
| `masked` | booléen | non | Si `true` (défaut), la valeur est masquée dans les journaux des jobs. |

Réponse 200 : `{ "id", "key", "masked" }`. L'appel est un « créer ou remplacer » : la clé est unique par dépôt.

```bash
curl -s -X POST "$BASE/api/repositories/$REPO_ID/ci-variables" \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d '{"key":"REGISTRY_TOKEN","value":"s3cret","masked":true}'
```

### `DELETE /api/repositories/{repository_id}/ci-variables/{id}`

Supprime une variable. Rôle minimal : Mainteneur. Réponse 204. Erreur : 404 (variable inconnue pour ce dépôt).

## Réglage lisible par tous

### `GET /api/settings/public`

Renvoie le moteur d'exécution de l'instance. Tout utilisateur connecté peut l'appeler.

Réponse 200 : `{ "executionEngine": "docker-runners" }`. La valeur est `docker-runners` ou `kubernetes`. Le réglage complet est dans [`GET /api/admin/settings`](/docs/api/administration).

## Runners (administration)

Un runner est un processus `ferrisgit-runner` qui interroge le serveur pour obtenir des jobs. Ces routes sont réservées aux administrateurs : sinon 401 `admin access required`.

### `POST /api/admin/runners`

Enregistre un runner et renvoie son jeton.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `name` | texte | oui | Nom du runner. |
| `tags` | tableau de textes | non | Étiquettes du runner. Il ne prendra que les jobs dont toutes les étiquettes sont dans cette liste. |

Réponse 200 :

```json
{ "id": "b3c4…", "name": "vps-1", "tags": ["docker"], "token": "fgr_9a7e…" }
```

Le jeton (`fgr_` suivi de 64 caractères hexadécimaux) n'est visible qu'ici. Il n'expire jamais : seule la révocation l'invalide.

### `GET /api/admin/runners`

Liste les runners. Réponse 200 : `[ { "id", "name", "tags", "lastHeartbeatAt", "createdAt" } ]`. `lastHeartbeatAt` est mis à jour à chaque requête authentifiée du runner, et vaut `null` s'il n'a jamais appelé le serveur.

### `DELETE /api/admin/runners/{id}`

Révoque un runner : son jeton cesse d'authentifier tout de suite. Les jobs qu'il avait pris et qui ne sont pas terminés repassent à `pending`, pour qu'un autre runner les reprenne. Réponse 204, même si l'identifiant n'existe pas.

## API des runners

Ces routes sont appelées par `ferrisgit-runner`. Vous n'en avez besoin que pour écrire votre propre runner.

L'authentification se fait avec le jeton du runner :

```http
Authorization: Bearer fgr_…
```

Un jeton inconnu répond 401 `invalid runner token`, un en-tête absent 401 `missing Authorization header`. Chaque requête authentifiée met à jour la date de dernier signe de vie du runner. Le même jeton permet aussi de cloner en lecture n'importe quel dépôt par Git en HTTP (le nom d'utilisateur n'est pas contrôlé), mais pas de pousser.

Le déroulement est une boucle d'interrogation : toutes les 5 secondes par défaut (`FERRISGIT_POLL_INTERVAL_SECS`), le runner demande un job. S'il en reçoit un, il clone le dépôt, exécute les commandes, envoie les journaux par morceaux, puis rapporte le résultat.

### `POST /api/runner/register`

Enregistre un runner sans passer par un administrateur, avec le jeton d'enregistrement que l'administrateur a configuré (réglage `runnerRegistrationToken`). Route anonyme.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `registration_token` | texte | oui | Jeton d'enregistrement de l'instance. Ce champ est en `snake_case`, contrairement au reste de l'API. |
| `name` | texte | oui | Nom du runner. |
| `tags` | tableau de textes | non | Étiquettes du runner. |

Réponse 200 : `{ "id", "name", "tags", "token" }`, comme `POST /api/admin/runners`.

Erreurs : 401 `self-service runner registration is disabled` (aucun jeton d'enregistrement n'est configuré), 401 `invalid registration token`.

> **Note** : `ferrisgit-runner` n'appelle pas cette route. Il se configure avec un jeton déjà émis (`FERRISGIT_RUNNER_TOKEN`).

### `POST /api/runner/jobs/claim`

Demande le prochain job disponible. Authentification : jeton de runner.

Le serveur choisit le plus ancien job `pending` dont toutes les étiquettes figurent parmi celles du runner et que l'ordonnancement libère : ses `needs` ont réussi, ou, sans `needs`, tous les jobs des étapes précédentes ont réussi (voir [Ordre de lancement](/docs/ci-cd/reference-yaml#ordre-de-lancement)). Le premier job qui démarre fait passer la pipeline à `running`. Le corps de la requête est ignoré : ce sont les étiquettes enregistrées avec le runner qui comptent. Le job passe à `running`.

Réponse 204 (corps vide) s'il n'y a rien à faire, y compris quand le plafond de jobs simultanés (`maxConcurrentJobs`) est atteint. Sinon, réponse 200 :

```json
{
  "id": "a1b2…",
  "stage": "build",
  "name": "compile",
  "image": "rust:1.85",
  "script": ["cargo build --release"],
  "variables": { "RUST_LOG": "info" },
  "repositoryOwner": "alice",
  "repositoryName": "blog",
  "commitSha": "9fceb02d…",
  "ciVariables": { "REGISTRY_TOKEN": "s3cret" },
  "maskedValues": ["s3cret"]
}
```

`variables` vient du fichier de pipeline. `ciVariables` contient les variables CI du dépôt en clair. `maskedValues` liste les valeurs de celles qui sont masquées : le runner les remplace dans chaque ligne de journal.

### `POST /api/runner/jobs/{id}/logs`

Ajoute un morceau de journal au job. Authentification : jeton de runner.

Corps : `{ "chunk": "Compiling…\n" }`. Le texte est ajouté à la suite des journaux déjà reçus. Réponse 204.

Erreur : 404 `job` si le job n'existe pas ou n'a pas été pris par ce runner (la réponse est la même, pour ne pas révéler les identifiants).

### `POST /api/runner/jobs/{id}/result`

Rapporte le résultat du job. Authentification : jeton de runner.

Corps : `{ "status": "success" }`, avec `status` parmi `success`, `failed` et `canceled`. Réponse 204.

Le serveur en déduit l'état de la pipeline. Un job `failed` ou `canceled` fait passer à `skipped` tous les jobs qui ne peuvent plus démarrer à cause de lui (ceux qui en dépendent par `needs` ou par une étape suivante, de proche en proche). Quand tous les jobs sont terminés, la pipeline passe à `failed` si un job a échoué, sinon à `canceled` si un job est annulé, sinon à `success`. L'échec d'une pipeline notifie la personne qui l'a déclenché et émet le webhook `pipeline_failed`. Un résultat pour un job déjà terminal est ignoré, pour qu'un runner en retard n'écrase pas une annulation.

Erreur : 404 `job` (inconnu, ou pris par un autre runner).
