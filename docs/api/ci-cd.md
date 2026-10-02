# Pipelines et runners

Cette page décrit les routes des pipelines et de leurs jobs, des variables CI d'un dépôt, des runners (côté administration) et l'API que `ferrisgit-runner` appelle sur le serveur. Pour la configuration d'un pipeline, voir [Référence de .ferrisgit-ci.yml](/docs/ci-cd/reference-yaml). Pour installer un runner, voir [Runners Docker](/docs/ci-cd/runners-docker).

Les routes de pipeline, de variables et d'administration demandent un JWT (voir [Authentification](/docs/api/authentification)). Un rôle insuffisant sur un dépôt répond 404, jamais 403. Les routes `/api/runner/jobs/*` s'authentifient avec un jeton de runner (voir plus bas).

Il n'y a pas de route pour créer un pipeline à la main : un pipeline démarre quand un `git push` réussit, et après la fusion d'une demande de fusion.

## Pipelines et jobs

Les statuts d'un pipeline sont `pending`, `running`, `success`, `failed` et `canceled`. Ceux d'un job sont les mêmes, plus `skipped` : un job qui n'a jamais démarré parce qu'un job dont il dépend (par `needs` ou par une étape précédente) a échoué ou a été annulé. Un pipeline est `pending` jusqu'à ce que son premier job démarre, puis `running` jusqu'à ce que tous ses jobs soient terminés.

### `GET /api/repositories/{repository_id}/pipelines`

Liste les pipelines du dépôt, du plus récent au plus ancien. Rôle minimal : Lecteur.

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

`finishedAt` est `null` tant que le pipeline n'est pas terminé. `triggeredBy` est `null` si le compte a été supprimé. `commitMessage` n'est résolu que pour les 50 pipelines les plus récents ; pour les autres, il vaut `null`.

`error` vaut `null` pour un pipeline normal. Quand le fichier de pipeline d'un push était présent mais invalide, FerrisGit crée tout de même un pipeline : `status` vaut `failed`, `finishedAt` est renseigné, il n'a aucun job, et `error` contient le message de l'analyseur, par exemple :

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

Les messages possibles sont ceux de [Erreurs de validation](/docs/ci-cd/reference-yaml#erreurs-de-validation). Un fichier absent ne crée aucun pipeline.

### `GET /api/pipelines/{id}`

Renvoie un pipeline avec ses jobs et leurs journaux. Rôle minimal : Lecteur sur le dépôt du pipeline.

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

Les journaux de chaque job sont dans le champ `logs`, en un seul texte. Il n'y a pas de route séparée pour les journaux : relisez le pipeline pour suivre un job en cours. Quand la rétention des journaux de l'instance (`logRetentionDays`, voir [API d'administration](/docs/api/administration)) a effacé le journal d'un job terminé, `logs` est vide et `logsPurgedAt` donne la date de l'effacement ; il vaut `null` tant que le journal est intact. `needs` liste les jobs dont celui-ci dépend, `tags` les étiquettes de runner exigées. Erreur : 404 `pipeline`.

### `POST /api/pipelines/{id}/cancel`

Annule un pipeline en cours : les jobs `pending` et `running` passent à `canceled`. Rôle minimal : Contributeur.

Réponse 204. Sur un pipeline déjà terminé, la route répond 204 sans rien changer : le résultat réel n'est pas écrasé. Un runner qui exécutait un job continue jusqu'au bout et son résultat est ignoré. Erreur : 404 `pipeline`.

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

Le serveur choisit le plus ancien job `pending` dont toutes les étiquettes figurent parmi celles du runner et que l'ordonnancement libère : ses `needs` ont réussi, ou, sans `needs`, tous les jobs des étapes précédentes ont réussi (voir [Ordre de lancement](/docs/ci-cd/reference-yaml#ordre-de-lancement)). Le premier job qui démarre fait passer le pipeline à `running`. Le corps de la requête est ignoré : ce sont les étiquettes enregistrées avec le runner qui comptent. Le job passe à `running`.

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

Le serveur en déduit l'état du pipeline. Un job `failed` ou `canceled` fait passer à `skipped` tous les jobs qui ne peuvent plus démarrer à cause de lui (ceux qui en dépendent par `needs` ou par une étape suivante, de proche en proche). Quand tous les jobs sont terminés, le pipeline passe à `failed` si un job a échoué, sinon à `canceled` si un job est annulé, sinon à `success`. L'échec d'un pipeline notifie la personne qui l'a déclenché et émet le webhook `pipeline_failed`. Un résultat pour un job déjà terminal est ignoré, pour qu'un runner en retard n'écrase pas une annulation.

Erreur : 404 `job` (inconnu, ou pris par un autre runner).
