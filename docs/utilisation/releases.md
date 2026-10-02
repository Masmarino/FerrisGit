# Releases et tags

Une release publie une version de votre projet : un tag Git, des notes de version et des fichiers à télécharger (binaires, archives, sommes de contrôle). Elles se trouvent dans l'entrée **Releases** du menu du dépôt.

## Qui peut faire quoi

| Action | Rôle minimum |
|---|---|
| Voir les releases publiées, télécharger leurs fichiers | Lecteur |
| Voir les brouillons | Mainteneur |
| Créer, modifier, publier, supprimer une release | Mainteneur (ou propriétaire d'un dépôt personnel) |
| Ajouter ou supprimer un fichier joint | Mainteneur |
| Supprimer un tag sans release | Mainteneur |

Un Contributeur ne peut donc pas publier de release. Les rôles sont détaillés dans [Rôles et permissions](/docs/utilisation/roles-et-permissions).

## Les tags

Un tag est un nom attaché à un commit précis. Pour une release, FerrisGit crée un tag **léger** (sans message ni signature), ou réutilise un tag existant. Les tags poussés avec Git (`git push origin v1.0.0`) sont donc utilisables directement. Tous les tags du dépôt apparaissent dans le sélecteur **Branche ou tag** de l'arborescence.

Le nom d'un tag ne peut pas être vide, ne peut pas contenir `/`, `..`, `@{` ni de caractère de contrôle, ne peut pas commencer par `-` ni se terminer par `.lock`. Il est unique par dépôt, et une seule release porte un tag donné.

FerrisGit ne déplace jamais un tag existant. Si vous demandez une release sur un tag qui pointe vers un autre commit que celui choisi, la création est refusée (« Le tag … existe déjà sur un autre commit. »).

## Créer une release

Cliquez sur **Nouvelle release** :

- **Tag** : choisissez un tag existant, ou **Nouveau tag**. Dans ce cas, donnez son **Nom** (par exemple `v1.0.0`) et la **Branche cible** : le tag sera posé sur le dernier commit de cette branche. Le commit visé s'affiche sous le sélecteur ;
- **Titre** (obligatoire) et **Notes de version** (Markdown) ;
- **Brouillon** : la release n'est visible que des Mainteneurs jusqu'à sa publication ;
- **Pré-version** : signale une version de test, pas encore stable.

Si la création échoue parce que le tag vient d'être créé ailleurs, la boîte de dialogue propose **Supprimer le tag « … » et réessayer**. Le serveur refuse cette suppression si une release utilise déjà ce tag.

## Brouillons, publication, pré-versions

Une release est dans l'un de ces trois états, affichés en badge : **Brouillon**, **Pré-version** ou **Publiée**.

- Un brouillon n'a pas de date de publication. **Publier** (page de la release) la rend visible de tous ceux qui peuvent lire le dépôt et enregistre la date de publication. Cette opération est à sens unique : on ne repasse pas une release en brouillon.
- Si vous créez la release sans cocher **Brouillon**, elle est publiée immédiatement.
- **Modifier** permet de changer le titre, les notes et le drapeau **Pré-version**. Le tag et le commit ne changent pas.

La liste est triée par défaut de la plus récente à la plus ancienne (date de publication, ou de création pour un brouillon), avec une recherche sur le titre et le tag, et une pagination par 20.

## Fichiers joints

Dans la page d'une release, la carte **Fichiers joints** permet à un Mainteneur d'**Ajouter un fichier** (glisser-déposer ou parcourir). Chaque fichier apparaît avec sa taille et son auteur. La taille maximale d'un envoi est de 100 Mio. Les fichiers sont stockés sur le disque du serveur, dans le volume des données (voir [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour)).

L'icône de téléchargement récupère le fichier avec son nom d'origine. L'icône de corbeille le supprime (confirmation demandée). Le panneau latéral affiche le tag, le commit visé, un lien pour parcourir les fichiers à ce tag, l'auteur, les dates, le nombre de fichiers et leur taille totale.

Si le tag Git a été supprimé après coup, la page le signale : « le commit cible n'est plus disponible ».

## Supprimer

**Supprimer** une release (confirmation par retape du tag) supprime ses fichiers joints, mais conserve le tag Git. Pour supprimer aussi le tag, utilisez la route d'API de suppression d'un tag (refusée tant qu'une release l'utilise) ou `git push origin --delete v1.0.0`.

## Visibilité publique

Sur un dépôt public, tant que l'administrateur n'a pas désactivé les pages publiques, les visiteurs sans compte voient :

- les releases **publiées** (les pré-versions comprises) avec leurs notes et leurs fichiers, qu'ils peuvent télécharger ;
- la liste des tags du dépôt.

Les brouillons restent invisibles : une demande de brouillon reçoit la même réponse qu'une release inexistante. En revanche, le tag d'un brouillon est un tag Git ordinaire : il figure dans la liste des tags et dans un clone du dépôt public. N'utilisez pas un nom de tag qui doit rester secret. Sur un dépôt privé, rien de tout cela n'est public. Voir [Pages publiques](/docs/utilisation/pages-publiques).

Les routes sont décrites dans [Releases et wikis](/docs/api/releases-et-wikis).
