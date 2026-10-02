# Runners Docker

Un runner est le programme `ferrisgit-runner`. Il interroge le serveur FerrisGit, prend un job disponible, clone le dépôt, lance le script dans un conteneur Docker et renvoie les journaux et le résultat. Il sert quand le moteur d'exécution de l'instance est **Docker / runners** (**Admin > Réglages > Exécution**).

## Ce que fait un runner, pas à pas

1. Toutes les `FERRISGIT_POLL_INTERVAL_SECS` secondes (5 par défaut), il demande un job au serveur. S'il n'y en a pas, il attend et recommence.
2. Il clone le dépôt dans `<FERRISGIT_WORKDIR_ROOT>/<identifiant du job>` avec `git clone`, puis `git checkout` sur le commit du pipeline. Le clonage utilise son propre jeton, en lecture seule.
3. Il supprime le dossier `.git` du clone, pour que le jeton qui s'y trouve ne soit pas lisible par le script.
4. Il lance `docker run --rm -v <dossier>:/workspace -w /workspace` avec une option `-e` par variable, l'image du job, puis `sh -c "<script>"`.
5. Il envoie chaque ligne de sortie (standard et erreur) au serveur dès qu'elle est produite, puis le résultat : réussi si le conteneur sort avec le code 0, échoué sinon.
6. Il supprime le dossier de travail et passe au job suivant.

Un runner traite **un seul job à la fois**. Pour exécuter des jobs en parallèle, lancez plusieurs runners.

Le script se lance donc dans `/workspace`, où se trouve le dépôt au commit voulu (sans `.git`). Le conteneur n'a ni accès au socket Docker de l'hôte, ni mode privilégié : un job ne peut pas lancer `docker build` sur le démon de l'hôte.

## Prérequis

Sur la machine du runner :

- `git` et la commande `docker`, avec un démon Docker auquel l'utilisateur qui lance le runner a accès ;
- un accès réseau au serveur FerrisGit et aux registres d'images utilisés par vos jobs ;
- un compilateur Rust pour construire le binaire (voir plus bas).

## Obtenir le binaire

FerrisGit ne publie pas de binaire du runner ni d'image de conteneur pour lui : l'image du serveur ne contient que le serveur. Construisez-le depuis les sources du dépôt FerrisGit :

```bash
cargo build --release -p ferrisgit-runner
```

Le binaire est `target/release/ferrisgit-runner`. Copiez-le sur la machine du runner.

## Enregistrer le runner

Chaque runner a son jeton. Il n'y a pas de jeton partagé : celui-ci est lié à l'enregistrement du runner dans l'instance.

### Depuis l'interface

Un administrateur ouvre **Runners** dans le menu principal (l'entrée n'apparaît que si le moteur Docker / runners est actif) :

1. Cliquez sur **Enregistrer un runner**.
2. Saisissez le **Nom du runner** (obligatoire, par exemple `vps-1`) et, si besoin, des **Tags** (Entrée ou virgule pour en ajouter).
3. Cliquez sur **Enregistrer**. L'interface affiche « Jeton du runner *nom* » : copiez-le tout de suite, il ne sera plus jamais affiché. Le jeton est de la forme `fgr_` suivi de 64 caractères hexadécimaux.

### Par l'API, avec un jeton d'enregistrement

Un administrateur peut aussi définir un jeton d'enregistrement d'instance, qui laisse des machines s'enregistrer elles-mêmes. Il se règle dans **Admin > Réglages > Exécution**, carte **Runners Docker** : saisissez un jeton, ou cliquez sur **Générer un jeton** pour en obtenir un aléatoire, affiché une seule fois (voir [Réglages de l'instance](/docs/administration/reglages)). Le même réglage existe dans l'API (champ `runnerRegistrationToken` de `PUT /api/admin/settings`, voir [API d'administration](/docs/api/administration)). Le serveur ne stocke que l'empreinte du jeton. Ensuite :

