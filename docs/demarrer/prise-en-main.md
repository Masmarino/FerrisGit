# Prise en main

Cette page vous mène de zéro à un premier `git push` : obtenir un compte, activer la double authentification, créer un dépôt, puis le cloner et y pousser du code avec un jeton.

## Obtenir un compte

Il y a deux façons, selon la configuration de l'instance.

### Inscription libre

Si un administrateur l'a activée, vous demandez vous-même votre compte. Elle est désactivée par défaut, et elle n'est proposée que si l'instance sait envoyer des e-mails. Quand elle est fermée, la page **Créer un compte** l'indique (« Les inscriptions sont fermées ») et vous renvoie vers une invitation.

1. Ouvrez la page de connexion et choisissez **Créer un compte**.
2. Renseignez le nom d'utilisateur et l'adresse e-mail, puis validez avec **Créer mon compte**. L'écran **Consultez votre boîte mail** confirme l'envoi.
3. Ouvrez l'e-mail « Confirmez votre inscription à FerrisGit » et suivez le lien dans les **24 heures**. Sur la page **Activez votre compte**, choisissez votre mot de passe (8 caractères au minimum) et confirmez-le.
4. Connectez-vous avec votre nom d'utilisateur et ce mot de passe. La double authentification se configure à ce moment (voir plus bas).

| Champ | Règle |
|---|---|
| Nom d'utilisateur | 3 à 32 caractères, lettres, chiffres, `-` et `_`, en commençant par une lettre. Enregistré en minuscules. Certains noms sont réservés (`admin`, `api`, `login`, `groups`...). |
| Adresse e-mail | Une adresse valide (un seul `@`, un domaine avec un point, 254 caractères au plus). Elle doit être unique sur l'instance. C'est elle qui reçoit le lien : sans lui, le compte reste inutilisable. |

