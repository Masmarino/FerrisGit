# Demandes de fusion

Une demande de fusion propose d'intégrer une branche (la source) dans une autre (la cible, en général la branche par défaut). Elle sert à relire le code, à en discuter ligne par ligne, puis à fusionner. Elle est accessible depuis l'entrée **Demandes de fusion** du menu du dépôt.

## Qui peut faire quoi

| Action | Rôle minimum |
|---|---|
| Voir la liste, la demande, ses commentaires, ses revues | Lecteur |
| Créer une demande, commenter, répondre, résoudre un fil | Contributeur |
| Approuver ou demander des changements | Contributeur |
| Appliquer une suggestion, fermer une demande | Contributeur |
| Modifier les étiquettes et le jalon | Contributeur |
| Fusionner | Mainteneur (ou propriétaire d'un dépôt personnel) |

Un Contributeur peut fermer ou commenter n'importe quelle demande, pas seulement les siennes. Sur un dépôt public, tout utilisateur connecté est Lecteur : il lit la demande, mais ne peut pas la commenter. Les rôles sont détaillés dans [Rôles et permissions](/docs/utilisation/roles-et-permissions).

L'interface n'affiche le bouton **Fusionner** qu'aux propriétaires et aux Mainteneurs. Un Contributeur voit l'état des approbations et peut approuver, commenter ou fermer, mais la fusion revient à un Mainteneur.

## Créer une demande de fusion

Dans la liste, cliquez sur **Nouvelle demande de fusion** et remplissez :

- **Branche source** et **Branche cible** : toutes deux doivent exister et être différentes. La cible est préremplie avec la branche par défaut.
- **Titre** (obligatoire) et **Description** (facultative).

La branche source doit déjà avoir été poussée (voir [Cloner et pousser](/docs/utilisation/cloner-et-pousser)). La demande affiche les changements entre le point de départ commun des deux branches et la pointe de la source : les commits que la cible a ajoutés de son côté n'apparaissent pas.

La liste propose trois onglets (**Ouvertes**, **Fusionnées**, **Fermées**, avec leur nombre), une recherche sur le titre, un tri par date de création ou par titre, et des filtres par étiquette et par jalon. Elle est paginée par 25.

Le titre et la description ne se modifient pas dans l'interface actuelle (seule l'API le permet). En revanche, les étiquettes et le jalon se changent dans le panneau latéral de la vue d'ensemble.

## Lire et relire les changements

La page d'une demande a deux onglets :

- **Vue d'ensemble** : la description, la chronologie, le champ pour ajouter un commentaire, et un panneau latéral (**Approbations**, **Labels**, **Milestone**, **Auteur**, **Participants**).
- **Modifications** : le diff fichier par fichier, avec l'ancienne version à gauche et la nouvelle à droite. Un fichier ajouté ou supprimé n'affiche qu'une colonne. Les fichiers binaires sont signalés « Fichier binaire modifié. ».

Une barre au-dessus du diff rappelle l'état des revues et reprend les boutons **Approuver** et **Demander des changements**.

## Commentaires et fils de discussion

### Commentaire général

Dans la vue d'ensemble, écrivez dans **Ajouter un commentaire** puis cliquez sur **Commenter**. Les commentaires s'affichent en texte brut (le Markdown n'est pas interprété).

### Commentaire en ligne

Dans l'onglet **Modifications**, passez sur une ligne : un bouton **+** apparaît dans la marge. Cliquez dessus pour commenter cette ligne. Pour commenter plusieurs lignes, appuyez sur le **+** et faites glisser jusqu'à la dernière ligne, du même côté.

Vous ne pouvez commenter que des lignes présentes dans le diff, et sur l'ancienne ou la nouvelle version. Si la ligne a disparu entre-temps, l'envoi échoue et l'interface vous invite à recharger.

Chaque commentaire en ligne ouvre un **fil**. Les autres personnes répondent avec **Répondre** (un seul niveau : on répond au fil, pas à une réponse). Le fil porte les badges suivants :

- **Ouverte** ou **Résolue** : **Résoudre** marque le fil comme traité, **Rouvrir** annule. Un fil résolu se replie, avec un bouton **Afficher**. Résoudre est un choix humain : un fil non résolu n'empêche pas la fusion.
- **Périmée** : la ligne commentée n'a plus le même contenu dans le diff actuel (une poussée l'a modifiée). Le fil reste lisible, sans extrait de code.

### Suggestions

Sur la **nouvelle** version, le bouton **Proposer un remplacement** ajoute un champ « Contenu de remplacement », prérempli avec les lignes sélectionnées. Modifiez-le et envoyez : le fil porte alors un bloc **Suggestion de modification**.

Un Contributeur peut cliquer sur **Appliquer la suggestion**. FerrisGit crée alors lui-même un commit sur la branche source :

