<!-- Bloc à coller dans le APOLLIA.md à la racine de votre workspace.
     Lu par email-triage via le system_prompt (qui demande au Reasoner de consulter ces sections). -->

## Email Triage — Classification Rules

<!-- Règles de classification email pour TON contexte. Exemples :

- urgent : sender ∈ {pdg@boite.com, cofondateur@boite.com}, ou subject contient
  ["URGENT", "deadline aujourd'hui", "blocage prod"]
- important : sender ∈ liste VIP, ou subject mentionne projet actif
- newsletter : sender ∈ {*@substack.com, *@bulletin.*}, list-unsubscribe header présent
- spam : score spamassassin > 5, ou patterns connus
- automatique : sender ∈ {noreply@*, notifications@*}, body sans signature humaine

Modifie cette liste pour refléter TON inbox. -->

(à remplir)

## Email Triage — VIP List

<!-- Liste des expéditeurs prioritaires (toujours triés "important" ou "urgent").

- pdg@maboite.com
- client-cle@example.com
- avocat@cabinet.com

L'agent leur applique le label "VIP" et les escalade systématiquement. -->

(à remplir)

## Email Triage — Auto-Reply Templates

<!-- Templates de réponse automatique (préparés en draft, jamais envoyés sans HITL).

### Template : accusé-réception
Bonjour {{sender_first_name}},

Merci pour votre message reçu le {{received_date}}.
Je reviens vers vous d'ici {{eta}}.

Cordialement,
{{user.name}}

### Template : agenda-meeting
Pour caler un rendez-vous, voici mes disponibilités cette semaine : {{slots}}.

Je reste à votre disposition.

### Template : decline-newsletter
Bonjour,

Merci de votre proposition. Je ne souhaite pas y donner suite pour le moment.

Bonne continuation.
-->

(à remplir)
