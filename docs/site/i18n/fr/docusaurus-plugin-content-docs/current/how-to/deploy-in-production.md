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

Le répertoire `packaging/` du dépôt prépare ce dont un paquet de bureau a
besoin, le bundle Python embarqué et le binaire `llama-server` ; le paquet de
bureau lui-même est assemblé par Tauri via
`crates/apollia-desktop/scripts/bundle-cli.sh`. Ni l'un ni l'autre ne produit
un daemon serveur. Pour un serveur, vous compilez le binaire depuis les sources
et l'encapsulez dans votre système d'init, ce que fait ce guide.

## Compiler un binaire optimisé

```sh
bash packaging/fetch-tesseract-sources.sh
cargo build -p apollia-cli --release --features ocr
```

La feature `ocr`, éteinte par défaut et active dans les archives livrées,
compile le moteur Tesseract embarqué qui lit les pages scannées et les images
des sources. Le script pose les sources de Leptonica et de Tesseract que cette
compilation construit, vérifiées contre
`packaging/tesseract-sources-checksums.txt`, pour qu'elle ne télécharge rien
de non vérifié. Les deux étapes demandent `cmake`, un compilateur C et C++,
`curl`, `shasum` ou `sha256sum`, et `unzip` ou `python3` ; sous Debian ou
Ubuntu, `sudo apt-get install -y cmake build-essential curl unzip`. Une
machine de compilation sans réseau pose `TESSERACT_SOURCES_DIR` sur un dossier
qui contient `leptonica-1.87.0.zip` et `tesseract-5.5.2.zip`, vérifiés de la
même façon. Sans `--features ocr`, le binaire se compile sans ces outils mais
ne lit pas les pages scannées.

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
sudo useradd --system --create-home --home /var/lib/apollia --shell /usr/sbin/nologin apollia
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
    reverse_proxy /api/v1/sessions/*/stream 127.0.0.1:7771 {
        flush_interval -1
    }
    reverse_proxy /api/v1/events 127.0.0.1:7771 {
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
    location ~ ^/api/v1/(tasks/.*/stream|sessions/.*/stream|mailbox/stream|events)$ {
        proxy_pass http://127.0.0.1:7771;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 3600s;
    }
}
```

Les points de terminaison de streaming `GET /api/v1/tasks/{id}/stream`,
`GET /api/v1/sessions/{id}/stream`, `GET /api/v1/mailbox/stream` et
`GET /api/v1/events` envoient les événements au fil de l'eau. Sans
désactivation de la mise en tampon, le proxy les retient jusqu'à la fermeture
de la réponse, et l'hôte ne voit rien en direct. La même précaution
s'applique côté client sous TLS natif : ne mettez pas la réponse en tampon.

## Vérifier après déploiement

```sh
# Disponibilité. La route de santé est le seul chemin de l'API servi sans le
# jeton porteur ; elle ne révèle que le marqueur de produit et la version.
curl -s http://127.0.0.1:7771/api/v1/health
# {"status":"ok","product":"apollia-os","version":"<version du runtime>"}

# État du runtime et des agents
apollia-os status

# De bout en bout : construire le paquet de départ, l'installer et exécuter son agent.
apollia-os package new postcheck
apollia-os package build postcheck
apollia-os package install postcheck-0.1.0.apo
apollia-os run postcheck "post-deploy check"
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
  apollia-os plan cache clear --confirm
  ```

- **Audit.** Chaque action gouvernée est enregistrée dans un journal signé et
  chaîné par hachage. Lisez-le et vérifiez-le avec le
  [flux d'audit](/how-to/audit-and-verify).

## Mettre à jour

```sh
git pull
bash packaging/fetch-tesseract-sources.sh
cargo build -p apollia-cli --release --features ocr
sudo systemctl stop apollia
sudo install -m 755 target/release/apollia-os /usr/local/bin/apollia-os
sudo systemctl start apollia
```

## Inférence locale sur un serveur

Pour servir des modèles GGUF locaux sur le serveur, le daemon lance et
supervise `llama-server` (le llama.cpp amont). Il prend le premier binaire
qu'il trouve, dans cet ordre :

1. le chemin de `APOLLIA_LLAMA_SERVER_BIN`, quand la variable est posée ;
2. un build du magasin de moteurs, selon le réglage `engines.gguf` : la
   variante qu'il nomme, ou, sous `auto`, le build installé classé recommandé
   pour la machine, sinon le meilleur compatible ;
3. un `llama-server` à côté du binaire `apollia-os`, ou dans le dossier
   `runners/` d'une disposition empaquetée autour de lui ;
4. le `PATH` de l'utilisateur du service ;
5. `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, puis `~/.local/bin` et
   `~/.cargo/bin` de l'utilisateur du service.

