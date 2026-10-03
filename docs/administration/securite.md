# Sécurité

Cette page décrit ce que FerrisGit protège par lui-même, ce qu'il demande à l'administrateur de configurer, et ce qu'il laisse volontairement visible. Elle se termine par une liste de contrôle de durcissement.

## Authentification

### Double authentification obligatoire

Chaque connexion se fait en deux étapes : le mot de passe, puis un second facteur. Il n'existe aucun réglage pour la désactiver, pour un compte ou pour l'instance. Un compte qui n'a aucun facteur est obligé de le configurer avant toute autre action, y compris les comptes créés par invitation ou par inscription libre.

Trois méthodes, qui peuvent coexister :

- **application d'authentification** (TOTP : 6 chiffres, renouvelés toutes les 30 secondes). Un code déjà accepté ne peut pas être rejoué ;
- **clés d'accès** (passkeys, WebAuthn), jusqu'à 20 par utilisateur ;
- **10 codes de secours**, à usage unique, stockés sous forme hachée.

Entre le mot de passe et le second facteur, l'utilisateur détient un jeton intermédiaire valable 5 minutes. Le jeton d'une connexion terminée ne peut pas être réutilisé. La configuration d'une application d'authentification doit être confirmée dans les 15 minutes.

Les facteurs se gèrent dans **Mon compte** (voir [Compte et sécurité](/docs/utilisation/compte-et-securite)). Changer de mot de passe ou retirer son application d'authentification demande le mot de passe actuel. Un administrateur peut réinitialiser les facteurs d'un compte : voir [Utilisateurs et invitations](/docs/administration/utilisateurs).

### Mots de passe

Les mots de passe font au moins 8 caractères et sont hachés avec Argon2 (paramètres par défaut de la bibliothèque), avec un sel propre à chaque mot de passe. Un nom d'utilisateur inconnu coûte autant de calcul qu'un mot de passe faux, et la réponse est identique : on ne peut pas deviner quels comptes existent.

### Sessions

Une session est un jeton JWT signé avec `JWT_SECRET`, d'une durée de 12 heures par défaut, réglable de 1 à 720 heures (voir [Réglages de l'instance](/docs/administration/reglages)). Le navigateur le conserve dans le `localStorage`, pas dans un cookie `HttpOnly` : un script injecté dans la page pourrait le lire. Pour réduire ce risque, chaque réponse du serveur porte :

- `Content-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'` : aucun script externe ;
- `X-Frame-Options: DENY` : l'application ne s'affiche pas dans un cadre d'une autre page ;
- `X-Content-Type-Options: nosniff` et `Referrer-Policy: no-referrer`.

Un jeton est révoqué avant son échéance quand son titulaire change de mot de passe (une nouvelle session lui est alors remise), retire son application d'authentification, ou quand un administrateur réinitialise son mot de passe ou sa double authentification. Un compte supprimé n'a plus de session valable. Le statut d'administrateur n'est pas dans le jeton : il est relu à chaque requête, donc un retrait de droits est immédiat.

Changer `JWT_SECRET` déconnecte tout le monde.

### Jetons Git et jetons de runner

