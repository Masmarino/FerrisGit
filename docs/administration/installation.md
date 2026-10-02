# Installation

Cette page installe FerrisGit sur un serveur avec Docker Compose : le serveur FerrisGit et sa base PostgreSQL, un compte administrateur initial, puis un accès en HTTPS. Pour un cluster Kubernetes, voyez [Déploiement sur Kubernetes](/docs/administration/deploiement-kubernetes).

## Ce qu'il vous faut

- Docker avec le plugin Compose.
- Un nom de domaine et un certificat TLS si l'instance doit être joignable par d'autres personnes. Sans HTTPS, les jetons Git et les sessions circulent en clair.
- Un serveur SMTP, si vous voulez que FerrisGit envoie les invitations et les alertes de sécurité par e-mail. Il se configure plus tard, dans l'interface (voir [Réglages de l'instance](/docs/administration/reglages)).

FerrisGit est un seul processus (`ferrisgit-api`) qui sert l'API, l'application web et le protocole Git sur HTTP, sur un seul port (8080). Il stocke ses données dans PostgreSQL 18 et sur un répertoire local (`STORAGE_ROOT`) qui contient les dépôts Git, les wikis et les fichiers joints aux releases.

> **Note** : le fichier `docker-compose.yml` du dépôt se décrit lui-même comme la pile de développement et de test. Cette page indique les changements à y apporter pour un usage durable.

## Récupérer les fichiers

Le plus simple est de cloner le dépôt : il contient `docker-compose.yml`, le `Dockerfile` et un modèle de configuration.

```bash
git clone https://github.com/Masmarino/FerrisGit.git
cd FerrisGit
cp .env.example .env
```

Par défaut, `docker compose` construit l'image à partir des sources (`build: .`). Pour utiliser plutôt l'image publiée, voyez la section « Utiliser l'image publiée », plus bas.

## Renseigner le fichier .env

Compose lit le fichier `.env` placé à côté de `docker-compose.yml`. Les variables minimales sont les suivantes.

| Variable | Rôle | Contrainte |
|---|---|---|
| `POSTGRES_PASSWORD` | Mot de passe de la base, utilisé par PostgreSQL et par la chaîne de connexion de FerrisGit. | Obligatoire. Évitez les caractères spéciaux (`@`, `:`, `/`, `?`, `#`) : la valeur est insérée telle quelle dans une URL. |
| `JWT_SECRET` | Clé de signature des sessions. | Obligatoire, 32 caractères au minimum. |
| `SETTINGS_ENCRYPTION_KEY` | Clé de chiffrement des secrets stockés. | Obligatoire, **exactement** 32 octets. |
| `PUBLIC_URL` | L'adresse par laquelle les utilisateurs atteignent FerrisGit. | Compose la remplace par `http://localhost:8080` si elle est absente. En production, mettez votre adresse HTTPS. |
| `FERRISGIT_BOOTSTRAP_ADMIN_USERNAME` | Nom du premier administrateur. | Voir plus bas. |
| `FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD` | Mot de passe du premier administrateur. | 8 caractères au minimum. |

Pour générer des valeurs aléatoires :

```bash
openssl rand -hex 24      # POSTGRES_PASSWORD
openssl rand -base64 48   # JWT_SECRET (64 caractères)
openssl rand -base64 24   # SETTINGS_ENCRYPTION_KEY (exactement 32 caractères)
```

`SETTINGS_ENCRYPTION_KEY` est utilisée telle quelle, caractère par caractère : ce n'est ni du base64 ni de l'hexadécimal décodé. Une chaîne ASCII de 32 caractères convient. La commande `openssl rand -base64 24` en produit une de 32 caractères.

> **Attention** : les valeurs de `.env.example` (`change-me…`) respectent les longueurs exigées. Le serveur démarre donc avec elles sans protester. Remplacez-les toutes avant le premier démarrage. Conservez `SETTINGS_ENCRYPTION_KEY` et `JWT_SECRET` dans un endroit sûr : la clé de chiffrement ne peut pas être changée après coup (voir [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour)).

La liste complète des variables, avec leurs valeurs par défaut et leurs validations, est dans [Configuration](/docs/administration/configuration).

## Démarrer

```bash
docker compose up -d --build
docker compose logs -f ferrisgit-server
```

Le service `ferrisgit-server` attend que PostgreSQL réponde (`pg_isready`), puis :

