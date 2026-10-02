# API publique

Les routes `/api/public/*` donnent un accès en lecture seule aux dépôts publics, sans compte et sans en-tête `Authorization`. L'interface les utilise pour les pages que voient les visiteurs (voir [Pages publiques](/docs/utilisation/pages-publiques)).

## Règles communes

- **Seuls les dépôts publics sont lisibles.** Les groupes ne sont jamais publics : seul un dépôt public se résout.
- **Un refus ne dit pas pourquoi.** Un dépôt privé, un dépôt inconnu et un dépôt public dont l'administrateur a désactivé les pages publiques répondent exactement la même chose : `404` avec `{ "error": "repository" }` (ou `{ "error": "path" }` pour la résolution de chemin). La réponse ne dépend pas non plus du temps de calcul, pour ne pas servir à deviner.
- **Les pages publiques peuvent être coupées.** Quand l'administrateur les désactive (réglage `publicPagesEnabled`, voir [Réglages de l'instance](/docs/administration/reglages)), toutes ces routes répondent 404. `GET /api/auth/config` le dit d'avance.
- **Limite par adresse IP : 120 requêtes par minute** pour l'ensemble des routes `/api/public/*`. Au-delà, la réponse est `429` avec `{ "error": "too many requests, try again later" }` et un en-tête `Retry-After` en secondes. Voir [Limitation de débit](/docs/api/introduction).
- **Pas de cache.** Toutes les réponses portent `Cache-Control: no-cache`, y compris les 429.
- Les corps de réponse sont les mêmes que ceux des routes authentifiées équivalentes, décrites dans [Dépôts et groupes](/docs/api/depots) et [Releases et wikis](/docs/api/releases-et-wikis). Le champ `role` d'un dépôt vaut `null`, et `isStarred` vaut `false`.
- Le paramètre `{ref}` est une branche, un tag, `HEAD` ou un SHA. Encodez les barres obliques en `%2F`.

## Catalogue et résolution

### `GET /api/public/repositories`

Liste les dépôts publics, avec recherche, tri et pagination.

Paramètres de requête (tous facultatifs) :

| Paramètre | Description |
|---|---|
| `q` | Texte cherché (insensible à la casse) dans le nom, le chemin et la description. 100 caractères au plus. |
| `sort` | `stars` (défaut, les plus étoilés d'abord), `name` ou `created` (les plus récents d'abord). |
| `page` | Numéro de page, de 1 à 100. 1 par défaut. |
| `perPage` | Taille de page, de 1 à 50. 20 par défaut. |

Réponse 200 :

```json
{
  "items": [
    {
      "id": "3c1f…",
      "name": "blog",
      "path": ["alice", "blog"],
      "owner": "alice",
      "description": "Mon blog",
      "stars": 3,
      "createdAt": "2026-03-01T10:00:00Z"
    }
  ],
  "total": 1,
  "page": 1,
  "perPage": 20
}
```

`total` est le nombre de dépôts correspondants, toutes pages confondues.

Erreurs : 400 `q must not exceed 100 characters`, 400 `unknown sort: … (expected stars, name or created)`, 400 `page must be a number between 1 and 100`, 400 `perPage must be a number between 1 and 50`, 404 (pages publiques désactivées).

```bash
curl -s "$BASE/api/public/repositories?q=blog&sort=name&perPage=10"
```

### `GET /api/public/resolve/{*path}`

Transforme le chemin d'un dépôt public (`alice/blog`, `acme/backend/api`) en identifiant.

Réponse 200 : `{ "type": "personalRepository", "repositoryId": "3c1f…" }` ou `{ "type": "groupRepository", "repositoryId": "3c1f…", "chain": [ { "id": "…", "name": "acme" }, { "id": "…", "name": "backend" } ] }`. Même règle de résolution que [`GET /api/resolve/{*path}`](/docs/api/depots).

Erreur : 404 `path` pour un chemin inconnu, privé, qui désigne un groupe, ou quand les pages publiques sont désactivées.

## Dépôt et fichiers

### `GET /api/public/repositories/by-id/{id}`

Renvoie le dépôt public (même forme que [`GET /api/repositories/by-id/{id}`](/docs/api/depots), avec `starCount` et `sizeBytes`). Erreur : 404 `repository`.