- le message est « Apply suggestion from @nom », avec le nom et l'e-mail de la personne qui applique ;
- les lignes d'origine sont remplacées par le texte de la suggestion, tel quel. Conservez donc le retour à la ligne final, sinon la ligne suivante est collée à la dernière ligne proposée ;
- la suggestion est ensuite marquée « Suggestion appliquée dans » suivi du commit.

L'application échoue si la demande n'est plus ouverte, si la suggestion a déjà été appliquée, si la ligne ciblée a changé depuis (« outdated ») ou si la branche source a bougé pendant l'opération. Rechargez et réessayez. Comme la pointe de la branche change, les approbations déjà données deviennent obsolètes (voir plus bas). Une suggestion ne peut pas porter sur l'ancienne version, et seul le premier message d'un fil peut en contenir une.

## Revues : approbations et demandes de changements

Tout Contributeur peut cliquer sur **Approuver** ou **Demander des changements**. Il n'y a pas de relecteur désigné : n'importe quel Contributeur convient. Chacun n'a qu'une revue par demande, qu'il remplace en en envoyant une nouvelle. L'auteur d'une demande ne peut pas l'approuver lui-même.

Une revue est liée à la pointe de la branche source au moment où elle est donnée. Si de nouveaux commits sont poussés, elle est marquée **obsolète — nouveau commit poussé** et ne compte plus.

Le nombre d'approbations exigées se règle par dépôt, dans **Réglages > Pipeline**, carte **Demandes de fusion**, champ **Approbations requises avant fusion** (0 par défaut, ce qui signifie aucune exigence). Réglé sur 1 ou plus, la fusion est bloquée tant que :

- il n'y a pas assez d'approbations à jour (« N/M approbations requises. ») ;
- ou qu'une demande de changements à jour existe (« Des changements ont été demandés. »).

Avec 0, aucune revue ne bloque, y compris une demande de changements. Le bouton **Fusionner** est alors grisé dans la page de la demande, et le serveur refuse aussi la fusion.

## Fusionner

Le bouton **Fusionner** (page de la demande, ou menu de la ligne dans la liste) n'existe que pour les demandes ouvertes, et seulement pour les propriétaires et les Mainteneurs. FerrisGit n'a qu'une seule stratégie : un **commit de fusion** à deux parents, intitulé « Merge branch 'source' into cible », signé « FerrisGit ». Il n'y a pas d'option de squash, de rebase ni d'avance rapide. La branche source n'est pas supprimée.

Après la fusion, la demande passe à **Fusionnée** et si l'intégration continue est activée sur le dépôt, un pipeline démarre sur le nouveau commit (voir [Premiers pas CI/CD](/docs/ci-cd/premiers-pas)).

### Conflits

Si Git ne peut pas fusionner automatiquement, rien n'est écrit et la page affiche : « Impossible de fusionner automatiquement — mettez à jour la branche et réessayez. » FerrisGit ne résout pas les conflits dans l'interface. Fusionnez ou rebasez la cible dans votre branche en local, résolvez, poussez, puis relancez la fusion.

Si la branche cible a avancé pendant la fusion, celle-ci est refusée avec un message vous invitant à réessayer.

## Fermer une demande

**Fermer** abandonne la demande sans fusionner. Une demande fermée ne peut pas être rouverte : créez-en une nouvelle.

## Chronologie

La vue d'ensemble liste dans l'ordre chronologique les commentaires, les fils et les événements. Un filtre propose **Tout**, **Discussions** et **Activité**. Les événements enregistrés sont :

- une revue envoyée (approuvée, changements demandés) ;
- des étiquettes ajoutées ou retirées, un jalon défini, changé ou retiré ;
- un renommage de la demande ;
- de nouveaux commits poussés sur la branche source (avec le commit atteint) ;
- la fusion (avec le commit de fusion) et la fermeture.

Les fils résolus ou rouverts apparaissent dans le fil lui-même, avec la personne qui l'a résolu.

## Liens avec les tickets

Il n'existe pas de lien automatique entre demandes de fusion et tickets : écrire « #12 » dans une description ne crée pas de lien, et fusionner ne ferme aucun ticket. Les deux partagent en revanche les mêmes [étiquettes et jalons](/docs/utilisation/tickets), ce qui permet de les suivre ensemble.

## Notifications et webhooks

L'auteur de la demande est notifié quand quelqu'un la commente, l'approuve, demande des changements, la fusionne ou la ferme (jamais pour ses propres actions). Les mêmes événements existent pour les [webhooks](/docs/utilisation/webhooks). Voir aussi [Notifications et recherche](/docs/utilisation/notifications-et-recherche). Les routes correspondantes sont décrites dans [l'API des demandes de fusion](/docs/api/merge-requests).
