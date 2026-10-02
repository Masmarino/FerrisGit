# Releases et wikis

Cette page décrit les routes des tags, des releases avec leurs fichiers joints, et des wikis. Toutes demandent un JWT (voir [Authentification](/docs/api/authentification)). Un rôle insuffisant répond 404, jamais 403. Pour l'usage dans l'interface, voir [Releases et tags](/docs/utilisation/releases) et [Wikis](/docs/utilisation/wikis). Les versions publiées d'un dépôt public se lisent aussi sans compte, par [l'API publique](/docs/api/api-publique).

## Tags

### `GET /api/repositories/{repository_id}/tags`

Liste les tags du dépôt. Rôle minimal : Lecteur.

Réponse 200 : `[ { "name": "v1.0.0", "targetSha": "9fceb02d…" } ]`.

### `DELETE /api/repositories/{repository_id}/tags/{tag_name}`

Supprime un tag qui n'a plus de release, par exemple après la suppression d'un brouillon créé sur le mauvais commit. Rôle minimal : Mainteneur.

Réponse 204. Erreurs : 404 `tag '<nom>'` (le tag n'existe pas), 409 `tag '<nom>' is still used by a release` (supprimez d'abord la release).

## Releases

Une release est attachée à un tag. Elle peut être un brouillon (`draft`), visible seulement des Mainteneurs, ou publiée. Un brouillon inconnu d'un non-Mainteneur répond comme une release absente.

### Résumé d'une release

```json
{
  "id": "d4e5…",
  "tagName": "v1.0.0",
  "title": "Première version",
  "draft": false,
  "prerelease": false,
  "authorId": "0b6c…",
  "author": { "id": "0b6c…", "username": "alice" },
  "notesExcerpt": "Les 240 premiers caractères des notes…",
  "assetCount": 2,
  "createdAt": "2026-03-01T10:00:00Z",
  "publishedAt": "2026-03-01T10:00:00Z"
}
```

`authorId` et `author` valent `null` si le compte a été supprimé. `publishedAt` est `null` pour un brouillon.

### Détail d'une release

Le détail reprend `id`, `tagName`, `title`, `draft`, `prerelease`, `authorId`, `author`, `createdAt` et `publishedAt`, remplace `notesExcerpt` et `assetCount` par les notes complètes `notes`, et ajoute :

- `targetCommitSha` : le commit visé par le tag, lu dans Git à l'instant de la requête (`null` si le tag a été supprimé depuis) ;
- `assets` : un tableau de fichiers joints `{ "id", "filename", "contentType", "sizeBytes", "uploadedBy", "uploader", "createdAt" }`. `uploadedBy` et `uploader` valent `null` si le compte a été supprimé.

### `GET /api/repositories/{repository_id}/releases`

Liste les releases, de la plus récente à la plus ancienne (selon `publishedAt`, ou `createdAt` pour un brouillon). Rôle minimal : Lecteur. Les brouillons n'y figurent que pour un Mainteneur.

Réponse 200 : un tableau de résumés.

### `POST /api/repositories/{repository_id}/releases`

Crée une release, et le tag correspondant s'il n'existe pas. Rôle minimal : Mainteneur.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `tagName` | texte | oui | Nom du tag. Pas de `/`, pas de `-` au début, pas de `..`, pas de `@{`, pas de caractère de contrôle, pas de suffixe `.lock`. |
| `targetCommitSha` | texte | oui | Commit visé : 7 à 40 caractères hexadécimaux en minuscules. |
| `title` | texte | oui | Titre. |
| `notes` | texte | non | Notes de version. Vide par défaut. |
| `draft` | booléen | non | Crée un brouillon. `false` par défaut. |
| `prerelease` | booléen | non | Marque une pré-version. `false` par défaut. |

Si le tag existe déjà et pointe sur ce commit, il est réutilisé. S'il existe ailleurs, la création est refusée : FerrisGit ne déplace jamais un tag. Pour réutiliser un tag existant, donnez son SHA complet.

Réponse 200 : le résumé de la release.

Erreurs : 400 nom de tag invalide (message précis), 400 `'<valeur>' is not a plausible commit sha`, 400 `tag '<nom>' already exists and points at a different commit (…) than requested (…)`, 409 `a release for tag '<nom>' already exists`, 409 `a tag named '<nom>' already exists: …`.

```bash
curl -s -X POST "$BASE/api/repositories/$REPO_ID/releases" \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d '{"tagName":"v1.0.0","targetCommitSha":"9fceb02d","title":"Première version","notes":"Premier jet."}'
```

### `GET /api/repositories/{repository_id}/releases/{tag_name}`

Renvoie le détail d'une release par le nom de son tag. Rôle minimal : Lecteur ; un brouillon exige Mainteneur. Réponse 200 : le détail. Erreur : 404 `release`.

### `PATCH /api/repositories/{repository_id}/releases/{tag_name}`

Modifie une release. Rôle minimal : Mainteneur. Tous les champs sont facultatifs.

Corps :

| Champ | Type | Description |
|---|---|---|
| `title` | texte | Nouveau titre. |
| `notes` | texte | Nouvelles notes. |
| `prerelease` | booléen | Marque ou non comme pré-version. |
| `draft` | booléen | Seul `false` sur un brouillon agit : il publie la release (et fixe `publishedAt`, une seule fois). On ne peut pas repasser une release publiée en brouillon. |

Réponse 200 : le détail de la release. Erreur : 404 `release`.

### `DELETE /api/repositories/{repository_id}/releases/{tag_name}`

Supprime la release et ses fichiers joints. Le tag reste dans le dépôt. Rôle minimal : Mainteneur. Réponse 204. Erreur : 404 `release`.

