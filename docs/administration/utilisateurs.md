# Utilisateurs et invitations

Les comptes se gèrent dans **Admin**, puis **Utilisateurs** (`/admin/users`). La page est réservée aux super-administrateurs. Elle liste les comptes, permet d'en inviter, et ouvre la fiche de chacun pour les actions sensibles.

Cette page décrit l'interface. Les routes correspondantes sont dans [API d'administration](/docs/api/administration).

## La liste des utilisateurs

La liste affiche jusqu'à 1000 comptes, sans pagination. Chaque ligne montre le nom d'utilisateur, l'adresse e-mail, la date de création et des pastilles :

- **Super-administrateur** pour les administrateurs, et **Vous** sur votre propre ligne ;
- l'état du compte : **Actif**, **Invitation en attente** (avec la date d'expiration) ou **Invitation expirée** ;
- pour un compte actif, **Double authentification active** ou **Non configurée**.

En haut, une recherche (« Nom d'utilisateur ou e-mail »), un tri (**Date de création**, **Nom d'utilisateur**) et un filtre **Tous** / **Invitations en attente**. Le menu de chaque ligne propose les actions décrites ci-dessous ; un clic sur le nom ouvre la fiche. Votre propre compte n'a pas de fiche : ses réglages sont dans **Mon compte**.

## La fiche d'un utilisateur

La fiche montre le nom, l'état, l'adresse e-mail et la date de création, puis la liste de ses **dépôts personnels** : nom, visibilité, description, date de création et taille sur le disque. Les administrateurs voient ainsi des métadonnées et des tailles, jamais le contenu des dépôts. Les dépôts de groupe ne sont pas listés : ils appartiennent au groupe.

Le menu **Actions** et le bouton **Supprimer l'utilisateur** regroupent les opérations.

## Créer un compte

### Par invitation (recommandé)

Le bouton **Inviter un utilisateur** ouvre un formulaire :

- **Adresse e-mail** ;
- l'interrupteur **Super-administrateur**, pour que le compte soit administrateur dès l'activation.

Vous ne choisissez pas le nom d'utilisateur : c'est la personne invitée qui le choisit en activant son compte. L'adresse doit être libre (sans tenir compte de la casse).

FerrisGit crée le compte **sans nom ni mot de passe utilisable** et envoie à l'adresse un e-mail contenant un lien d'activation, de la forme `PUBLIC_URL/invitation#token=…`. Ce lien est **valable 24 heures** et ne sert qu'une fois. La personne y choisit son nom d'utilisateur et son mot de passe (8 caractères au minimum), puis se connecte et configure la double authentification, comme tout le monde. Le nom suit les mêmes règles qu'à l'inscription : 3 à 32 caractères, lettres, chiffres, `-` et `_`, commençant par une lettre, enregistré en minuscules, libre, hors noms réservés (`admin`, `api`, `login`, `settings`…) et différent d'un groupe racine existant.

Le compte apparaît tout de suite dans la liste avec l'état **Invitation en attente**. Tant que la personne n'a pas choisi son nom, la liste l'affiche par son adresse e-mail. Il ne peut pas se connecter avant l'activation. Un compte qui n'est jamais activé est nettoyé automatiquement (voir [Comptes jamais activés](#comptes-jamais-actives)).

**Quand l'e-mail n'a pas pu partir** (serveur SMTP non configuré, injoignable, adresse refusée), le compte est créé quand même, et une alerte **Le mail n'a pas pu être envoyé** donne le motif et le lien d'activation à copier. Transmettez-le vous-même à la personne. Le lien n'est affiché que cette fois : **Renvoyer l'invitation** en génère un nouveau. Voir [Réglages de l'instance](/docs/administration/reglages) pour configurer l'envoi.

### Renvoyer une invitation

Pour un compte en attente (y compris expiré), **Renvoyer l'invitation** génère un nouveau lien valable 24 heures et l'envoie par e-mail ; l'ancien lien cesse de fonctionner. Sur un compte déjà actif, l'action n'existe pas (l'API répond « user is already active »).

### Comptes jamais activés

Que le compte vienne d'une invitation ou d'une inscription libre, FerrisGit ne laisse pas traîner un compte que personne n'active :

- **Rappel quotidien.** Dès que le lien du dernier e-mail a expiré (24 heures après son envoi), un e-mail de rappel contient un nouveau lien et la date de suppression. Le précédent cesse de fonctionner. Un rappel part donc environ une fois par jour, tant que le compte n'est pas activé.
- **Suppression à 7 jours.** Le compte est supprimé 7 jours après sa création, sans dernier e-mail. Son nom d'utilisateur et son adresse sont de nouveau libres. Renvoyer une invitation ne prolonge pas ce délai : seule l'activation y met fin.
- **Fréquence.** Le serveur examine ces comptes au démarrage puis toutes les heures, donc un rappel peut arriver jusqu'à une heure après l'expiration du lien. Si l'envoi échoue (SMTP en panne), le lien en place n'est pas touché et l'envoi est retenté à la vérification suivante.

