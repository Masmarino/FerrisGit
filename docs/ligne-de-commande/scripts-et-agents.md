# Scripts et agents

`fgit` est fait pour être piloté par un programme, script ou agent d'IA. Ce qui suit est un contrat : il ne change qu'avec une version, signalée dans le journal des versions.

## S'authentifier sans personne

Créez un [jeton d'API](/docs/api/authentification#jetons-dapi) dans [Mon compte](/docs/utilisation/compte-et-securite), avec les seules portées utiles, et donnez-le par l'environnement :

```bash
export FERRISGIT_TOKEN=fga_…
export FERRISGIT_HOST=git.example.com
fgit mr list --repo acme/api --json
```

| Variable | Effet |
|---|---|
| `FERRISGIT_TOKEN` | Le jeton à utiliser à la place de celui qui est gardé. Rien n'est écrit sur le disque. |
| `FERRISGIT_HOST` | L'instance à utiliser à la place de celle du remote Git. |
| `FERRISGIT_CONFIG_DIR` | Le dossier de configuration de `fgit`. |
| `NO_COLOR` | Pas de couleur, même dans un terminal. |

Quand `FERRISGIT_HOST` est défini, `FERRISGIT_TOKEN` ne part que vers cette instance. Sans lui, le jeton va à l'instance du remote Git du dossier courant. Définissez-le donc dans un job qui travaille sur des dépôts que vous ne contrôlez pas.

Git utilise le même jeton : `fgit repo clone`, `fgit mr create --push` et `fgit mr checkout` n'ont besoin de rien d'autre.

## Jamais de question

Sans terminal, `fgit` ne pose aucune question :

- Une valeur obligatoire qui manque est une erreur d'usage, qui nomme l'option : `error: --title is required`.
- Ce qui ne se défait pas (`mr merge`, `mr close`, `pipeline cancel`, `variable delete`, `secret delete`) demande `--yes`.
- `fgit auth login` ne reste pas à attendre, sauf avec `--web`.
- Git ne demande pas non plus d'identifiants : `GIT_TERMINAL_PROMPT` est à 0.

## Les sorties

Le résultat est sur la sortie standard. Les messages et la progression sont sur la sortie d'erreur. Hors d'un terminal, il n'y a ni couleur ni animation.

`--json` donne le résultat en JSON, sur une ligne hors d'un terminal. `--jq` le filtre avec une expression jq et implique `--json`. Une chaîne s'écrit alors sans guillemets, pour aller directement dans une variable, et une expression invalide échoue avant que la commande fasse quoi que ce soit.

```bash
MR=$(fgit mr create --title "Correctif" --push --jq .id)
fgit pipeline view --mr "$MR" --watch --jq .status
fgit mr merge "$MR" --yes
```

Les commandes qui écrivent renvoient aussi l'objet créé ou modifié. `variable delete`, `secret delete` et `auth logout` n'ont pas de résultat.

## Les codes de sortie

| Code | Sens | Réessayer ? |
|---|---|---|
| 0 | Réussi. | |
| 1 | Autre chose : Git a échoué, un pipeline suivi n'a pas réussi, un fichier local est illisible. | Selon le cas. |
| 2 | Usage : argument mauvais ou manquant, confirmation sans terminal pour répondre. | Non, corrigez la commande. |
| 3 | Authentification : pas de connexion, jeton invalide ou expiré. | Non, il faut un jeton. |
| 4 | Introuvable. C'est aussi la réponse pour ce que votre compte n'a pas le droit de voir. | Non. |
| 5 | Refusé par le serveur : portée, validation, conflit, limite de débit, conditions de fusion, branches en conflit. | Non, sauf la limite de débit. |
| 6 | Erreur du serveur, ou serveur injoignable. | Oui. |

## Découvrir l'interface

`fgit --help` liste les commandes et les codes de sortie, et chaque commande a son aide, avec ses options et des exemples : `fgit mr merge --help`. Un agent peut s'y retrouver seul.

Pour un agent, donnez un jeton aux portées étroites : un agent qui ne fait que lire et commenter n'a pas besoin de `merge-requests:write` sur tout, et un jeton n'atteint jamais ce que son propriétaire ne peut pas faire. Une portée manquante répond le code 5, avec le nom de la portée dans le message.

## Les champs JSON

Un champ n'apparaît ici que s'il fait partie du contrat. Ce que le serveur ajouterait n'atteint pas la sortie de `fgit` tant qu'il n'est pas nommé, pour qu'une mise à jour du serveur ne change pas ce que lit un script. `fgit api` fait exception : il rend la réponse du serveur telle quelle, comme décrite dans [l'API REST](/docs/api/introduction).

Les identifiants sont des UUID, les dates sont au format ISO 8601. `UserRef` vaut `{ id, username }`.

| Commandes | Champs |
|---|---|
| `repo list`, `view`, `create` | `id`, `name`, `description`, `owner`, `role`, `visibility`, `createdAt`, `path`. `repo view` ajoute `starCount`, `isStarred`, `sizeBytes`. |
| `issue list`, `view`, `create`, `edit`, `close`, `reopen`, `assign`, `milestone` | `id`, `number`, `title`, `description`, `status`, `kind`, `milestoneId`, `author`, `assignee`, `labels`, `commentCount`, `createdAt`, `closedAt`. `issue view --comments` ajoute `comments`. |
| `issue labels`, et `labels` ci-dessus | Tableau de `id`, `name`, `color`. |
| `issue comment`, `mr comment`, et `comments` ci-dessus | `id`, `author`, `body`, `createdAt`. |
| `mr list`, `view`, `create`, `merge`, `close` | `id`, `sourceBranch`, `targetBranch`, `title`, `description`, `status`, `mergeCommitSha`, `milestoneId`, `author`, `labels`, `commentCount`, `pipeline`, `createdAt`, `closedAt`. `mr view --comments` ajoute `comments`. |
| `pipeline` d'une demande | `id`, `status`, `commitSha`, `sourceSha`, `createdAt`, `finishedAt`. |
| `mr review` | `userId`, `username`, `decision`, `stale`, `createdAt`. |
| `mr diff` | Tableau de `path`, `change`, `hunks`. Chaque hunk a des `rows` : `oldLine`, `oldContent`, `newLine`, `newContent`, `kind`. |
| `pipeline list`, `cancel` | `id`, `commitSha`, `status`, `createdAt`, `finishedAt`, `triggeredBy`, `commitMessage`, `error`, `mergeRequestId`, `sourceSha`. |
| `pipeline view` | Les mêmes, avec `jobs` : `id`, `stage`, `name`, `status`, `needs`, `logs`, `logsPurgedAt`, `createdAt`, `startedAt`, `finishedAt`. |
| `pipeline logs` | Tableau de `name`, `status`, `logs`. Avec `--follow`, il sort une fois, à la fin du pipeline. |
| `release list` | `id`, `tagName`, `title`, `draft`, `prerelease`, `author`, `notesExcerpt`, `assetCount`, `createdAt`, `publishedAt`. |
| `release view`, `create` | `id`, `tagName`, `title`, `notes`, `draft`, `prerelease`, `author`, `targetCommitSha`, `assets`, `createdAt`, `publishedAt`. |
| `release upload`, et `assets` ci-dessus | `id`, `filename`, `contentType`, `sizeBytes`, `createdAt`. |
| `release download` | Tableau des chemins enregistrés. |
| `variable list`, `set`, `secret list`, `set` | `id`, `key`, `masked`. |
| `auth login` | `host`, `username`, `source` (`environment`, `keychain` ou `file`), `scopes`, `expiresAt`. |
| `auth status` | Tableau de `host`, `username`, `source`, `valid`, `error`, `isDefault`. |

Les valeurs possibles :

| Champ | Valeurs |
|---|---|
| `status` d'un ticket | `todo`, `in_progress`, `in_review`, `done` |
| `kind` d'un ticket | `bug`, `feature`, `task`, `epic` |
| `status` d'une demande | `open`, `merged`, `closed` |
| `decision` | `approved`, `changes_requested` |
| `status` d'un pipeline ou d'un job | `pending`, `running`, `success`, `failed`, `canceled` |
| `visibility` | `private`, `public` |

## Dans un job FerrisGit

Rangez le jeton dans un [secret de CI](/docs/ci-cd/variables-et-secrets) du dépôt, sous le nom `FERRISGIT_TOKEN` : les jobs le reçoivent dans leur environnement. Un job n'a aucun droit sur l'API tant que vous ne lui donnez pas de jeton.

```yaml
stages: [publier]

jobs:
  release:
    stage: publier
    image: alpine:3.24
    script:
      - wget -qO- "$URL_DE_FGIT" | tar xz -C /usr/local/bin
      - fgit release create "v1.2.3" --repo git.example.com/acme/api --title "Version 1.2.3" --asset dist/app.tar.gz
```
