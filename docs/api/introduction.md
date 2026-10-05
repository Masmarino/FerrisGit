# Introduction

L'API REST de FerrisGit est celle que l'interface web utilise elle-même. Tout ce que vous faites dans l'interface passe par elle, et vous pouvez l'appeler depuis un script ou un outil externe.

Ces pages décrivent l'état actuel du code. Les chemins ne portent pas de numéro de version (ils commencent tous par `/api`) : si une route change, la page change avec elle. En cas de doute entre une page et le comportement observé, c'est le comportement de votre version de FerrisGit qui fait foi.

> **Attention** : l'API s'utilise avec la session JWT obtenue à la connexion, pas avec un jeton Git. Comme la double authentification est obligatoire, un script doit refaire le parcours de connexion complet. Voir [Authentification](/docs/api/authentification).

## Les pages de la référence

| Page | Contenu |
|---|---|
| [Authentification](/docs/api/authentification) | Connexion, double authentification, compte, jetons Git |
| [Dépôts et groupes](/docs/api/depots) | Dépôts, groupes, fichiers, collaborateurs, paramètres |
| [Demandes de fusion](/docs/api/merge-requests) | Demandes de fusion, commentaires, relectures, fusion |
| [Tickets, étiquettes et jalons](/docs/api/tickets) | Tickets, kanban, étiquettes, jalons |
| [Pipelines et runners](/docs/api/ci-cd) | Pipelines, variables CI, runners et API des runners |
| [Releases et wikis](/docs/api/releases-et-wikis) | Releases, fichiers joints, tags, wikis |
| [Webhooks et notifications](/docs/api/webhooks-et-notifications) | Webhooks, notifications, tableau de bord, recherche |
| [Administration](/docs/api/administration) | Routes réservées aux administrateurs |
| [API publique](/docs/api/api-publique) | Routes anonymes des dépôts publics |

## Principes

- La base de toutes les routes est `/api`, sur l'adresse de votre instance : `https://git.example.com/api`.
- Les corps de requête et de réponse sont du JSON en UTF-8. Envoyez `Content-Type: application/json` avec un corps.
- Les noms de champs sont en `camelCase` (`createdAt`, `repositoryId`). Les valeurs d'énumération gardent leur forme propre : `in_progress`, `changes_requested`, `docker-runners`. Une seule route de la référence attend un champ en `snake_case` : `registration_token` dans [`POST /api/runner/register`](/docs/api/ci-cd).
- Les dates sont des horodatages RFC 3339 en UTC, par exemple `2026-03-01T00:00:00Z`. En entrée, seul ce format complet est accepté : une date seule comme `2026-03-01` est refusée avec un code 422.
- Les identifiants sont des UUID au format texte. Les tickets ont en plus un numéro (`number`), propre à leur dépôt.
- Certains champs optionnels sont absents de la réponse quand ils n'ont pas de valeur, d'autres valent `null`. Chaque route indique ce qui s'applique.
- Sur un `PATCH` ou un `PUT` qui accepte des champs facultatifs, un champ absent laisse la valeur actuelle. Quand une route accepte `null` pour effacer une valeur, la page le précise.

## Authentification

Toute route de la référence qui n'est pas marquée « anonyme » attend l'en-tête :

```http
Authorization: Bearer <JWT>
```

Le JWT s'obtient en deux étapes (mot de passe, puis second facteur), décrites dans [Authentification](/docs/api/authentification). Il est valable 12 heures par défaut. Un administrateur peut changer cette durée entre 1 et 720 heures.

Voici les réponses 401 que vous pouvez rencontrer sur une route authentifiée :

| Message (`error`) | Cause |
|---|---|
| `missing Authorization header` | L'en-tête est absent. |
| `expected Bearer token` | L'en-tête n'utilise pas le schéma `Bearer`. |
| `invalid or expired token` | Le jeton est mal formé, expiré, ou ce n'est pas un JWT de session. |
| `token has been revoked` | Le mot de passe a changé, un facteur a été supprimé ou l'administrateur a réinitialisé le compte depuis l'émission du jeton. |
| `user no longer exists` | Le compte a été supprimé. |