- **Sans envoi d'e-mails configuré**, il n'y a pas de rappel (vous transmettez les liens vous-même), mais la suppression à 7 jours s'applique quand même.
- **Dès le démarrage.** Les comptes déjà en attente sont concernés : au premier passage, ceux qui ont été créés il y a plus de 7 jours sont supprimés.

Un compte déjà activé n'est jamais concerné, même s'il n'a jamais configuré la double authentification. Ces suppressions n'ont pas de réglage : la durée de 7 jours et le rythme sont fixes.

### Directement, avec un mot de passe (API seulement)

L'interface n'a pas de formulaire de création directe. L'API le permet : `POST /api/admin/users` avec un nom, une adresse et un mot de passe de 8 caractères au minimum crée un compte actif, non administrateur, sans envoyer d'e-mail. La personne sera quand même amenée à configurer la double authentification à sa première connexion. Cette route contrôle moins de choses que l'invitation : elle n'exige pas une adresse e-mail valide et ne met pas le nom en minuscules. Préférez l'invitation.

### Inscription libre

Si vous activez l'inscription libre dans les réglages, chacun peut demander son compte sur `/register` avec un nom et une adresse. Il reçoit un lien d'activation par e-mail, comme pour une invitation, et choisit son mot de passe à ce moment : l'adresse est donc vérifiée. Le compte apparaît dans la liste avec l'état **Invitation en attente** jusqu'à l'activation, et **Renvoyer l'invitation** fonctionne comme pour une invitation. Voir [Réglages de l'instance](/docs/administration/reglages).

## Réinitialiser le mot de passe

**Réinitialiser le mot de passe** sert quand quelqu'un a perdu le sien. Après confirmation :

- le mot de passe actuel cesse de fonctionner immédiatement ;
- toutes les sessions de l'utilisateur sont fermées ;
- un e-mail lui envoie un lien `PUBLIC_URL/reset-password#token=…` **valable 1 heure** pour choisir un nouveau mot de passe. Il ne peut pas se connecter avant de l'avoir utilisé.

Si l'envoi échoue, ou si l'adresse du compte n'est pas distribuable (c'est le cas du compte administrateur initial, `<nom>@localhost`), l'alerte **Le mail n'a pas pu être envoyé** affiche le lien à transmettre. Relancer l'action en génère un nouveau et invalide l'ancien. L'événement est enregistré dans le journal d'audit.

Impossible pour votre propre compte : changez votre mot de passe dans **Mon compte**. Impossible aussi pour un compte dont l'invitation est en attente : renvoyez l'invitation.

Dans l'attente que le lien soit utilisé, le compte ne compte pas comme administrateur actif (voir plus bas).

## Réinitialiser la double authentification

À utiliser quand quelqu'un a perdu son appareil **et** ses codes de secours. **Réinitialiser la double authentification** :

