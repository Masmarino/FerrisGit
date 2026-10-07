# Cloner et pousser

FerrisGit sert Git en HTTP(S). Il n'y a pas d'accès SSH : vous vous authentifiez avec votre nom d'utilisateur et un jeton Git.

## Adresse d'un dépôt

L'adresse est le chemin du dépôt suivi de `.git`, sur l'adresse de l'instance :

| Dépôt | Adresse |
|---|---|
| Personnel | `https://ferrisgit.example.com/alice/site-web.git` |
| Dans un groupe | `https://ferrisgit.example.com/acme/backend/api.git` |
| Wiki du dépôt `alice/site-web` | `https://ferrisgit.example.com/alice/site-web.wiki.git` |

Le bouton **Cloner** de l'en-tête du dépôt amène à l'adresse prête à copier. La liste des dépôts propose aussi **Copier l'URL de clonage**.

## S'authentifier avec un jeton

Git utilise l'authentification HTTP Basic : le **nom d'utilisateur** est le vôtre, le **mot de passe** est un **jeton Git**, c'est-à-dire un jeton d'accès personnel. Votre mot de passe de compte ne fonctionne pas pour Git.

1. Créez un jeton dans **Mon compte**, onglet **Jetons Git**, avec **Générer**. Il commence par `fg_` et n'est affiché qu'une fois.
2. Lancez une commande Git. Quand Git demande les identifiants, saisissez votre nom d'utilisateur puis le jeton.
3. Laissez le gestionnaire d'identifiants de votre système les mémoriser, pour ne pas les retaper.

```bash
git clone https://ferrisgit.example.com/alice/site-web.git
Username for 'https://ferrisgit.example.com': alice
Password for 'https://alice@ferrisgit.example.com': fg_...
```

Ce que le jeton permet exactement :

- Il donne les droits de votre compte sur tous les dépôts auxquels vous avez accès : Lecteur pour cloner, **Contributeur** au minimum pour pousser. Il n'est lié à aucun dépôt en particulier.
- Il n'expire pas. Il vaut jusqu'à sa révocation.
- Il contourne la double authentification : il remplace le mot de passe et le second facteur pour Git. Traitez-le comme un mot de passe.
- FerrisGit reconnaît le compte à partir du jeton lui-même. Le nom d'utilisateur saisi n'est pas comparé : utilisez le vôtre quand même, c'est l'usage et ce que Git mémorise.
- Il ne donne **pas** accès à l'API REST. Voir [Compte et sécurité](/docs/utilisation/compte-et-securite).

> **Attention** : évitez d'écrire le jeton dans l'adresse (`https://alice:fg_...@hôte/...`). Il serait enregistré en clair dans `.git/config` et dans l'historique de votre shell. Dans un script ou une CI, passez-le par une variable secrète et ne le journalisez pas.

## Cloner

```bash
git clone https://ferrisgit.example.com/acme/backend/api.git
```

Un dépôt **public** se clone sans identifiants, même sans compte, tant que les [pages publiques](/docs/utilisation/pages-publiques) sont activées sur l'instance. Si un administrateur les désactive, le clonage anonyme est refusé exactement comme pour un dépôt privé (réponse 401) : il faut alors un jeton. Tout utilisateur connecté peut ensuite cloner un dépôt public, sans y avoir de rôle. Tant que les pages publiques sont activées, des identifiants envoyés sur un dépôt public sont ignorés pour la lecture.

## Pousser un premier projet

Sur un dépôt vide :

```bash
git init -b main
git add .
git commit -m "Premier commit"
git remote add origin https://ferrisgit.example.com/alice/site-web.git
git push -u origin main
```

Pour envoyer un dépôt existant, ajoutez simplement le remote et poussez :

```bash
git remote add origin https://ferrisgit.example.com/alice/site-web.git
git push -u origin main
```

> **Note** : lors du tout premier push, envoyez une seule branche (par exemple `main`). FerrisGit fait alors de cette branche la branche par défaut du dépôt. Si vous poussez plusieurs branches d'un coup, il ne peut pas deviner laquelle choisir : si aucune ne porte le nom de branche par défaut créé avec le dépôt, l'interface montre le dépôt comme vide. Poussez donc une seule branche au départ.

