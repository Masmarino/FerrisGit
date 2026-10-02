# Présentation

FerrisGit est une plateforme Git que vous hébergez vous-même. Elle stocke vos dépôts et ajoute autour tout ce qu'une équipe utilise au quotidien : demandes de fusion, tickets, wikis, releases, webhooks et une intégration continue.

Côté technique, c'est un seul binaire Rust qui sert à la fois l'interface web (Angular), l'API REST et le protocole Git en HTTP. Les données vivent dans PostgreSQL, et le contenu des dépôts (Git, wikis, fichiers de releases) sur le disque du serveur. L'interface est en français.

## Ce que vous pouvez faire

| Besoin | Fonctionnalité | Page |
|---|---|---|
| Héberger du code | Dépôts personnels ou rangés dans des groupes imbriqués, privés ou publics | [Dépôts et groupes](/docs/utilisation/depots-et-groupes) |
| Donner accès | Rôles Lecteur, Contributeur et Mainteneur, sur un dépôt ou sur un groupe | [Rôles et permissions](/docs/utilisation/roles-et-permissions) |
| Travailler avec Git | Clone, fetch et push en HTTP, avec un jeton Git | [Cloner et pousser](/docs/utilisation/cloner-et-pousser) |
| Relire le code | Demandes de fusion avec commentaires en ligne, suggestions, approbations | [Demandes de fusion](/docs/utilisation/merge-requests) |
| Suivre le travail | Tickets, étiquettes, jalons et tableau kanban | [Tickets, étiquettes et jalons](/docs/utilisation/tickets) |
| Documenter | Un wiki par dépôt, stocké dans son propre dépôt Git | [Wikis](/docs/utilisation/wikis) |
| Publier | Releases avec fichiers joints | [Releases et tags](/docs/utilisation/releases) |
| Brancher d'autres outils | Webhooks signés | [Webhooks](/docs/utilisation/webhooks) |
| Rester informé | Notifications dans l'application, recherche | [Notifications et recherche](/docs/utilisation/notifications-et-recherche) |
| Ouvrir un projet | Pages publiques lisibles sans compte | [Pages publiques](/docs/utilisation/pages-publiques) |
| Automatiser | Pipelines décrits dans `.ferrisgit-ci.yml`, exécutés par des runners Docker ou dans des Pods Kubernetes | [Premiers pas CI/CD](/docs/ci-cd/premiers-pas) |

L'écran **Accueil** regroupe ce qui vous attend (« À traiter ») et votre activité récente. Chaque dépôt a ses onglets : Aperçu, Pipelines, Demandes de fusion, Tickets, Releases, Wiki et, pour les propriétaires et Mainteneurs, Réglages.

## Comment c'est organisé

- Un **utilisateur** possède des dépôts personnels, accessibles à l'adresse `/<nom-d'utilisateur>/<dépôt>`.
- Un **groupe** range des dépôts et d'autres groupes (des sous-groupes). Un dépôt de groupe est accessible à l'adresse `/<groupe>/<sous-groupe>/<dépôt>`.
- Un **rôle** donne des droits à un utilisateur sur un dépôt ou sur un groupe. Un rôle de groupe vaut pour tout ce qui se trouve en dessous.
- Les **administrateurs** (« Super-administrateurs ») gèrent l'instance : utilisateurs, invitations, réglages, runners, métriques. Ils ne voient pas le code des dépôts privés des autres, seulement leurs métriques (taille, date de création).

## Sécurité du compte

La double authentification est obligatoire pour tous les comptes, sans exception. À la première connexion, vous configurez une clé d'accès (passkey) ou une application d'authentification, et vous recevez des codes de secours. Voir [Compte et sécurité](/docs/utilisation/compte-et-securite).

## Limites à connaître

- Git est servi **en HTTP uniquement** : il n'y a pas d'accès SSH. L'authentification se fait par un jeton Git.
- Il n'y a pas de branches protégées : un Contributeur peut pousser sur n'importe quelle branche du dépôt.
- Ne sont pas encore disponibles : l'analyse de qualité de code, les audits de sécurité intégrés aux demandes de fusion et le lien avec ArtiFerris. Ils figurent sur la feuille de route du projet, pas dans le produit.

## Par où commencer

- Vous découvrez FerrisGit : [Prise en main](/docs/demarrer/prise-en-main).
- Vous installez l'instance : [Installation](/docs/administration/installation).
- Vous automatisez avec l'API : [Introduction à l'API REST](/docs/api/introduction).
