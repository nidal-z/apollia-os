---
sidebar_position: 8
title: Déployer en production
---

# Déployer en production

Ce guide fait tourner Apollia comme un service de longue durée sur un serveur
Linux : un binaire optimisé, un gestionnaire de service, une posture réseau
durcie, et les vérifications à effectuer après chaque déploiement. Il suppose
que vous savez déjà compiler et exécuter le daemon en local ; si ce n'est pas
le cas, commencez par [Installer et exécuter le runtime](/how-to/install-and-run).

Le répertoire `packaging/` du dépôt produit des paquets de bureau (DMG et
AppImage) destinés aux utilisateurs finaux, pas un daemon serveur. Pour un
serveur, vous compilez le binaire depuis les sources et l'encapsulez dans
votre système d'init, ce que fait ce guide.

## Compiler un binaire optimisé

```sh
cargo build -p apollia-cli --release
```

Le résultat est `target/release/apollia-os`. Installez-le à l'endroit où
votre service le trouvera :

```sh
sudo install -m 755 target/release/apollia-os /usr/local/bin/apollia-os
```

## Exécuter sous systemd

Aucun fichier d'unité n'est fourni dans le dépôt ; vous en écrivez un qui
encapsule les commandes réelles `start` et `stop`. Exécutez le daemon sous un
utilisateur dédié sans privilèges, afin qu'un agent ne puisse pas lire le
reste du système.

```ini
# /etc/systemd/system/apollia.service
[Unit]
Description=Apollia OS runtime
After=network.target

[Service]
Type=simple
User=apollia
Group=apollia
ExecStart=/usr/local/bin/apollia-os start --port 7771
ExecStop=/usr/local/bin/apollia-os stop
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

```sh
sudo useradd --system --home /var/lib/apollia --shell /usr/sbin/nologin apollia
sudo systemctl daemon-reload
sudo systemctl enable --now apollia
sudo systemctl status apollia
```

## Durcir la posture réseau

- **Par défaut : loopback plus jeton.** L'API TCP écoute par défaut sur
  `127.0.0.1`, et les appelants TCP doivent présenter le jeton porteur écrit
  dans `~/.apollia/api-token` au premier démarrage (le socket Unix repose sur
  une confiance locale). Pour une intégration sur la même machine, conservez
  l'écoute sur loopback et laissez `[api].require_token = true`.
- **Exposez-le via TLS, jamais en clair.** Pour joindre le daemon depuis une
  autre machine, le runtime peut terminer lui-même le TLS : définissez
  `[api].tls_cert` et `[api].tls_key` dans `apollia.toml` (les deux, ou aucun
  des deux) et l'écouteur TCP sert directement du HTTPS, sans composant
  supplémentaire à exploiter. Autre possibilité : conserver l'écoute sur
  loopback et placer devant un proxy inverse qui termine le TLS, en le
  faisant suivre vers `127.0.0.1:7771`. Dans les deux cas, gardez le jeton
  porteur obligatoire.
- **Les écoutes non sécurisées échouent immédiatement.** Écouter sur une
  adresse non-loopback avec `require_token = false` est refusé au démarrage,
  si bien qu'un port public n'est jamais exposé sans authentification par
  accident. Gardez l'exigence de jeton active pour toute interface exposée.
- **Protégez le fichier du jeton.** `~/.apollia/api-token`, pour
  l'utilisateur du service, donne un contrôle total sur le runtime. Gardez-le
  lisible uniquement par cet utilisateur.
- **Prérequis de sandboxing (Linux), qui entrent en conflit avec
  l'utilisateur sans privilèges ci-dessus.** `bash_executor` et
  `python_executor` isolent leur processus enfant avec
  `unshare --pid --mount --fork`. Ces options sont appelées **sans**
  `--user`, ce qui nécessite `CAP_SYS_ADMIN` : activer les espaces de noms
  utilisateur non privilégiés sur l'hôte ne l'accorde pas. Sous un service
  `User=apollia` classique, les deux exécuteurs échouent, et rien ne s'exécute
  hors de l'espace de noms. Le refus parvient à l'appelant sous la forme du code
  de sortie du programme lui-même, et non d'une erreur de bac à sable distincte :
  ne le lisez pas comme un signal de refus par défaut.
  <!-- claim:unshare-sandbox-requires-cap-sys-admin -->

  Choisissez-en une, en connaissance de cause :

  - accorder la capacité à l'unité, `AmbientCapabilities=CAP_SYS_ADMIN` plus
    `CapabilityBoundingSet=CAP_SYS_ADMIN`, et conserver l'utilisateur sans
    privilèges ;
  - ou fonctionner sans les deux outils d'exécution de code, en les
    désactivant dans `[tools]` ;
  - ou accepter qu'ils échoueront au moment de l'appel.

  Exécuter le service en tant que root pour obtenir cette capacité échange un
  outil confiné contre un daemon non confiné, ce qui est l'inverse de ce
  qu'il faut faire.

### Proxy inverse avec terminaison TLS

Si votre infrastructure termine déjà le TLS en amont (Caddy, nginx, un
ingress), laissez `[api].tls_cert` / `[api].tls_key` non définis, gardez le
daemon sur loopback, et faites suivre vers `127.0.0.1:7771`.

```
apollia.example.com {
    reverse_proxy 127.0.0.1:7771
    # SSE : désactiver la mise en tampon pour les points de terminaison de streaming
    reverse_proxy /api/v1/tasks/*/stream 127.0.0.1:7771 {
        flush_interval -1
    }
    reverse_proxy /api/v1/mailbox/stream 127.0.0.1:7771 {
        flush_interval -1
    }
}
```

```nginx
server {
    listen 443 ssl;
    server_name apollia.example.com;
    ssl_certificate     /etc/ssl/apollia/fullchain.pem;
    ssl_certificate_key /etc/ssl/apollia/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:7771;
        proxy_set_header Authorization $http_authorization;
    }
    # SSE : streaming sans mise en tampon
    location ~ ^/api/v1/(tasks/.*/stream|mailbox/stream)$ {
        proxy_pass http://127.0.0.1:7771;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 3600s;
    }
}
```

Les points de terminaison de streaming `GET /api/v1/tasks/{id}/stream` et
`GET /api/v1/mailbox/stream` envoient les événements au fil de l'eau. Sans
désactivation de la mise en tampon, le proxy les retient jusqu'à la fermeture
de la réponse, et l'hôte ne voit rien en direct. La même précaution
s'applique côté client sous TLS natif : ne mettez pas la réponse en tampon.

## Vérifier après déploiement

```sh
# Disponibilité
curl http://127.0.0.1:7771/api/v1/health          # {"status":"ok"}

