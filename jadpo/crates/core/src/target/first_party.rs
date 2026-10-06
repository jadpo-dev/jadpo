use super::*;

impl TargetGenerator<'_> {
    fn authentication_authority_fields<'a>(
        &'a self,
        resolution: &jadpo_syntax::AuthenticationResolutionDeclaration,
    ) -> Vec<&'a FieldDeclaration> {
        let entity = &resolution.authority.path[0].text;
        let record = self.records.get(entity).expect("checked authority entity");
        let lifecycle_owned = self.lifecycle_owned_field_names(entity);
        if lifecycle_owned.is_empty() {
            return record.fields.iter().collect();
        }

        let mut selected = lifecycle_owned;
        selected.insert(resolution.authority.path[1].text.clone());
        selected.extend(
            resolution
                .mappings
                .iter()
                .map(|mapping| mapping.source.text.clone()),
        );
        record
            .fields
            .iter()
            .filter(|field| selected.contains(&field.name.text))
            .collect()
    }

    fn first_party_strength_supported(&self, reference: &TypeReference) -> bool {
        let root = self.representation_root_for(reference);
        root == "Text"
            || self.enums.get(&root).is_some_and(|declaration| {
                let variants = declaration
                    .variants
                    .iter()
                    .map(|variant| variant.name.text.as_str())
                    .collect::<BTreeSet<_>>();
                variants == BTreeSet::from(["primary", "multi_factor"])
            })
    }

    pub(super) fn first_party_supported(&self) -> bool {
        if !self.has_authentication() || self.configuration.is_none() {
            return false;
        }
        let bounded = self.application.unwrap().authentication.revocation.mode
            == jadpo_syntax::RevocationMode::Bounded;
        self.authentication_strategies.iter().all(|strategy| {
            let mut modes = BTreeSet::new();
            if strategy.validators.is_empty() {
                return false;
            }
            if strategy.resolutions.iter().any(|resolution| {
                !strategy
                    .validators
                    .iter()
                    .any(|validator| validator.principal.text == resolution.principal.text)
            }) {
                return false;
            }
            for validator in &strategy.validators {
                let kind = validator.principal.text.as_str();
                let mode = validator.mode.text.as_str();
                if !modes.insert((kind, mode))
                    || !matches!(
                        (kind, mode),
                        ("user", "signed" | "opaque" | "jwt") | ("service", "api_key" | "signed")
                    )
                    || (mode == "signed" && !bounded)
                {
                    return false;
                }
                let cookie = matches!(
                    strategy.transport.location,
                    jadpo_syntax::CredentialLocation::Cookie(_)
                );
                if (kind == "service" || mode == "jwt") && cookie {
                    return false;
                }
                let names = validator
                    .settings
                    .iter()
                    .map(|s| s.name.text.as_str())
                    .collect::<BTreeSet<_>>();
                if (mode != "jwt" && !names.contains("secret"))
                    || (mode == "jwt" && !names.contains("issuer"))
                    || !names.contains("audience")
                    || (cookie && !names.contains("origin"))
                {
                    return false;
                }
                let Some(resolution) = strategy
                    .resolutions
                    .iter()
                    .find(|r| r.principal.text == kind)
                else {
                    return false;
                };
                if resolution.authority.path.len() != 2 {
                    return false;
                }
                let Some(record) = self.records.get(&resolution.authority.path[0].text) else {
                    return false;
                };
                if !record.is_persistent_entity() {
                    return false;
                }
                let id_name = if kind == "user" {
                    "user_id"
                } else {
                    "service_id"
                };
                let variant = self
                    .principal
                    .unwrap()
                    .variants
                    .iter()
                    .find(|v| v.kind.as_str() == kind)
                    .unwrap();
                if !variant.fields.iter().any(|f| f.name.text == id_name)
                    || variant.fields.iter().any(|f| {
                        f.optional
                            || self.reference_is_nullable(&f.field_type)
                            || !matches!(
                                f.name.text.as_str(),
                                "subject" | "authentication_strength"
                            ) && f.name.text != id_name
                            || f.name.text == "subject"
                                && self.representation_root_for(&f.field_type) != "Text"
                            || f.name.text == "authentication_strength"
                                && !self.first_party_strength_supported(&f.field_type)
                    })
                {
                    return false;
                }
                let Some(id) = resolution
                    .mappings
                    .iter()
                    .find(|m| m.target.path.last().is_some_and(|n| n.text == id_name))
                else {
                    return false;
                };
                let id_field = record.fields.iter().find(|f| f.name.text == id.source.text);
                let direct_identity = id_field.is_some_and(|f| {
                    f.persistence.contains(&PersistenceModifier::Identity)
                        && self.representation_root_for(&f.field_type) == "Uuid"
                });
                if !direct_identity
                    || !resolution.mappings.iter().any(|m| {
                        m.source.text == resolution.authority.path[1].text
                            && m.target.path.last().is_some_and(|n| n.text == "subject")
                    })
                {
                    return false;
                }
                if let Some(binding) = &validator.credentials {
                    let Some(credential_record) = self.records.get(&binding.identity.path[0].text)
                    else {
                        return false;
                    };
                    let Some((active_field, _)) = self.credential_active_binding(&binding.active)
                    else {
                        return false;
                    };
                    if self.credential_active_binding(&resolution.active).is_none() {
                        return false;
                    }
                    let supplied = [
                        &binding.identity,
                        &binding.principal,
                        &binding.verifier,
                        &binding.expires,
                        &binding.revoked,
                    ]
                    .map(|name| name.path[1].text.as_str());
                    if credential_record.fields.iter().any(|field| {
                        field.generated == Some(jadpo_syntax::GeneratedFieldRole::CreateOrChange)
                            || !supplied.contains(&field.name.text.as_str())
                                && field.name.text != active_field
                                && !(matches!(
                                    field.generated,
                                    Some(jadpo_syntax::GeneratedFieldRole::Create)
                                ) && self.representation_root_for(&field.field_type)
                                    == "Instant")
                                && !field.optional
                                && !self.reference_is_nullable(&field.field_type)
                    }) {
                        return false;
                    }
                }
                if mode == "api_key" {
                    let Some(owner) = validator.settings.iter().find(|s| s.name.text == "owner")
                    else {
                        return false;
                    };
                    let Expression::Name(owner) = &owner.value else {
                        return false;
                    };
                    if owner.path.len() != 2 || owner.path[0].text != record.name.text {
                        return false;
                    }
                    let Some(field) = record
                        .fields
                        .iter()
                        .find(|f| f.name.text == owner.path[1].text)
                    else {
                        return false;
                    };
                    if field.optional || self.reference_is_nullable(&field.field_type) {
                        return false;
                    }
                    let Some(reference) = &field.reference else {
                        return false;
                    };
                    let target = type_name(&reference.target);
                    let Some((entity, field)) = target.split_once('.') else {
                        return false;
                    };
                    let Some(owner_record) = self.records.get(entity) else {
                        return false;
                    };
                    if !owner_record.is_persistent_entity()
                        || !owner_record.fields.iter().any(|f| {
                            f.name.text == field
                                && f.persistence.contains(&PersistenceModifier::Identity)
                        })
                    {
                        return false;
                    }
                }
            }
            // A bounded service token can only originate from the declared key
            // authority in this same credential slot; there is no host mint shortcut.
            !modes.contains(&("service", "signed")) || modes.contains(&("service", "api_key"))
        })
    }

    pub(super) fn first_party_application(&self, output: &mut String) {
        if !self.first_party_supported() {
            return;
        }
        line(output, "let firstPartyAuthentication: Awaited<ReturnType<typeof createFirstPartyAuthentication>> | undefined;");
        line(output, "let authenticationInitialization = 0;");
        line(output, "async function initializeAuthentication() {");
        line(
            output,
            "  const generation = ++authenticationInitialization;",
        );
        line(output, "  firstPartyAuthentication?.invalidateDeliveryCredentialProofs(); firstPartyAuthentication = undefined;");
        line(output, "  const resolvePrincipal = async (strategy: string, subject: string, strength: string, principalKind: \"user\" | \"service\" = \"user\"): Promise<import(\"./authentication.ts\").ResolutionResult> => {");
        line(
            output,
            "    const rows = await resolveAuthenticationAuthority(strategy, subject, principalKind);",
        );
        line(
            output,
            "    if (rows.length === 0) return { kind: \"missing\" };",
        );
        line(
            output,
            "    if (rows.length !== 1) return { kind: \"duplicate\" };",
        );
        line(output, "    switch (`${strategy}:${principalKind}`) {");
        for strategy in &self.authentication_strategies {
            for resolution in &strategy.resolutions {
                let entity = &resolution.authority.path[0].text;
                let record = self.records.get(entity).unwrap();
                let lifecycle_owned = self.lifecycle_owned_field_names(entity);
                let id = resolution
                    .mappings
                    .iter()
                    .find(|m| {
                        m.target.path.last().is_some_and(|n| {
                            n.text
                                == if resolution.principal.text == "user" {
                                    "user_id"
                                } else {
                                    "service_id"
                                }
                        })
                    })
                    .unwrap();
                line(
                    output,
                    &format!(
                        "      case {}: {{",
                        ts_string(&format!(
                            "{}:{}",
                            strategy.name.text, resolution.principal.text
                        ))
                    ),
                );
                line(
                    output,
                    "        const authorityRow = expectObject(rows[0], \"authentication.authority\");",
                );
                if lifecycle_owned.is_empty() {
                    let fields = record
                        .fields
                        .iter()
                        .map(|f| f.name.text.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    line(
                        output,
                        &format!("        const {{ {fields} }} = validate_{entity}({{"),
                    );
                    for field in &record.fields {
                        let raw = format!("authorityRow[{}]", ts_string(&field.name.text));
                        let decoded = self.database_decode_expression(
                            &field.field_type,
                            &raw,
                            "\"authentication.authority\"",
                        );
                        line(
                            output,
                            &format!("          {}: {},", field.name.text, decoded),
                        );
                    }
                    line(output, "        }, \"authentication.authority\");");
                } else {
                    for field in self.authentication_authority_fields(resolution) {
                        let raw = format!("authorityRow[{}]", ts_string(&field.name.text));
                        let decoded = self.database_decode_expression(
                            &field.field_type,
                            &raw,
                            &ts_string(&format!("authentication.authority.{}", field.name.text)),
                        );
                        let validator = self
                            .field_validator_name(&format!("{entity}.{}", field.name.text))
                            .expect("authority field has a validator");
                        line(
                            output,
                            &format!(
                                "        const {} = {validator}({decoded}, {});",
                                field.name.text,
                                ts_string(&format!("authentication.authority.{}", field.name.text))
                            ),
                        );
                    }
                }
                line(
                    output,
                    &format!(
                        "        if (!({})) return {{ kind: \"inactive\", failureName: {} }};",
                        self.expression(&resolution.active),
                        ts_string(&resolution.inactive.text)
                    ),
                );
                line(output, &format!("        return {{ kind: \"active\", principal: {{ kind: {}, subject: {}, authenticationStrength: firstPartyAuthenticationStrength({}, strength), values: {{ {}: {} }} }} }};", ts_string(&resolution.principal.text), resolution.authority.path[1].text, ts_string(&resolution.principal.text), if resolution.principal.text == "user" { "user_id" } else { "service_id" }, id.source.text));
                line(output, "      }");
            }
        }
        line(
            output,
            "      default: throw new AuthenticationFault(\"authentication_misconfigured\", 503);",
        );
        line(output, "    }");
        line(output, "  };");
        if self.has_jwt() {
            line(output, "  const jwt = createJwtAuthentication([");
            for strategy in &self.authentication_strategies {
                for validator in &strategy.validators {
                    if validator.mode.text != "jwt" {
                        continue;
                    }
                    let setting = |name: &str| {
                        validator
                            .settings
                            .iter()
                            .find(|s| s.name.text == name)
                            .map(|s| self.expression(&s.value))
                            .unwrap_or("undefined".to_owned())
                    };
                    line(
                        output,
                        &format!(
                            "    {{ name: {}, issuer: {}, audience: {}, jwksUri: {} }},",
                            ts_string(&strategy.name.text),
                            setting("issuer"),
                            setting("audience"),
                            setting("jwks_uri")
                        ),
                    );
                }
            }
            line(output, "  ]);");
            line(output, "  if (!jwt.configured) throw new AuthenticationFault(\"authentication_misconfigured\", 503);");
            let slots = self
                .authentication_strategies
                .iter()
                .filter(|s| s.validators.iter().any(|v| v.mode.text == "jwt"))
                .map(|s| ts_string(&s.name.text))
                .collect::<Vec<_>>()
                .join(", ");
            line(output, &format!("  const jwtSlots = new Set([{slots}]);"));
            output.push_str(r#"
  const external = { async validate(name: string, credential: string, now: number): Promise<import("./authentication.ts").ValidationResult> {
    if (!jwtSlots.has(name)) return { kind: "invalid" };
    const result = await jwt.verify(name, credential, now);
    if (result.kind !== "valid") return result;
    const resolution = await resolvePrincipal(name, result.subject, result.authenticationStrength, "user");
    return { kind: "authoritative", principalKind: "user", subject: result.subject, resolution };
  } };
"#);
        }
        line(
            output,
            "  const initialized = await createFirstPartyAuthentication([",
        );
        let delay = self.application.unwrap().authentication.revocation.maximum_delay.as_ref().map(|v| format!("durationMilliseconds(normalizeConfigurationDuration({}, \"authentication.maximum_delay\"), \"authentication.maximum_delay\")", ts_string(&v.text))).unwrap_or("0".to_owned());
        for strategy in &self.authentication_strategies {
            for validator in &strategy.validators {
                if validator.mode.text == "jwt" {
                    continue;
                }
                let setting = |name: &str| {
                    validator
                        .settings
                        .iter()
                        .find(|s| s.name.text == name)
                        .map(|s| self.expression(&s.value))
                        .unwrap_or("undefined".to_owned())
                };
                let cookie = match &strategy.transport.location {
                    jadpo_syntax::CredentialLocation::Cookie(c) => ts_string(unquote(&c.text)),
                    _ => "null".to_owned(),
                };
                line(output, &format!("    {{ name: {}, principal: {}, mode: {}, cookie: {}, secret: {}, previousSecret: {}, audience: {}, origin: {} ?? null, maximumDelayMs: {} }},", ts_string(&strategy.name.text), ts_string(&validator.principal.text), ts_string(&validator.mode.text), cookie, setting("secret"), setting("previous_secret"), setting("audience"), setting("origin"), delay));
            }
        }
        line(
            output,
            if self.has_jwt() {
                "  ], authenticationStorage, resolvePrincipal, external);"
            } else {
                "  ], authenticationStorage, resolvePrincipal);"
            },
        );
        line(output, "  if (generation !== authenticationInitialization) { initialized.invalidateDeliveryCredentialProofs(); throw new AuthenticationFault(\"authentication_misconfigured\", 503); }");
        line(output, "  firstPartyAuthentication = initialized;");
        line(output, "}");
        // The host integration surface is not an authored callable or a route.
        if self.has_lifecycle_purges() {
            line(output, "export async function initializeApplication(environment: Record<string, string | undefined>) { ++authenticationInitialization; firstPartyAuthentication?.invalidateDeliveryCredentialProofs(); firstPartyAuthentication = undefined; configuration = loadConfiguration(environment); await initializeAuthentication(); bindLifecycleRetention(configuration); }");
        } else {
            line(output, "export async function initializeApplication(environment: Record<string, string | undefined>) { ++authenticationInitialization; firstPartyAuthentication?.invalidateDeliveryCredentialProofs(); firstPartyAuthentication = undefined; configuration = loadConfiguration(environment); await initializeAuthentication(); }");
        }
        line(output, "export function authenticationHost() { if (!firstPartyAuthentication) throw new AuthenticationFault(\"authentication_misconfigured\", 503); return firstPartyAuthentication; }");
        if !self.project.delivery_model().bindings().is_empty() {
            line(output, "// Native read-only worker bridge: the selected issuer is this private current host, never a caller-provided principal, callback or setter.");
            line(output, "export function readCurrentDeliveryCredentialProof(proof: unknown) { return readDeliveryCredentialProof(proof, authenticationStorage, firstPartyAuthentication); }");
        }
    }

    // The declared lifecycle subset is deliberately synthesizable at trusted issuance.
    fn credential_active_binding(&self, expression: &Expression) -> Option<(String, String)> {
        let Expression::Binary(binary) = expression else {
            return None;
        };
        if binary.operator != BinaryOperator::Equal {
            return None;
        }
        let Expression::Name(field) = binary.left.as_ref() else {
            return None;
        };
        if field.path.len() != 1 {
            return None;
        }
        match binary.right.as_ref() {
            Expression::Name(value)
                if value.path.len() == 2
                    && self
                        .enums
                        .get(&value.path[0].text)
                        .is_some_and(|declaration| {
                            declaration.variants.iter().any(|variant| {
                                variant.name.text == value.path[1].text && variant.fields.is_empty()
                            })
                        }) =>
            {
                Some((field.path[0].text.clone(), ts_string(&value.path[1].text)))
            }
            Expression::Literal(value) if value.kind == LiteralKind::Boolean => {
                Some((field.path[0].text.clone(), value.text.clone()))
            }
            _ => None,
        }
    }

    fn declared_credential_storage(&self, output: &mut String) {
        line(output, "const declaredCredentialBindings: Record<string, DeclaredCredentialBinding> = Object.create(null);");
        for strategy in &self.authentication_strategies {
            for validator in &strategy.validators {
                let Some(binding) = &validator.credentials else {
                    continue;
                };
                let record = self.records.get(&binding.identity.path[0].text).unwrap();
                let resolution = strategy
                    .resolutions
                    .iter()
                    .find(|r| r.principal.text == "service")
                    .unwrap();
                let service = self
                    .records
                    .get(&resolution.authority.path[0].text)
                    .unwrap();
                let id = resolution
                    .mappings
                    .iter()
                    .find(|m| m.target.path.last().is_some_and(|n| n.text == "service_id"))
                    .unwrap();
                let (active_field, active_value) =
                    self.credential_active_binding(&binding.active).unwrap();
                let (service_active_field, service_active_value) =
                    self.credential_active_binding(&resolution.active).unwrap();
                line(
                    output,
                    &format!(
                        "declaredCredentialBindings[{}] = {{",
                        ts_string(&strategy.name.text)
                    ),
                );
                for (key, value) in [
                    ("binding", record.name.text.clone()),
                    ("table", snake_case(&record.name.text)),
                    ("identity", snake_case(&binding.identity.path[1].text)),
                    ("principal", snake_case(&binding.principal.path[1].text)),
                    ("verifier", snake_case(&binding.verifier.path[1].text)),
                    ("expires", snake_case(&binding.expires.path[1].text)),
                    ("revoked", snake_case(&binding.revoked.path[1].text)),
                    ("activeField", snake_case(&active_field)),
                    ("authorityTable", snake_case(&service.name.text)),
                    (
                        "authoritySubject",
                        snake_case(&resolution.authority.path[1].text),
                    ),
                    ("authorityId", snake_case(&id.source.text)),
                    ("authorityActiveField", snake_case(&service_active_field)),
                ] {
                    line(output, &format!("  {key}: {},", ts_string(&value)));
                }
                line(output, &format!("  activeValue: {active_value}, authorityActiveValue: {service_active_value},"));
                let created = record
                    .fields
                    .iter()
                    .filter(|field| {
                        matches!(
                            field.generated,
                            Some(jadpo_syntax::GeneratedFieldRole::Create)
                        )
                    })
                    .map(|field| ts_string(&snake_case(&field.name.text)))
                    .collect::<Vec<_>>()
                    .join(", ");
                line(output, &format!("  created: [{created}],"));
                line(output, "};");
            }
        }
        output.push_str(include_str!("../runtime/declared_credential_storage.ts"));
    }

    pub(super) fn first_party_storage(&self, output: &mut String) {
        if !self.first_party_supported() {
            return;
        }
        self.declared_credential_storage(output);
        output.push_str(r#"
// Internal auth data cannot be reached through authored persistence operations.
async function initializeAuthenticationStorage(initialization: ReturnType<typeof createInitializationControl>) {
const authenticationSchema = 'CREATE TABLE IF NOT EXISTS "__jadpo_auth_sessions" ("id" TEXT PRIMARY KEY, "data" TEXT NOT NULL, "revoked" INTEGER NOT NULL DEFAULT 0)';
if (postgres !== null) await persistenceAsync("authentication.schema", () => initialization.execute(postgres!.unsafe(authenticationSchema)));
else persistenceSync("authentication.schema", () => sqlite!.exec(authenticationSchema));
// Reject an incompatible pre-existing internal table before opening a listener.
const probe = 'SELECT sessions.id, sessions.data, sessions.revoked FROM "__jadpo_auth_sessions" AS sessions LIMIT 0';
if (postgres !== null) await persistenceAsync("authentication.schema.validate", () => initialization.execute(postgres!.unsafe(probe)));
else persistenceSync("authentication.schema.validate", () => sqlite!.prepare(probe).all());
const serviceSchema = 'CREATE TABLE IF NOT EXISTS "__jadpo_auth_service_credentials" ("id" TEXT PRIMARY KEY, "data" TEXT NOT NULL, "revoked" INTEGER NOT NULL DEFAULT 0)';
if (postgres !== null) await persistenceAsync("authentication.schema", () => initialization.execute(postgres!.unsafe(serviceSchema)));
else persistenceSync("authentication.schema", () => sqlite!.exec(serviceSchema));
const serviceProbe = 'SELECT credentials.id, credentials.data, credentials.revoked FROM "__jadpo_auth_service_credentials" AS credentials LIMIT 0';
if (postgres !== null) await persistenceAsync("authentication.schema.validate", () => initialization.execute(postgres!.unsafe(serviceProbe)));
else persistenceSync("authentication.schema.validate", () => sqlite!.prepare(serviceProbe).all());
}
async function authenticationRows(sql: string, values: string[]): Promise<any[]> {
  if (postgres !== null) return persistenceAsync("authentication.authority", () => postgres!.unsafe(sql, values));
  return serializeSQLiteTransaction(async () => persistenceSync("authentication.authority", () => sqlite!.prepare(sql.replace(/\$[0-9]+/gu, "?")).all(...values)));
}
export const authenticationStorage = {
  async get(id: string) {
    const rows = await authenticationRows('SELECT "data", "revoked" FROM "__jadpo_auth_sessions" WHERE "id" = $1', [id]);
    if (rows.length === 0) return null;
    if (rows.length !== 1) throw new Error("Invalid authentication authority cardinality");
    const record = JSON.parse(rows[0].data);
    return { ...record, revoked: rows[0].revoked !== 0 };
  },
  async put(session: any) {
    const values = [session.id, JSON.stringify(session)];
    if (postgres !== null) await persistenceAsync("authentication.issue", () => postgres!.unsafe('INSERT INTO "__jadpo_auth_sessions" ("id", "data") VALUES ($1, $2)', values));
    else await serializeSQLiteTransaction(async () => persistenceSync("authentication.issue", () => sqlite!.prepare('INSERT INTO "__jadpo_auth_sessions" ("id", "data") VALUES (?, ?)').run(...values)));
  },
  async revoke(id: string) {
    if (postgres !== null) await persistenceAsync("authentication.revoke", () => postgres!.unsafe('UPDATE "__jadpo_auth_sessions" SET "revoked" = 1 WHERE "id" = $1', [id]));
    else await serializeSQLiteTransaction(async () => persistenceSync("authentication.revoke", () => sqlite!.prepare('UPDATE "__jadpo_auth_sessions" SET "revoked" = 1 WHERE "id" = ?').run(id)));
  },
  getServiceCredential: getStoredServiceCredential,
  putServiceCredential: putStoredServiceCredential,
  revokeServiceCredential: revokeStoredServiceCredential,
};
export async function resolveAuthenticationAuthority(strategy: string, subject: string, principalKind = "user"): Promise<unknown[]> {
  switch (`${strategy}:${principalKind}`) {
"#);
        for strategy in &self.authentication_strategies {
            for resolution in &strategy.resolutions {
                let entity = &resolution.authority.path[0].text;
                let selected_fields = self.authentication_authority_fields(resolution);
                let query = format!(
                    "SELECT {} FROM {} WHERE {} = $1 LIMIT 2",
                    selected_fields
                        .iter()
                        .map(|field| sql_identifier(&snake_case(&field.name.text)))
                        .collect::<Vec<_>>()
                        .join(", "),
                    sql_identifier(&snake_case(entity)),
                    sql_identifier(&resolution.authority.path[1].text)
                );
                // Reuse the persistence result decoder, including Boolean/Temporal values.
                line(
                    output,
                    &format!(
                    "    case {}: return (await authenticationRows({}, [subject])).map(row => ({{",
                    ts_string(&format!("{}:{}", strategy.name.text, resolution.principal.text)),
                    ts_string(&query)
                ),
                );
                for field in &selected_fields {
                    let raw = format!("row[{}]", ts_string(&snake_case(&field.name.text)));
                    let value = if self.representation_root_for(&field.field_type) == "Bool" {
                        format!("({raw} === 0 ? false : {raw} === 1 ? true : {raw})")
                    } else {
                        raw
                    };
                    line(output, &format!("      {}: {},", field.name.text, value));
                }
                line(output, "    }));");
            }
        }
        line(
            output,
            "    default: throw new Error(\"Unsupported authentication authority\");",
        );
        line(output, "  }");
        line(output, "}");
    }
}
