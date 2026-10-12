# Personal data: research and proposals (area 15)

Research for [area 15](../15-personal-data.md): what GDPR and similar laws ask
of an application's code, how privacy tools and frameworks handle it, and
three ways Jadpo could build it in. Checked 2026-10-12; claims marked
*uncertain* come from secondary sources.

## 1. What the law asks of the code

| Requirement | Source | What the application must do |
|---|---|---|
| Minimisation, storage limitation, accountability | GDPR Art 5(1)(c), (e), 5(2) ([text](https://gdpr-info.eu/art-5-gdpr/)) | Hold only what is needed, for a limited time, and be able to show it |
| Access | Art 15 ([text](https://gdpr-info.eu/art-15-gdpr/)) | A copy of the data, plus purposes, recipients and retention |
| Erasure | Art 17 ([text](https://gdpr-info.eu/art-17-gdpr/)) | Delete on request; 17(3) lets you keep data for a legal obligation or legal claims |
| Tell recipients | Art 19 ([text](https://gdpr-info.eu/art-19-gdpr/)) | Pass the erasure on to whoever received the data, unless impossible |
| Restriction | Art 18 ([text](https://gdpr-info.eu/art-18-gdpr/)) | Store but stop using data while a dispute is open |
| Portability | Art 20 ([text](https://gdpr-info.eu/art-20-gdpr/)) | "Structured, commonly used and machine-readable" copy, for consent or contract data |
| Privacy by design and default | Art 25 ([text](https://gdpr-info.eu/art-25-gdpr/)) | Defaults limit amount, retention and who can access |
| Records of processing | Art 30 ([text](https://gdpr-info.eu/art-30-gdpr/)) | Purposes, categories of people and data, recipients, transfers, erasure time limits, security measures |
| Security | Art 32 ([text](https://gdpr-info.eu/art-32-gdpr/)) | Pseudonymisation and encryption named as measures; restore after incidents |
| Deadline | Art 12(3) ([text](https://gdpr-info.eu/art-12-gdpr/)) | One month, extendable by two |
| Special categories | Arts 9, 10 ([9](https://gdpr-info.eu/art-9-gdpr/), [10](https://gdpr-info.eu/art-10-gdpr/)) | Ethnicity, politics, religion, union membership, genetics, biometrics, health, sex life; criminal data separately |

CCPA/CPRA ([statute](https://cppa.ca.gov/regulations/pdf/ccpa_statute.pdf))
adds the same rights to know (§1798.110, 45 days) and delete (§1798.105,
with exceptions for completing a contract, security, debugging and legal
obligations), and a "sensitive personal information" list (§1798.140(ae))
that adds government IDs, precise geolocation and message contents to GDPR's
special categories.

**Backups.** The ICO says the key is to put backup data
["beyond use"](https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/individual-rights/individual-rights/right-to-erasure/)
until the backup cycle overwrites it; a restore must not bring erased data
back into use (*uncertain*: the wording on restores comes from secondary
sources).

**Pseudonymised data is still personal data** for whoever holds the key
([ICO, 2025](https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/data-sharing/anonymisation/pseudonymisation/);
EDPB Guidelines 01/2025). Encryption is "typically a pseudonymisation
technique" ([ICO](https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/security/encryption/encryption-and-data-protection/)).

**Logging access** is not a named article; it follows from accountability and
security, and the ICO's audit framework expects access to records to be
[monitored](https://ico.org.uk/for-organisations/advice-and-services/audits/data-protection-audit-framework/toolkits/records-management/access/).

**Residency.** After Schrems II, the EDPB's
[Recommendations 01/2020](https://www.edpb.europa.eu/our-work-tools/our-documents/recommendations/recommendations-012020-measures-supplement-transfer_en)
treat remote access from another country as a transfer too, so EU-only
storage alone does not settle it. Per-region databases are common practice,
not a legal requirement.

## 2. How others do it

- **Fides (Ethyca), privacy as code.** Dataset files tag each field with
  `data_categories` from the open
  [fideslang taxonomy](https://ethyca.github.io/fideslang/taxonomy/data_categories/)
  (`user.contact.email`, `user.health_and_medical`, `user.government_id`, …),
  mark identity fields (`identity: email`) and `references` between
  collections ([reference](https://www.ethyca.com/docs/dev-docs/datasets/dataset-annotation-reference)).
  Access and erasure requests walk that graph from the identity. Erasure
  applies a masking strategy per category: null, fixed string, random
  string, hash, HMAC or encryption
  ([strategies](https://ethyca.com/docs/dev-docs/privacy-requests/masking-strategies)).
  Collections can set `erase_after` ordering. Categories and uses are
  extensible.
- **Transcend and OneTrust** discover personal data after the fact by
  scanning schemas and content with name matching, patterns and models
  ([Transcend](https://docs.transcend.io/docs/articles/classification/classification-technical-overview),
  [OneTrust](https://onetrust.com/blog/how-data-discovery-enhances-your-dsar-workflow/)),
  then run requests against each system. A compiler that already knows every
  field does not need discovery.
- **Django.** [django-gdpr-assist](https://django-gdpr-assist.readthedocs.io/en/latest/privacy_meta.html)
  adds a `PrivacyMeta` per model (fields to anonymise, export fields), an
  `on_delete=ANONYMISE(SET_NULL)` for related rows, and logs every erasure so
  [`gdpr_rerun`](https://django-gdpr-assist.readthedocs.io/en/latest/commands.html)
  can replay them after a backup is restored.
- **Rails.** [`encrypts`](https://guides.rubyonrails.org/v7.2/active_record_encryption.html)
  encrypts columns; `deterministic: true` keeps equality lookups and unique
  indexes but loses `LIKE`, grouping and bulk updates. Erasure and export are
  gems with per-model policy classes, such as
  [gdpr_admin](https://codeclimate.com/github/Colex/gdpr_admin/README.md/source).
- **Prisma and Hasura** have no personal-data concept. Prisma's `///`
  comments can feed a custom generator
  ([schema docs](https://www.prisma.io/docs/orm/prisma-schema/overview));
  Hasura hides columns per role
  ([permissions](https://hasura.io/docs/2.0/auth/authorization/permissions/column-presets/)).

**Crypto-shredding** encrypts each person's data with their own key and
erases by deleting the key, which reaches backups too. NIST SP 800-88 Rev 2
([2025](https://csrc.nist.gov/pubs/sp/800/88/r2/final)) accepts
cryptographic erase for media. Practitioners note that keys in backups
defeat it, managed-key deletion is delayed, and anything derived in plain
text (indexes, caches, search, logs) needs its own erasure
([Conduktor](https://www.conduktor.io/glossary/crypto-shredding-for-kafka)).
No regulator guidance found says key deletion satisfies Article 17
(*uncertain*); treat it as a strong "beyond use" measure.

## 3. Options

All three use the same marking, `personal: PersonalData.contact`, and the
same report; they differ in how erasure and export are found and done.

### Option A: annotations only

Fields are marked; the team writes its own erase and export actions. The
compiler hashes marked fields in logs and produces the report.

```jadpo fragment
email: Email {
    personal: PersonalData.contact
}
```

Cheap and predictable, but the hard part (finding every copy) stays manual,
which is exactly where Rails and Django apps go wrong when a table is added.
It does not deliver "out of the box".

### Option B: a person graph with generated erase and export (recommended)

One entity says `person: true`; the compiler follows references to find what
belongs to each person and generates `User.erase(id)` and `User.export(id)`.
Erasure takes its outcome from `on_delete:` and needs an explicit `erase:`
only where that is `restricted`:

```jadpo fragment
owner_id: User.id {
    on_delete: OnDelete.restricted
    erase: Erasure.deleted
}
customer_id: User.id? {
    on_delete: OnDelete.restricted
    erase: Erasure.retained          // kept for tax, with its own retention
}
```

Backups are handled by an erasure log of ids replayed after a restore, as
`gdpr_rerun` does; caches are wiped at once (area 5). This is Fides' graph
walk, done by the compiler that already knows every reference, with nothing
to keep in sync. The cost: three outcome names to learn, a compile error when
an erasure cannot complete, and an erasure log the runtime must replay.

### Option C: B plus crypto-shredding

As B, but marked fields are encrypted with a per-person key, and erasure
deletes the key, so backups and replicas become unreadable without a replay.

```jadpo fragment
application {
    personal_data: {
        keys: config.personal_data_keys   // a key service
    }
}
```

It reaches backups, but unique rules and lookups on `email` need
deterministic encryption or a separate hash, ordering, `LIKE` and search
stop working on marked fields, every read pays a decryption, and the
application needs a key service in every environment, tests included. The
legal benefit is unconfirmed.

## 4. Recommendation

**Option B.** It is the only option that makes erasure and export work
without hand-written code, and it costs little syntax because references
already say who owns what. Keep crypto-shredding as a later option for
sensitive categories, if a health or similar application needs backups
erased without a replay. Leave purpose, lawful basis and consent to the
privacy notice and to application data: the compiler cannot check them, and
the report gives the team everything else an Article 30 record needs.