Deux autres types de jetons existent, mais ils ne servent pas sur l'API :

- Les jetons Git (`fg_…`) ne servent que pour Git en HTTP. Sur `/api`, ils sont refusés comme n'importe quelle valeur qui n'est pas un JWT.
- Les jetons de runner (`fgr_…`) ne servent que sur les routes `/api/runner/jobs/*`, et pour cloner en lecture. Voir [Pipelines et runners](/docs/api/ci-cd).

## Droits d'accès

Les routes d'un dépôt ou d'un groupe vérifient votre rôle : Lecteur, Contributeur ou Mainteneur. Chaque route indique le rôle minimal. Voir [Rôles et permissions](/docs/utilisation/roles-et-permissions) pour la règle de calcul, héritage de groupe compris.

Quelques règles valent pour toute l'API :

- **Il n'y a jamais de code 403.** Si vous n'avez pas le rôle nécessaire sur un dépôt ou un groupe, la route répond 404, comme si la ressource n'existait pas. Un dépôt privé ne révèle donc pas son existence.
- Un dépôt public donne le rôle Lecteur à tout utilisateur connecté sur les routes de lecture. La liste des collaborateurs fait exception : elle exige un rôle explicite.
- Une route réservée aux administrateurs répond 401 avec `admin access required` à un utilisateur qui ne l'est pas.

## Erreurs

Une erreur renvoie un code HTTP et un corps JSON à un seul champ :

```json
{ "error": "repository" }
```

Le message est écrit en anglais, pour un humain. Appuyez-vous sur le code HTTP, pas sur le texte, qui peut changer d'une version à l'autre.

| Code | Sens |
|---|---|
| 400 | Valeur refusée par une règle : nom invalide, rôle inconnu, état qui ne permet pas l'action. Le message dit laquelle. |
| 401 | Jeton absent ou invalide, identifiants incorrects, ou droits d'administrateur manquants. |
| 404 | La ressource n'existe pas, ou vous n'avez pas le droit de la voir. |
| 409 | Conflit : nom déjà pris, dernier Mainteneur ou dernier administrateur, groupe non vide. |
| 413 | Corps trop gros. |
| 422 | Le corps JSON ne correspond pas au format attendu (champ obligatoire absent, mauvais type, date invalide). |
| 429 | Trop de requêtes. Voir plus bas. |
| 500 | Erreur interne. Le corps vaut toujours `{ "error": "internal error" }`, le détail reste dans les journaux du serveur. |
| 503 | Fonction désactivée par la configuration du serveur, par exemple les passkeys. |

Une erreur de format détectée avant la route (corps JSON invalide, champ absent) vient du framework web : le corps de la réponse est alors du texte, pas le JSON `{ "error": … }`.

Les limites de taille des corps sont les suivantes : 2 Mio par défaut, 16 Kio sur les routes anonymes de connexion et de double authentification, 100 Mio pour l'envoi d'un fichier joint à une release. Le `git push` suit une autre limite, réglée par l'administrateur (voir [Réglages de l'instance](/docs/administration/reglages)).

## Limitation de débit

Plusieurs routes comptent les requêtes par adresse IP, ou par compte, sur une fenêtre fixe. Passé le plafond, elles répondent 429 avec `{ "error": "…" }`.

| Routes | Plafond | Clé |
|---|---|---|
| `POST /api/auth/login` | 10 par minute | adresse IP |
| `POST /api/auth/register` | 10 par 5 minutes | adresse IP |
| `POST /api/auth/activate` et `POST /api/auth/reset-password` | 10 par 5 minutes, budget partagé | adresse IP |
| `POST /api/auth/mfa/passkey/start` et `POST /api/auth/mfa/setup/passkey/start` | 30 par 5 minutes | adresse IP |
| Vérifications et enrôlements du second facteur, changement de mot de passe, gestion des facteurs | 10 par 5 minutes | compte |
| Toutes les routes `/api/public/*` | 120 par minute | adresse IP |