- supprime toutes ses méthodes : application d'authentification, clés d'accès et codes de secours ;
- le déconnecte de tous ses appareils ;
- lui envoie un e-mail pour le prévenir (si l'adresse est distribuable) ;
- l'oblige à configurer de nouveau la double authentification à sa prochaine connexion, avec son mot de passe.

L'action est possible sur votre propre compte : vous êtes alors déconnecté aussitôt. Elle est enregistrée dans le journal d'audit. Si aucun administrateur ne peut plus se connecter, voyez la procédure dans la base de données dans [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour).

Pour un utilisateur qui a seulement oublié son mot de passe, réinitialisez le mot de passe : sa double authentification reste en place.

## Nommer ou retirer un super-administrateur

Le menu propose **Nommer super-administrateur** ou **Retirer les droits de super-administrateur**. Un super-administrateur gère les utilisateurs, les réglages de l'instance et les runners. Le changement prend effet à la requête suivante de la personne, sans la déconnecter. Il est enregistré dans le journal d'audit. Vous pouvez vous retirer vous-même les droits.

Un super-administrateur n'a **pas** accès au contenu des dépôts des autres : il voit leurs métadonnées et leur taille, pas leurs fichiers.

### Règle « dernier administrateur »

L'instance ne peut pas se retrouver sans administrateur actif : retirer les droits du dernier, ou le supprimer, est refusé (« cannot remove the last administrator »). Un administrateur **actif** est un administrateur qui peut se connecter : un compte dont l'invitation ou la réinitialisation de mot de passe est en attente ne compte pas. Nommez d'abord un autre administrateur.

## Supprimer un utilisateur

Le bouton **Supprimer l'utilisateur** de la fiche demande de taper le nom d'utilisateur pour confirmer. L'action est **irréversible**. Vous ne pouvez pas supprimer votre propre compte.

**Ce qui est supprimé**

- le compte, et avec lui ses jetons Git, ses méthodes de double authentification, ses appartenances à des groupes, ses collaborations sur des dépôts, ses étoiles, ses notifications et ses liens d'invitation ou de réinitialisation ;
- ses **dépôts personnels**, y compris leurs fichiers sur le disque, leurs wikis et les fichiers joints à leurs releases ;
- les **tickets** qu'il a rédigés et ses commentaires de tickets, y compris dans des dépôts qui ne lui appartiennent pas ;
- ses **revues** de demandes de fusion ;
- les **pipelines** qu'il a déclenchés.

**Ce qui est conservé**

- les **dépôts qu'il a créés dans un groupe** : ils appartiennent au groupe et sont réattribués à l'administrateur qui supprime le compte ;
- ses **demandes de fusion**, ses **commentaires** sur des demandes de fusion et ses **releases** (avec leurs fichiers joints) : ils restent, et l'auteur s'affiche comme **Utilisateur supprimé** ;
- les groupes qu'il a créés (leur créateur n'est plus renseigné) ;
- les tickets qui lui étaient assignés, qui n'ont plus d'assigné ;
- l'historique Git des dépôts qui subsistent, avec les noms et adresses des auteurs de commits : FerrisGit ne réécrit pas l'historique ;
- les événements du journal d'audit qui le concernent (voir plus bas).

Si la suppression des fichiers sur le disque échoue après coup, l'erreur est journalisée et un répertoire orphelin peut subsister : la suppression du compte, elle, a eu lieu.

### Règle « dernier Mainteneur »

La suppression est refusée si le compte est le **dernier Mainteneur** d'une hiérarchie de groupes : personne ne pourrait plus gérer ce groupe, même un administrateur. L'alerte **Suppression impossible** nomme le groupe (« the user is the last maintainer of the group … ; promote another member first »). Donnez d'abord le rôle de Mainteneur à un autre membre (voir [Rôles et permissions](/docs/utilisation/roles-et-permissions)).

Les refus sont tous prononcés avant que quoi que ce soit ne soit modifié. La suppression d'un compte administrateur est aussi refusée s'il est le dernier administrateur actif.

## Le journal d'audit

FerrisGit enregistre les événements de sécurité dans la table `domain_events` de la base, avec `aggregate_type = 'Security'`. **L'interface n'a pas de page pour les consulter** : on les lit avec SQL.

```sql
SELECT occurred_at, event_type, actor_id, payload
FROM domain_events
WHERE aggregate_type = 'Security'
ORDER BY occurred_at DESC
LIMIT 50;
```

Chaque ligne a l'heure, le type, l'identifiant de la personne qui a agi (`actor_id`, quand elle est connue) et le détail en JSON. Les types enregistrés :

| Type | Quand |
|---|---|
| `LoginFailed` | Mot de passe ou nom d'utilisateur incorrect (avec le nom saisi). |
| `LoginSucceeded` | Une session est émise, après le second facteur. |
| `MfaVerificationFailed`, `PasskeyVerificationFailed` | Code ou clé d'accès refusé. |
| `MfaEnrolled`, `MfaDisabled` | Un utilisateur ajoute ou retire une application d'authentification. |
| `PasskeyAdded`, `PasskeyDeleted` | Un utilisateur ajoute ou supprime une clé d'accès. |
| `GitAccessDenied`, `GitTokenInvalid` | Accès Git refusé, ou jeton inconnu. |
| `MfaResetByAdmin` | Un administrateur réinitialise la double authentification d'un compte. |
| `PasswordResetByAdmin` | Un administrateur réinitialise le mot de passe d'un compte. |
| `AdminGranted`, `AdminRevoked` | Un compte devient ou cesse d'être super-administrateur (seulement si le droit a réellement changé). |
| `UserDeletedByAdmin` | Un compte est supprimé : l'événement garde le nom d'utilisateur et la liste des dépôts personnels détruits. |

Ces événements ne sont pas purgés à la suppression d'un compte, et FerrisGit n'a pas de durée de conservation pour eux. Les créations de comptes, les invitations et les modifications des réglages ne sont pas enregistrées. Pour la place du journal dans la sécurité de l'instance, voir [Sécurité](/docs/administration/securite).
