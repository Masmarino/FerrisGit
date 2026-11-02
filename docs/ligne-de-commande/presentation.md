# Présentation et connexion

`fgit` est le client en ligne de commande de FerrisGit. Il sert aux personnes dans un terminal, aux scripts et aux jobs de CI, et aux agents d'IA qui travaillent dans un dépôt. Il couvre les dépôts, les tickets, les demandes de fusion, les pipelines, les releases et les variables de CI, et il peut appeler n'importe quelle route de l'[API REST](/docs/api/introduction).

## Installer

Chaque [release GitHub](https://github.com/Masmarino/FerrisGit/releases) contient `fgit` pour Linux, macOS et Windows, en x86_64 et en arm64, avec un fichier de sommes de contrôle. Le binaire Linux est statique : il n'a besoin d'aucune bibliothèque.

```bash
tar xzf fgit-*-x86_64-unknown-linux-musl.tar.gz
sudo mv fgit /usr/local/bin/
fgit --version
```

Avec une chaîne d'outils Rust, il s'installe aussi depuis crates.io. Le paquet s'appelle `ferrisgit-cli`, parce que le nom `fgit` y est déjà pris. La commande reste `fgit`.

```bash
cargo install ferrisgit-cli
```

Le nom `fg` a été écarté : il est pris par la commande intégrée des shells.

## Se connecter

```bash
fgit auth login --hostname git.example.com
```

`fgit login` est un raccourci de cette commande. Elle montre un code et ouvre le navigateur sur la page de confirmation :

1. Connectez-vous à FerrisGit dans le navigateur, si ce n'est pas déjà fait.
2. Vérifiez que le code affiché est bien celui du terminal, et regardez les portées demandées. Ne confirmez que si c'est vous qui venez de lancer la commande.
3. Confirmez avec votre second facteur : code TOTP, passkey ou code de secours. Il est redemandé même si votre session est récente, parce que cette étape délivre un jeton de longue durée.

Le terminal reçoit alors un [jeton d'API](/docs/api/authentification#jetons-dapi), qu'il range dans le trousseau du système. Le fichier `hosts.json`, dans le dossier de configuration de `fgit`, ne dit que les instances connectées, pas leurs jetons.

| Option | Effet |
|---|---|
| `--scopes` | Les portées demandées, séparées par des virgules. Par défaut, `write` partout sauf `admin`. |
| `--expires-in-days` | Durée de vie du jeton, 90 jours par défaut, 365 au plus. |
| `--no-browser` | N'ouvre pas le navigateur et montre seulement l'adresse. |
| `--with-token` | Lit un jeton existant sur l'entrée standard. Il est vérifié auprès du serveur avant d'être gardé. |
| `--web` | Fait le parcours par le navigateur même sans terminal : le code est sur la sortie d'erreur, et la commande attend que quelqu'un confirme. |
| `--insecure-storage` | Garde le jeton dans `hosts.json`, en clair, pour une machine sans trousseau. `fgit` ne le fait jamais de lui-même : sans cette option, un trousseau indisponible est une erreur. |

### Plusieurs instances

Vous pouvez être connecté à plusieurs instances. La première devient l'instance par défaut.

```bash
fgit auth status
```

`fgit auth logout` oublie le jeton sur cette machine, mais il reste valable jusqu'à son expiration. Pour y mettre fin tout de suite, révoquez-le dans [Mon compte](/docs/utilisation/compte-et-securite).

### Git

`fgit repo clone` configure le clone pour que `git push` et `git pull` demandent leurs identifiants à `fgit`. Il n'y a donc pas de second jeton à gérer. Pour des dépôts clonés autrement :

```bash
fgit auth setup-git
```

Cette commande déclare `fgit` comme aide d'authentification dans la configuration globale de Git, seulement pour les instances connectées. Le jeton a besoin de la portée `repositories` : `read` pour récupérer, `write` pour pousser.

## Quel dépôt, quelle instance

Dans un clone, le dépôt et l'instance viennent du remote Git : `origin`, ou le seul remote qui pointe vers une instance. Ailleurs, nommez le dépôt avec `--repo` :

```bash
fgit mr list --repo git.example.com/acme/backend/api
fgit mr list --repo acme/backend/api
```

La forme courte utilise l'instance courante. Les groupes s'imbriquent, le chemin peut donc être long.

L'instance se détermine dans cet ordre : celle de `--repo`, `--hostname`, `FERRISGIT_HOST`, le remote Git, puis l'instance par défaut.

Un remote en HTTP simple vers une autre machine ne reçoit pas de jeton, parce qu'il pourrait être lu en chemin : la commande échoue, sauf si vous avez choisi cette instance vous-même (`--hostname`, `FERRISGIT_HOST` ou une connexion préalable). `fgit` ne suit pas non plus les redirections, pour qu'un jeton ne parte jamais ailleurs que sur l'instance prévue.

## Le parcours complet

```bash
fgit repo clone git.example.com/acme/api
cd api
git switch -c correctif
# … des commits …
fgit mr create --title "Corrige la connexion" --push
fgit pipeline logs --mr "$(fgit mr view --jq .id)" --job test --follow
fgit mr merge --yes
```

Sans identifiant, les commandes `mr` prennent la demande ouverte de la branche courante.

La suite : [toutes les commandes](/docs/ligne-de-commande/commandes), et [l'usage par des scripts et des agents](/docs/ligne-de-commande/scripts-et-agents).
