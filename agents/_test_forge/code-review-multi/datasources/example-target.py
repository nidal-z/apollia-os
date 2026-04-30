# Datasource d'exemple : un fichier Python à reviewer
# (Volontairement avec quelques anti-patterns pour démontrer ce que les workers détectent.)

import hashlib

API_KEY = "sk_live_abcd1234EXEMPLE"  # ⚠️ security : credential en dur


def find_user(users: list, email: str):
    # ⚠️ perf : O(n) lookup, pas de structure indexée
    # ⚠️ style : pas de type hints, naming générique
    for u in users:
        if u["email"] == email:
            return u
    return None


def hash_password(password: str) -> str:
    # ⚠️ security : MD5 pour mot de passe
    return hashlib.md5(password.encode()).hexdigest()


def query_users(emails):
    # ⚠️ perf : N+1 query
    results = []
    for email in emails:
        user = db.query(f"SELECT * FROM users WHERE email = '{email}'")  # ⚠️ security : SQL injection
        results.append(user)
    return results
