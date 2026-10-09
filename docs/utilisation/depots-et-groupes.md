# Dépôts et groupes

Un dépôt contient votre code. Il est soit rattaché à votre compte (dépôt personnel), soit rangé dans un groupe. Les groupes servent à regrouper des dépôts par équipe ou par projet, et à donner des accès à toute une arborescence d'un coup.

## Adresses

| Type | Chemin | Exemple |
|---|---|---|
| Dépôt personnel | `/<utilisateur>/<dépôt>` | `alice/site-web` |
| Dépôt de groupe | `/<groupe>/<sous-groupe>/.../<dépôt>` | `acme/backend/api` |

Dans l'interface, un dépôt s'ouvre à `/repositories/<chemin>`. Pour Git, l'adresse est le chemin suivi de `.git` (voir [Cloner et pousser](/docs/utilisation/cloner-et-pousser)).

Les noms d'utilisateurs et de groupes de premier niveau partagent le même espace : un groupe racine ne peut pas porter le nom d'un utilisateur existant, et inversement.

## Créer un dépôt

Dans **Dépôts**, cliquez sur **Nouveau dépôt**. Depuis la page d'un groupe, le même bouton propose ce groupe comme emplacement.

| Champ | Détail |
|---|---|
| Emplacement | **Personnel**, ou un groupe dont vous êtes Mainteneur (directement, ou par un groupe parent). |
| Nom du dépôt | Obligatoire. Lettres, chiffres, `-` et `_` uniquement, sans point ni espace. Unique parmi vos dépôts personnels, ou parmi les dépôts du groupe. |
| Description | Facultative. |
| Visibilité | **Privé** ou **Public**. |
| CI activée | Activée par défaut. Une pipeline est lancée à chaque push si le dépôt contient le fichier de pipeline. |
| Approbations requises avant fusion | `0` par défaut. |
| Chemin du fichier pipeline | `.ferrisgit-ci.yml` par défaut. |

Une erreur « Ce nom est déjà utilisé » signale un doublon, « Nom invalide » un caractère interdit.

Le nom d'un dépôt se choisit à la création : il fait partie de l'adresse de clonage et ne se modifie pas ensuite, pas plus que le propriétaire ou le groupe qui l'accueille. Si vous vous trompez de nom, créez un nouveau dépôt, poussez-y le contenu et supprimez l'ancien.

La description et la visibilité, elles, se modifient à tout moment (voir plus bas), de même que les réglages de CI et d'approbations. Tout cela se fait dans l'onglet **Réglages** du dépôt, réservé aux propriétaires et aux Mainteneurs.

## Modifier la description et la visibilité

Ouvrez le dépôt, puis **Réglages**. L'onglet **Informations** (celui qui s'ouvre en premier) contient la carte **Informations** :

- **Description** : texte libre, vide si vous le souhaitez. Elle est enregistrée dès que vous quittez le champ.
- **Visibilité** : **Privé** ou **Public**. Un changement demande toujours une confirmation, qui rappelle la conséquence, puis il s'applique tout de suite.

Rendre un dépôt **public** l'ajoute au catalogue public : toute personne peut alors lire son code, ses commits et ses releases sans compte, tant que les [pages publiques](/docs/utilisation/pages-publiques) de l'instance sont activées, et le cloner sans identifiants. Rendre un dépôt **privé** le retire aussitôt du catalogue et de l'API publique, et les visiteurs sans compte ne peuvent plus le lire ni le cloner : leurs liens cessent de fonctionner. Les collaborateurs et les membres des groupes gardent leur accès.

Les mêmes modifications sont possibles par l'API (`PATCH /api/repositories/by-id/{id}`, voir [l'API des dépôts et groupes](/docs/api/depots)).

## Visibilité : privé ou public