```bash
curl -X POST https://ferrisgit.example.com/api/runner/register \
  -H "Content-Type: application/json" \
  -d '{"registration_token": "<jeton d'\''enregistrement>", "name": "vps-2", "tags": ["docker", "linux"]}'
```

La réponse JSON contient `id`, `name`, `tags` et `token`, le jeton du runner. Tant qu'aucun jeton d'enregistrement n'est défini, la route répond `401` avec « self-service runner registration is disabled » ; un jeton incorrect donne « invalid registration token ». Supprimer le jeton d'enregistrement désactive de nouveau la route, sans toucher aux runners déjà enregistrés. Notez que `registration_token` est en `snake_case` dans cette requête, alors que la réponse est en `camelCase`.

> **Note** : le binaire `ferrisgit-runner` ne s'enregistre pas lui-même. Il lui faut un jeton de runner déjà obtenu par l'une des deux voies ci-dessus.

## Configurer et lancer

Le runner se configure uniquement par variables d'environnement.

| Variable | Obligatoire | Défaut | Rôle |
|---|---|---|---|
| `FERRISGIT_SERVER_URL` | oui | aucun | Adresse de base du serveur, par exemple `https://ferrisgit.example.com`, sans `/` final. |
| `FERRISGIT_RUNNER_TOKEN` | oui | aucun | Le jeton du runner (`fgr_…`). |
| `FERRISGIT_RUNNER_TAGS` | non | vide | Liste séparée par des virgules, lue et affichée au démarrage. Voir ci-dessous : elle ne change pas les jobs reçus. |
| `FERRISGIT_POLL_INTERVAL_SECS` | non | `5` | Secondes d'attente entre deux demandes de job. Une valeur qui n'est pas un entier positif ou nul est ignorée et remplacée par `5`. |
| `FERRISGIT_WORKDIR_ROOT` | non | `/tmp/ferrisgit-runner` | Dossier où sont clonés les dépôts, un sous-dossier par job. |

Si `FERRISGIT_SERVER_URL` ou `FERRISGIT_RUNNER_TOKEN` manque, le runner s'arrête au démarrage avec « FERRISGIT_SERVER_URL must be set » ou « FERRISGIT_RUNNER_TOKEN must be set ».

```bash
FERRISGIT_SERVER_URL=https://ferrisgit.example.com \
FERRISGIT_RUNNER_TOKEN=fgr_... \
FERRISGIT_POLL_INTERVAL_SECS=5 \
FERRISGIT_WORKDIR_ROOT=/var/lib/ferrisgit-runner \
./ferrisgit-runner
```