### `GET /api/public/repositories/by-id/{id}/tree/{ref}`

Liste la racine du dépôt pour une référence. Paramètre de requête : `lastCommit` (`true` par défaut), comme pour la route authentifiée. Réponse 200 : un tableau d'entrées `{ "name", "isDir", "lastCommit" }`. Erreurs : 404 `repository`, 404 `this ref or path does not exist`.

### `GET /api/public/repositories/by-id/{id}/tree/{ref}/{*path}`

Liste un sous-dossier. Même réponse que la route précédente. `path` est le chemin du dossier.

### `GET /api/public/repositories/by-id/{id}/blob/{ref}/{*path}`

Lit un fichier. Réponse 200 : `{ "sha", "size", "isBinary", "content" }`, où `sha` est le commit résolu et `content` vaut `null` pour un fichier binaire ou de plus de 1 Mio.

### `GET /api/public/repositories/by-id/{id}/readme/{ref}`

Renvoie le `README.md` de la racine. Réponse 200 : `{ "content": "…" }` (`null` s'il n'y en a pas).

### `GET /api/public/repositories/by-id/{id}/contributors/{ref}`

Liste les 20 contributeurs les plus actifs d'après les 1000 derniers commits, par nombre de commits décroissant. Réponse 200 : `[ { "name", "email", "commitCount" } ]`.

### `GET /api/public/repositories/by-id/{id}/languages/{ref}`

Répartition des langages. Réponse 200 : `{ "languages": [ { "name", "bytes", "percentage" } ] }`.

### `GET /api/public/repositories/by-id/{id}/commits`

Les 20 derniers commits. Paramètre de requête : `ref` (`HEAD` par défaut). Réponse 200 : `[ { "sha", "message", "authorName", "authorEmail", "committedAt" } ]`. Erreurs : 404 `repository`, 404 `this ref does not exist`.

### `GET /api/public/repositories/{id}/branches`

Liste les branches. Réponse 200 : `[ { "name", "tipSha", "isDefault" } ]`. Notez que cette route et les trois suivantes n'ont pas de segment `by-id`.

### `GET /api/public/repositories/{id}/tags`

Liste les tags. Réponse 200 : `[ { "name", "targetSha" } ]`.

## Releases

Seules les releases publiées sont visibles. Un brouillon répond comme une release inexistante.

### `GET /api/public/repositories/{id}/releases`

Liste les releases publiées, de la plus récente à la plus ancienne. Réponse 200 : un tableau de résumés (`id`, `tagName`, `title`, `draft`, `prerelease`, `authorId`, `author`, `notesExcerpt`, `assetCount`, `createdAt`, `publishedAt`).

### `GET /api/public/repositories/{id}/releases/{tag_name}`

Renvoie le détail d'une release publiée, avec ses notes complètes, `targetCommitSha` et la liste de ses fichiers joints. Erreur : 404 `release`.

### `GET /api/public/repositories/{id}/releases/{tag_name}/assets/{asset_id}`

Télécharge un fichier joint d'une release publiée. Réponse 200 : le contenu en flux, avec `Content-Disposition: attachment`, le `Content-Type` d'origine et `X-Content-Type-Options: nosniff`. Erreurs : 404 `release`, 404 `release asset`.

```bash
curl -sOJ "$BASE/api/public/repositories/$REPO_ID/releases/v1.0.0/assets/$ASSET_ID"
```

## robots.txt

### `GET /robots.txt`

Indique aux moteurs de recherche ce qu'ils peuvent explorer. Route anonyme, à la racine du serveur, hors de `/api` et hors de la limite de 120 requêtes par minute. La réponse est du texte brut (`text/plain; charset=utf-8`) :

- Si les pages publiques et l'indexation sont toutes deux activées (`publicPagesEnabled` et `seoIndexingEnabled`), les robots peuvent tout explorer sauf `/api/`, `/account` et `/admin/` :

  ```text
  User-agent: *
  Disallow: /api/
  Disallow: /account
  Disallow: /admin/
  ```

- Sinon, tout est interdit :

  ```text
  User-agent: *
  Disallow: /
  ```

Dans ce second cas, ou si le réglage ne peut pas être lu, le serveur ajoute aussi `X-Robots-Tag: noindex, nofollow` à toutes ses réponses, pages et API comprises. L'indexation est désactivée par défaut.
