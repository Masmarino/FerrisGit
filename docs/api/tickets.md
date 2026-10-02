# Tickets, étiquettes et jalons

Cette page décrit les routes des tickets (issues), de leurs commentaires, des étiquettes et des jalons. Le tableau kanban n'a pas de route propre : c'est la liste des tickets groupée par `status`, et on déplace une carte avec `PATCH …/status`. Toutes les routes demandent un JWT (voir [Authentification](/docs/api/authentification)). Un rôle insuffisant répond 404, jamais 403. Pour l'usage dans l'interface, voir [Tickets, étiquettes et jalons](/docs/utilisation/tickets).

## Objets

### Ticket

```json
{
  "id": "4a7b…",
  "number": 12,
  "authorId": "0b6c…",
  "assigneeId": null,
  "milestoneId": null,
  "title": "Le menu se ferme tout seul",
  "description": "…",
  "status": "todo",
  "kind": "bug",
  "parentIssueId": null,
  "createdAt": "2026-03-01T10:00:00Z",
  "closedAt": null,
  "labels": [],
  "author": { "id": "0b6c…", "username": "alice" },
  "assignee": null,
  "commentCount": 0
}
```

- `number` est le numéro du ticket dans son dépôt, à partir de 1. Les routes de détail l'utilisent.
- `status` vaut `todo` (À faire), `in_progress` (En cours), `in_review` (En revue) ou `done` (Terminé). Ce sont les quatre colonnes du kanban.
- `kind` vaut `bug`, `feature`, `task` ou `epic`.
- `closedAt` est renseigné quand le statut est `done`.
- `parentIssueId` est toujours `null` aujourd'hui : aucune route ne permet de le définir.
- `author` et `assignee` valent `null` quand le compte n'existe plus ou quand il n'y a pas d'assigné.

### Étiquette

```json
{ "id": "5c8d…", "name": "bug", "color": "#d73a4a", "repositoryId": "3c1f…", "groupId": null, "createdAt": "2026-03-01T10:00:00Z" }
```

Une étiquette appartient soit à un dépôt (`repositoryId` renseigné), soit à un groupe (`groupId` renseigné), jamais aux deux.

### Jalon

```json
{ "id": "6d9e…", "title": "v1.0", "description": "", "dueDate": "2026-06-01T00:00:00Z", "state": "open", "repositoryId": "3c1f…", "groupId": null, "createdAt": "2026-03-01T10:00:00Z" }
```

`state` vaut `open` ou `closed`. `dueDate` est facultative (`null`).

## Tickets

### `GET /api/repositories/{repository_id}/issues`

Liste les tickets du dépôt, par numéro croissant. Rôle minimal : Lecteur.

Paramètres de requête :

| Paramètre | Description |
|---|---|
| `labelIds` | UUID séparés par des virgules. Garde les tickets qui portent au moins l'une de ces étiquettes. |
| `milestoneId` | UUID d'un jalon. |

Réponse 200 : un tableau de tickets. Pour construire un kanban, regroupez-les par `status`. Erreur : 400 `labelIds must be a comma-separated list of UUIDs`.

### `POST /api/repositories/{repository_id}/issues`

Crée un ticket, au statut `todo`. Rôle minimal : Contributeur. Sur un dépôt public, un utilisateur connecté sans rôle est Lecteur : il lit les tickets mais n'en ouvre pas (404).

Corps (tous les champs sont obligatoires) :

| Champ | Type | Description |
|---|---|---|
| `title` | texte | Titre. |
| `description` | texte | Description, éventuellement vide. |
| `kind` | texte | `bug`, `feature`, `task` ou `epic`. |

Réponse 200 : le ticket. Erreur : 400 `unknown issue kind: …`.

```bash
curl -s -X POST "$BASE/api/repositories/$REPO_ID/issues" \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d '{"title":"Le menu se ferme tout seul","description":"","kind":"bug"}'
```