### `POST /api/repositories/{repository_id}/releases/{tag_name}/assets`

Ajoute un fichier joint, par envoi `multipart/form-data`. Rôle minimal : Mainteneur. L'envoi est limité à 100 Mio.

Le formulaire doit contenir un champ nommé `file`, avec un nom de fichier. Le type de contenu du champ est conservé (`application/octet-stream` à défaut).

Réponse 200 : le fichier joint `{ "id", "filename", "contentType", "sizeBytes", "uploadedBy", "uploader", "createdAt" }`.

Erreurs : 400 `multipart upload is missing a 'file' field with a filename`, 400 `multipart upload is missing a 'file' field`, 404 `release`.

```bash
curl -s -X POST "$BASE/api/repositories/$REPO_ID/releases/v1.0.0/assets" \
  -H "Authorization: Bearer $JWT" -F 'file=@blog-1.0.0.tar.gz'
```

### `GET /api/repositories/{repository_id}/releases/{tag_name}/assets/{asset_id}`

Télécharge un fichier joint. Rôle minimal : Lecteur ; un brouillon exige Mainteneur.

Réponse 200 : le contenu du fichier en flux, avec le `Content-Type` d'origine, `Content-Disposition: attachment; filename="…"` et `X-Content-Type-Options: nosniff`. Erreurs : 404 `release`, 404 `release asset`.

### `DELETE /api/repositories/{repository_id}/releases/{tag_name}/assets/{asset_id}`

Supprime un fichier joint. Rôle minimal : Mainteneur. Réponse 204. Erreurs : 404 `release`, 404 `release asset`.

## Wikis

Chaque dépôt peut avoir un wiki, stocké dans son propre dépôt Git. Une page est identifiée par un **slug** : 1 à 100 caractères, lettres, chiffres, `-` et `_`, sans commencer par `-`. Le titre affiché est le slug où les `-` sont remplacés par des espaces (`Getting-Started` devient `Getting Started`).

Chaque enregistrement ou suppression crée un commit dans le dépôt du wiki. Pour éviter d'écraser le travail d'un autre, ces écritures utilisent un contrôle de version : vous envoyez le SHA du dernier état que vous avez lu (`baseSha`), et l'écriture échoue en 409 si le wiki a changé depuis, sur n'importe quelle page ou par un `git push`.

### `GET /api/repositories/{repository_id}/wiki`

Liste les pages du wiki. Rôle minimal : Lecteur.

Réponse 200 :

```json
{ "headSha": "1a2b3c4d…", "pages": [ { "slug": "Home", "title": "Home" } ] }
```

Un dépôt sans wiki donne `{ "headSha": null, "pages": [] }` (et non une erreur). `headSha` est la valeur à renvoyer comme `baseSha` pour écrire.

### `GET /api/repositories/{repository_id}/wiki/pages/{slug}`

Lit une page. Rôle minimal : Lecteur.

Réponse 200 : `{ "content": "# Accueil\n…", "headSha": "1a2b3c4d…", "title": "Home" }`. Erreur : 404 `wiki page`.

### `PUT /api/repositories/{repository_id}/wiki/pages/{slug}`

Crée ou remplace une page. Rôle minimal : Contributeur. Le wiki est créé au premier enregistrement. Le commit est signé du nom et de l'adresse e-mail du compte.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `content` | texte | oui | Contenu de la page, en Markdown. |
| `baseSha` | texte | oui, sauf pour la toute première page | `headSha` que vous avez lu : 7 à 40 caractères hexadécimaux en minuscules. Peut être omis ou `null` uniquement pour un wiki sans commit. |
| `message` | texte | non | Message du commit. `Update <slug>` par défaut. |

Réponse 200 : `{ "content": "…", "headSha": "<SHA du nouveau commit>", "title": "Home" }`. Gardez le nouveau `headSha` pour l'écriture suivante.

Erreurs : 400 `'<slug>' is not a valid wiki page name`, 400 `'<valeur>' is not a plausible commit sha`, 409 `base revision not found`, et 409 quand le wiki a changé depuis votre lecture.

### `DELETE /api/repositories/{repository_id}/wiki/pages/{slug}`

Supprime une page. Rôle minimal : Mainteneur. Le commit s'appelle `Delete <slug>`.

Paramètre de requête obligatoire : `baseSha`, le `headSha` que vous avez lu (`?baseSha=1a2b3c4d…`). C'est un paramètre de requête, car aucune route `DELETE` de l'API ne prend de corps.

Réponse 204. Erreurs : 400 `'<slug>' is not a valid wiki page name`, 400 `'<valeur>' is not a plausible commit sha`, 404 `wiki` (aucun wiki), 404 `wiki page '<slug>'` (la page n'existe pas à cette version), 409 `base revision not found` ou wiki modifié depuis.

### `GET /api/repositories/{repository_id}/wiki/pages/{slug}/revisions`

Liste l'historique d'une page : les commits qui ont changé son contenu, du plus récent au plus ancien. Rôle minimal : Lecteur.

Réponse 200 :

```json
[ { "commitSha": "1a2b3c4d…", "authorName": "Alice", "authorEmail": "alice@example.com", "committedAt": "2026-03-01T10:00:00Z", "message": "Update Home" } ]
```

Erreur : 404 `wiki page` (le dépôt n'a pas de wiki).

### `GET /api/repositories/{repository_id}/wiki/pages/{slug}/revisions/{commit_sha}`

Lit le contenu d'une page à une version donnée. Rôle minimal : Lecteur. Réponse 200 : `{ "content": "…" }`. Erreurs : 404 `wiki page` (aucun wiki), 404 `wiki page revision` (la page n'existait pas à ce commit, ou le commit n'existe pas).
