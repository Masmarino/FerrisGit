# Commandes

Chaque commande a une aide complète, avec des exemples : `fgit mr create --help`. Cette page en donne la vue d'ensemble.

Quatre options marchent partout :

| Option | Effet |
|---|---|
| `--repo`, `-R` | Le dépôt visé : `hôte/propriétaire/nom` ou `propriétaire/nom`. Voir [Quel dépôt, quelle instance](/docs/ligne-de-commande/presentation#quel-dépôt-quelle-instance). |
| `--json` | Le résultat en JSON. Voir [les champs](/docs/ligne-de-commande/scripts-et-agents#les-champs-json). |
| `--jq` | Filtre le résultat avec une expression jq. |
| `--yes`, `-y` | Confirme ce qui ne se défait pas. |

Un identifiant de demande de fusion ou de pipeline peut s'abréger : quatre caractères au moins, tant qu'un seul correspond dans le dépôt.

## `auth`

| Commande | Effet |
|---|---|
| `fgit auth login` | Se connecte. `fgit login` est un raccourci. |
| `fgit auth logout` | Oublie un jeton sur cette machine. `fgit logout` est un raccourci. |
| `fgit auth status` | Montre les instances connectées et si leurs jetons marchent encore. |
| `fgit auth token` | Affiche le jeton d'une instance. |
| `fgit auth setup-git` | Fait de `fgit` l'aide d'authentification de Git pour les instances connectées. |

## `repo`

| Commande | Effet |
|---|---|
| `fgit repo list` | Les dépôts que vous voyez. `--starred` pour vos favoris. |
| `fgit repo view [dépôt]` | Un dépôt. |
| `fgit repo create <nom>` | Crée un dépôt, privé sauf avec `--public`. `--group acme/equipe` le crée dans un groupe. |
| `fgit repo clone <dépôt> [dossier]` | Clone un dépôt et le configure pour que Git demande ses identifiants à `fgit`. |

## `issue`

| Commande | Effet |
|---|---|
| `fgit issue list` | Les tickets. `--state open\|closed\|all` (`open` par défaut : tous les statuts sauf `done`), `--label`, `--assignee`. |
| `fgit issue view <n>` | Un ticket. `--comments` ajoute ses commentaires. |
| `fgit issue create` | Ouvre un ticket : `--title`, `--body` ou `--body-file`, `--kind bug\|feature\|task\|epic` (`task` par défaut). |
| `fgit issue edit <n>` | Change `--title`, `--body`, `--kind` ou `--status todo\|in_progress\|in_review\|done`. Le reste ne bouge pas. |
| `fgit issue comment <n>` | Commente, avec `--body` ou `--body-file`. |
| `fgit issue close <n>` | Ferme (statut `done`). |
| `fgit issue reopen <n>` | Rouvre (statut `todo`). |
| `fgit issue assign <n> <utilisateur>` | Assigne. `--unassign` retire l'assignation. |
| `fgit issue labels <n>` | `--add` et `--remove`, répétables, par nom. |
| `fgit issue milestone <n> <titre>` | Pose un jalon. `--clear` le retire. |

`--body-file -` lit le texte sur l'entrée standard.

## `mr`

Sans identifiant, ces commandes prennent la demande ouverte de la branche courante.

| Commande | Effet |
|---|---|
| `fgit mr list` | Les demandes. `--state open\|merged\|closed\|all`, `open` par défaut. |
| `fgit mr view [id]` | Une demande. `--comments` ajoute ses commentaires. |
| `fgit mr create` | Ouvre une demande depuis la branche courante : `--title`, `--body`, `--target` (la branche par défaut sinon), `--source`. La branche doit être sur le serveur : `--push` l'y envoie d'abord. |
| `fgit mr checkout <id>` | Récupère la branche d'une demande et s'y place. |
| `fgit mr diff [id]` | Les changements. |
| `fgit mr comment [id]` | Commente, avec `--body` ou `--body-file`. |
| `fgit mr review [id]` | `--approve` ou `--request-changes`. Une relecture ne porte pas de texte : `--body` s'ajoute d'abord en commentaire. |
| `fgit mr merge [id]` | Fusionne. `--method merge\|squash\|rebase`, la méthode du dépôt sinon. Demande `--yes` sans terminal. |
| `fgit mr close [id]` | Ferme sans fusionner. Demande `--yes` sans terminal. |

## `pipeline`

| Commande | Effet |
|---|---|
| `fgit pipeline list` | Les pipelines, du plus récent au plus ancien. `--limit`, 20 par défaut. |
| `fgit pipeline view [id]` | Un pipeline et ses jobs, le plus récent sans identifiant. `--mr` prend celui d'une demande. `--watch` attend la fin. |
| `fgit pipeline logs [id]` | Les journaux des jobs. `--job` pour un seul, `--mr` pour une demande, `--follow` pour les suivre jusqu'à la fin. |
| `fgit pipeline cancel <id>` | Annule un pipeline. Demande `--yes` sans terminal. |

Avec `--watch` et `--follow`, le code de sortie est 1 si le pipeline ne réussit pas : un script peut s'arrêter là. Le suivi interroge le serveur toutes les 3 secondes, comme l'interface web.

## `release`

| Commande | Effet |
|---|---|
| `fgit release list` | Les releases. |
| `fgit release view <tag>` | Une release et ses fichiers. |
| `fgit release create <tag>` | Crée une release : `--title`, `--notes` ou `--notes-file`, `--target` (une branche ou un commit complet, la branche par défaut sinon), `--draft`, `--prerelease`, `--asset`. |
| `fgit release upload <tag> <fichiers…>` | Joint des fichiers, 100 Mio au plus chacun. |
| `fgit release download <tag>` | Télécharge les fichiers. `--asset` pour certains, `--dir` pour le dossier, `--clobber` pour remplacer ceux qui existent. |

## `variable` et `secret`

Ce sont les [variables de CI](/docs/ci-cd/variables-et-secrets) d'un dépôt, qui demandent le rôle Mainteneur. Un secret est une variable dont la valeur est masquée dans les journaux. Chaque commande ne voit que les siens : `secret delete` ne peut pas supprimer une variable simple par erreur. Le serveur ne renvoie jamais les valeurs.

| Commande | Effet |
|---|---|
| `fgit variable list` | Les noms. |
| `fgit variable set <clé>` | Crée ou remplace. La valeur vient de `--value`, de `--value-file` ou de l'entrée standard. |
| `fgit variable delete <clé>` | Supprime. Demande `--yes` sans terminal. |

`fgit secret` prend les mêmes sous-commandes. Pour un secret, préférez l'entrée standard : une ligne de commande reste dans l'historique du shell.

```bash
printf %s "$JETON_DE_DEPLOIEMENT" | fgit secret set DEPLOY_TOKEN
```

## `api`

Une requête authentifiée vers n'importe quelle route de l'[API REST](/docs/api/introduction), pour ce que les commandes ne couvrent pas. Le chemin part de `/api`.

```bash
fgit api /auth/me --jq .username
fgit api "/repositories/$ID/labels" -f name=bug -f color=#d73a4a
fgit api "/merge-requests/$ID" -X PATCH --input corps.json
fgit api "/repositories/$ID/issues/1/assign" -F assigneeId=null
```

`-f` donne un champ texte, `-F` un champ JSON (`true`, `12`, `null`, `["a"]`), `--input` envoie un fichier tel quel. La méthode est `GET`, ou `POST` avec un corps, sauf si `-X` en dit une autre.

Seul un chemin est accepté, pas une adresse complète, pour que le jeton reste sur l'instance prévue. Les limites sont celles du jeton : voir [les portées](/docs/api/authentification#portées).