# État du runtime et des agents
apollia-os status

# De bout en bout : installer un agent trivial et l'exécuter
apollia-os agent install clients/examples/echo_agent.py --skip-tests
apollia-os run echo "post-deploy check"
```

Les mêmes informations que lit la CLI sont disponibles via HTTP pour une
intégration côté hôte ; voir la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api).

## Exploiter le service en cours d'exécution

- **Journaux.** Suivez le journal du runtime avec `apollia-os logs
  --follow`, ou lisez le journal du service avec `journalctl -u apollia -f`.
- **Cache de plans.** Les exécutions orchestrées mettent leurs plans en
  cache. Inspectez-le ou videz-le pour diagnostiquer une planification
  obsolète :

  ```sh
  apollia-os plan cache stats
  apollia-os plan cache clear
  ```

- **Audit.** Chaque action gouvernée est enregistrée dans un journal signé et
  chaîné par hachage. Lisez-le et vérifiez-le avec le
  [flux d'audit](/how-to/audit-and-verify).

## Mettre à jour

```sh
git pull
cargo build -p apollia-cli --release
sudo systemctl stop apollia
sudo install -m 755 target/release/apollia-os /usr/local/bin/apollia-os
sudo systemctl start apollia
```

## Inférence locale sur un serveur

Pour servir des modèles GGUF locaux sur le serveur, rendez `llama-server`
(le llama.cpp amont) disponible pour le daemon : celui-ci le lance et le
supervise, et le trouve via le `PATH` de l'utilisateur du service.
Installez-le une fois, là où le service s'exécute, exactement comme décrit
dans [Installer et exécuter le runtime](/how-to/install-and-run#local-gguf-inference).
Le traitement par lots continu et l'appel d'outils natif sont intégrés à ce
moteur, si bien qu'un seul backend local sert déjà des requêtes concurrentes ;
voir [Tirer le meilleur parti de l'inférence locale](/how-to/accelerate-local-inference).

## Voir aussi

- [Installer et exécuter le runtime](/how-to/install-and-run) pour le détail
  de la compilation.
- La [référence CLI](/reference/cli) pour toutes les commandes
  opérationnelles.
- [Auditer et vérifier une exécution](/how-to/audit-and-verify) pour le flux
  de responsabilisation.