### `GET /api/repositories/{repository_id}/issues/{number}`

Renvoie un ticket par son numéro. Rôle minimal : Lecteur. Réponse 200 : le ticket. Erreur : 404 `issue`.

### `PATCH /api/repositories/{repository_id}/issues/{number}`

Modifie le titre, la description, le type et le jalon. Rôle minimal : Contributeur.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `title` | texte | oui | Titre. |
| `description` | texte | oui | Description. |
| `kind` | texte | oui | `bug`, `feature`, `task` ou `epic`. |
| `milestoneId` | UUID ou `null` | oui | Jalon associé. `null` le retire. Le champ doit être présent. |

Réponse 200 : le ticket à jour.

Erreurs : 400 `unknown issue kind: …`, 400 `milestoneId is required on an issue update; send null to clear it`, 400 `milestone … is not usable on this repository`, 404 `issue`, 404 `milestone`.

### `PATCH /api/repositories/{repository_id}/issues/{number}/status`

Déplace le ticket dans une colonne du kanban. Rôle minimal : Contributeur.

Corps : `{ "status": "in_progress" }`, avec `status` parmi `todo`, `in_progress`, `in_review`, `done`. Passer à `done` renseigne `closedAt` ; tout autre statut l'efface.

Réponse 200 : le ticket. Erreurs : 400 `unknown issue status: …`, 404 `issue`.

### `POST /api/repositories/{repository_id}/issues/{number}/assign`

Assigne le ticket, ou retire l'assignation. Rôle minimal : Contributeur. La personne assignée reçoit une notification (sauf si c'est vous) et le webhook `issue_assigned` part.

Corps : `{ "assigneeId": "…" }`, ou `{ "assigneeId": null }` pour désassigner. L'assigné doit avoir au moins le rôle Contributeur sur le dépôt : propriétaire d'un dépôt personnel, collaborateur direct, ou membre d'un groupe qui contient le dépôt (le rôle hérité d'un groupe parent compte). Un Lecteur, ou une personne sans rôle, est refusé.

Réponse 200 : le ticket. Erreurs : 400 `assignee must be at least a contributor on this repository`, 404 `issue`.

### `PUT /api/repositories/{repository_id}/issues/{number}/labels`

Remplace l'ensemble des étiquettes du ticket. Rôle minimal : Contributeur.

Corps : `{ "labelIds": ["…"] }` (une liste vide les retire toutes). Réponse 200 : le tableau des étiquettes du ticket. Erreurs : 404 `label`, 404 `issue`, 400 `label … is not usable on this repository` (l'étiquette doit appartenir au dépôt ou à l'un de ses groupes).

### `POST /api/repositories/{repository_id}/issues/{number}/close`