Seules les routes publiques ajoutent un en-tête `Retry-After`, en secondes entières (au moins 1). Pour les autres, attendez la fin de la fenêtre.

L'adresse IP est celle de la connexion TCP. Derrière un reverse proxy, FerrisGit ne lit `X-Forwarded-For` que si le proxy figure dans `TRUSTED_PROXY_CIDRS` (voir [Configuration](/docs/administration/configuration)). Sans cela, tous les clients partagent le compteur du proxy.

## Volumes et pagination

La plupart des listes renvoient tous les éléments, sans pagination. Les exceptions, avec leur limite :

| Route | Limite |
|---|---|
| Commits d'un dépôt | 20 derniers commits, pas de page suivante |
| Contributeurs d'un dépôt | 20 contributeurs les plus actifs, sur les 1000 derniers commits |
| Notifications | 50 plus récentes |
| Tableau de bord | 20 éléments par catégorie |
| Recherche | 8 résultats par type |
| Livraisons d'un webhook | 20 plus récentes |
| Liste des utilisateurs (administration) | 1000 lignes |
| Catalogue public | paginé : `page` et `perPage` |

## Chemins de dépôt

Un dépôt a un chemin, qui est aussi celui de son URL dans l'interface et de son URL Git.

- Un dépôt personnel : `utilisateur/depot`, par exemple `alice/blog`.
- Un dépôt de groupe : le chemin du groupe, puis le nom du dépôt, par exemple `acme/backend/api`. Les groupes peuvent être imbriqués autant de fois que nécessaire.
- Un groupe : son chemin seul, par exemple `acme/backend`.

Les noms de dépôts et de groupes ne contiennent que des lettres, des chiffres, `-` et `_`. Un nom d'utilisateur et le nom d'un groupe racine partagent le même espace : ils ne peuvent pas être identiques.

L'API travaille presque partout avec des identifiants UUID, pas avec des chemins. Pour passer de l'un à l'autre, appelez [`GET /api/resolve/{*path}`](/docs/api/depots), qui répond avec l'identifiant et le type (dépôt personnel, dépôt de groupe ou groupe). La résolution suit cette règle :

1. Si le premier segment est un nom d'utilisateur, le chemin doit avoir exactement deux segments et désigne un dépôt personnel.
2. Sinon, FerrisGit descend la chaîne de groupes depuis la racine. Si tous les segments sont des groupes, le chemin désigne un groupe. Si le dernier segment n'est pas un groupe, il désigne un dépôt de ce groupe.
3. Tout autre cas est un 404.

Pour les dépôts publics, la version anonyme est [`GET /api/public/resolve/{*path}`](/docs/api/api-publique). Elle ne résout que des dépôts publics, jamais un groupe.

## Git en HTTP

Les routes Git ne sont pas sous `/api`. Elles vivent à la racine, sous la forme `/{utilisateur}/{depot}.git/…` pour un dépôt personnel, et `/{groupe}/…/{depot}.git/…` pour un dépôt de groupe. Elles s'authentifient en HTTP Basic, avec un jeton Git comme mot de passe. Un dépôt public se lit sans identifiants tant que les pages publiques sont activées. Voir [Cloner et pousser](/docs/utilisation/cloner-et-pousser).

## Santé du serveur

`GET /health`, `GET /healthz` et `GET /readyz` répondent sans authentification, hors de `/api`. `/healthz` dit que le processus tourne (sonde de vivacité) ; `/readyz` vérifie en plus que la base répond (sonde de disponibilité). Voir [Administration](/docs/api/administration).

## Version du serveur

### `GET /api/version`

La version de FerrisGit qui répond, sans authentification. L'interface l'affiche au pied du menu de navigation.

Réponse 200 :

```json
{ "version": "0.1.3" }
```
