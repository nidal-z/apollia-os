# Installation Production — Apollia OS

> Déploiement d'Apollia OS en conditions de production sur Linux.
> Public cible : administrateur système, ingénieur DevOps

---

## Prérequis système

| Composant | Version | Notes |
|---|---|---|
| Linux kernel | 4.18+ | Pour les user namespaces (sandbox) |
| Python | 3.11+ | Requis par les agents |
| SQLite | 3.35+ (FTS5) | Inclus dans la plupart des distros récentes |
| Rust | 1.75+ | Pour compiler le binaire |
| RAM | 256 MB min | 512 MB recommandé pour agents complexes |
| Disque | 500 MB | Build + venvs Python + données mémoire |

Vérifier les user namespaces (sandbox) :
```bash
cat /proc/sys/kernel/unprivileged_userns_clone
# Doit afficher 1 (ou le paramètre ne doit pas exister)

# Si 0, activer :
echo 1 > /proc/sys/kernel/unprivileged_userns_clone
# Pour le rendre permanent :
echo "kernel.unprivileged_userns_clone = 1" >> /etc/sysctl.conf
```

---

## Build optimisé

```bash
git clone https://github.com/nidal-z/apollia-os.git
cd apollia-os

# Binaire cloud uniquement (léger, ~20–30 MB) — backends API Anthropic/OpenAI
cargo build --workspace --release

# Vérifier la taille du binaire
ls -lh target/release/apollia-os
# Environ 20-30 MB (statiquement lié, cloud uniquement)
```

**Avec inférence locale (modèle .gguf sur la machine) :**

```bash
# CPU (Linux x86_64, ARM)
cargo build --workspace --release --features local

# GPU Apple Silicon macOS — fonctionne directement (MISTRALRS_METAL_PRECOMPILE=0 dans .cargo/config.toml)
cargo build --workspace --release --features local-metal

# Avec précompilation des shaders (Xcode complet requis, optimal pour la distribution)
MISTRALRS_METAL_PRECOMPILE=1 cargo build --workspace --release --features local-metal
```

Le binaire avec `--features local` est plus lourd (~200–400 MB selon la plateforme — moteur d'inférence mistralrs lié statiquement). La taille du modèle `.gguf` (1–8 GB) n'est **pas** dans le binaire : elle est chargée depuis `~/.apollia/models/` au démarrage.

---

## Installation système

```bash
# Binaire
install -m 755 target/release/apollia-os /usr/local/bin/apollia-os

# Répertoires de données
mkdir -p /var/lib/apollia/{memory,venvs,agents}
mkdir -p /etc/apollia
mkdir -p /run/apollia

# Configuration
cat > /etc/apollia/apollia.toml << 'EOF'
[runtime]
log_level = "warn"
socket = "/run/apollia/apollia.sock"
port   = 7771
drain_timeout_seconds = 60

[memory]
path = "/var/lib/apollia/memory.db"
max_size_mb = 2048
episode_ttl_days = 90

[tools]
sandbox = true
venv_base_path = "/var/lib/apollia/venvs"

[api]
bind_address = "127.0.0.1"

[budget]
max_steps = 10
max_tool_calls = 20
wall_clock_timeout_secs = 300
EOF
```

---

## Service systemd

```bash
cat > /etc/systemd/system/apollia-os.service << 'EOF'
[Unit]
Description=Apollia OS Runtime
After=network.target
Wants=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/apollia-os start --foreground --config /etc/apollia/apollia.toml
ExecStop=/usr/local/bin/apollia-os stop
Restart=on-failure
RestartSec=5s
TimeoutStopSec=60s

# Sécurité
User=apollia
Group=apollia
WorkingDirectory=/var/lib/apollia

# Journaux
StandardOutput=journal
StandardError=journal
SyslogIdentifier=apollia-os

# Limites
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF

# Créer l'utilisateur système
useradd --system --home /var/lib/apollia --shell /usr/sbin/nologin apollia
chown -R apollia:apollia /var/lib/apollia /run/apollia

# Activer et démarrer
systemctl daemon-reload
systemctl enable apollia-os
systemctl start apollia-os
systemctl status apollia-os
```

---

## Vérification post-déploiement

```bash
# Statut du service
systemctl status apollia-os

# Logs en temps réel
journalctl -u apollia-os -f

# Test API
curl http://localhost:7771/api/v1/health
# {"status":"ok","version":"0.1.0"}

# Déployer un agent
apollia-os --socket /run/apollia/apollia.sock agent start /var/lib/apollia/agents/hello_agent.py

# Test end-to-end
apollia-os run hello-agent "test production"
```

---

## Considérations sécurité

**Isolation réseau :** L'API est liée sur `127.0.0.1` par défaut. Ne jamais exposer sur `0.0.0.0` sans reverse proxy + authentification.

**Agents et sandbox :** Les `bash_executor` et `python_executor` utilisent les Linux namespaces (unshare) pour l'isolation. Vérifier que `unprivileged_userns_clone = 1`.

**Permissions fichiers :** L'utilisateur `apollia` ne doit pas pouvoir lire des fichiers sensibles système. Configurer `file_io` avec des chemins racines restreints.

**Réseau des agents :** Par défaut `network_allowlist: null` = pas d'accès réseau depuis les outils. Les agents LLM qui appellent des APIs externes doivent déclarer la whitelist dans leur manifest.

---

## Mise à jour

```bash
# 1. Compiler la nouvelle version
git pull && cargo build --workspace --release

# 2. Arrêter proprement (drain 30s)
apollia-os stop

# 3. Remplacer le binaire
install -m 755 target/release/apollia-os /usr/local/bin/apollia-os

# 4. Redémarrer
systemctl start apollia-os
```

---

## Voir aussi

- [Config apollia.toml](./Config-apollia-toml) — toutes les options de configuration
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) — monitoring et debug en production
- [Sécurité Sandbox Isolation](./Securite-Sandbox-Isolation) — Linux namespaces détaillés
- [ADR-005](../adr/ADR-005-sandbox-sans-docker) — pourquoi namespaces plutôt que Docker