1. se connecte à la base et applique les migrations automatiquement ;
2. crée le compte administrateur initial si la table des utilisateurs est vide ;
3. écoute sur `0.0.0.0:8080` et affiche `listening on 0.0.0.0:8080`.

Si une variable obligatoire manque ou est invalide, le serveur s'arrête au démarrage avec un message explicite. Les messages sont listés dans [Configuration](/docs/administration/configuration).

Le volume `ferrisgit-postgres-data` contient la base et le volume `ferrisgit-storage` (monté sur `/data`) contient les dépôts. Ils survivent à `docker compose down`. Ne les supprimez pas avec `down -v`.

> **Attention** : en plus de `docker-compose.yml`, `docker compose` charge automatiquement `docker-compose.override.yml`. Ce fichier publie PostgreSQL sur le port 5435 de la machine. Pour un serveur, supprimez-le ou lancez `docker compose -f docker-compose.yml up -d`, afin que la base ne soit pas exposée.

## Le compte administrateur initial

Au démarrage, si `FERRISGIT_BOOTSTRAP_ADMIN_USERNAME` et `FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD` sont renseignées et que la base ne contient **aucun utilisateur**, FerrisGit crée un compte administrateur (super-administrateur) avec ce nom et ce mot de passe.

- Sans ces deux variables, il n'existe aucun moyen de créer le premier compte.
- Le mot de passe doit faire au moins 8 caractères. Sinon, FerrisGit ne crée rien et écrit dans son journal `bootstrap admin password is shorter than 8 characters, skipping bootstrap`. Le démarrage continue quand même.
- Le compte n'est créé qu'une fois : les variables peuvent rester définies lors des redémarrages suivants, elles sont alors ignorées. Une fois connecté et le mot de passe changé, retirez-les de votre configuration.
- L'adresse e-mail du compte est `<nom>@localhost`. Elle n'est pas distribuable : FerrisGit n'enverra aucun e-mail à ce compte. Renseignez votre vraie adresse dans **Mon compte**, section **Profil**, champ **Email**.
- La création n'applique pas les règles de nom des autres comptes (3 à 32 caractères, lettres minuscules, chiffres, `-` et `_`, commençant par une lettre). Suivez-les quand même : le nom est cherché en minuscules à la connexion.

## Première connexion et double authentification

Ouvrez `PUBLIC_URL` dans un navigateur, puis connectez-vous (**Connexion**) avec le nom et le mot de passe du compte initial.

La double authentification est obligatoire pour tous les comptes, administrateurs compris. Un compte qui n'a encore aucun second facteur est amené à l'écran **Protégez votre compte** avant de pouvoir faire quoi que ce soit d'autre :

1. choisissez **Clé d'accès** (recommandé) ou **Application d'authentification** ;
2. avec une application, scannez le QR code et saisissez le code à 6 chiffres ; avec une clé d'accès, validez sur votre appareil ;
3. enregistrez les **codes de secours** affichés, puis cliquez sur **J'ai enregistré mes codes de secours**. Ils ne s'afficheront plus ; chacun ne sert qu'une fois.

Les clés d'accès ne sont proposées que si `PUBLIC_URL` s'y prête (voir plus bas). Dans le cas contraire, l'application d'authentification reste disponible.

Si vous perdez le second facteur et les codes de secours du seul administrateur, suivez la procédure de [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour).

Vous pouvez ensuite [inviter les autres utilisateurs](/docs/administration/utilisateurs). Pour le point de vue d'un utilisateur, voyez [Prise en main](/docs/demarrer/prise-en-main).

## Vérifier l'état

```bash
curl -i http://localhost:8080/health
```

La route `GET /health` répond `200` avec le texte `ok` tant que le processus tourne. Elle ne vérifie ni la base ni le disque. `GET /healthz` est identique ; `GET /readyz` répond `200` avec `ok` si la base répond à une requête simple dans les 2 secondes, et `503` sinon, sans détail. Ces trois routes sont anonymes et sans limitation de débit. Pour l'état de la base de données, l'espace disque de `STORAGE_ROOT` et la durée de fonctionnement, ouvrez **Admin**, puis **Santé**.

## Reverse proxy et HTTPS

FerrisGit ne gère pas TLS lui-même. Placez-le derrière un reverse proxy qui termine HTTPS (Caddy, nginx, Traefik…), et faites trois choses.

