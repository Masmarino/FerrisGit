# Webhooks et notifications

Cette page décrit les routes des webhooks d'un dépôt, des notifications d'un utilisateur, et deux routes qui s'y rattachent dans l'interface : le tableau de bord et la recherche. Toutes demandent un JWT (voir [Authentification](/docs/api/authentification)). Un rôle insuffisant répond 404, jamais 403. Pour l'usage dans l'interface, voir [Webhooks](/docs/utilisation/webhooks) et [Notifications et recherche](/docs/utilisation/notifications-et-recherche).

## Types d'événements

Les webhooks et les notifications partagent la même liste de 12 types :

| Type | Déclencheur |
|---|---|
| `merge_request_approved` | Une demande de fusion est approuvée. |
| `merge_request_changes_requested` | Des changements sont demandés sur une demande de fusion. |
| `merge_request_commented` | Une demande de fusion reçoit un commentaire. |
| `merge_request_merged` | Une demande de fusion est fusionnée. |
| `merge_request_closed` | Une demande de fusion est fermée. |
| `collaborator_added` | Un collaborateur est ajouté. |
| `collaborator_role_changed` | Le rôle d'un collaborateur change. |
| `collaborator_removed` | Un collaborateur est retiré. |
| `pipeline_failed` | Une pipeline échoue. |
| `issue_assigned` | Un ticket est assigné. |
| `issue_commented` | Un ticket reçoit un commentaire. |
| `issue_closed` | Un ticket est fermé. |

## Webhooks

Les routes de webhook demandent le rôle Mainteneur sur le dépôt.

Un webhook est renvoyé sous cette forme. Le secret n'est jamais renvoyé :

```json
{
  "id": "f6a7…",
  "url": "https://ci.example.org/hooks/ferrisgit",
  "events": ["merge_request_merged", "pipeline_failed"],
  "active": true,
  "createdAt": "2026-03-01T10:00:00Z"
}
```

### `GET /api/repositories/{repository_id}/webhooks`

Liste les webhooks du dépôt. Rôle minimal : Mainteneur. Réponse 200 : un tableau de webhooks.

### `POST /api/repositories/{repository_id}/webhooks`

Crée un webhook, actif. Rôle minimal : Mainteneur.

Corps (tous les champs sont obligatoires) :

| Champ | Type | Description |
|---|---|---|
| `url` | texte | Adresse appelée, en `http` ou `https`. |
| `secret` | texte | Secret partagé, stocké chiffré, qui sert à signer les envois. |
| `events` | tableau de textes | Types d'événements auxquels s'abonner (voir le tableau plus haut). |

L'adresse est contrôlée à la création, puis à chaque envoi : elle ne doit pas viser une adresse de bouclage, privée (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16), locale au lien (dont l'adresse des métadonnées cloud 169.254.169.254), multicast ou partagée (100.64.0.0/10), que l'hôte soit donné en toutes lettres ou en IP. Un dépôt peut avoir 20 webhooks au plus.

Réponse 200 : le webhook.

Erreurs : 400 `invalid webhook url`, 400 `webhook url scheme must be http or https, got <schéma>`, 400 `webhook url must have a host`, 400 `could not resolve webhook host: …`, 400 `webhook url resolves to a disallowed address (<ip>)`, 400 `unknown webhook event kind(s): …`, 400 `a repository may have at most 20 webhooks`.

```bash
curl -s -X POST "$BASE/api/repositories/$REPO_ID/webhooks" \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d '{"url":"https://ci.example.org/hooks/ferrisgit","secret":"un-secret","events":["pipeline_failed"]}'
```

### `PATCH /api/repositories/{repository_id}/webhooks/{id}`

Modifie un webhook. Rôle minimal : Mainteneur. Tous les champs sont facultatifs : un champ absent garde sa valeur.

Corps :

| Champ | Type | Description |
|---|---|---|
| `url` | texte | Nouvelle adresse, contrôlée comme à la création. |
| `secret` | texte | Nouveau secret. |
| `events` | tableau de textes | Nouvelle liste d'événements. |
| `active` | booléen | Active ou suspend le webhook. |

Réponse 200 : le webhook à jour. Erreurs : celles de la création pour `url` et `events`, 404 (webhook inconnu pour ce dépôt).

### `DELETE /api/repositories/{repository_id}/webhooks/{id}`

Supprime un webhook. Rôle minimal : Mainteneur. Réponse 204. Erreur : 404 (webhook inconnu pour ce dépôt).

### `GET /api/repositories/{repository_id}/webhooks/{id}/deliveries`

