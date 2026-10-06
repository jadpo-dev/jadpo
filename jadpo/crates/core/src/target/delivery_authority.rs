//! One admission authority read, bound to finished identities and native proof.
//! No ordinary entity grant, worker activation or post-admission cut is added.
use super::{line, snake_case, sql_identifier, ts_string, TargetGenerator};

fn parts(identity: &str) -> (&str, &str) {
    identity.split_once('.').expect("finished entity field")
}
fn variant(name: &str) -> &str {
    name.rsplit_once('.').expect("finished enum variant").1
}

impl TargetGenerator<'_> {
    pub(super) fn delivery_authority_methods(&self, output: &mut String) {
        if self.project.delivery_model().bindings().is_empty() {
            return;
        }
        line(
            output,
            "  async check_delivery_authority(binding: string, proof: unknown): Promise<boolean> {",
        );
        line(output, "    assertTransactionActive(); if (!transactional) throw new PersistenceFault(\"delivery.transaction_required\", \"driver\", new Error(\"authority requires owning transaction\"));");
        line(output, "    return client.transaction(async () => {");
        line(
            output,
            "    const credential = readCurrentDeliveryCredentialProof(proof);",
        );
        line(output, "    switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            let a = binding.authority_for_lowering();
            let column = |alias: &str, identity: &str| {
                format!("{alias}.{}", sql_identifier(&snake_case(parts(identity).1)))
            };
            let table = |identity: &str| sql_identifier(&snake_case(parts(identity).0));
            let strategy = a.strategy;
            let mut sql = format!(
                "SELECT k.\"data\" AS metadata, {} AS credential_expires FROM {} c JOIN {} s ON {} = {} JOIN {} m ON {} = {} JOIN \"__jadpo_auth_service_credentials\" k ON k.\"id\" = {} WHERE {} = $1 AND {} = $2 AND {} = $3 AND {} = $4 AND {} = $5 AND {} IS NULL AND {} > {{now}} AND {} = $6 AND {} = $7",
                column("c", a.expires), table(a.credential_identity), table(a.service_identity),
                column("c", a.credential_principal), column("s", a.service_identity),
                table(a.membership_identity), column("m", a.member), column("s", a.service_identity),
                column("c", a.credential_identity), column("c", a.credential_identity),
                column("s", a.service_identity), column("c", a.verifier),
                column("s", a.resolution_authority), column("c", a.credential_active.0),
                column("c", a.revoked), column("c", a.expires),
                column("s", a.service_active.0), column("m", a.role_field),
            );
            sql.push_str(" LIMIT 2");
            let pg = format!(
                "{} FOR SHARE OF c, s, m, k",
                sql.replace("{now}", "clock_timestamp()")
            );
            let lite = sql.replace(
                "{now}",
                "CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)",
            );
            line(
                output,
                &format!("      case {}: {{", ts_string(binding.binding_identity())),
            );
            line(
                output,
                &format!(
                    "        if (credential.strategy !== {}) return false;",
                    ts_string(strategy)
                ),
            );
            line(output, &format!("        const values = [credential.id, credential.serviceId, credential.verifier, credential.subject, {}, {}, {}];", ts_string(variant(a.credential_active.1)), ts_string(variant(a.service_active.1)), ts_string(variant(a.role))));
            line(output, &format!("        const rows = postgres !== null ? await persistenceAsync(\"delivery.authority\", () => postgres!.unsafe({}, values)) : persistenceSync(\"delivery.authority\", () => {{ const statement = sqlite!.prepare({}); try {{ return statement.all(...values); }} finally {{ statement.finalize(); }} }});", ts_string(&pg), ts_string(&lite)));
            line(output, "        if (rows.length !== 1) return false;");
            line(output, "        let metadata: any; try { metadata = JSON.parse((rows[0] as any).metadata); } catch { return false; }");
            line(output, &format!("        if (metadata === null || typeof metadata !== \"object\" || Array.isArray(metadata) || Object.keys(metadata).sort().join(\",\") !== \"binding,id,keyId,serviceId,strategy,subject,verifierDigest\" || metadata.binding !== {} || metadata.id !== credential.id || metadata.keyId !== credential.keyId || metadata.serviceId !== credential.serviceId || metadata.strategy !== credential.strategy || metadata.subject !== credential.subject || metadata.verifierDigest !== await credentialVerifierDigest(credential.verifier)) return false;", ts_string(parts(a.credential_identity).0)));
            line(
                output,
                "        // WHERE may have evaluated before a row-lock wait. Admission time is
        // sampled AFTER locks and metadata verification, in the same transaction.",
            );
            line(output, "        const clocks = postgres !== null ? await persistenceAsync(\"delivery.authority.time\", () => postgres!.unsafe(\"SELECT clock_timestamp() AS now\")) : persistenceSync(\"delivery.authority.time\", () => { const statement = sqlite!.prepare(\"SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now') AS now\"); try { return statement.all(); } finally { statement.finalize(); } });");
            line(output, "        if (clocks.length !== 1 || authMilliseconds((rows[0] as any).credential_expires) <= authMilliseconds((clocks[0] as any).now)) return false;");
            line(
                output,
                "        // A reconfiguration while awaiting SQL/crypto also withdraws the proof.",
            );
            line(
                output,
                "        readCurrentDeliveryCredentialProof(proof); return true;",
            );
            line(output, "      }");
        }
        line(output, "      default: return false;");
        line(output, "    }");
        line(output, "    });");
        line(output, "  },");
    }
}
