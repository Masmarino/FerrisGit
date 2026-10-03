# Ressources

Cette page donne les chiffres que le site annonce, la façon de les mesurer chez vous, et ce qu'ils ne disent pas.

## Ce qui a été mesuré

Sur une instance **vide** (un seul compte, aucun dépôt, aucune pipeline) et **au repos** :

| Mesure | Valeur |
|---|---|
| Mémoire du serveur FerrisGit (`docker stats`) | 3,3 Mio |
| Mémoire résidente du serveur (`ps`, binaire compris) | 11 Mio |
| Mémoire de PostgreSQL | 66 Mio |
| Taille de l'image Docker du serveur | 121 Mo |
| Taille du binaire `ferrisgit-api` | 25 Mo |
| Premier démarrage, migrations comprises, jusqu'à la première réponse `200` sur `/health` | 1,0 s |
| Redémarrage du serveur, jusqu'à la première réponse `200` | 0,26 s |

Conditions : version 0.1.2, image construite depuis les sources avec `docker build .`, PostgreSQL 18 (image `postgres:18-alpine3.24`, celle du fichier `docker-compose.yml`), Docker Desktop sur un Mac Apple silicon (arm64). Après 300 requêtes sur `/api/auth/config`, la mémoire du serveur passait de 3,3 à 3,4 Mio.

## Le mesurer chez vous

Avec l'instance lancée par [Docker Compose](/docs/administration/installation) :

```bash
docker compose up -d
docker stats --no-stream
```

La colonne `MEM USAGE` donne la mémoire de chaque conteneur à cet instant. Le « limit » qui suit est la mémoire que Docker met à disposition de l'ensemble, pas celle que FerrisGit utilise.

Pourquoi deux chiffres pour le serveur : `docker stats` retire de la mémoire utilisée le cache de fichiers, dont font partie les pages du binaire en cours d'exécution, alors que `ps` compte tout ce qui est résident, ces pages comprises. Le premier est celui que Docker affiche, le second celui que verrait `top` ; le site annonce le total serveur et base de données, environ 70 Mio.

Pour le délai de démarrage, interrogez `/health` jusqu'à la première réponse `200` depuis le lancement du conteneur :

```bash
docker compose up -d postgres
docker compose up -d ferrisgit-server
until [ "$(curl -s -o /dev/null -w '%{http_code}' http://localhost:8080/health)" = 200 ]; do sleep 0.02; done
```

Lancez la boucle juste après le deuxième `up` et chronométrez-la. Elle inclut le temps de `docker compose`, donc elle majore le démarrage du serveur.

## Ce que ces chiffres ne disent pas

- **C'est une instance vide, au repos.** Elle ne dit rien d'une instance qui travaille. La mémoire monte avec l'usage : chaque clone ou push lance un processus `git`, et PostgreSQL garde en cache ce qu'il lit.
- **Les jobs de CI ne sont pas comptés.** Un job tourne dans un conteneur Docker (runner) ou dans un Pod Kubernetes, hors du serveur : ses ressources sont celles de votre image et de votre commande.
- **Ce n'est pas un dimensionnement.** Pour savoir ce qu'il faut à votre équipe, mesurez avec votre usage réel.
- **Il n'y a pas de comparaison avec d'autres outils.** Les chiffres sont ceux de FerrisGit, mesurés comme ci-dessus ; comparez avec l'outil que vous utilisez, dans les mêmes conditions.
