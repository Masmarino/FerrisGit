# Rôles et permissions

FerrisGit a trois rôles, du moins au plus puissant : **Lecteur**, **Contributeur** et **Mainteneur**. Chacun inclut les droits du précédent. Un rôle s'accorde sur un dépôt (on parle alors de collaborateur) ou sur un groupe (on parle de membre).

## Ce que permet chaque rôle sur un dépôt

| Action | Lecteur | Contributeur | Mainteneur |
|---|---|---|---|
| Parcourir les fichiers, les commits, les branches, les tags | oui | oui | oui |
| Cloner et récupérer (`git clone`, `git fetch`) | oui | oui | oui |
| Lire tickets, demandes de fusion, pipelines, wiki, releases publiées | oui | oui | oui |
| Étoiler le dépôt | oui | oui | oui |
| Pousser (`git push`), y compris sur le wiki | non | oui | oui |
| Créer des tickets, les commenter, les modifier, les assigner, les fermer, déplacer les cartes du kanban | non | oui | oui |
| Créer, modifier et fermer une demande de fusion, la commenter, la relire, l'approuver, appliquer une suggestion | non | oui | oui |
| Annuler une pipeline | non | oui | oui |
| Créer et modifier étiquettes et jalons | non | oui | oui |
| Modifier une page du wiki | non | oui | oui |
| **Fusionner** une demande de fusion | non | non | oui |
| Supprimer une page du wiki | non | non | oui |
| Créer, modifier et supprimer des releases, y compris leurs fichiers joints ; supprimer un tag ; voir les brouillons | non | non | oui |
| Réglages du dépôt : pipeline, approbations requises, variables CI/CD | non | non | oui |
| Webhooks | non | non | oui |
| Gérer les collaborateurs | non | non | oui |
| Supprimer le dépôt | non | non | oui |

Quelques précisions :

- L'onglet **Réglages** du dépôt n'apparaît que pour les propriétaires et les Mainteneurs.
- Un Lecteur lit les tickets et leurs commentaires, mais ne peut ni ouvrir un ticket ni commenter : l'interface et l'API demandent toutes deux le rôle Contributeur. Sur un dépôt public, c'est aussi le cas des utilisateurs connectés qui n'ont pas de rôle.
- Une personne ne peut être assignée à un ticket que si elle est au moins Contributeur sur le dépôt, directement ou par un groupe.
- L'interface n'affiche que les boutons que le serveur accepterait : par exemple, **Fusionner** n'apparaît que pour les propriétaires et les Mainteneurs.
- FerrisGit n'a pas de branches protégées. Un Contributeur peut pousser sur n'importe quelle branche, y compris la branche principale. Pour imposer une relecture, réglez **Approbations requises avant fusion** et ne donnez le rôle Mainteneur qu'aux personnes qui fusionnent. Cela encadre les fusions faites dans FerrisGit, pas les pushs directs.

## Le propriétaire d'un dépôt personnel

Sur un dépôt personnel, la personne qui l'a créé est « Propriétaire ». Elle a tous les droits d'un Mainteneur, sans qu'on ait à les lui accorder, et ne peut pas être ajoutée comme collaborateur de son propre dépôt.

Sur un dépôt de groupe, la personne qui l'a créé n'a aucun droit de plus : seuls comptent les rôles du groupe et ceux accordés directement sur le dépôt.

## Dépôts publics

Un dépôt public donne le rôle Lecteur à tout utilisateur connecté de l'instance, sans qu'il soit collaborateur. Les visiteurs sans compte peuvent aussi le lire si les [pages publiques](/docs/utilisation/pages-publiques) sont activées. La liste des collaborateurs d'un dépôt public reste visible des seuls membres qui ont un rôle explicite.

## Rôles de groupe

Un rôle donné sur un groupe vaut pour le groupe, ses sous-groupes et tous leurs dépôts.

| Action | Lecteur | Contributeur | Mainteneur |
|---|---|---|---|
| Voir le groupe, ses sous-groupes, ses dépôts et ses membres | oui | oui | oui |
| Droits sur les dépôts du groupe | ceux d'un Lecteur | ceux d'un Contributeur | ceux d'un Mainteneur |
| Créer des étiquettes et des jalons au niveau du groupe | non | oui | oui |
| Créer un sous-groupe ou un dépôt dans le groupe | non | non | oui |
| Ajouter, retirer, changer le rôle d'un membre | non | non | oui |
| Gérer les collaborateurs des dépôts du groupe | non | non | oui |
| Supprimer le groupe (s'il est vide) | non | non | oui |

### Règles de calcul

- Le rôle effectif est **le plus élevé** de tous ceux que vous avez : sur le groupe, sur ses groupes parents, et directement sur le dépôt.
- Un rôle plus bas sur un sous-groupe ne réduit jamais un rôle hérité d'un groupe parent.
- Un groupe garde toujours au moins un Mainteneur dans sa hiérarchie : le dernier ne peut être ni retiré ni rétrogradé.
- Quand vous créez un groupe, vous en devenez Mainteneur.

## Donner un rôle

### Sur un dépôt

1. Ouvrez le dépôt, puis **Réglages**, puis **Collaborateurs**.
2. Dans **Ajouter un collaborateur**, saisissez le nom d'utilisateur (il doit déjà avoir un compte), choisissez le rôle (Contributeur par défaut) et validez.
3. Pour changer un rôle, utilisez la liste déroulante de la ligne. **Retirer** (icône corbeille) supprime l'accès, sauf si le dépôt est public ou si la personne a un rôle par un groupe.

La personne reçoit une notification dans l'application quand elle est ajoutée, quand son rôle change et quand elle est retirée. Ces changements déclenchent aussi les [webhooks](/docs/utilisation/webhooks) du dépôt.

Pour un dépôt de groupe, le plus simple est de gérer les accès par les membres du groupe, ce qui couvre aussi tous les dépôts qu'il contient.

### Sur un groupe

Ouvrez le groupe, puis **Membres**, et utilisez **Ajouter un membre** (nom d'utilisateur et rôle). Voir [Dépôts et groupes](/docs/utilisation/depots-et-groupes#groupes).

## Administrateurs

Le rôle de **Super-administrateur** gère l'instance (utilisateurs, réglages, runners), mais ne donne aucun accès au contenu des dépôts : un administrateur lit un dépôt privé seulement s'il y a un rôle comme n'importe qui. Voir [Utilisateurs et invitations](/docs/administration/utilisateurs).