Ferme le ticket : statut `done`, `closedAt` à l'instant. Rôle minimal : Contributeur. L'auteur est notifié (sauf s'il ferme lui-même) et le webhook `issue_closed` part.

Réponse 200 : le ticket. Erreurs : 400 `issue is already closed`, 404 `issue`.

### `POST /api/repositories/{repository_id}/issues/{number}/reopen`

Rouvre un ticket fermé : statut `todo`, `closedAt` effacé. Rôle minimal : Contributeur. Réponse 200 : le ticket. Erreurs : 400 `issue is not closed`, 404 `issue`.

### `GET /api/repositories/{repository_id}/issues/{number}/comments`

Liste les commentaires du ticket, dans l'ordre. Rôle minimal : Lecteur.

Réponse 200 :

```json
[ { "id": "7e0f…", "authorId": "0b6c…", "author": { "id": "0b6c…", "username": "alice" }, "body": "Je regarde.", "createdAt": "2026-03-01T10:30:00Z" } ]
```

### `POST /api/repositories/{repository_id}/issues/{number}/comments`

Ajoute un commentaire. Rôle minimal : Contributeur (un Lecteur peut lire les commentaires, pas en ajouter). L'auteur et la personne assignée du ticket sont notifiés (sauf vous-même) et le webhook `issue_commented` part.

Corps : `{ "body": "Je regarde." }`. Réponse 200 : le commentaire, même forme que ci-dessus. Erreur : 404 `issue`.

## Étiquettes

Les étiquettes se créent sur un dépôt ou sur un groupe. Une étiquette de groupe est utilisable sur tous les dépôts de ce groupe et de ses sous-groupes. Le nom et la couleur sont libres : aucun format n'est contrôlé.

### `GET /api/repositories/{repository_id}/labels`

Liste les étiquettes propres au dépôt (pas celles de ses groupes), par nom. Rôle minimal : Lecteur. Réponse 200 : un tableau d'étiquettes.

### `POST /api/repositories/{repository_id}/labels`

Crée une étiquette de dépôt. Rôle minimal : Contributeur. Corps : `{ "name": "bug", "color": "#d73a4a" }` (les deux obligatoires). Réponse 200 : l'étiquette.

### `GET /api/groups/{group_id}/labels`

Liste les étiquettes d'un groupe. Rôle minimal : Lecteur du groupe. Réponse 200 : un tableau d'étiquettes. Erreur : 404 `group`.

### `POST /api/groups/{group_id}/labels`

Crée une étiquette de groupe. Rôle minimal : Contributeur du groupe. Corps et réponse : comme pour un dépôt. Erreur : 404 `group`.

### `PATCH /api/labels/{id}`

Modifie le nom et la couleur d'une étiquette. Rôle minimal : Contributeur du dépôt ou du groupe auquel elle appartient. Corps : `{ "name": "…", "color": "…" }` (les deux obligatoires). Réponse 200 : l'étiquette. Erreur : 404 `label`.

### `DELETE /api/labels/{id}`

Supprime l'étiquette, qui disparaît des tickets et demandes de fusion qui la portaient. Même droit que `PATCH`. Réponse 204. Erreur : 404 `label` (étiquette inconnue, ou droit insuffisant).

## Jalons

### `GET /api/repositories/{repository_id}/milestones`

Liste les jalons propres au dépôt. Rôle minimal : Lecteur. Réponse 200 : un tableau de jalons.

### `POST /api/repositories/{repository_id}/milestones`

Crée un jalon de dépôt, à l'état `open`. Rôle minimal : Contributeur.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `title` | texte | oui | Titre. |
| `description` | texte | oui | Description, éventuellement vide. |
| `dueDate` | date | non | Échéance, en RFC 3339 complet (`2026-06-01T00:00:00Z`). Une date seule est refusée avec un 422. |

Réponse 200 : le jalon.

### `GET /api/groups/{group_id}/milestones`

Liste les jalons d'un groupe. Rôle minimal : Lecteur du groupe. Réponse 200 : un tableau de jalons. Erreur : 404 `group`.

### `POST /api/groups/{group_id}/milestones`

Crée un jalon de groupe, utilisable sur les dépôts du groupe et de ses sous-groupes. Rôle minimal : Contributeur du groupe. Corps et réponse : comme pour un dépôt. Erreur : 404 `group`.

### `PATCH /api/milestones/{id}`

Modifie un jalon, y compris pour le fermer. Rôle minimal : Contributeur du dépôt ou du groupe propriétaire.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `title` | texte | oui | Titre. |
| `description` | texte | oui | Description. |
| `dueDate` | date ou `null` | non | Échéance. Un `dueDate` absent ou `null` efface l'échéance. |
| `state` | texte | oui | `open` ou `closed`. |

Réponse 200 : le jalon. Erreurs : 400 `invalid milestone state: …`, 404 `milestone`.

### `DELETE /api/milestones/{id}`

Supprime le jalon. Même droit que `PATCH`. Réponse 204. Erreur : 404 `milestone`.
