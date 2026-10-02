# Pages publiques

Les pages publiques permettent à quelqu'un qui n'a pas de compte de **lire** les dépôts publics de l'instance : parcourir le catalogue, le code, les commits et les releases, et télécharger les fichiers des releases. Tout est en lecture seule.

La fonctionnalité est active par défaut. Un administrateur peut la couper, et décider séparément si les moteurs de recherche ont le droit d'indexer ces pages.

## Ce que voient les visiteurs

Les visiteurs sans session utilisent les mêmes adresses que les utilisateurs connectés. Une personne connectée qui ouvre ces adresses garde l'interface habituelle.

| Adresse | Contenu |
|---|---|
| `/` et `/explore` | Le catalogue des dépôts publics : recherche par nom, chemin ou description (100 caractères au plus), tri **Populaires**, **Nom** ou **Récents**, 20 dépôts par page. Chaque carte montre le chemin, la description, le nombre d'étoiles et la date de création. |
| `/repositories/<chemin>` | L'**Aperçu** du dépôt : README, fichiers, langages, contributeurs avec leur nombre de commits, sélecteur de branche. |
| `/repositories/<chemin>/-/tree/<branche>/...` et `.../-/blob/<branche>/...` | Les dossiers et le contenu des fichiers. Un fichier binaire, ou de plus de 1 Mio, n'est pas affiché. |
| `/repositories/<chemin>/-/commits` | La liste des commits. |
| `/repositories/<chemin>/-/releases` et `.../-/releases/<tag>` | Les releases publiées, leurs notes, et le téléchargement de leurs fichiers joints. |

Le bouton **Se connecter** de l'en-tête ramène le visiteur sur la page qu'il consultait une fois connecté. Pour un dépôt de groupe, `<chemin>` contient le chemin du groupe : `/repositories/acme/backend/api`.

Les visiteurs peuvent aussi **cloner** un dépôt public avec Git, sans identifiants, y compris son wiki (`.wiki.git`). Ce droit suit le réglage « Pages publiques » : une fois les pages publiques coupées, le clonage anonyme est refusé comme pour un dépôt privé. Les utilisateurs connectés, eux, clonent toujours avec leur nom et un jeton. Voir [Cloner et pousser](/docs/utilisation/cloner-et-pousser).

## Ce qui n'est pas exposé

- Les tickets, les demandes de fusion, les pipelines, les wikis (dans l'interface web) et les réglages du dépôt.
- Les collaborateurs et les membres des groupes. Une page de groupe n'est jamais publique : seul le chemin d'un dépôt public se résout.
- Les releases en brouillon.
- Les dépôts privés, et tout ce qui nécessite un compte (étoiler un dépôt, par exemple).

Un dépôt privé, un dépôt qui n'existe pas et un dépôt public dont les pages publiques sont désactivées répondent exactement de la même façon : la page « Ce dépôt n'existe pas ou n'est pas public » sur le web, et une demande d'authentification (401) pour Git. Le visiteur ne peut donc pas deviner quels dépôts privés existent.

## Ce qui est tout de même visible

Un dépôt public montre ce qu'un `git clone` anonyme révèle déjà. En particulier :

- Les **noms et adresses e-mail des auteurs de commits** font partie des données de commit renvoyées par l'API publique et lisibles dans l'historique Git. L'interface web affiche les noms, pas les adresses, mais les adresses ne sont pas cachées : l'API publique les renvoie, notamment pour la liste des contributeurs.
- Les **noms de tous les tags**, y compris celui d'une release encore en brouillon, sont listés.
- Les **branches** sont listées.
- Le **chemin** du dépôt révèle les noms des groupes qui le contiennent. Le catalogue renvoie aussi le nom d'utilisateur du propriétaire du dépôt, c'est-à-dire, pour un dépôt de groupe, de la personne qui l'a créé.

> **Attention** : si ces informations posent problème (adresse e-mail personnelle dans vos commits, nom de tag interne), ne rendez pas le dépôt public, ou réécrivez l'historique avant de le publier. La visibilité d'un dépôt se change à tout moment, mais ce qui a été lu entre-temps reste lu : voir [Dépôts et groupes](/docs/utilisation/depots-et-groupes).

## Les deux réglages d'administration

Ils se trouvent sous **Admin**, **Réglages**, onglet **Sécurité**, carte **Pages publiques**. Seul un administrateur peut les modifier ; chaque changement est enregistré dès qu'on bascule l'interrupteur.

| Réglage | Par défaut | Effet |
|---|---|---|
| **Pages publiques** | activé | Active le catalogue, les pages de dépôt publiques, l'API publique en lecture seule (`/api/public/...`) et le clonage Git anonyme des dépôts publics et de leurs wikis. Désactivé, `/` redirige vers la page de connexion, `/explore` affiche « Les pages publiques sont désactivées », l'API publique répond 404 et un `git clone` sans identifiants reçoit la même réponse 401 qu'un dépôt privé. |
| **Référencement par les moteurs de recherche** | désactivé | Autorise les moteurs à indexer les pages publiques. Il faut que les pages publiques soient activées : l'interrupteur est inactif sinon. |

### Effet sur `robots.txt` et `X-Robots-Tag`

L'indexation n'est autorisée que si **les deux** réglages sont activés.

| Situation | `/robots.txt` | En-tête `X-Robots-Tag` |
|---|---|---|
| Indexation non autorisée (par défaut) | `Disallow: /` pour tous les robots | `noindex, nofollow` sur **toutes** les réponses du serveur |
| Indexation autorisée | Seuls `/api/`, `/account` et `/admin/` sont interdits | Absent |

Avec les réglages par défaut, les moteurs de recherche n'indexent donc rien de l'instance. Si la lecture du réglage échoue, le serveur retombe du côté prudent (pas d'indexation).

Le fichier `robots.txt` est une demande faite aux robots : il n'empêche pas une personne ou un robot qui l'ignore de lire les pages.

## Limites

- Les requêtes publiques sont limitées à **120 par minute et par adresse IP**. Au-delà, le visiteur voit « Trop de requêtes en peu de temps » et doit patienter environ une minute. Derrière un proxy, l'administrateur doit déclarer le proxy pour que la limite soit comptée par visiteur ([configuration](/docs/administration/configuration)).
- Les réponses publiques ne sont pas mises en cache (`Cache-Control: no-cache`) : une modification est visible immédiatement.

Pour l'API utilisée par ces pages, voir [l'API publique](/docs/api/api-publique).
