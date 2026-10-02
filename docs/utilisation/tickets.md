# Tickets, étiquettes et jalons

Les tickets suivent les bugs, les idées et les tâches d'un dépôt. Les étiquettes les classent, les jalons les regroupent autour d'une échéance, et le tableau kanban montre leur avancement. L'interface les appelle « labels » et « milestones ». Tout se trouve dans l'entrée **Tickets** du menu du dépôt, sauf les étiquettes et les jalons, qui se gèrent dans **Réglages**.

## Qui peut faire quoi

| Action | Rôle minimum |
|---|---|
| Voir les tickets et lire les commentaires | Lecteur |
| Créer un ticket, commenter | Contributeur |
| Modifier un ticket, changer son état, le fermer ou le rouvrir | Contributeur |
| Assigner, poser des étiquettes ou un jalon | Contributeur |
| Créer, supprimer des étiquettes et des jalons | Contributeur (API), Mainteneur (interface) |

Sur un dépôt public, tout utilisateur connecté est Lecteur : il lit les tickets, mais ne peut ni en ouvrir ni les commenter. Pour cela, il faut le rôle Contributeur sur le dépôt ou sur son groupe. Voir [Rôles et permissions](/docs/utilisation/roles-et-permissions).

## Créer et suivre un ticket

**Nouveau ticket** demande un **Titre** (obligatoire), une **Description** (facultative, en Markdown) et un **Type** : Bug, Fonctionnalité ou Tâche. Le type « Epic » existe dans l'API, mais l'interface ne le propose pas et rien ne rattache de sous-ticket à un epic.

Chaque ticket reçoit un numéro propre au dépôt (`#1`, `#2`…), qui sert dans son adresse. Sa page montre la description puis les commentaires (Markdown), et un panneau latéral : **Statut**, **Assigné**, **Labels**, **Milestone**, **Dates**, **Participants**. Le bouton **Fermer le ticket** devient **Rouvrir le ticket** quand il est fermé.

Dans l'interface, le titre, la description et le type ne se modifient pas après création (seule l'API le permet).

## États

Un ticket a quatre états : **À faire**, **En cours**, **En revue**, **Terminé**. Les trois premiers sont des tickets ouverts. **Terminé** équivaut à fermé, et inversement :

- fermer un ticket le passe à **Terminé** et enregistre la date de fermeture ;
- le rouvrir le remet à **À faire** ;
- déplacer un ticket vers une autre colonne que **Terminé** efface sa date de fermeture.

On ne peut fermer qu'un ticket ouvert, et rouvrir qu'un ticket fermé.

## Tableau kanban

**Vue kanban** (en haut de la liste) affiche une colonne par état. Un Contributeur change l'état d'un ticket en glissant sa carte vers une autre colonne (sur tactile, après un appui d'un quart de seconde), ou avec le menu de la carte, **Déplacer le ticket #N vers**, qui fonctionne aussi au clavier. Les lecteurs voient le tableau sans pouvoir le modifier.

Une carte indique le numéro, le titre, le type, les étiquettes, le jalon, la personne assignée et le nombre de commentaires. L'ordre des cartes dans une colonne n'est pas enregistré : seul le changement de colonne l'est. Si l'enregistrement échoue, la carte revient à sa place.

## Liste, filtres et recherche

La liste a deux onglets, **Ouverts** et **Fermés**, avec leur nombre. Le panneau de droite propose :

- **Recherche** : filtre sur le **titre** (sans tenir compte de la casse). Elle ne regarde ni la description ni les commentaires ;
- **Trier** par date de création (plus récents d'abord par défaut) ou par titre, en croissant ou décroissant ;
- **Labels** : garde les tickets qui portent au moins une des étiquettes choisies ;
- **Milestone** : garde les tickets d'un jalon.

**Réinitialiser** efface les filtres. Le tableau kanban reprend la recherche, les étiquettes et le jalon. La liste est paginée par 25. Pour chercher dans les titres et les descriptions de tous vos dépôts, utilisez la [recherche globale](/docs/utilisation/notifications-et-recherche).

## Assignation

Le bouton **M'assigner** (dans la page du ticket, ou dans le menu de la liste) vous place comme responsable. Le panneau **Assigné** montre une seule personne.

L'interface ne permet que de s'assigner soi-même ; l'API accepte un autre utilisateur, ou `null` pour retirer l'assignation. La personne assignée doit avoir au moins le rôle Contributeur sur le dépôt, que ce rôle soit accordé directement ou hérité d'un groupe. Un Lecteur ne peut pas être assigné.

L'assigné est notifié, sauf s'il s'assigne lui-même.

## Étiquettes

Une étiquette a un nom et une couleur. Dans **Réglages > Labels**, saisissez un nom, choisissez une couleur dans la palette (douze couleurs) et cliquez sur **Créer le label**. Le bouton de suppression demande de retaper le nom, et retire l'étiquette de tous les tickets et demandes de fusion qui l'utilisent.

Les étiquettes d'un dépôt se posent sur ses tickets et ses [demandes de fusion](/docs/utilisation/merge-requests) (sélecteur **Labels** du panneau latéral). L'API permet aussi des étiquettes de groupe, utilisables par tous les dépôts du groupe et de ses sous-groupes, mais l'interface ne propose que celles du dépôt. L'interface ne permet pas de renommer une étiquette (l'API le permet).

## Jalons

Un jalon a un titre, une date d'échéance facultative et un état, ouvert ou fermé. Dans **Réglages > Milestones**, **Créer le milestone** enregistre un titre et une échéance. La liste indique **Ouvert** ou **Fermé**, et **En retard** quand l'échéance est passée. Supprimer un jalon le retire des tickets et demandes de fusion concernés, après confirmation.

L'interface ne permet pas de fermer ni de modifier un jalon : l'état « fermé » ne se pose que par l'API. Comme pour les étiquettes, un jalon de groupe n'existe que par l'API.

Un ticket ou une demande de fusion n'a qu'un jalon, choisi dans son panneau latéral.

## Mentions et liens

FerrisGit ne gère pas les mentions (`@nom`) ni les liens automatiques entre tickets (`#12`) : ces textes restent du texte. Les personnes concernées sont prévenues autrement (voir [Notifications et recherche](/docs/utilisation/notifications-et-recherche)) :

- l'auteur et l'assigné d'un ticket quand quelqu'un le commente ;
- l'auteur quand quelqu'un d'autre ferme son ticket ;
- l'assigné quand on lui attribue un ticket.

Les [webhooks](/docs/utilisation/webhooks) peuvent aussi signaler ces trois événements. Les routes sont décrites dans [l'API des tickets](/docs/api/tickets).