**1. Définir `PUBLIC_URL`.** C'est l'origine exacte que les navigateurs utilisent : schéma, hôte et port éventuel, sans chemin. Par exemple `https://git.example.com`. FerrisGit s'en sert pour :

- construire les liens d'activation (`/activate#token=…`) et de réinitialisation de mot de passe (`/reset-password#token=…`) envoyés par e-mail ;
- lier les clés d'accès (passkeys) à l'origine : une clé enregistrée pour `https://git.example.com` ne fonctionne pas sur une autre adresse.

Les clés d'accès sont indisponibles (les routes répondent 503, la connexion par application d'authentification continue) quand `PUBLIC_URL` est en `http://` sur un hôte autre que `localhost`, ou quand l'hôte est une adresse IP. Changer `PUBLIC_URL` plus tard rend inutilisables les clés d'accès déjà enregistrées.

**2. Déclarer le proxy dans `TRUSTED_PROXY_CIDRS`.** Les limites de débit (connexion, inscription, activation, pages publiques) sont comptées par adresse IP. Derrière un proxy, toutes les requêtes semblent venir de lui : sans réglage, toute l'instance partage un seul quota. Indiquez dans `TRUSTED_PROXY_CIDRS` les adresses réseau (séparées par des virgules, par exemple `172.18.0.1/32`) depuis lesquelles votre proxy se connecte à FerrisGit. Pour une connexion venant de ces adresses, FerrisGit lit l'en-tête `X-Forwarded-For` ; pour toute autre, il l'ignore. N'indiquez jamais un réseau depuis lequel des clients non fiables peuvent se connecter. Votre proxy doit ajouter l'adresse du client à `X-Forwarded-For`. Détails dans [Configuration](/docs/administration/configuration).

**3. Autoriser les gros envois.** Un `git push` peut peser jusqu'à 600 Mio (la limite réglable est dans [Réglages de l'instance](/docs/administration/reglages)), et un fichier joint à une release jusqu'à 100 Mio. Vérifiez que votre proxy n'impose pas de limite plus basse.

Avec Caddy, qui obtient lui-même le certificat :

```text
git.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

Avec nginx, dans le bloc `server` qui écoute en HTTPS :

```nginx
client_max_body_size 600m;

location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
```

Quand le proxy tourne sur la même machine, publiez le port de FerrisGit sur l'interface locale seulement, dans `docker-compose.yml` :

```yaml
    ports:
      - "127.0.0.1:8080:8080"
```

FerrisGit n'envoie pas l'en-tête `Strict-Transport-Security` : ajoutez-le au niveau du proxy si vous le souhaitez.

Après le changement de `PUBLIC_URL` et de `TRUSTED_PROXY_CIDRS` dans `.env`, relancez : `docker compose up -d`.

## Utiliser l'image publiée

Le projet publie l'image `masmarino/ferrisgit` sur Docker Hub, avec les étiquettes `latest` et `sha-…` pour la branche `main`, et le numéro de version (par exemple `0.1.0`, sans le `v`) pour chaque version. La publication n'a lieu que si les identifiants Docker Hub sont configurés dans le dépôt GitHub du projet. Le workflow ne précise pas de plateforme : l'image est construite pour l'architecture du runner de CI. Sur une autre architecture, construisez l'image vous-même avec le `Dockerfile`.

Pour l'utiliser avec Compose, remplacez dans le service `ferrisgit-server`, la ligne `build: .` par :

```yaml
    image: masmarino/ferrisgit:0.1.1
```

Puis `docker compose pull` et `docker compose up -d`. Épinglez une version plutôt que `latest` : vous maîtrisez ainsi le moment des mises à jour (voir [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour)).

L'image fonctionne avec l'utilisateur `ferrisgit` (uid 100, gid 101), contient `git`, et attend ses données dans `STORAGE_ROOT`. Montez toujours un volume sur ce répertoire et définissez `STORAGE_ROOT` (le fichier Compose utilise `/data`). Avec un répertoire de l'hôte, donnez-le à l'uid 100 : `chown 100:101`.

## Étapes suivantes

- [Réglages de l'instance](/docs/administration/reglages) : SMTP, inscription libre, moteur d'exécution des pipelines.
- [Utilisateurs et invitations](/docs/administration/utilisateurs).
- [Sécurité](/docs/administration/securite) : la liste de contrôle de durcissement.
