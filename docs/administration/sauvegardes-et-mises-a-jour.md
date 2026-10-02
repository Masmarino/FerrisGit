# Sauvegardes et mises à jour

## Ce qu'il faut sauvegarder

L'état de FerrisGit est à deux endroits, et il faut sauvegarder les deux ensemble :

| Quoi | Contient | Comment |
|---|---|---|
| La base PostgreSQL | Comptes, groupes, droits, tickets, demandes de fusion, pipelines et journaux de jobs, réglages (dont le serveur SMTP), journal d'audit, références vers les dépôts. | `pg_dump` |
| Le répertoire `STORAGE_ROOT` | Les dépôts Git, les wikis et les fichiers joints aux releases. | Copie du volume ou du répertoire |

Gardez aussi, **à part et en lieu sûr**, deux valeurs de configuration :

- **`SETTINGS_ENCRYPTION_KEY`** : sans elle, une base restaurée ne peut plus déchiffrer ses secrets (variables CI, secrets de webhooks, mot de passe SMTP, secrets TOTP). Elle ne peut pas être remplacée après coup. Voir [Configuration](/docs/administration/configuration).
- **`JWT_SECRET`** : sans elle, rien n'est perdu de définitif, mais tout le monde doit se reconnecter.

Les variables de votre déploiement (le fichier `.env`, le Secret Kubernetes `ferrisgit-secrets`) doivent donc figurer dans vos sauvegardes, stockées séparément des données.

La base référence les dépôts par leur chemin sur le disque : une base et un répertoire de dépôts de deux moments différents ne correspondent pas. Pour une sauvegarde cohérente, arrêtez le serveur (`docker compose stop ferrisgit-server`) le temps de la copie, ou à défaut sauvegardez pendant une période sans activité.

## Sauvegarder

### Avec Docker Compose

La base, au format personnalisé de PostgreSQL :

```bash
docker compose exec -T postgres pg_dump -U ferrisgit -Fc ferrisgit > ferrisgit-$(date +%F).dump
```

Le répertoire de stockage. Trouvez d'abord le nom du volume (`docker volume ls`, il se termine par `ferrisgit-storage`) :

```bash
docker compose stop ferrisgit-server
docker run --rm -v <nom-du-volume>:/data:ro -v "$PWD":/backup alpine \
  tar czf /backup/ferrisgit-storage-$(date +%F).tgz -C /data .
docker compose start ferrisgit-server
```

### Sur Kubernetes

```bash
kubectl -n ferrisgit exec deploy/ferrisgit-postgres -- pg_dump -U ferrisgit -Fc ferrisgit > ferrisgit.dump
kubectl -n ferrisgit exec deploy/ferrisgit -- tar czf - -C /data . > ferrisgit-storage.tgz
```

Vous pouvez aussi utiliser les instantanés de volume de votre fournisseur de stockage, si le cluster les propose.

Testez vos sauvegardes : restaurez-les de temps en temps sur une instance de test.

## Restaurer

1. Préparez une instance avec **les mêmes** `SETTINGS_ENCRYPTION_KEY` et `JWT_SECRET` (le second est facultatif, voir plus haut).
2. Arrêtez le serveur FerrisGit, en laissant PostgreSQL tourner.
3. Restaurez la base, dans une base vide :

   ```bash
   docker compose exec -T postgres pg_restore -U ferrisgit -d ferrisgit --clean --if-exists < ferrisgit-2026-10-01.dump
   ```

4. Restaurez le répertoire de stockage dans le volume, en conservant les propriétaires : les fichiers doivent appartenir à l'utilisateur `ferrisgit` de l'image (uid 100, gid 101).

   ```bash
   docker run --rm -v <nom-du-volume>:/data -v "$PWD":/backup alpine \
     tar xzf /backup/ferrisgit-storage-2026-10-01.tgz -C /data
   ```

5. Démarrez le serveur : il applique les migrations manquantes éventuelles, puis écoute.

Si vous restaurez sur une instance dotée d'une autre `SETTINGS_ENCRYPTION_KEY`, les secrets chiffrés sont illisibles : voyez [Configuration](/docs/administration/configuration) pour la conséquence et ce qu'il faut ressaisir.

## Les migrations de base de données

Les migrations sont embarquées dans le binaire et appliquées **automatiquement à chaque démarrage**, avant que le serveur écoute. Vous n'avez aucune commande à lancer. Deux protections sont à connaître :

