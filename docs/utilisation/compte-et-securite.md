# Compte et sécurité

Votre compte se gère depuis **Mon compte** : ouvrez le menu qui porte votre nom d'utilisateur, en haut à droite. La page a quatre onglets : **Profil**, **Mot de passe**, **Sécurité** et **Jetons Git**.

## Profil

Le nom d'utilisateur est affiché mais ne peut pas être modifié. L'adresse e-mail se change dans le champ **Email** et s'enregistre quand vous quittez le champ. C'est à cette adresse que FerrisGit envoie ses messages (notification de changement de mot de passe, d'activation d'un second facteur, de réinitialisation).

Le bouton **Déconnexion** ferme la session de ce navigateur.

## Se connecter

La connexion se fait en deux étapes :

1. Votre nom d'utilisateur et votre mot de passe.
2. Un second facteur : clé d'accès, code à 6 chiffres de votre application, ou code de secours.

Une session dure **12 heures** par défaut. L'administrateur règle cette durée entre 1 et 720 heures (**Admin**, **Réglages**, **Sécurité**, carte **Sessions**) ; le nouveau réglage s'applique aux prochaines connexions.

Les tentatives sont limitées : 10 essais de connexion par minute et par adresse IP, et 10 essais de second facteur par période de 5 minutes pour un même compte. Au-delà, FerrisGit répond « Trop de tentatives, réessayez dans quelques minutes ».

## Mot de passe

Dans l'onglet **Mot de passe**, saisissez le mot de passe actuel, le nouveau (8 caractères au minimum) et sa confirmation, puis cliquez sur **Changer le mot de passe**.