Tant que le lien n'a pas servi, le compte ne peut pas se connecter, et le nom comme l'adresse restent réservés. Chaque jour, un e-mail de rappel contient un nouveau lien (l'ancien cesse alors de fonctionner), et **le compte est supprimé 7 jours après sa création** s'il n'a pas été activé : son nom et son adresse sont alors de nouveau libres. Si l'e-mail n'arrive pas, vous pouvez aussi vous inscrire de nouveau avec exactement les mêmes informations pour recevoir un nouveau lien. Chaque lien ne sert qu'une fois.

Le nom d'utilisateur ne peut pas être modifié ensuite. Il fait partie de l'adresse de vos dépôts personnels, et il est refusé s'il est déjà pris, y compris par un groupe de premier niveau du même nom.

### Invitation par e-mail

Un administrateur crée votre compte depuis **Admin**, **Utilisateurs** ([la procédure côté administrateur](/docs/administration/utilisateurs)). Vous recevez un e-mail intitulé « Votre compte FerrisGit » avec un lien d'activation.

1. Ouvrez le lien dans les **24 heures**. Passé ce délai, un rappel avec un nouveau lien vous est envoyé chaque jour, jusqu'à la suppression du compte, **7 jours après sa création**. L'administrateur peut aussi vous renvoyer une invitation.
2. Sur la page **Activez votre compte**, choisissez votre nom d'utilisateur et votre mot de passe (8 caractères au minimum), puis confirmez le mot de passe. Le nom suit les mêmes règles qu'à l'inscription, et ne pourra plus être modifié.
3. Connectez-vous avec votre nom d'utilisateur et ce mot de passe. La double authentification se configure à ce moment.

Si l'instance ne peut pas envoyer d'e-mails, l'administrateur reçoit le lien à vous transmettre lui-même. Chaque lien ne sert qu'une fois.

## Activer la double authentification

La double authentification est obligatoire. Tant qu'aucun second facteur n'est configuré, vous ne pouvez rien faire d'autre. À la première connexion, l'écran **Protégez votre compte** propose deux choix.

### Avec une clé d'accès (recommandé)

1. Choisissez **Utiliser une clé d'accès**.
2. Donnez-lui si vous le souhaitez un nom pour la reconnaître plus tard, puis cliquez sur **Créer la clé d'accès**.
3. Validez sur votre appareil : Touch ID, Windows Hello, code PIN ou clé de sécurité.

Les clés d'accès ne sont proposées que si l'instance est servie sous un nom de domaine en HTTPS (ou sur `localhost`). Sinon, utilisez une application d'authentification.

### Avec une application d'authentification

1. Choisissez **Utiliser une application**, puis **Commencer**.
2. Scannez le QR code avec Google Authenticator, Authy, 1Password ou toute application TOTP. À défaut, saisissez la clé affichée sous le QR code.
3. Saisissez le code à 6 chiffres affiché par l'application et cliquez sur **Activer**.

### Conserver les codes de secours

Dans les deux cas, FerrisGit affiche ensuite **10 codes de secours**. Chacun ne fonctionne qu'une seule fois et sert à vous connecter si vous perdez votre téléphone ou votre clé. Ils ne seront plus jamais affichés : copiez-les ou téléchargez le fichier `ferrisgit-codes-de-secours.txt`, puis cliquez sur **J'ai enregistré mes codes de secours**.

> **Attention** : l'enrôlement doit se terminer dans les minutes qui suivent la saisie du mot de passe (5 minutes). Si la page indique que la configuration a expiré, reconnectez-vous et recommencez.

Votre session reste ouverte 12 heures par défaut (réglable par l'administrateur), après quoi vous vous reconnectez avec le mot de passe puis le second facteur. Le détail est dans [Compte et sécurité](/docs/utilisation/compte-et-securite).

## Créer un dépôt

1. Dans le menu de gauche, ouvrez **Dépôts**, puis cliquez sur **Nouveau dépôt**.
2. Choisissez l'**Emplacement** : **Personnel** ou l'un des groupes où vous êtes Mainteneur.
3. Saisissez le **Nom du dépôt** (lettres, chiffres, `-` et `_`) et, si vous voulez, une **Description**.
4. Réglez la **Visibilité** : **Privé** (vous et les personnes à qui vous donnez accès) ou **Public**. Vous pourrez la modifier plus tard dans les **Réglages** du dépôt.
5. Les **Options avancées** (CI activée, approbations requises avant fusion, chemin du fichier de pipeline) ont des valeurs par défaut utilisables. Laissez-les.
6. Cliquez sur **Créer le dépôt**.

Plus de détails dans [Dépôts et groupes](/docs/utilisation/depots-et-groupes).

## Créer un jeton Git

Git n'accepte pas votre mot de passe de compte : il vous faut un jeton Git (un jeton d'accès personnel).

1. Ouvrez le menu qui porte votre nom d'utilisateur, en haut à droite, puis **Mon compte**.
2. Allez dans l'onglet **Jetons Git**.
3. Donnez un nom au jeton (par exemple le nom de votre machine) et cliquez sur **Générer**.
4. Copiez le jeton tout de suite. Il commence par `fg_` et **ne sera plus jamais affiché**. En cas de perte, révoquez-le et générez-en un autre.

## Pousser un premier projet

Sur la page d'un dépôt vide, FerrisGit affiche l'adresse de clonage et les commandes à lancer. Voici le même parcours, avec l'adresse d'un dépôt personnel :

```bash
git init -b main
git add .
git commit -m "Premier commit"
git remote add origin https://ferrisgit.example.com/alice/mon-projet.git
git push -u origin main
```

Quand Git vous demande des identifiants, saisissez votre **nom d'utilisateur**, puis le **jeton à la place du mot de passe**. Laissez le gestionnaire d'identifiants de votre système mémoriser le jeton plutôt que de l'écrire dans l'adresse du dépôt.

Rechargez la page du dépôt : vos fichiers apparaissent. Pour le récupérer sur une autre machine :

```bash
git clone https://ferrisgit.example.com/alice/mon-projet.git
```

L'adresse d'un dépôt de groupe contient le chemin du groupe : `https://ferrisgit.example.com/mon-groupe/mon-projet.git`. Le bouton **Cloner** de l'en-tête du dépôt, ou **Copier l'URL de clonage** dans le menu de la liste, donne toujours la bonne adresse.

## Et ensuite

- [Cloner et pousser](/docs/utilisation/cloner-et-pousser) : erreurs d'authentification, wikis, taille maximale d'un push.
- [Rôles et permissions](/docs/utilisation/roles-et-permissions) : donner accès à vos collègues.
- [Demandes de fusion](/docs/utilisation/merge-requests) et [Tickets](/docs/utilisation/tickets) : travailler à plusieurs.
- [Premiers pas CI/CD](/docs/ci-cd/premiers-pas) : lancer une pipeline à chaque push.
