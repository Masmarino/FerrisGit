# Demandes de fusion

Cette page décrit les routes des demandes de fusion (merge requests) : création, modification, commentaires, relectures, fusion et fermeture. Toutes demandent un JWT (voir [Authentification](/docs/api/authentification)). Un rôle insuffisant répond 404, jamais 403. Pour l'usage dans l'interface, voir [Demandes de fusion](/docs/utilisation/merge-requests).

La liste des branches d'un dépôt, utile pour créer une demande, est décrite dans [Dépôts et groupes](/docs/api/depots) (`GET /api/repositories/{repository_id}/branches`).

## Objets

### Demande de fusion

```json
{
  "id": "c2d4…",
  "sourceBranch": "feature/menu",
  "targetBranch": "main",
  "title": "Corrige le menu",
  "description": "…",
  "status": "open",
  "mergeCommitSha": null,
  "milestoneId": null,
  "createdAt": "2026-03-01T10:00:00Z",
  "closedAt": null,
  "labels": [ { "id": "…", "name": "bug", "color": "#d73a4a", "repositoryId": "…", "groupId": null, "createdAt": "…" } ],
  "author": { "id": "0b6c…", "username": "alice" },
  "commentCount": 2
}
```

`status` vaut `open`, `merged` ou `closed`. `author` vaut `null` si le compte de l'auteur a été supprimé. `mergeCommitSha` est renseigné après la fusion.

### Commentaire

```json
{
  "id": "e5f6…",
  "authorId": "0b6c…",
  "body": "Je suggère ceci",
  "createdAt": "2026-03-01T10:05:00Z",
  "replyToId": null,
  "filePath": "src/menu.ts",
  "lineNumber": 12,
  "endLine": null,
  "side": "new",
  "outdated": false,
  "resolved": false,
  "suggestedContent": "const a = 1;",
  "appliedAt": null,
  "appliedCommitSha": null,
  "author": { "id": "0b6c…", "username": "alice" }
}
```

Un commentaire général n'a ni `filePath`, ni `lineNumber`, ni `side` (ils valent `null`). Un commentaire ancré porte ces trois champs, et `endLine` pour une plage. `side` vaut `old` ou `new`. `outdated` est vrai quand la ligne commentée a changé dans le diff actuel ; il n'est calculé que pour une demande ouverte.

## Demandes

### `GET /api/repositories/{repository_id}/merge-requests`

Liste les demandes de fusion du dépôt, les plus récentes d'abord. Rôle minimal : Lecteur.

Paramètres de requête :

| Paramètre | Description |
|---|---|
| `labelIds` | Liste d'UUID séparés par des virgules. Garde les demandes qui portent au moins l'une de ces étiquettes. |
| `milestoneId` | UUID d'un jalon. |

Réponse 200 : un tableau de demandes. Erreur : 400 `labelIds must be a comma-separated list of UUIDs`.

### `POST /api/repositories/{repository_id}/merge-requests`

Crée une demande de fusion. Rôle minimal : Contributeur.

Corps (tous les champs sont obligatoires) :

| Champ | Type | Description |
|---|---|---|
| `sourceBranch` | texte | Branche à fusionner. |
| `targetBranch` | texte | Branche qui la recevra. |
| `title` | texte | Titre. |
| `description` | texte | Description, éventuellement vide. |

Réponse 200 : la demande.

Erreurs : 400 `source and target branch must be different`, 400 `branch not found: <nom>`.

```bash
curl -s -X POST "$BASE/api/repositories/$REPO_ID/merge-requests" \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d '{"sourceBranch":"feature/menu","targetBranch":"main","title":"Corrige le menu","description":""}'
```

### `GET /api/merge-requests/{id}`

Renvoie une demande. Rôle minimal : Lecteur. Réponse 200 : la demande. Erreur : 404 `merge request`.

### `PATCH /api/merge-requests/{id}`

Modifie le titre, la description et le jalon. Rôle minimal : Contributeur.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `title` | texte | oui | Nouveau titre. |
| `description` | texte | oui | Nouvelle description. |
| `milestoneId` | UUID ou `null` | oui | Jalon à associer. `null` retire le jalon. Le champ doit être présent. |

Réponse 200 : la demande à jour.