- Les autres sessions ouvertes sont fermées. Votre session actuelle continue.
- Un e-mail vous informe du changement.
- Les [jetons Git](#jetons-git) ne sont **pas** révoqués. Si votre mot de passe a fuité, révoquez aussi vos jetons.

**Il n'y a pas de « mot de passe oublié » en libre-service.** Demandez à un administrateur de réinitialiser votre mot de passe : votre ancien mot de passe cesse de fonctionner immédiatement, vos sessions sont fermées et vous recevez par e-mail un lien valable **1 heure** pour en choisir un nouveau. Vos facteurs d'authentification restent en place.

## Double authentification

La double authentification est obligatoire pour tous les comptes. Vous pouvez avoir une clé d'accès, une application d'authentification, ou les deux, et vos codes de secours. Tout se gère dans l'onglet **Sécurité**.

### Clés d'accès (passkeys)

Une clé d'accès valide votre connexion avec Touch ID, Windows Hello, le code de votre appareil ou une clé de sécurité. Il n'y a rien à recopier, et elle ne peut pas être hameçonnée.

- **Ajouter** : **Ajouter une clé d'accès**, un nom (facultatif, 40 caractères au plus), votre mot de passe actuel, puis la validation sur votre appareil.
- **Supprimer** : icône **Supprimer** de la clé, avec votre mot de passe. Vous êtes déconnecté de tous vos appareils.
- Vous pouvez avoir jusqu'à 20 clés, par exemple une par appareil. La liste indique pour chacune sa date d'ajout et sa dernière utilisation.

Les clés d'accès ne sont disponibles que si l'instance est servie sous un nom de domaine en HTTPS (ou sur `localhost`). Elles sont liées à l'adresse exacte de l'instance : si l'administrateur change cette adresse, les clés déjà enregistrées ne fonctionnent plus et il faut utiliser un autre facteur, puis en recréer.

### Application d'authentification (TOTP)

Toute application TOTP convient (Google Authenticator, Authy, 1Password...). FerrisGit utilise le format courant : code à 6 chiffres, renouvelé toutes les 30 secondes. Un code ne peut servir qu'une fois, et une légère dérive d'horloge de l'ordre d'une période est tolérée.

- **Configurer** : **Ajouter une application** (si vous avez déjà une clé d'accès) ou l'enrôlement de première connexion. Confirmez votre mot de passe, scannez le QR code, saisissez un code, puis enregistrez les codes de secours affichés. Le QR code reste valable 15 minutes (5 minutes lors de l'enrôlement de première connexion, voir [Prise en main](/docs/demarrer/prise-en-main#activer-la-double-authentification)).
- **Supprimer** : **Supprimer** si une clé d'accès reste disponible. Vous êtes déconnecté de tous vos appareils.
- **Réinitialiser** : **Réinitialiser** si l'application est votre seul facteur. Vous êtes déconnecté et devrez en configurer un nouveau à la prochaine connexion.

Ces deux actions demandent votre mot de passe.

### Codes de secours

Vous recevez **10 codes de secours** quand vous activez votre premier facteur. Chacun fonctionne **une seule fois**. À la connexion, choisissez **Utiliser un code de secours** et saisissez-en un. Ils servent à vous connecter quand vous n'avez plus votre téléphone ou votre clé.

- Ils ne sont affichés qu'une fois. Copiez-les ou téléchargez le fichier, et rangez-les à part (gestionnaire de mots de passe, coffre).
- La page **Sécurité** indique combien il en reste. Quand il en reste peu, utilisez **Régénérer les codes de secours** : après confirmation de votre mot de passe, 10 nouveaux codes remplacent les anciens, qui cessent de fonctionner immédiatement.
- Les codes de secours ne sont valables que tant qu'au moins un facteur est configuré. Supprimer votre dernier facteur les efface.

### Perte d'un second facteur

| Situation | Que faire |
|---|---|
| Vous avez perdu un facteur mais en gardez un autre | Connectez-vous avec l'autre, puis supprimez celui qui est perdu et ajoutez-en un nouveau. |
| Vous avez perdu tous vos facteurs, mais vous avez un code de secours | Connectez-vous avec le code de secours, puis configurez un nouveau facteur et régénérez les codes. |
| Vous avez perdu tous vos facteurs **et** vos codes de secours | Demandez à un administrateur de réinitialiser votre double authentification. |

La réinitialisation par un administrateur (**Admin**, **Utilisateurs**, la fiche de la personne, **Réinitialiser la double authentification**) supprime l'application d'authentification, les clés d'accès et les codes de secours, ferme toutes vos sessions et vous prévient par e-mail. À la connexion suivante, avec votre mot de passe, vous repassez par la configuration obligatoire d'un nouveau facteur.

Si vous avez aussi perdu votre mot de passe, l'administrateur réinitialise les deux. Pour la procédure de dernier recours quand plus aucun administrateur ne peut se connecter, voir [Utilisateurs et invitations](/docs/administration/utilisateurs) et [Sécurité](/docs/administration/securite).

## Jetons Git

Les jetons Git (jetons d'accès personnels) se gèrent dans l'onglet **Jetons Git**. Ils servent à **Git en HTTP(S) uniquement** : cloner, récupérer et pousser.

| | |
|---|---|
| Créer | Donnez un nom (celui de l'outil ou de la machine qui s'en sert) et cliquez sur **Générer**. |
| Format | `fg_` suivi de 64 caractères hexadécimaux. |
| Affichage | Une seule fois, à la création. FerrisGit ne conserve que son empreinte et ne peut pas le retrouver. |
| Liste | **Jetons actifs** : nom, date de création, dernière utilisation (ou « Jamais utilisé »). |
| Révoquer | Icône **Révoquer**. Les outils qui l'utilisent perdent l'accès aussitôt ; un jeton révoqué ne peut pas être réactivé. |

### Ce qu'un jeton permet

- **Git en HTTP.** Un jeton sert de mot de passe pour `git clone`, `git fetch` et `git push`. Voir [Cloner et pousser](/docs/utilisation/cloner-et-pousser).
- **Pas l'API REST.** Le serveur n'accepte sur `/api` que la session obtenue à la connexion (en-tête `Authorization: Bearer <JWT de session>`). Présenter un jeton `fg_...` à l'API donne une erreur 401. Comme la double authentification est obligatoire, un script qui appelle l'API doit refaire le parcours de connexion et de second facteur. Voir [Authentification de l'API](/docs/api/authentification).
- **Mêmes droits que vous.** Un jeton n'a ni périmètre ni expiration : il vaut ce que vaut votre compte, sur tous vos dépôts, jusqu'à sa révocation. Il permet de cloner si vous êtes au moins Lecteur, de pousser si vous êtes au moins Contributeur.
- **Il remplace le second facteur pour Git.** Quiconque détient le jeton accède à vos dépôts sans mot de passe ni code.
- **Il survit aux changements de mot de passe** et aux réinitialisations de la double authentification. Il disparaît avec la révocation ou la suppression du compte.

Bonnes pratiques : un jeton par machine ou par outil, un nom parlant, une révocation dès qu'un outil n'est plus utilisé, et jamais de jeton dans un dépôt ou dans un message.
