# Dépôts et groupes

Cette page décrit les routes des dépôts, des groupes, de la lecture des fichiers, des collaborateurs et des paramètres d'un dépôt. Toutes demandent un JWT (voir [Authentification](/docs/api/authentification)), sauf mention contraire.

Les rôles sont Lecteur, Contributeur et Mainteneur. Un rôle insuffisant répond 404, jamais 403 (voir l'[introduction](/docs/api/introduction)). Les routes de lecture d'un dépôt public sont ouvertes en tant que Lecteur à tout utilisateur connecté.

Beaucoup de routes se désignent par l'identifiant du dépôt (`by-id/{id}`). Pour retrouver cet identifiant depuis un chemin, utilisez `GET /api/resolve/{*path}`.

Le paramètre `{ref}` des routes de fichiers accepte tout ce que Git sait résoudre : un nom de branche, un tag, `HEAD` ou un SHA de commit. Un nom contenant une barre oblique (`feature/login`) doit être encodé en `feature%2Flogin`.

## Réponse « dépôt »

Plusieurs routes renvoient un dépôt sous cette forme :

```json
{
  "id": "3c1f…",
  "name": "blog",
  "description": "Mon blog",
  "owner": "alice",
  "role": "owner",
  "visibility": "private",
  "createdAt": "2026-03-01T10:00:00Z",
  "path": ["alice", "blog"],
  "starCount": 3,
  "isStarred": false,
  "sizeBytes": 204800
}
```

- `role` est votre rôle : `owner` pour le propriétaire d'un dépôt personnel, sinon `reader`, `contributor` ou `maintainer`. Il vaut `null` pour un visiteur anonyme.
- `visibility` vaut `private` ou `public`.
- `path` est le chemin du dépôt, découpé en segments. Utilisez-le pour construire les liens : les dépôts de groupe n'ont pas de chemin `owner/name`.
- `starCount`, `isStarred` et `sizeBytes` sont absents de la liste `GET /api/repositories` et des listes de groupe. `sizeBytes` vaut `null` si la taille n'a pas pu être calculée.

## Dépôts

### `POST /api/repositories`

Crée un dépôt, personnel ou dans un groupe. Tout utilisateur connecté peut créer un dépôt personnel.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `name` | texte | oui | Lettres, chiffres, `-` et `_` uniquement. |
| `visibility` | texte | oui | `private` ou `public`. |
| `description` | texte | non | Vide par défaut. |
| `groupPath` | texte | non | Chemin du groupe qui accueillera le dépôt, par exemple `acme/backend`. Il faut en être Mainteneur. |
| `ciEnabled` | booléen | non | Active la CI. `true` par défaut. |
| `requiredApprovals` | entier | non | Approbations nécessaires pour fusionner. `0` par défaut. |
| `pipelineFilePath` | texte | non | Chemin du fichier de pipeline. `.ferrisgit-ci.yml` par défaut. |

Réponse 200 : le dépôt, avec `role: "owner"`.

Erreurs : 400 nom invalide, 400 `unknown visibility: …`, 400 `groupPath must not be empty`, 404 `group` (groupe inconnu, ou rôle de Mainteneur manquant), 409 `a repository named '…' already exists` (dans le même espace : chez vous, ou dans ce groupe).

```bash
curl -s -X POST "$BASE/api/repositories" -H "Authorization: Bearer $JWT" \
  -H 'Content-Type: application/json' \
  -d '{"name":"blog","visibility":"private","description":"Mon blog"}'
```

### `GET /api/repositories`

Liste les dépôts auxquels vous avez accès : ceux dont vous êtes propriétaire, ceux où vous êtes collaborateur, et ceux des groupes dont vous êtes membre (sous-groupes compris). Les dépôts publics des autres n'y figurent pas : ils se trouvent par la recherche ou le catalogue public.

Paramètre de requête : `starred=true` ne garde que les dépôts que vous avez mis en favori.

Réponse 200 : un tableau de dépôts (sans `starCount`, `isStarred` ni `sizeBytes`). Un dépôt atteint par plusieurs chemins n'apparaît qu'une fois.

### `GET /api/repositories/{owner}/{name}`

Renvoie un dépôt **personnel** par le nom de son propriétaire et le sien. Pour un dépôt de groupe, passez par `GET /api/resolve/{*path}` puis `GET /api/repositories/by-id/{id}`.

Rôle minimal : Lecteur. Réponse 200 : le dépôt, avec `starCount`, `isStarred` et `sizeBytes`. Erreur : 404 `repository`.

### `GET /api/repositories/by-id/{id}`

Renvoie un dépôt par son identifiant, personnel ou de groupe. Rôle minimal : Lecteur. Réponse 200 : le dépôt, avec `starCount`, `isStarred` et `sizeBytes`. Erreur : 404 `repository`.

### `PATCH /api/repositories/by-id/{id}`

Modifie la description et la visibilité d'un dépôt. Seuls les champs présents changent ; un corps `{}` ne change rien et renvoie le dépôt tel qu'il est.

Rôle minimal : Mainteneur (le propriétaire d'un dépôt personnel en a tous les droits). Un Contributeur, un Lecteur ou un utilisateur sans accès reçoit 404.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `description` | texte | non | Nouvelle description. Une chaîne vide efface la description. |
| `visibility` | texte | non | `private` ou `public`. |

Réponse 200 : le dépôt, avec `starCount`, `isStarred` et `sizeBytes`.

Le changement de visibilité est immédiat. Un dépôt rendu privé disparaît du catalogue public et de l'API publique (`/api/public/…`), et le clonage anonyme cesse de fonctionner ; rendu public, il apparaît dans le catalogue (si les pages publiques sont activées).

Erreurs : 400 `unknown visibility: …` (rien n'est modifié), 404 `repository`.

```bash
curl -s -X PATCH "$BASE/api/repositories/by-id/$REPO_ID" -H "Authorization: Bearer $JWT" \
  -H 'Content-Type: application/json' \
  -d '{"description":"Mon blog, version publique","visibility":"public"}'
```

Le nom et le propriétaire d'un dépôt ne se modifient pas : ils font partie de l'adresse de clonage, et aucune route ne les change.

### `DELETE /api/repositories/by-id/{id}`

Supprime le dépôt, avec ses tickets, demandes de fusion, pipelines, releases et fichiers joints, son wiki et son stockage Git. L'action est définitive.

Rôle minimal : Mainteneur. Réponse 204. Erreur : 404 `repository`.

### `POST /api/repositories/by-id/{id}/star`

Met le dépôt en favori. Rôle minimal : Lecteur. Répéter l'appel ne change rien.

Réponse 200 :

```json
{ "starCount": 4, "isStarred": true }
```

### `DELETE /api/repositories/by-id/{id}/star`

Retire le favori. Rôle minimal : Lecteur. Réponse 200 : `{ "starCount": 3, "isStarred": false }`.

## Lire le contenu

### `GET /api/repositories/{owner}/{name}/commits`

Liste les 20 derniers commits d'un dépôt personnel.

Rôle minimal : Lecteur. Paramètre de requête : `ref`, une branche, un tag ou un SHA (`HEAD` par défaut).

Réponse 200 :

```json
[
  {
    "sha": "9fceb02d…",
    "message": "Corrige le lien du menu",
    "authorName": "Alice",
    "authorEmail": "alice@example.com",
    "committedAt": "2026-03-01T10:00:00Z"
  }
]
```

Un dépôt vide donne `[]`. Il n'y a pas de page suivante. Erreur : 404 (`repository`, ou `this ref does not exist`).

### `GET /api/repositories/by-id/{id}/commits`

Même réponse que la route précédente, pour n'importe quel dépôt désigné par son identifiant. Rôle minimal : Lecteur. Paramètre de requête : `ref`.

### `GET /api/repositories/by-id/{id}/tree/{ref}`

Liste le contenu de la racine du dépôt pour une référence. Rôle minimal : Lecteur.

Paramètre de requête : `lastCommit` (`true` par défaut). Avec `lastCommit=false`, la réponse arrive plus vite parce que le dernier commit de chaque entrée n'est pas cherché.

Réponse 200 :

```json
[
  { "name": "src", "isDir": true, "lastCommit": { "sha": "9fceb02d…", "message": "…", "authorName": "Alice", "authorEmail": "alice@example.com", "committedAt": "2026-03-01T10:00:00Z" } },
  { "name": "README.md", "isDir": false, "lastCommit": null }
]
```

`lastCommit` est `null` si le dernier commit de l'entrée est plus ancien que les 200 derniers commits examinés, ou si `lastCommit=false`. Erreur : 404 `this ref or path does not exist`.

### `GET /api/repositories/by-id/{id}/tree/{ref}/{*path}`

Liste le contenu d'un sous-dossier. Mêmes droits, paramètre et réponse que la route précédente. `path` est le chemin du dossier (`src/components`). Erreur : 404 `this ref or path does not exist`.

### `GET /api/repositories/by-id/{id}/blob/{ref}/{*path}`

Lit un fichier. Rôle minimal : Lecteur.

Réponse 200 :

```json
{ "sha": "9fceb02d…", "size": 1203, "isBinary": false, "content": "# Titre\n…" }
```

`sha` est le commit résolu pour `ref`. `content` est `null` pour un fichier binaire (un octet nul dans les 8000 premiers) ou de plus de 1 Mio, mais `size` est toujours donnée. Erreur : 404 `this ref or path does not exist`.

### `GET /api/repositories/by-id/{id}/readme/{ref}`

Renvoie le `README.md` de la racine (la casse du nom est libre). Rôle minimal : Lecteur.

Réponse 200 : `{ "content": "# Titre\n…" }`. `content` vaut `null` s'il n'y a pas de README ou si la référence n'existe pas.

### `GET /api/repositories/by-id/{id}/contributors/{ref}`

Liste les 20 contributeurs les plus actifs d'une référence, par nombre de commits décroissant, d'après les 1000 derniers commits. Rôle minimal : Lecteur.

Réponse 200 : `[ { "name": "Alice", "email": "alice@example.com", "commitCount": 42 } ]`. Une référence inconnue donne `[]`.

### `GET /api/repositories/by-id/{id}/languages/{ref}`

Répartition des langages d'une référence. Rôle minimal : Lecteur.

Réponse 200 :

```json
{ "languages": [ { "name": "Rust", "bytes": 120400, "percentage": 71.3 } ] }
```

La liste est vide pour une référence inconnue, et aussi quand le dépôt compte plus de 2000 fichiers (aucune barre partielle n'est fournie).

### `GET /api/repositories/{repository_id}/branches`

Liste les branches locales. Rôle minimal : Lecteur.

Réponse 200 : `[ { "name": "main", "tipSha": "9fceb02d…", "isDefault": true } ]`.

## Paramètres du dépôt

### `GET /api/repositories/{repository_id}/settings`

Renvoie les paramètres de CI et de fusion. Rôle minimal : Mainteneur.

Réponse 200 :

```json
{ "pipelineFilePath": ".ferrisgit-ci.yml", "ciEnabled": true, "requiredApprovals": 0 }
```

### `PUT /api/repositories/{repository_id}/settings`

Modifie les paramètres. Rôle minimal : Mainteneur. Tous les champs sont facultatifs : un champ absent garde sa valeur.

Corps :

| Champ | Type | Description |
|---|---|---|
| `pipelineFilePath` | texte | Chemin du fichier de pipeline. |
| `ciEnabled` | booléen | Active ou coupe la CI. |
| `requiredApprovals` | entier | Approbations nécessaires avant fusion. Pas de valeur négative. |

Réponse 200 : les paramètres, comme `GET`. Erreur : 400 `required_approvals must not be negative`.

## Collaborateurs

Les collaborateurs sont des accès directs à un dépôt. Les rôles possibles sont `reader`, `contributor` et `maintainer`.

> **Note** : ajouter, changer ou retirer un collaborateur demande le rôle Mainteneur, rôle hérité d'un groupe compris, ou d'être le propriétaire d'un dépôt personnel. Sinon la route répond 404 `repository`.

### `GET /api/repositories/{repository_id}/collaborators`

Liste les collaborateurs directs. Il faut un rôle explicite : l'ouverture d'un dépôt public aux Lecteurs ne suffit pas.

Réponse 200 : `[ { "userId": "…", "username": "bob", "role": "contributor", "createdAt": "2026-03-01T10:00:00Z" } ]`.

### `POST /api/repositories/{repository_id}/collaborators`

Ajoute un collaborateur. Le collaborateur reçoit une notification et un webhook `collaborator_added` est émis.

Corps : `{ "username": "bob", "role": "contributor" }` (les deux obligatoires). Réponse 204.

Erreurs : 400 `invalid role: …`, 400 `no such user`, 400 `the owner is already a collaborator`, 404 `repository`, 409 si l'utilisateur est déjà collaborateur.

```bash
curl -s -X POST "$BASE/api/repositories/$REPO_ID/collaborators" \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d '{"username":"bob","role":"contributor"}'
```

### `PATCH /api/repositories/{repository_id}/collaborators/{username}`

Change le rôle d'un collaborateur. Corps : `{ "role": "maintainer" }`. Réponse 204.

Erreurs : 400 `invalid role: …`, 400 `no such user`, 400 `the owner is already a collaborator`, 404 `repository`, 404 `collaborator` (cet utilisateur n'est pas collaborateur). Une notification et un webhook `collaborator_role_changed` partent si le rôle change.

### `DELETE /api/repositories/{repository_id}/collaborators/{username}`

Retire un collaborateur. Réponse 204, y compris si l'utilisateur n'était pas collaborateur. Erreurs : 400 `no such user`, 404 `repository`. Un collaborateur retiré reçoit une notification et un webhook `collaborator_removed` est émis.

## Résolution de chemin

### `GET /api/resolve/{*path}`

Transforme un chemin (`alice/blog`, `acme/backend/api`, `acme/backend`) en identifiant. Rôle minimal : Lecteur sur la cible. Voir [Chemins de dépôt](/docs/api/introduction) pour la règle de résolution.

Réponse 200, selon le type de la cible :

```json
{ "type": "personalRepository", "repositoryId": "3c1f…" }
```

```json
{ "type": "groupRepository", "repositoryId": "3c1f…", "chain": [ { "id": "7d2a…", "name": "acme" }, { "id": "81b0…", "name": "backend" } ] }
```

```json
{ "type": "group", "groupId": "81b0…", "chain": [ { "id": "7d2a…", "name": "acme" }, { "id": "81b0…", "name": "backend" } ], "role": "maintainer" }
```

`chain` liste les groupes depuis la racine. Pour un groupe, `role` est votre rôle effectif, héritage compris.

Erreur : 404 `path` pour un chemin vide, inconnu, ou que vous n'avez pas le droit de voir (la réponse est la même).

## Groupes

Un groupe rassemble des dépôts et des sous-groupes, et distribue des rôles à ses membres. Les droits d'un groupe s'appliquent aussi à tous ses descendants.

Le corps d'un groupe renvoyé par l'API a cette forme :

```json
{ "id": "81b0…", "parentGroupId": "7d2a…", "name": "backend", "description": "", "createdAt": "2026-03-01T10:00:00Z" }
```

`parentGroupId` vaut `null` pour un groupe racine.

### `POST /api/groups`

Crée un groupe racine. Tout utilisateur connecté peut le faire ; il en devient Mainteneur.

Corps : `{ "name": "acme", "description": "Notre équipe" }` (`name` obligatoire, `description` facultative). Réponse 200 : le groupe.

Erreurs : 400 nom invalide (lettres, chiffres, `-` et `_`), 409 `'acme' is already taken by a user account`, 409 `a group named 'acme' already exists here`.

### `POST /api/groups/{id}/subgroups`

Crée un sous-groupe. Rôle minimal : Mainteneur du groupe parent (ou d'un de ses ancêtres). Corps et réponse : comme `POST /api/groups`. Erreurs : 400 nom invalide, 404 `group`, 409 `a group named '…' already exists here`.

### `GET /api/groups/writable`

Liste les groupes où vous pouvez créer des dépôts et des sous-groupes : ceux où vous êtes Mainteneur direct, avec tous leurs descendants.

Réponse 200 : `[ { "id": "81b0…", "path": "acme/backend" } ]`.

### `GET /api/groups/member`

Liste les groupes dont vous êtes membre, quel que soit le rôle, avec leurs descendants.

Réponse 200 : `[ { "id": "81b0…", "path": "acme/backend", "role": "maintainer" } ]`. `role` est le rôle effectif, héritage compris.

### `DELETE /api/groups/{id}`

Supprime un groupe vide. Rôle minimal : Mainteneur. Réponse 204. Erreurs : 404 `group`, 409 `this group still has at least one subgroup — remove it first`, 409 `this group still has at least one repository — remove it first`.

### `GET /api/groups/{id}/children`

Liste les sous-groupes directs. Rôle minimal : Lecteur. Réponse 200 : un tableau de groupes. Erreur : 404 `group`.

### `GET /api/groups/{id}/repositories`

Liste les dépôts directs du groupe. Rôle minimal : Lecteur.

Réponse 200 : un tableau de dépôts. `owner` est le créateur du dépôt et `role` votre rôle dans le groupe. Erreur : 404 `group`.

### `GET /api/groups/{id}/members`

Liste les membres directs du groupe (pas ceux des groupes parents). Rôle minimal : Lecteur.

Réponse 200 : `[ { "userId": "…", "username": "bob", "role": "reader", "createdAt": "2026-03-01T10:00:00Z" } ]`. Erreur : 404 `group`.

### `POST /api/groups/{id}/members`

Ajoute un membre. Rôle minimal : Mainteneur.

Corps : `{ "username": "bob", "role": "reader" }`, avec `role` parmi `reader`, `contributor`, `maintainer`. Réponse 200, corps vide. Erreurs : 400 `invalid role: …`, 400 `no such user`, 404 `group`.

### `PATCH /api/groups/{id}/members/{username}`

Change le rôle d'un membre. Rôle minimal : Mainteneur. Corps : `{ "role": "contributor" }`. Réponse 200, corps vide.

Erreurs : 400 `invalid role: …`, 400 `no such user`, 400 `cannot remove the last maintainer of this group hierarchy` (rétrograder le dernier Mainteneur de la hiérarchie est refusé), 404 `group`.

### `DELETE /api/groups/{id}/members/{username}`

Retire un membre. Rôle minimal : Mainteneur. Réponse 200, corps vide. Erreurs : 400 `no such user`, 400 `cannot remove the last maintainer of this group hierarchy`, 404 `group`.
