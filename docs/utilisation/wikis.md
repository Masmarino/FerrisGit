# Wikis

Chaque dépôt peut avoir un wiki pour sa documentation : guides, décisions, procédures. Le wiki est stocké dans un dépôt Git à part, ce qui donne deux façons de l'éditer : dans l'interface, ou avec Git.

## Où il se trouve

Le wiki est dans l'entrée **Wiki** du menu du dépôt. Il n'existe pas tant que personne n'a écrit de page : il est créé au premier enregistrement (ou à la première poussée), pas à la création du dépôt. D'ici là, la page affiche « Aucune page pour l'instant ».

Son dépôt Git a la même adresse que le dépôt, avec `.wiki.git` à la place de `.git` :

```bash
git clone https://ferrisgit.example.com/mon-groupe/mon-depot.wiki.git
```

L'authentification est la même que pour le dépôt : votre nom d'utilisateur et un jeton Git (voir [Cloner et pousser](/docs/utilisation/cloner-et-pousser)). Les modifications se font sur la branche `main`.

## Qui peut faire quoi

| Action | Rôle minimum |
|---|---|
| Lire les pages et leur historique | Lecteur |
| Créer ou modifier une page, pousser avec Git | Contributeur |
| Supprimer une page depuis l'interface | Mainteneur (ou propriétaire d'un dépôt personnel) |

Sur un dépôt public, tout utilisateur connecté est Lecteur. De plus, le dépôt Git du wiki d'un dépôt public se clone sans authentification, comme le dépôt lui-même, tant que les pages publiques sont activées sur l'instance ; sinon, le clonage anonyme est refusé et il faut un jeton. Les visiteurs anonymes n'ont en revanche pas accès aux pages du wiki dans l'interface (voir [Pages publiques](/docs/utilisation/pages-publiques)).

> **Attention** : si le contenu de votre wiki ne doit pas être lisible par tous, ne le mettez pas sur un dépôt public.

## Les pages

Chaque page est un fichier Markdown à la racine du dépôt du wiki : la page `Getting-Started` est le fichier `Getting-Started.md`. Le nom d'une page (son « slug »), qui devient son adresse :

- ne contient que des lettres non accentuées, des chiffres, `-` et `_` ;
- ne commence pas par `-` ;
- compte au plus 100 caractères.

Le titre affiché est le nom où les tirets sont remplacés par des espaces : `Getting-Started` s'affiche « Getting Started ». Il n'y a pas de page d'accueil imposée, pas de sous-dossiers (les fichiers dans un dossier sont ignorés), et seuls les fichiers `.md` de la racine dont le nom respecte la règle ci-dessus apparaissent. Dans l'interface, la liste des pages est triée par ordre alphabétique, avec un champ **Rechercher une page** (sur le titre et le nom).

## Éditer dans l'interface

- **Nouvelle page** : saisissez le **Nom de la page** (par exemple `Getting-Started`), écrivez en Markdown. Un aperçu s'affiche à droite, en direct. Cliquez sur **Créer la page**. Si une page porte déjà ce nom, l'interface refuse.
- **Modifier** (en haut d'une page) : mêmes champs, puis **Enregistrer**.
- Le **Message de commit** est facultatif. Sans lui, le message est « Update nom-de-la-page ».

Chaque enregistrement est un commit Git signé de votre nom d'utilisateur et de votre e-mail. Il n'y a pas de renommage de page ni d'envoi d'images dans l'interface.

### Conflits d'édition

Quand vous ouvrez une page, FerrisGit retient l'état du wiki entier. Si quelqu'un enregistre n'importe quelle page entre-temps, votre enregistrement est refusé : « Quelqu'un d'autre a modifié cette page depuis que vous l'avez ouverte. Rechargez et réappliquez vos changements. » Votre texte reste à l'écran : copiez-le, rechargez, puis réappliquez vos changements.

## Historique

**Historique** (en haut d'une page) liste les révisions : message, auteur, date, empreinte courte du commit, la plus récente étant marquée « Version actuelle ». **Voir cette version** affiche le contenu de la page à cette date. L'interface ne propose pas de restauration : copiez le texte voulu dans l'éditeur. La page elle-même montre aussi les trois révisions les plus récentes dans son panneau latéral.

La suppression d'une page est une révision comme les autres (« Delete nom »), mais elle ne figure pas dans l'historique de cette page.

## Supprimer une page

**Supprimer**, réservé aux Mainteneurs, demande de retaper le titre de la page. La page disparaît du wiki, mais reste dans l'historique Git du dépôt du wiki.

## Éditer avec Git

Clonez le dépôt du wiki, ajoutez ou modifiez des fichiers `.md` à la racine, et poussez sur `main`. Les modifications apparaissent dans l'interface. Un Contributeur peut tout y faire, y compris supprimer des fichiers, sans la restriction de rôle de l'interface.

Si la toute première poussée a lieu sur une autre branche que `main` et qu'elle est la seule branche du wiki, FerrisGit la renomme en `main` pour que le wiki s'affiche. Avec plusieurs branches, rien n'est modifié : poussez sur `main`.

Les routes de l'API sont décrites dans [Releases et wikis](/docs/api/releases-et-wikis).
