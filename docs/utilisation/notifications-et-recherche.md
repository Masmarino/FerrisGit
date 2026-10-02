# Notifications et recherche

Trois outils aident à suivre l'activité et à retrouver quelque chose : les notifications de la cloche, la recherche globale de la barre du haut, et le tableau de bord de la page **Accueil**.

## Notifications

Les notifications sont internes à FerrisGit : FerrisGit n'envoie pas d'e-mail pour l'activité des dépôts. Elles sont personnelles et ne concernent que des actions faites par d'autres. Vous ne vous notifiez jamais vous-même.

### Ce qui en déclenche une

| Vous êtes… | Notification quand… |
|---|---|
| Auteur d'une demande de fusion | quelqu'un la commente, l'approuve, demande des changements, la fusionne ou la ferme |
| Assigné à un ticket | on vous l'assigne |
| Auteur ou assigné d'un ticket | quelqu'un d'autre le commente |
| Auteur d'un ticket | quelqu'un d'autre le ferme |
| Collaborateur d'un dépôt | on vous ajoute, on change votre rôle ou on vous retire |
| Personne dont l'action a lancé un pipeline (poussée, fusion) | ce pipeline échoue |

Une revue répétée à l'identique (même avis, même commit) ne génère pas de nouvelle notification. Il n'y a pas de notification pour l'ouverture d'un ticket ou d'une demande de fusion, ni pour les mentions : FerrisGit ne gère pas les `@nom`. Il n'existe pas de réglage pour désactiver certaines notifications.

### La cloche

La cloche en haut à droite affiche le nombre de notifications non lues, mis à jour toutes les 20 secondes. En l'ouvrant, vous voyez vos 50 dernières notifications, les non lues en évidence. Cliquer sur l'une la marque comme lue et vous amène à la page concernée (demande de fusion, ticket, pipeline, réglages des collaborateurs, ou liste de vos dépôts si vous avez été retiré). **Tout marquer comme lu** vide le compteur.

> **Note** : le lien d'une notification est construit à partir du nom du propriétaire du dépôt et du nom du dépôt. Pour un dépôt de groupe, il peut mener à un dépôt personnel de même nom du créateur, ou à une page introuvable.

## Recherche globale

La barre **Rechercher…** en haut de chaque page cherche pendant la frappe, après une courte pause. Quatre catégories sont proposées :

| Catégorie | Ce qui est comparé | Où |
|---|---|---|
| **Dépôts** | le nom et la description | les dépôts visibles par vous |
| **Tickets** | le titre et la description | les dépôts visibles par vous |
| **Demandes de fusion** | le titre et la description | les dépôts visibles par vous |
| **Utilisateurs** | le nom d'utilisateur | tous les comptes |

Les dépôts visibles sont ceux que vous possédez, ceux où vous êtes collaborateur ou membre d'un groupe, et tous les dépôts publics. Un résultat est un lien vers le dépôt, le ticket ou la demande ; les utilisateurs ne sont pas cliquables.

La recherche ne porte pas sur le contenu des fichiers, les messages de commit, les commentaires, les pages de wiki ni les releases.

### Comment les mots sont compris

- Les mots sont comparés en entier, sans racine ni synonymes : `widget` ne trouve pas `widgets`, et `widg` ne trouve rien. La casse est ignorée, mais pas les accents.
- Plusieurs mots : tous doivent être présents. `crash startup` trouve un ticket qui contient les deux.
- Une expression entre guillemets (`"crash au démarrage"`) cherche ces mots à la suite.
- `or` entre deux mots accepte l'un ou l'autre, et un tiret devant un mot l'exclut (`widget -interne`).
- Les résultats sont triés par pertinence, un mot trouvé dans le titre ou le nom comptant plus qu'un mot trouvé dans la description.

La liste déroulante affiche au plus 8 résultats par catégorie. Appuyer sur Entrée ouvre la page complète `/search?q=…` quand la liste déroulante ne propose rien. Cette page montre les mêmes catégories, avec un onglet pour chacune, les statuts, les branches et les dates, mais le plafond de 8 par catégorie s'y applique aussi : « Seuls les 8 résultats les plus pertinents sont affichés ». Précisez alors la recherche.

Pour chercher dans le catalogue public sans compte, voir [Pages publiques](/docs/utilisation/pages-publiques).

## Tableau de bord

La page **Accueil** (« Bonjour, votre-nom ») rassemble ce qui vous attend. Quatre compteurs en haut, puis la carte **À traiter**, regroupée en :

- **Tickets assignés** : les tickets non terminés qui vous sont assignés ;
- **Tickets créés** : les tickets non terminés que vous avez ouverts ;
- **Mes demandes de fusion** : vos demandes de fusion ouvertes ;
- **À relire** : les demandes ouvertes d'autres auteurs que vous n'avez pas encore relues, dans les dépôts auxquels vous appartenez (propriétaire, collaborateur ou membre d'un groupe). FerrisGit n'a pas de relecteur désigné : toute demande ouverte où vous n'avez pas donné d'avis y figure.

Chaque groupe montre ses 20 plus récents éléments (« 20+ » au-delà), du plus récent au plus ancien. Un groupe vide indique « Tout est à jour ». Le panneau **Activité récente**, à droite, reprend vos 20 dernières notifications, avec le nombre de non lues.

Pour le contenu des éléments, voir [Demandes de fusion](/docs/utilisation/merge-requests) et [Tickets, étiquettes et jalons](/docs/utilisation/tickets). Les routes sont décrites dans [Webhooks et notifications](/docs/api/webhooks-et-notifications).