Liste les 20 dernières livraisons du webhook, de la plus récente à la plus ancienne. Rôle minimal : Mainteneur.

Réponse 200 :

```json
[
  { "id": "a8b9…", "eventKind": "pipeline_failed", "httpStatus": 200, "success": true, "errorMessage": null, "createdAt": "2026-03-01T10:05:00Z" }
]
```

`httpStatus` vaut `null` si la requête n'a pas pu aboutir ; `errorMessage` dit alors pourquoi. Erreur : 404 (webhook inconnu pour ce dépôt).

## Notifications

Les notifications sont personnelles : chaque route ne voit que celles du compte connecté.

Une notification a cette forme :

```json
{
  "id": "b9c0…",
  "kind": "merge_request_approved",
  "repositoryOwner": "alice",
  "repositoryName": "blog",
  "actorUsername": "bob",
  "mergeRequestId": "c2d4…",
  "mergeRequestTitle": "Corrige le menu",
  "pipelineId": null,
  "commitSha": null,
  "issueId": null,
  "issueNumber": null,
  "issueTitle": null,
  "role": null,
  "read": false,
  "createdAt": "2026-03-01T11:00:00Z"
}
```

`kind` est l'un des types du tableau plus haut. Les champs qui ne concernent pas ce type valent `null` : `mergeRequest*` pour les demandes de fusion, `pipelineId` et `commitSha` pour une pipeline, `issue*` pour un ticket, `role` pour un changement de rôle de collaborateur. `actorUsername` est la personne à l'origine de l'événement (`null` pour une pipeline).

### `GET /api/notifications`

Liste vos 50 notifications les plus récentes. Réponse 200 : un tableau de notifications.

### `GET /api/notifications/unread-count`

Compte vos notifications non lues. Réponse 200 : `{ "count": 3 }`.

### `POST /api/notifications/{id}/read`

Marque une notification comme lue. Réponse 204. La route répond aussi 204 si l'identifiant n'existe pas ou appartient à quelqu'un d'autre, sans rien changer.

### `POST /api/notifications/read-all`

Marque toutes vos notifications comme lues. Réponse 204.

## Tableau de bord

### `GET /api/dashboard`

Rassemble ce qui vous concerne. Aucun paramètre.

Réponse 200 :

```json
{
  "assignedIssues": [],
  "authoredIssues": [],
  "authoredMergeRequests": [],
  "mergeRequestsToReview": [],
  "activity": []
}
```

Chaque liste compte 20 éléments au plus.

- `assignedIssues` : les tickets qui vous sont assignés. `authoredIssues` : les tickets que vous avez ouverts. Dans les deux cas, ils viennent des dépôts que vous pouvez voir, publics compris.
- `authoredMergeRequests` : vos demandes de fusion.
- `mergeRequestsToReview` : les demandes de fusion à relire, limitées aux dépôts dont vous êtes membre (propriétaire, collaborateur ou membre d'un groupe).
- `activity` : vos 20 dernières notifications, au format ci-dessus.

Un ticket est de la forme `{ "id", "number", "title", "status", "kind", "createdAt", "repository": { "id", "name", "path" } }`. Une demande de fusion est de la forme `{ "id", "title", "status", "sourceBranch", "targetBranch", "createdAt", "repository": { "id", "name", "path" } }`. `path` est le chemin du dépôt en segments.

## Recherche

### `GET /api/search`

Cherche dans les dépôts, tickets, demandes de fusion et utilisateurs que vous pouvez voir : vos dépôts, ceux où vous êtes collaborateur ou membre d'un groupe, et les dépôts publics.

Paramètre de requête : `q`, le texte cherché. Un `q` vide ou fait d'espaces donne quatre listes vides. Les utilisateurs se trouvent par leur nom d'utilisateur, tous comptes confondus.

Réponse 200 : au plus 8 résultats par type.

```json
{
  "repositories": [ { "id": "…", "name": "blog", "description": "…", "path": ["alice", "blog"], "visibility": "private" } ],
  "issues": [ { "id": "…", "number": 12, "title": "…", "status": "todo", "kind": "bug", "createdAt": "…", "repository": { "id": "…", "name": "blog", "path": ["alice", "blog"] } } ],
  "mergeRequests": [ { "id": "…", "title": "…", "status": "open", "sourceBranch": "…", "targetBranch": "main", "createdAt": "…", "repository": { "id": "…", "name": "blog", "path": ["alice", "blog"] } } ],
  "users": [ { "id": "…", "username": "alice" } ]
}
```

```bash
curl -s "$BASE/api/search?q=menu" -H "Authorization: Bearer $JWT"
```