Il n'y a pas de branche protégée : tout Contributeur peut pousser sur n'importe quelle branche.

### Ce qui se passe après un push

- Si la CI du dépôt est activée et que le fichier de pipeline existe, une pipeline démarre. Voir [Premiers pas CI/CD](/docs/ci-cd/premiers-pas).
- Les demandes de fusion ouvertes dont la branche source a bougé enregistrent les nouveaux commits dans leur chronologie.

## Wikis

Le wiki d'un dépôt est un dépôt Git à part, à l'adresse `<dépôt>.wiki.git`. Il s'utilise comme un dépôt normal, avec les mêmes identifiants :

```bash
git clone https://ferrisgit.example.com/alice/site-web.wiki.git
```

- Il n'existe qu'une fois une première page créée (depuis l'onglet **Wiki**) ou un premier push. Cloner un wiki qui n'existe pas encore échoue comme pour un dépôt introuvable.
- Pousser demande le rôle Contributeur, comme pour le dépôt. Cloner demande le rôle Lecteur, et un wiki de dépôt public se clone sans identifiants tant que les pages publiques sont activées, comme le dépôt.
- La branche du wiki s'appelle `main`. Si vous poussez une seule branche d'un autre nom lors du premier push, FerrisGit la renomme en `main`.

Voir [Wikis](/docs/utilisation/wikis).

## Taille maximale d'un push

Le serveur refuse un push dont les données dépassent la limite configurée par l'administrateur, avec une erreur HTTP 413.

- La valeur par défaut est de **500 Mio**.
- Elle se règle sous **Admin**, **Réglages**, **Sécurité**, carte **Pushs** (« Taille maximale d'un push »), de 1 à 600 Mio.
- Il existe en plus un plafond de 600 Mio que rien ne permet de dépasser.
- La limite porte sur la quantité de données envoyées par un push (l'historique compressé, pas la taille de la copie de travail), pas sur la taille totale du dépôt. Elle ne gêne en pratique ni les clones ni les fetch.

Si votre push est refusé, envoyez l'historique en plusieurs fois, en poussant d'abord un ancien commit :

```bash
git push origin <sha-d-un-ancien-commit>:refs/heads/main
git push origin main
```

Évitez aussi de versionner des archives, des binaires ou des dépendances. Si c'est un besoin légitime, demandez à l'administrateur de relever la limite.

## Erreurs courantes

| Message de Git | Causes possibles |
|---|---|
| `fatal: Authentication failed` ou `HTTP 401` | Mot de passe de compte saisi à la place du jeton. Jeton incomplet, mal copié ou révoqué. Identifiants périmés dans votre gestionnaire d'identifiants. Chemin du dépôt faux. Dépôt privé auquel vous n'avez pas accès. Dépôt public dont l'instance a désactivé les pages publiques, cloné sans jeton. Vous poussez avec le rôle Lecteur. |
| `fatal: could not read Username` | Git ne peut pas poser la question (script, CI) : fournissez les identifiants autrement. |
| `HTTP 413` ou `RPC failed` pendant un push | Push plus gros que la limite. Voir ci-dessus. |
| `HTTP 500` | Erreur côté serveur. Réessayez, puis prévenez l'administrateur. |
| `repository '...' not found` en clonant un `.wiki.git` | Le wiki n'a pas encore de page. |

Le serveur répond **401 dans tous les cas** d'échec d'accès : jeton invalide, dépôt inexistant, dépôt privé sans droit, droits insuffisants pour pousser. C'est voulu, afin de ne pas révéler quels dépôts privés existent. Quand un 401 vous surprend, vérifiez dans cet ordre :

1. Le chemin du dépôt (une faute de frappe donne aussi un 401).
2. Que vous utilisez un jeton et non votre mot de passe, et qu'il figure encore dans la liste **Jetons actifs** de **Mon compte**.
3. Votre rôle sur le dépôt, dans la liste **Dépôts** : il faut au moins Contributeur pour pousser.
4. Les identifiants mémorisés : supprimez l'ancienne entrée de votre gestionnaire d'identifiants pour que Git redemande le jeton.