Les jetons Git ne servent que pour Git (HTTP Basic, avec le nom d'utilisateur) : ils ne donnent pas accès à l'API REST. Ils sont stockés sous forme hachée et ne sont affichés qu'à leur création.

Les jetons de runner sont stockés hachés, n'expirent jamais, et donnent un accès **en lecture à tous les dépôts** de l'instance (ils ne peuvent pas pousser) ; un runner reçoit aussi les variables CI déchiffrées des jobs qu'il exécute. Traitez-les comme des secrets, et supprimez un runner (page **Runners**) dont vous ne vous servez plus ou dont le jeton a fuité. De même, le jeton d'enregistrement des runners (voir [Réglages de l'instance](/docs/administration/reglages)) permet d'en créer.

## Secrets au repos

La clé `SETTINGS_ENCRYPTION_KEY` chiffre en AES-256-GCM les variables CI des dépôts, les secrets des webhooks, le mot de passe SMTP et les secrets TOTP. Elle ne se change pas en place : voir [Configuration](/docs/administration/configuration). Les mots de passe des utilisateurs, les jetons Git, les jetons de runner, les codes de secours et les liens d'invitation ou de réinitialisation ne sont pas chiffrés mais hachés : le serveur ne peut pas les relire.

Les liens d'activation et de réinitialisation placent leur jeton après un `#` : les navigateurs ne l'envoient pas au serveur, il ne figure donc pas dans les journaux d'accès du serveur ni de votre proxy.

## Limitation de débit

Plusieurs routes sensibles sont limitées. Les compteurs sont en mémoire : ils repartent de zéro au redémarrage.

| Ce qui est limité | Clé de comptage | Limite |
|---|---|---|
| Connexion (`/api/auth/login`) | adresse IP du client | 10 requêtes par minute |
| Inscription, activation d'invitation, réinitialisation de mot de passe | adresse IP du client | 10 requêtes par période de 5 minutes (activation et réinitialisation partagent le même quota) |
| Pages publiques (`/api/public/…`) | adresse IP du client | 120 requêtes par minute |
| Début d'une connexion par clé d'accès | adresse IP du client | 30 requêtes par période de 5 minutes |
| Codes de la double authentification et changement de mot de passe | compte utilisateur | 10 essais par période de 5 minutes |

Au-delà, le serveur répond « 429 » avec un message ; sur les pages publiques, il ajoute un en-tête `Retry-After`. Les adresses IPv6 sont comptées par bloc /64, car un client en possède un entier. Les essais du second facteur sont comptés par compte et non par adresse : un code TOTP n'a qu'un million de valeurs, il faut empêcher de les essayer depuis plusieurs adresses.

Les limites par adresse ne sont justes que si FerrisGit connaît la vraie adresse du client. Derrière un reverse proxy, renseignez `TRUSTED_PROXY_CIDRS` avec **uniquement** les adresses du proxy : sinon tous les clients partagent un seul quota, ou, si vous listez trop large, un client peut choisir son adresse. Voir [Configuration](/docs/administration/configuration).

## Clés d'accès et PUBLIC_URL

Une clé d'accès est liée à l'origine de `PUBLIC_URL`, c'est-à-dire à l'hôte et au schéma. Changer l'hôte invalide les clés déjà enregistrées. Elles ne fonctionnent que si `PUBLIC_URL` est en HTTPS (ou en `http://localhost`) sur un nom de domaine : avec une adresse IP, ou en `http://` sur un autre hôte, elles sont indisponibles. La connexion par application d'authentification reste possible dans tous les cas.

## Journal d'audit

Les événements de sécurité (connexions réussies et échouées, ajout ou retrait de facteurs, accès Git refusés, actions d'administration : réinitialisations, changement de droits, suppression de compte) sont enregistrés dans la table `domain_events` de la base. L'interface ne les affiche pas : lisez-les en SQL. La liste des événements et une requête d'exemple sont dans [Utilisateurs et invitations](/docs/administration/utilisateurs). Aucun délai de conservation n'est appliqué : surveillez la taille de la table si l'instance est exposée à des tentatives répétées.

## Pages publiques et ce qu'elles exposent

Les pages publiques ne montrent que les dépôts marqués **publics**, par une API en lecture seule qui prend une décision d'autorisation unique à chaque requête. Un dépôt privé, un dépôt inconnu et un dépôt masqué parce que les pages sont désactivées reçoivent exactement la même réponse « introuvable » : rien ne révèle l'existence d'un dépôt privé. Les groupes ne sont jamais publics. Les tickets, demandes de fusion, pipelines, wikis et réglages ne sont jamais exposés.

Un visiteur voit ce qu'un `git clone` anonyme révèle déjà, en particulier les noms et adresses e-mail des auteurs de commits et les noms de tous les tags : la liste est dans [Pages publiques](/docs/utilisation/pages-publiques#ce-qui-est-tout-de-même-visible). Si cela vous gêne, rendez le dépôt privé ou désactivez les pages publiques (**Admin**, **Réglages**, **Sécurité**). Désactivées, elles refusent aussi le clonage anonyme, avec la même réponse 401 qu'un dépôt privé ; les utilisateurs authentifiés lisent toujours les dépôts publics.

Le référencement par les moteurs de recherche est désactivé par défaut : tant que vous ne l'activez pas, les réponses portent `X-Robots-Tag: noindex, nofollow` et `robots.txt` interdit tout.

## HTTPS

Servez toujours FerrisGit en HTTPS en production. Sans lui, mots de passe, codes, sessions et jetons Git (envoyés en HTTP Basic) transitent en clair. FerrisGit ne gère pas TLS : c'est le rôle de votre reverse proxy ou de votre contrôleur d'entrée (voir [Installation](/docs/administration/installation)). Il n'envoie pas d'en-tête `Strict-Transport-Security` : ajoutez-le au niveau du proxy.

## Inscription libre

Si vous l'activez, n'importe qui peut demander un compte. Il ne devient utilisable qu'après avoir suivi le lien envoyé à son adresse e-mail, ce qui vérifie l'adresse sans filtrer les personnes : toute adresse que quelqu'un lit peut ouvrir un compte. Le compte n'a aucun droit particulier, mais il peut créer des dépôts et, si vous avez des runners, déclencher des pipelines qui s'exécutent sur votre infrastructure. Gardez-la désactivée (c'est le défaut) sauf si c'est le but de l'instance.

## Signaler une vulnérabilité

N'ouvrez pas de ticket public. Utilisez le signalement privé de GitHub : dans le dépôt FerrisGit, onglet **Security**, puis **Report a vulnerability**.

## Liste de contrôle de durcissement

- `JWT_SECRET` et `SETTINGS_ENCRYPTION_KEY` générés aléatoirement, jamais les valeurs de `.env.example`, et sauvegardés à part.
- Mot de passe PostgreSQL fort, base non exposée (supprimez `docker-compose.override.yml` qui publie le port 5435, ou ne publiez rien).
- Instance en HTTPS, avec `PUBLIC_URL` à l'adresse HTTPS exacte ; port 8080 non exposé directement (`127.0.0.1:8080:8080` derrière un proxy local).
- `TRUSTED_PROXY_CIDRS` limité à votre proxy, ou vide si le serveur est joint directement.
- Mot de passe de l'administrateur initial changé, adresse e-mail réelle renseignée, variables `FERRISGIT_BOOTSTRAP_ADMIN_*` retirées de la configuration.
- Au moins deux super-administrateurs, chacun avec ses codes de secours rangés. Voir [Utilisateurs et invitations](/docs/administration/utilisateurs).
- Serveur SMTP configuré et testé, pour que les alertes de sécurité (mot de passe changé, double authentification modifiée) arrivent aux utilisateurs.
- Inscription libre désactivée, sauf besoin explicite.
- Pages publiques et référencement réglés selon votre besoin ; dépôts sensibles en privé.
- Jetons de runner et jeton d'enregistrement traités comme des secrets ; runners inutilisés supprimés.
- Limite de taille de push et limite de mémoire du conteneur cohérentes entre elles.
- Sauvegardes de la base et du stockage planifiées, et une restauration testée. Voir [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour).
- Version épinglée, et lecture des notes de version avant chaque mise à jour.
- Consultation occasionnelle du journal d'audit (`LoginFailed`, `GitAccessDenied`, `MfaResetByAdmin`).
