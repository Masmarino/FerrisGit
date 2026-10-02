# Configuration

Le serveur FerrisGit (`ferrisgit-api`) se configure par variables d'environnement, lues **une seule fois au démarrage** : pour changer une valeur, modifiez-la puis redémarrez le serveur. Une variable obligatoire absente ou une valeur invalide arrête le démarrage avec un message explicite.

Les réglages qui se modifient en cours de service (SMTP, inscription libre, durée des sessions, jeton d'enregistrement des runners, durée de conservation des journaux…) ne sont pas des variables : ils sont dans l'interface, voir [Réglages de l'instance](/docs/administration/reglages). Il n'existe en particulier **aucune variable pour le serveur SMTP**.

## Tableau des variables

| Variable | Obligatoire | Défaut | Format | Effet |
|---|---|---|---|---|
| `DATABASE_URL` | oui | aucun | URL PostgreSQL, par exemple `postgres://ferrisgit:motdepasse@postgres:5432/ferrisgit` | Base de données. Le pool de connexions est fixé à 10 et n'est pas configurable. Les migrations sont appliquées à chaque démarrage. |
| `JWT_SECRET` | oui | aucun | Texte d'au moins 32 octets | Signe les jetons de session ; sert aussi à dériver la clé des jetons intermédiaires de la double authentification. |
| `SETTINGS_ENCRYPTION_KEY` | oui | aucun | Texte d'exactement 32 octets, utilisé tel quel comme clé AES-256 | Chiffre au repos les variables CI des dépôts, les secrets des webhooks, le mot de passe SMTP et les secrets TOTP. |
| `PUBLIC_URL` | oui | aucun | Origine : `https://hôte[:port]`, sans chemin | Adresse publique : liens des e-mails, origine des clés d'accès. |
| `TRUSTED_PROXY_CIDRS` | non | vide | Liste de réseaux CIDR séparés par des virgules | Réseaux de votre reverse proxy : pour une connexion venant de là, l'adresse du client est lue dans `X-Forwarded-For`. |
| `STORAGE_ROOT` | non | `./data` | Chemin d'un répertoire | Racine des dépôts Git, des wikis et des fichiers joints aux releases. |
| `BIND_ADDR` | non | `0.0.0.0:8080` | `adresse:port`, par exemple `127.0.0.1:8080` ou `[::]:8080` | Adresse et port d'écoute. |
| `STATIC_DIR` | non | `./static` (`/app/static` dans l'image) | Chemin d'un répertoire | Application web compilée, servie par le serveur. |
| `FERRISGIT_BOOTSTRAP_ADMIN_USERNAME` | non | aucun | Nom d'utilisateur | Nom du premier administrateur. |
| `FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD` | non | aucun | 8 caractères au minimum | Mot de passe du premier administrateur. |
| `RUST_LOG` | non | niveau `info` | Directives de filtrage, par exemple `info,ferrisgit_api=debug` | Niveau des journaux. |

Une valeur vide est traitée comme une valeur absente pour `FERRISGIT_BOOTSTRAP_ADMIN_USERNAME` et `FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD`, et pour `TRUSTED_PROXY_CIDRS` (aucun proxy de confiance).

D'autres variables apparaissent dans les fichiers de déploiement sans être lues par le serveur :

| Variable | Où | Rôle |
|---|---|---|
| `POSTGRES_PASSWORD` | `docker-compose.yml`, `.env` | Mot de passe de la base, partagé par le service PostgreSQL et par `DATABASE_URL`. Dans Kubernetes, c'est la clé `POSTGRES_PASSWORD` du Secret. |
| `KUBECONFIG` | Environnement du serveur | Lue par la bibliothèque cliente Kubernetes, pas par FerrisGit : chemin d'un fichier kubeconfig quand le serveur tourne hors du cluster où s'exécutent les jobs. Dans un cluster, le jeton du `ServiceAccount` suffit. |

Les valeurs fixées par `docker-compose.yml` : `STORAGE_ROOT=/data`, `BIND_ADDR=0.0.0.0:8080`, `RUST_LOG` à `info` si absente, `PUBLIC_URL` à `http://localhost:8080` si absente, `TRUSTED_PROXY_CIDRS` vide si absente.

## Validations au démarrage

Le serveur contrôle ses variables dans cet ordre et s'arrête au premier problème. Le message est affiché sur la sortie d'erreur, suivi d'un détail technique de Rust (« panicked at… »).

| Message | Cause |
|---|---|
| `JWT_SECRET must be set` | La variable est absente. |
| `JWT_SECRET must be at least 32 characters long, got N` | Moins de 32 octets. |
| `SETTINGS_ENCRYPTION_KEY must be set` | La variable est absente. |
| `SETTINGS_ENCRYPTION_KEY must be exactly 32 bytes long, got N` | La longueur n'est pas 32 octets. |
| `PUBLIC_URL must be set` | La variable est absente. |
| `PUBLIC_URL must not contain whitespace, control characters or backslashes, got "…"` | Espace, caractère de contrôle ou antislash dans la valeur. |
| `PUBLIC_URL must start with http:// or https://, got "…"` | Autre schéma, ou schéma absent. |
| `PUBLIC_URL is not a valid origin (…), got "…"` | URL non analysable. |
| `PUBLIC_URL must contain a host, got "…"` | Pas d'hôte. |
| `PUBLIC_URL must be a bare origin (scheme, host and optional port; no path, query, fragment or credentials), got "…"` | Chemin, paramètres, fragment ou identifiants dans la valeur. |
| `TRUSTED_PROXY_CIDRS: invalid entry "…" (…)` | Une entrée n'est pas une adresse IP (`not an IP address`) ou son préfixe est hors limites (`the prefix length must be a number from 0 to 32` pour IPv4, `0 to 128` pour IPv6). |
| `DATABASE_URL must be set` | La variable est absente. |
| `failed to connect to postgres` | La base est injoignable ou l'URL est incorrecte. Il n'y a pas de nouvelle tentative : le processus s'arrête, à l'orchestrateur de le relancer. |
| `failed to run migrations` | Une migration a échoué, ou la base ne correspond pas à cette version (voir [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour)). |
| `invalid BIND_ADDR` | `BIND_ADDR` n'est pas de la forme `adresse:port`. |
| `failed to bind` | Le port est déjà utilisé ou interdit. |

Deux problèmes ne bloquent pas le démarrage : un mot de passe d'administrateur initial de moins de 8 caractères (le compte n'est pas créé, un avertissement est journalisé) et un `RUST_LOG` invalide (signalé sur la sortie d'erreur, puis ignoré).

## PUBLIC_URL

C'est l'origine exacte que les navigateurs utilisent, par exemple `https://git.example.com` ou `http://localhost:8080`. FerrisGit la normalise : espaces de début et de fin retirés, schéma et hôte en minuscules, port par défaut supprimé (`https://app.example.com:443` devient `https://app.example.com`), barre oblique finale retirée.

Elle sert à deux choses :

- les liens des e-mails : `PUBLIC_URL/activate#token=…` pour une invitation et `PUBLIC_URL/reset-password#token=…` pour une réinitialisation de mot de passe ;
- les clés d'accès (passkeys) : elles sont liées à l'hôte et à l'origine.

Les clés d'accès sont indisponibles, sans empêcher le démarrage, quand `PUBLIC_URL` est en `http://` sur un hôte autre que `localhost` ou `*.localhost`, ou quand son hôte est une adresse IP. Les routes de clés d'accès répondent alors 503 et la double authentification par application reste possible. Changer l'hôte de `PUBLIC_URL` rend inutilisables les clés d'accès déjà enregistrées.

## TRUSTED_PROXY_CIDRS

Les limites de débit de la connexion, de l'inscription, de l'activation et des pages publiques sont comptées par adresse IP du client (les adresses IPv6 sont regroupées par bloc /64). Derrière un reverse proxy, l'adresse que le serveur voit est celle du proxy.

- **Vide** (par défaut) : `X-Forwarded-For` n'est jamais lu. À utiliser quand le serveur est joint directement.
- **Renseignée** : pour une connexion dont l'adresse source appartient à l'un des réseaux listés, FerrisGit lit `X-Forwarded-For` en partant de la droite, saute les adresses elles-mêmes listées et retient la première qui ne l'est pas : c'est le client. Si l'en-tête est absent, ne contient que des adresses de confiance ou comporte une valeur illisible avant une adresse non fiable, l'adresse source est utilisée. Une connexion qui vient d'ailleurs ne peut pas choisir son adresse.

Format : des entrées séparées par des virgules, `adresse/préfixe` ou une adresse seule (équivalente à `/32` en IPv4, `/128` en IPv6). Les espaces autour des entrées sont ignorés, les bits d'hôte aussi (`10.2.0.5/16` vaut `10.2.0.0/16`). Exemple : `10.2.0.0/16, fd00::/8`.

> **Attention** : ne listez que votre proxy. Un réseau que des clients non fiables peuvent atteindre leur permettrait de choisir leur adresse et donc d'échapper aux limites.

## SETTINGS_ENCRYPTION_KEY

Exactement 32 octets : c'est la clé AES-256 brute. La valeur n'est pas décodée (ni base64 ni hexadécimal). Une chaîne ASCII de 32 caractères convient, par exemple celle de `openssl rand -base64 24`. Un caractère non ASCII compte pour plusieurs octets.

Elle chiffre (AES-256-GCM, avec un nonce aléatoire par valeur) :

- les variables CI des dépôts ;
- les secrets des webhooks ;
- le mot de passe SMTP ;
- les secrets TOTP des utilisateurs.

> **Attention** : cette clé ne peut pas être changée en place. Il n'y a pas de rechiffrement. Avec une autre clé, ou en restaurant une sauvegarde de la base sur une instance dotée d'une autre clé, ces valeurs deviennent illisibles : il faut ressaisir les variables CI, les secrets de webhooks et le mot de passe SMTP, et réinitialiser la double authentification des utilisateurs dont le secret TOTP ne peut plus être lu (leur connexion échoue avec une erreur serveur). Conservez la clé avec vos sauvegardes.

## JWT_SECRET

Au moins 32 octets. Il signe les sessions (jetons JWT de 12 heures par défaut, voir la durée de vie dans [Réglages de l'instance](/docs/administration/reglages)) et, par dérivation, les jetons intermédiaires qui séparent le mot de passe et le second facteur à la connexion.

Le changer déconnecte tous les utilisateurs et interrompt les connexions en cours : chacun doit se reconnecter. Rien d'autre n'est perdu. Faites-le si vous pensez la valeur compromise.

## STORAGE_ROOT

Répertoire où le serveur écrit. Il doit être accessible en écriture par l'utilisateur du processus (uid 100 dans l'image). On y trouve :

- les dépôts Git nus, dans un répertoire par propriétaire (`<identifiant>/<nom>.git`) ;
- les wikis, à côté de leur dépôt (`<nom>.wiki.git`) ;
- les fichiers joints aux releases, sous `release-assets/`.

Montez un volume persistant dessus et sauvegardez-le avec la base : voir [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour). Le défaut `./data` est relatif au répertoire de travail du processus (`/app/data` dans l'image) : définissez toujours `STORAGE_ROOT` explicitement.

## BIND_ADDR

Adresse d'écoute au format `adresse:port`. `0.0.0.0:8080` écoute sur toutes les interfaces IPv4. Pour que seul un proxy local atteigne le serveur, utilisez `127.0.0.1:8080`. Avec Docker, gardez `0.0.0.0` dans le conteneur et limitez la publication du port dans Compose (`127.0.0.1:8080:8080`).

## RUST_LOG

Filtre des journaux, écrits sur la sortie standard. Sans valeur, le niveau est `info`. La syntaxe est une liste, séparée par des virgules, de directives `niveau` ou `module=niveau`, avec les niveaux `error`, `warn`, `info`, `debug` et `trace`.

```text
RUST_LOG=info
RUST_LOG=info,ferrisgit_api=debug
RUST_LOG=warn,sqlx=error
```

## Image Docker et variables

L'image définit `STATIC_DIR=/app/static` et lance `./ferrisgit-api` depuis `/app`, avec l'utilisateur `ferrisgit`. Elle expose le port 8080. Tout le reste est à fournir par vous : voir [Installation](/docs/administration/installation) pour Compose et [Déploiement sur Kubernetes](/docs/administration/deploiement-kubernetes) pour un cluster.