- **Une migration déjà appliquée ne doit jamais être modifiée.** La base garde une somme de contrôle de chaque migration appliquée ; si le fichier a changé, le démarrage échoue avec `failed to run migrations`, et le détail « migration N was previously applied but has been modified ». Tout changement de schéma passe par une nouvelle migration numérotée.
- **Une base plus récente que le binaire est refusée.** Si la base contient une migration que le binaire ne connaît pas (c'est le cas quand on relance une ancienne version après une mise à jour qui avait changé le schéma), le démarrage échoue avec « migration N was previously applied but is missing in the resolved migrations ».

À ce jour, quatre migrations existent : `0001_init` (le schéma complet), `0002_public_pages` (les deux réglages des pages publiques), `0003_pipeline_errors` (l'erreur d'un fichier de pipeline invalide et le statut de job « ignoré ») et `0004_job_log_retention` (les journaux effacés par la rétention). Les notes de chaque version, dans le [CHANGELOG](https://github.com/Masmarino/FerrisGit/blob/main/CHANGELOG.md) et sur la page des releases GitHub, indiquent les migrations qu'elle apporte.

## Mettre à jour

1. Lisez les notes de la nouvelle version (section « Migrations » du CHANGELOG).
2. **Sauvegardez** la base et le stockage (voir plus haut). C'est votre seul vrai retour arrière si la version applique une migration.
3. Mettez à jour l'image :
   - avec l'image publiée, changez l'étiquette de version dans `docker-compose.yml` (voir [Installation](/docs/administration/installation)) puis `docker compose pull` et `docker compose up -d` ;
   - avec la construction locale, `git pull` puis `docker compose up -d --build` ;
   - sur Kubernetes, la CI déploie la nouvelle image avec Helm à chaque tag de version ; à la main, relancez `helm upgrade` avec le nouveau `image.tag` (voir [Déploiement sur Kubernetes](/docs/administration/deploiement-kubernetes)) : la stratégie `Recreate` coupe le service le temps du remplacement.
4. Suivez les journaux : le serveur applique les migrations puis affiche `listening on …`. Vérifiez `GET /readyz` et la page **Admin**, **Santé**.

Utilisez une version précise plutôt que `latest`, pour décider vous-même du moment de la mise à jour. Les migrations sont appliquées avec un verrou dans la base : démarrer deux instances en même temps ne les applique pas deux fois.

### Revenir en arrière

- **Si la nouvelle version n'a appliqué aucune migration**, remettez simplement l'ancienne étiquette d'image et redéployez.
- **Si elle en a appliqué**, l'ancienne version refusera de démarrer sur la base migrée. Restaurez alors la sauvegarde de la base faite avant la mise à jour, avec le stockage correspondant, puis redémarrez l'ancienne version. Les données écrites depuis la mise à jour sont perdues avec la restauration.

## Récupérer un compte qui a perdu son second facteur

Chaque compte doit avoir un second facteur. Si une personne perd tous ses facteurs **et** ses codes de secours, il faut réinitialiser sa double authentification. Elle devra ensuite la reconfigurer à sa prochaine connexion avec son mot de passe.

### Depuis l'interface (cas normal)

Un autre super-administrateur ouvre **Admin**, **Utilisateurs**, le menu de la ligne du compte, puis **Réinitialiser la double authentification**. Cela supprime l'application d'authentification, les clés d'accès et les codes de secours, déconnecte la personne de partout et l'en informe par e-mail. Voir [Utilisateurs et invitations](/docs/administration/utilisateurs).

### Directement dans la base (si plus aucun administrateur ne peut se connecter)

Si le seul administrateur a perdu tous ses facteurs, faites la même chose en SQL. Remplacez `<USER>` par son nom d'utilisateur :

```sql
BEGIN;
-- 1. terminer toutes les sessions et connexions en cours de cet utilisateur
UPDATE users SET token_epoch = token_epoch + 1 WHERE username = '<USER>';
-- 2. retirer les facteurs
DELETE FROM mfa_backup_codes     WHERE user_id = (SELECT id FROM users WHERE username = '<USER>');
DELETE FROM totp_credentials     WHERE user_id = (SELECT id FROM users WHERE username = '<USER>');
DELETE FROM webauthn_credentials WHERE user_id = (SELECT id FROM users WHERE username = '<USER>');
COMMIT;
```

Avec Docker Compose, ouvrez `psql` dans le conteneur :

```bash
docker compose exec postgres psql -U ferrisgit ferrisgit
```

Sur Kubernetes :

```bash
kubectl -n ferrisgit exec -it deploy/ferrisgit-postgres -- psql -U ferrisgit ferrisgit
```

Les quatre opérations sont celles que fait l'interface (le numéro de session `token_epoch` est ce qui invalide les sessions ouvertes). À la prochaine connexion avec son mot de passe, la personne est menée à la configuration de la double authentification.

### Quand le secret TOTP n'est plus lisible

Si le secret TOTP d'un utilisateur ne peut plus être déchiffré (mauvaise `SETTINGS_ENCRYPTION_KEY`, par exemple après une restauration sur une autre instance), sa connexion répond par une erreur serveur (HTTP 500) : c'est voulu, ce n'est jamais un contournement. La même réinitialisation de la double authentification règle le problème.