Erreurs : 400 `milestoneId is required on a merge request update; send null to clear it`, 400 `milestone … is not usable on this repository` (le jalon ne vient ni du dépôt ni d'un de ses groupes), 404 `merge request`, 404 `milestone`.

### `PUT /api/merge-requests/{id}/labels`

Remplace l'ensemble des étiquettes de la demande. Rôle minimal : Contributeur.

Corps : `{ "labelIds": ["…", "…"] }`. Une liste vide retire toutes les étiquettes. Réponse 200 : le tableau des étiquettes de la demande.

Erreurs : 404 `label`, 400 `label … is not usable on this repository` (une étiquette doit appartenir au dépôt ou à l'un de ses groupes).

### `POST /api/merge-requests/{id}/close`

Ferme la demande sans la fusionner. Rôle minimal : Contributeur. Réponse 204. L'auteur est notifié et le webhook `merge_request_closed` part. Erreur : 400 `merge request is not open`.

### `POST /api/merge-requests/{id}/merge`

Fusionne la demande. Rôle minimal : Mainteneur.

FerrisGit ne fait que des fusions sans conflit, avec le message `Merge branch '<source>' into <cible>`. Après une fusion réussie, un pipeline démarre sur le nouveau sommet de la branche cible, et l'auteur est notifié.

Si le dépôt exige des approbations (`requiredApprovals` supérieur à 0), la fusion est refusée tant qu'il n'y a pas assez d'approbations à jour (données sur le sommet actuel de la branche source) ou tant qu'une relecture à jour demande des changements.

Réponse 200, avec un champ `outcome` :

```json
{ "outcome": "merged", "id": "c2d4…", "status": "merged", "mergeCommitSha": "9fceb02d…", "title": "Corrige le menu", "…": "…" }
```

En cas de succès, la réponse reprend tous les champs de la demande, avec `outcome: "merged"`. En cas de conflit, la demande reste ouverte et la réponse est simplement `{ "outcome": "conflicting" }` (code 200).

Erreurs : 400 `merge request is not open`, 400 `changes requested by <utilisateur>`, 400 `<n>/<requis> approvals`, 400 `source branch no longer exists`, 404 `merge request`.

## Diff et fil d'activité

### `GET /api/merge-requests/{id}/diff`

Renvoie le diff entre la branche source et la branche cible, en lignes appariées pour un affichage en deux colonnes. Rôle minimal : Lecteur.

Réponse 200 :

```json
[
  {
    "path": "src/menu.ts",
    "change": "modified",
    "hunks": [
      { "rows": [
        { "oldLine": 12, "oldContent": "const a = 0;", "newLine": 12, "newContent": "const a = 1;", "kind": "modified" }
      ] }
    ]
  }
]
```

`change` vaut `added`, `modified`, `deleted` ou `binary`. `kind` vaut `context`, `modified` (une ligne retirée suivie d'une ligne ajoutée), `removed` ou `added`. Les champs absents d'un côté valent `null`.

### `GET /api/merge-requests/{id}/timeline`

Renvoie l'historique complet : commentaires, fils de discussion et événements, dans l'ordre. Rôle minimal : Lecteur.

Réponse 200 : `{ "author": { "id", "username" } | null, "items": [ … ] }`. Chaque élément de `items` a un champ `type` :

| `type` | Contenu |
|---|---|
| `comment` | Un commentaire général : `id`, `createdAt`, `author`, `body`. |
| `thread` | Un fil de discussion sur le code : `id`, `createdAt`, `filePath`, `lineNumber`, `endLine`, `side`, `outdated`, `resolved`, `resolvedBy`, `resolvedAt`, `excerpt` (les lignes de code concernées, avec `line`, `kind`, `content`), `root` (le commentaire racine) et `replies` (les réponses). |
| `event` | Un événement : `id`, `createdAt`, `actor`, `kind`, `payload` (objet libre selon le type). |

Les valeurs de `kind` des événements sont `review_submitted`, `labels_changed`, `milestone_changed`, `title_changed`, `commits_pushed`, `merged`, `closed`, `thread_resolved` et `thread_reopened`. Si le diff ne peut pas être calculé (branche source supprimée, par exemple), la réponse reste valide, sans extraits.

## Commentaires

### `GET /api/merge-requests/{id}/comments`

Liste tous les commentaires de la demande, réponses comprises, à plat. Rôle minimal : Lecteur. Réponse 200 : un tableau de commentaires.

### `POST /api/merge-requests/{id}/comments`

Ajoute un commentaire, une réponse ou une suggestion. Rôle minimal : Contributeur. L'auteur de la demande est notifié (sauf s'il commente lui-même) et le webhook `merge_request_commented` part.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `body` | texte | oui | Le texte du commentaire. |
| `replyToId` | UUID | non | Répond à ce commentaire. Il doit être la racine d'un fil. |
| `filePath` | texte | non | Fichier commenté. |
| `lineNumber` | entier | non | Ligne commentée. |
| `endLine` | entier | non | Dernière ligne, pour commenter une plage. |
| `side` | texte | non | `old` ou `new`. |
| `suggestedContent` | texte | non | Remplacement proposé pour les lignes commentées. |

Règles :

- `filePath`, `lineNumber` et `side` vont ensemble : tous présents (commentaire de code) ou tous absents (commentaire général). `endLine` et `suggestedContent` exigent les trois.
- Une suggestion ne peut viser que le côté `new` du diff.
- Dans une réponse (`replyToId`), l'ancrage et la suggestion sont repris de la racine du fil, et ceux du corps sont ignorés.

Réponse 200 : le commentaire.

Erreurs : 400 `filePath, lineNumber and side must all be present, or all absent`, 400 `endLine and suggestedContent require filePath, lineNumber and side`, 400 `unknown diff side: …`, 400 `a suggestion can only be anchored to the new side of the diff`, 400 `that line is no longer part of the diff — reload and try again`, 400 `can only reply to a thread's root comment on this merge request`, 404 `merge request`.

### `POST /api/merge-requests/{id}/comments/{comment_id}/resolve`

Marque un fil comme résolu. Rôle minimal : Contributeur. Réponse 204. `comment_id` est l'identifiant du commentaire racine.

Erreurs : 400 `only a thread's root comment can be resolved`, 404 `comment`.

### `POST /api/merge-requests/{id}/comments/{comment_id}/unresolve`

Rouvre un fil résolu. Mêmes droits, réponse et erreurs que `resolve`.

### `POST /api/merge-requests/{id}/comments/{comment_id}/apply-suggestion`

Applique la suggestion d'un commentaire : FerrisGit crée un commit sur la branche source, au nom de la personne qui applique, avec le message `Apply suggestion from @<utilisateur>`. Rôle minimal : Contributeur. La demande doit être ouverte.

Réponse 200 : le commentaire, avec `appliedAt` et `appliedCommitSha` renseignés.

Erreurs : 400 `merge request is not open`, 404 `comment`, 400 `only a thread's root comment can carry a suggestion`, 400 `this comment has no suggestion to apply`, 400 `this suggestion has already been applied`, 400 `this suggestion is outdated and can no longer be applied — reload and try again`, 400 `source branch no longer exists`, et 409 si un push est arrivé sur la branche source pendant l'opération.

## Relectures

### `GET /api/merge-requests/{id}/reviews`

Résumé des relectures et de l'état des approbations. Rôle minimal : Lecteur.

Réponse 200 :

```json
{
  "reviews": [
    { "userId": "…", "username": "bob", "decision": "approved", "stale": false, "createdAt": "2026-03-01T11:00:00Z" }
  ],
  "requiredApprovals": 1,
  "liveApprovalCount": 1,
  "blocked": false
}
```

Une relecture est `stale` quand la branche source a reçu un nouveau commit depuis. Seules les relectures à jour comptent. `blocked` est vrai quand le dépôt exige des approbations et que le nombre d'approbations à jour est insuffisant, ou qu'une relecture à jour demande des changements.

### `POST /api/merge-requests/{id}/reviews`

Soumet votre relecture. Rôle minimal : Contributeur. Une nouvelle relecture remplace la vôtre précédente. Elle est liée au sommet actuel de la branche source.

Corps : `{ "decision": "approved" }`, avec `decision` valant `approved` ou `changes_requested`.

Réponse 200 : `{ "userId", "username", "decision", "stale": false, "createdAt" }`. L'auteur de la demande est notifié quand la relecture change.

Erreurs : 400 `unknown review decision: …`, 400 `a merge request's author cannot approve their own merge request`, 400 `source branch no longer exists`, 404 `merge request`.