| | Privé | Public |
|---|---|---|
| Qui peut le lire | Le propriétaire (dépôt personnel), les collaborateurs et les membres des groupes parents, selon leur [rôle](/docs/utilisation/roles-et-permissions). | Tout utilisateur connecté de l'instance, en lecture seule. Les visiteurs sans compte aussi, si les [pages publiques](/docs/utilisation/pages-publiques) sont activées. |
| Qui peut écrire | Les rôles Contributeur et Mainteneur. | Idem : être connecté ne suffit pas pour pousser, il faut un rôle. |
| Clone en Git | Avec un jeton Git. | Sans identifiants. |

Un dépôt privé inconnu ou inaccessible répond comme un dépôt qui n'existe pas : rien ne révèle son existence.

## Retrouver ses dépôts

La page **Dépôts** a quatre onglets, avec leur nombre entre parenthèses :

- **Tous** : vos dépôts personnels, ceux où vous êtes collaborateur, ceux de vos groupes, et vos groupes.
- **Mes dépôts** : vos dépôts personnels (rôle « Propriétaire »).
- **Favoris** : les dépôts que vous avez étoilés avec le bouton étoile de l'en-tête du dépôt.
- **Groupes** : les groupes dont vous êtes membre.

Le panneau latéral permet de rechercher par nom ou chemin et de trier par date de création ou par nom. Chaque ligne affiche votre rôle (Propriétaire, Lecteur, Contributeur, Mainteneur) et un menu **Copier le chemin**, **Copier l'URL de clonage**, **Supprimer**.

## Groupes

Un groupe peut contenir des sous-groupes et des dépôts. Un rôle donné sur un groupe s'applique à tous ses sous-groupes et dépôts.

### Créer un groupe

- **Un sous-groupe** : ouvrez le groupe parent, cliquez sur **Nouveau sous-groupe**, donnez un nom (lettres, chiffres, `-` et `_`) et une description facultative. Il faut être Mainteneur du groupe parent ou d'un de ses ancêtres. Le nom doit être unique dans le parent.
- **Un groupe de premier niveau** : dans **Dépôts**, cliquez sur **Nouveau groupe** (à côté de **Nouveau dépôt**), donnez un nom (lettres, chiffres, `-` et `_`) et une description facultative. N'importe quel utilisateur connecté peut le faire. Le nom ne doit être ni celui d'un compte utilisateur, ni celui d'un autre groupe de premier niveau (« Ce nom est déjà utilisé »). Même chose par l'API : `POST /api/groups`, voir [l'API des dépôts et groupes](/docs/api/depots).

Le créateur d'un groupe en devient Mainteneur. Le nom et la description d'un groupe ne se modifient pas après coup.

### Gérer les membres

Sur la page du groupe, le bouton **Membres** (ou le panneau **Membres**) ouvre la liste. Les Mainteneurs y utilisent **Ajouter un membre** : nom d'utilisateur existant et rôle, que l'on change ensuite dans la liste. **Retirer** supprime l'accès au groupe et à ses dépôts privés.

Un groupe garde toujours au moins un Mainteneur : on ne peut pas retirer ni rétrograder le dernier Mainteneur de la hiérarchie (les Mainteneurs des groupes parents comptent).

### Dépôts d'un groupe

Dans un dépôt de groupe, la personne qui l'a créé n'a aucun droit particulier : seuls comptent les rôles du groupe (et ceux accordés directement sur le dépôt). Sur un dépôt personnel, au contraire, le propriétaire a tous les droits d'un Mainteneur.

## Supprimer

- **Un dépôt** : menu **Supprimer** de la liste, réservé au propriétaire et aux Mainteneurs. Il faut retaper le nom du dépôt pour confirmer. Le dépôt, ses tickets, ses demandes de fusion, ses releases et son wiki sont supprimés définitivement.
- **Un groupe** : menu **Supprimer** dans la liste des groupes, réservé aux Mainteneurs. Le groupe doit être vide : supprimez ou videz d'abord ses sous-groupes et ses dépôts.

> **Attention** : il n'y a ni corbeille ni restauration. Pensez à cloner le dépôt avant de le supprimer.