Les journaux du runner sortent sur la sortie standard (format `tracing` : démarrage avec les étiquettes et l'intervalle, « claimed job », « job finished », erreurs). Le niveau n'est pas réglable : il n'y a pas de variable de filtrage.

### Étiquettes

Les étiquettes (tags) qui comptent sont celles données **à l'enregistrement** du runner, dans l'interface ou dans le champ `tags` de l'API : le serveur les retrouve à partir du jeton. `FERRISGIT_RUNNER_TAGS` est envoyée au serveur mais le serveur ne s'en sert pas. Pour changer les étiquettes d'un runner, enregistrez-en un nouveau avec les bonnes étiquettes et révoquez l'ancien.

Un job qui déclare `tags:` n'est donné qu'à un runner qui porte toutes ces étiquettes ; un job sans `tags` peut aller sur n'importe quel runner. Un runner sans étiquette ne prend donc que des jobs sans `tags`. Voir [la référence](/docs/ci-cd/reference-yaml#tags).

## L'installer comme service

Le runner est un processus qui tourne en continu. FerrisGit ne fournit pas de fichier de service : voici un exemple `systemd` à adapter (utilisateur membre du groupe `docker`, jeton dans un fichier lisible par lui seul).

```ini
[Unit]
Description=FerrisGit runner
After=network-online.target docker.service
Wants=network-online.target

[Service]
User=ferrisgit-runner
EnvironmentFile=/etc/ferrisgit-runner.env
ExecStart=/usr/local/bin/ferrisgit-runner
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Le fichier `/etc/ferrisgit-runner.env` contient les lignes `FERRISGIT_SERVER_URL=...`, `FERRISGIT_RUNNER_TOKEN=...` et les autres variables.

> **Note** : lancer le runner dans un conteneur n'est pas un cas prévu. Il commande Docker en créant des conteneurs frères avec `-v <dossier>:/workspace` : le dossier de travail doit donc exister, avec le même chemin, côté démon Docker. Si vous le faites malgré tout, il faut au minimum `git` et la commande `docker` dans l'image, l'accès au socket du démon, et `FERRISGIT_WORKDIR_ROOT` monté au même chemin dans le conteneur et sur l'hôte.

## Sécurité

- **Le jeton d'un runner est très puissant.** Il n'expire jamais et donne accès en lecture à tous les dépôts de l'instance (c'est ainsi que le runner clone). Il permet aussi de prendre les jobs de n'importe quel dépôt et de recevoir en clair ses variables CI. Il ne permet pas de pousser. Traitez-le comme un mot de passe.
- Un job s'exécute dans un conteneur sur la machine du runner, avec les droits que Docker lui donne : toute personne qui peut modifier un fichier de pipeline y exécute du code. N'enregistrez pas de runner sur une machine qui héberge autre chose de sensible.
- Les valeurs des variables arrivent au conteneur par des options `-e` de `docker run` : elles sont visibles, le temps du job, dans la liste des processus de la machine du runner et dans `docker inspect`.
- Le dossier `.git` est retiré du clone avant le lancement du conteneur, et le jeton du runner est masqué (`***`) dans les journaux.
- Pour révoquer un runner, supprimez-le par l'API : `DELETE /api/admin/runners/{id}`. Son jeton cesse de fonctionner immédiatement et ses jobs en cours repassent à **En attente**. L'interface actuelle n'a pas de bouton de révocation.

## Supervision

La page **Runners** (menu principal, pour les administrateurs) liste les runners enregistrés avec leurs étiquettes (ou « Aucun tag ») et un résumé du type « 2 runners · 1 en ligne ». Chaque ligne indique un état :

| État | Signification |
|---|---|
| **En ligne** | Le dernier signal date de moins de 2 minutes. |
| **Hors ligne** | Le dernier signal date de plus de 2 minutes. |
| **Jamais connecté** | Le runner est enregistré mais n'a encore jamais contacté le serveur. |

Le « signal » est mis à jour à chaque requête authentifiée du runner : sa demande de job (toutes les 5 secondes par défaut), l'envoi de journaux et les clonages de dépôt.

> **Note** : l'entrée **Runners** du menu n'apparaît que si le moteur actif est Docker / runners. Les routes sous-jacentes sont réservées aux administrateurs ; un compte non administrateur qui ouvre la page voit « Impossible de charger les runners. »

## Cas particuliers

- **Annulation** : annuler un pipeline marque ses jobs comme annulés, mais le runner n'est pas prévenu. Un conteneur déjà lancé continue jusqu'au bout et son résultat est ignoré.
- **Runner arrêté en plein job** : le serveur n'a pas de délai d'abandon. Le job reste **En cours** tant que vous n'annulez pas le pipeline ou ne révoquez pas le runner.
- **Dossiers de travail** : le runner supprime le dossier du job à la fin ; une erreur de suppression est ignorée sans message. Surveillez `FERRISGIT_WORKDIR_ROOT`.
- **Panne du serveur** : quand le serveur est injoignable, le runner journalise « failed to claim job » et réessaie à l'intervalle normal.

Si quelque chose ne marche pas, voir [Dépannage](/docs/ci-cd/depannage).