Une compilation depuis les sources ne livre aucun moteur, si bien que sur un
serveur neuf le magasin de moteurs est le chemin court :
[Activer CUDA sur un serveur](/how-to/enable-cuda-on-a-server) installe un
build pour la carte par `apollia-os engine install`, en ligne ou depuis un
dossier. Un `llama-server` installé à la main, comme décrit dans
[Installer et exécuter le runtime](/how-to/install-and-run#local-gguf-inference),
est trouvé sur le `PATH` quand le magasin ne contient rien. Le traitement par
lots continu et l'appel d'outils natif sont intégrés à ce moteur, si bien
qu'un seul backend local sert déjà des requêtes concurrentes ; voir
[Tirer le meilleur parti de l'inférence locale](/how-to/accelerate-local-inference).

Le daemon lance le moteur sans aucune variable `LLAMA_ARG_*` de son propre
environnement : une variable laissée dans le shell ou l'unité du service ne
peut activer aucune option du moteur ; passez les options supplémentaires par
`APOLLIA_LLAMA_EXTRA_ARGS`.

## Lire les pages scannées sans réseau

Une source lit le texte d'une page scannée ou d'une image par reconnaissance
optique de caractères, qui demande les données de langue du moteur Tesseract
embarqué : un fichier `<langue>.traineddata` par langue. Le daemon récupère
les fichiers épinglés la première fois qu'un passage laisse des pages en
attente de ces données. Un serveur sans réseau prend les mêmes fichiers depuis
un disque. Tout ceci demande un binaire compilé avec `--features ocr`, comme
plus haut ; sans elle, le contrôle `recognition` de `doctor` affiche
`not compiled, <n>/2 languages verified: this build does not read scanned pages`.

| Fichier | Taille en octets | SHA-256 |
|---|---|---|
| `fra.traineddata` | 1130365 | `ced037562e8c80c13122dece28dd477d399af80911a28791a66a63ac1e3445ca` |
| `eng.traineddata` | 4113088 | `7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2` |

Les deux viennent du dépôt amont `tesseract-ocr/tessdata_fast`, au commit
`923915d4ced2a7235221788285785a29c4a42d4a`. Sur une machine qui a le réseau,
prenez-les à ce commit et vérifiez-les :

```sh
for lang in fra eng; do
  curl -fsSL -o "$lang.traineddata" \
    "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/923915d4ced2a7235221788285785a29c4a42d4a/$lang.traineddata"
done
sha256sum fra.traineddata eng.traineddata
```

Copiez-les dans le dossier `tessdata/` du dossier des modèles de l'utilisateur
du service : `<dossier de données>/models/tessdata/`, soit
`/var/lib/apollia/.apollia/models/tessdata/` pour le service ci-dessus, ou
`<dossier>/tessdata/` quand `[models] dir` dans `apollia.toml` désigne un
autre dossier des modèles. Un fichier dont l'empreinte n'est pas celle
épinglée n'est jamais confié au moteur. Les pages en attente sont relues dans
un balayage des sources, sans redémarrage.

Vérifiez-le :

```sh
sudo -u apollia -H apollia-os doctor
sudo -u apollia -H apollia-os source show <id>
```

Le contrôle `recognition` de `doctor` affiche
`compiled, 2/2 languages verified` une fois les deux fichiers en place, et
avertit, avec le dossier lu, tant qu'aucun ne l'est. `source show` compte les
pages d'une source encore `awaiting recognition` ; la ligne disparaît une fois
qu'elles sont lues.

## Voir aussi

- [Installer et exécuter le runtime](/how-to/install-and-run) pour le détail
  de la compilation.
- La [référence CLI](/reference/cli) pour toutes les commandes
  opérationnelles.
- [Auditer et vérifier une exécution](/how-to/audit-and-verify) pour le flux
  de responsabilisation.
