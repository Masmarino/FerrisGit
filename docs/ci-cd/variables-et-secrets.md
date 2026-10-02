# Variables et secrets

Un job reçoit des variables d'environnement de deux sources : celles que vous écrivez dans `.ferrisgit-ci.yml` (clé `variables`) et les variables CI du dépôt, saisies dans l'interface et chiffrées. Cette page dit lesquelles arrivent où, dans quel ordre, et ce que le masquage protège vraiment.

> **Attention** : les variables CI du dépôt ne sont transmises qu'aux jobs des **runners Docker**. Avec le moteur Kubernetes, un job reçoit uniquement les variables de sa clé `variables`. Voir [Moteur Kubernetes](/docs/ci-cd/kubernetes#limites).

## Variables d'un job

Elles se déclarent dans le fichier de pipeline :

```yaml
jobs:
  build:
    stage: build
    image: node:22
    variables:
      NODE_ENV: production
      API_URL: https://api.example.com
    script:
      - echo "Cible = $API_URL"
```

- Les noms et les valeurs sont des chaînes. `PORT: 8080` donne la chaîne `8080`.
- Les valeurs sont transmises telles quelles : FerrisGit ne développe pas `$AUTRE` dans une valeur. Le développement a lieu dans le script, par le shell : `echo "$API_URL"`.
- Avec les runners Docker, chaque variable devient un `-e NOM=valeur` de la commande `docker run`. Avec Kubernetes, elle devient une variable d'environnement du conteneur.
- Ces variables sont écrites dans le dépôt, donc lisibles par tous ceux qui lisent le code. N'y mettez pas de secret.

## Variables prédéfinies

FerrisGit n'injecte aucune variable dans les jobs : pas de SHA de commit, de nom de branche, d'identifiant de pipeline ni de job. Le conteneur ne voit que les variables de l'image, celles de `variables` et (runners Docker) les variables CI du dépôt.

Avec les runners Docker, le dépôt est cloné au bon commit mais son dossier `.git` est supprimé avant le lancement du conteneur : `git rev-parse HEAD` ou `git describe` n'y fonctionnent pas. Pour connaître le commit dans un job, il n'y a pas aujourd'hui de moyen fourni par FerrisGit.

## Variables CI du dépôt

Ce sont les secrets : jetons d'API, mots de passe de registre, URL privées.

### Les créer

1. Ouvrez le dépôt, puis **Réglages**, puis la section **Variables CI/CD**. Il faut le rôle Mainteneur sur le dépôt.
2. Dans la carte **Ajouter une variable**, saisissez le **Nom** (par exemple `API_KEY`) et la **Valeur**.
3. Cliquez sur **Ajouter la variable**. Le toast « Variable ajoutée. » confirme.

La liste **Variables** affiche chaque variable avec un badge **Masquée**. Sa valeur n'est plus jamais réaffichée, ni par l'interface ni par l'API : pour la changer, ajoutez une variable du même nom, qui remplace l'ancienne. Pour la retirer, cliquez sur l'icône de corbeille puis confirmez **Supprimer** ; les prochains jobs ne la recevront plus.

Le nom n'est pas contrôlé par FerrisGit : choisissez un nom valide pour une variable d'environnement (lettres, chiffres et `_`, sans commencer par un chiffre).

### Chiffrement

Les valeurs sont chiffrées en base (AES-GCM) avec la clé `SETTINGS_ENCRYPTION_KEY` du serveur. Elles ne sont déchiffrées qu'au moment de confier un job à un runner Docker.

> **Attention** : si vous changez `SETTINGS_ENCRYPTION_KEY`, toutes les variables déjà enregistrées deviennent illisibles, et il n'existe pas de rechiffrement. Il faut les ressaisir à la main. Voir [Configuration](/docs/administration/configuration).

### Précédence

Avec les runners Docker, le runner assemble l'environnement du conteneur en partant des `variables` du job, puis en ajoutant les variables CI du dépôt. À nom égal, **la variable CI du dépôt l'emporte** sur celle du fichier de pipeline.

| Source | Runners Docker | Kubernetes |
|---|---|---|
| `variables` du job | oui | oui |
| Variables CI du dépôt | oui, prioritaires | non |
| Variables prédéfinies par FerrisGit | aucune | aucune |

## Masquage dans les journaux

Avec les runners Docker, le runner remplace par `***` toute occurrence, dans une ligne de sortie du job, de la valeur d'une variable **masquée**, avant d'envoyer la ligne au serveur. Il fait la même chose pour son propre jeton. Les variables créées dans l'interface sont toujours masquées. (Par l'API, `POST /api/repositories/{id}/ci-variables` accepte `"masked": false` ; la liste affiche alors le badge **Visible dans les journaux**. Voir [Pipelines et runners (API)](/docs/api/ci-cd).)

Ce masquage est un garde-fou contre les fuites accidentelles, pas une protection :

- il compare ligne par ligne. Une valeur sur plusieurs lignes (une clé privée, par exemple) n'est pas reconnue ;
- il ne reconnaît que la valeur exacte. Une version encodée (base64, URL), découpée ou transformée apparaît en clair ;
- une valeur vide n'est pas masquée.

> **Attention** : quiconque peut modifier le fichier de pipeline qui s'exécute peut écrire un script qui affiche ou envoie ailleurs les variables CI du dépôt. Ne donnez des secrets de déploiement qu'à un dépôt dont tous les Contributeurs sont dignes de confiance, et limitez-les avec les [rôles](/docs/utilisation/roles-et-permissions). Les runners reçoivent en clair les variables du dépôt dont ils exécutent un job : voir [Runners Docker](/docs/ci-cd/runners-docker#sécurité).

Avec Kubernetes, les journaux sont repris tels quels du Pod : aucun masquage n'est appliqué.

## Utiliser un secret dans un job

```yaml
jobs:
  publish:
    stage: deploy
    image: alpine:3.20
    script:
      - apk add --no-cache curl
      - 'curl -fsS -H "Authorization: Bearer $DEPLOY_TOKEN" https://deploy.example.com/hook'
```

Ici `DEPLOY_TOKEN` est une variable CI du dépôt. Le fichier complet figure dans [Exemples de pipelines](/docs/ci-cd/exemples#déploiement-par-appel-http).
