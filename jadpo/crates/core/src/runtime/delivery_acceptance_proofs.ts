// Compiler-private durable evidence. The only producer input is an opaque token
// read from the actual application issuer. These methods confer no authored
// capability and make no inference from absent proof or a storage failure.
type DeliveryAcceptanceContract = Readonly<{ binding: string; digest: string; serviceVersion: string; serviceDigest: string; operation: string; job: string; sourceEntity: string; sourceOperation: string; payloadVersion: string }>;
function createDeliveryAcceptanceProofPrimitives(
  nativeExecute: (operation: string, sql: string, values?: unknown[]) => Promise<any[]>,
  requireTransaction: () => void,
  isPostgres: boolean,
) {
  const table = '__jadpo_delivery_acceptance_proofs_v1';
  const invalid = (): never => { throw new PersistenceFault('delivery.acceptance.proof', 'driver', new Error('invalid durable acceptance proof')); };
  const numeric = new Map<string, string>([['schema_version','INTEGER'],['key_sequence','BIGINT'],['generation','BIGINT'],['invocation','INTEGER'],['max_invocations','INTEGER'],['lifetime_ms','BIGINT'],['execution_ms','BIGINT'],['lease_ms','BIGINT']]);
  const columns = ['schema_version','intent_id','binding_identity','binding_contract_sha256','service_contract_version','service_contract_sha256','service_operation','payload_version','payload','source_entity','source_entity_id','source_revision','ordering_key','key_sequence','claim_id','generation','invocation','max_invocations','lifetime_ms','execution_ms','lease_ms','first_claim_at','lifetime_deadline','execution_deadline','lease_until_at_proof','accepted_at','observed_at','recorded_at'];
  const checks = ['"schema_version" = 1','"key_sequence" > 0 AND "key_sequence" <= 9223372036854775807','"generation" > 0 AND "generation" <= 9223372036854775807','"invocation" > 0 AND "invocation" <= "max_invocations"','"max_invocations" > 0 AND "max_invocations" <= 2147483647','"lifetime_ms" > 0 AND "lifetime_ms" <= 9007199254740991','"execution_ms" > 0 AND "execution_ms" <= 9007199254740991','"lease_ms" > 0 AND "lease_ms" <= "execution_ms"'];
  const definitions = columns.map(key => `"${key}" ${numeric.get(key) ?? 'TEXT'}${key === 'intent_id' ? ' PRIMARY KEY' : ''} NOT NULL${key === 'intent_id' ? ' REFERENCES "__jadpo_deliveries_v1" ("intent_id")' : ''}`);
  const schema = `CREATE TABLE IF NOT EXISTS "${table}" (${[...definitions,...checks.map(value=>`CHECK (${value})`),...(!isPostgres ? [...numeric.keys()].map(key=>`CHECK (typeof("${key}") = 'integer')`) : [])].join(', ')})`;
  const normalizedSql = (sql: string) => sql.replace(/IF NOT EXISTS/gu,'').replace(/\s+/gu,' ').trim();
  const integer = (value: unknown, maximum = 9007199254740991n): string => {
    const text = typeof value === 'number' && Number.isSafeInteger(value) ? String(value) : typeof value === 'string' ? value : '';
    if (!/^[1-9][0-9]*$/u.test(text) || BigInt(text) > maximum) return invalid();
    return text;
  };
  const instant = (value: unknown): string => {
    if (typeof value !== 'string' || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/u.test(value) || !Number.isFinite(Date.parse(value)) || new Date(Date.parse(value)).toISOString() !== value) return invalid();
    return value;
  };
  const clockSql = isPostgres ? `to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')` : "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')";
  const guarded = (guard: () => void) => async (operation: string, sql: string, values: unknown[] = []): Promise<any[]> => { guard(); const rows = await nativeExecute(operation,sql,values); guard(); return rows; };
  const exists = async (execute: ReturnType<typeof guarded>): Promise<boolean> => {
    const rows = isPostgres ? await execute('delivery.acceptance.schema.present','SELECT to_regclass($1) IS NOT NULL AS present',[`"${table}"`]) : await execute('delivery.acceptance.schema.present','SELECT name FROM sqlite_master WHERE type = $1 AND name = $2',['table',table]);
    if (rows.length > 1 || isPostgres && rows.length !== 1) return invalid();
    return isPostgres ? rows[0].present === true : rows.length === 1;
  };
  const checkSchema = async (execute: ReturnType<typeof guarded>): Promise<void> => {
    if (!isPostgres) {
      const rows = await execute('delivery.acceptance.schema.inspect','SELECT sql FROM sqlite_master WHERE type = $1 AND name = $2',['table',table]);
      if (rows.length !== 1 || typeof rows[0].sql !== 'string' || normalizedSql(rows[0].sql) !== normalizedSql(schema)) return invalid();
      return;
    }
    const fields = await execute('delivery.acceptance.schema.inspect',`SELECT a.attname AS name, format_type(a.atttypid,a.atttypmod) AS type, a.attnotnull AS required, a.attgenerated AS generated, a.atthasdef AS has_default FROM pg_attribute a WHERE a.attrelid = to_regclass($1) AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum`,[`"${table}"`]);
    if (fields.length !== columns.length || fields.some((row,index)=>row.name !== columns[index] || row.type !== (numeric.get(row.name)==='BIGINT'?'bigint':numeric.has(row.name)?'integer':'text') || row.required !== true || row.generated !== '' || row.has_default !== false)) return invalid();
    const constraints = await execute('delivery.acceptance.schema.constraints',`SELECT contype AS type, pg_get_constraintdef(oid) AS definition FROM pg_constraint WHERE conrelid = to_regclass($1) ORDER BY contype, pg_get_constraintdef(oid)`,[`"${table}"`]);
    const normal = (value: string) => value.replace(/::(?:bigint|integer)/gu,'').replace(/[()"'\s]/gu,'').toLowerCase();
    const actual = constraints.map(row=>`${row.type}:${normal(row.definition)}`).sort();
    const expected = [...checks.map(value=>`c:${normal(`CHECK (${value})`)}`),'p:primarykeyintent_id','f:foreignkeyintent_idreferences__jadpo_deliveries_v1intent_id'].sort();
    if (actual.join('|') !== expected.join('|')) return invalid();
  };
  const selection = columns.map(key => numeric.has(key) ? `CAST("${key}" AS TEXT) AS "${key}"` : `"${key}"`).concat(isPostgres?[]:[...numeric.keys()].map(key=>`typeof("${key}") AS "__storage_${key}"`)).join(', ');
  const decode = (row: any): any => {
    const keys = columns.concat(isPostgres?[]:[...numeric.keys()].map(key=>'__storage_'+key));
    if (row === null || typeof row !== 'object' || Reflect.ownKeys(row).sort().join(',') !== keys.sort().join(',')) return invalid();
    for (const key of columns) {
      const descriptor = Object.getOwnPropertyDescriptor(row,key); if (!descriptor || !('value' in descriptor)) return invalid();
      if (numeric.has(key)) { if (!isPostgres && row['__storage_'+key] !== 'integer') return invalid(); row[key] = integer(row[key],['key_sequence','generation'].includes(key)?9223372036854775807n:9007199254740991n); }
      else if (typeof row[key] !== 'string' || row[key].length === 0) return invalid();
    }
    if (row.schema_version !== '1' || BigInt(row.max_invocations)>2147483647n || BigInt(row.invocation)>BigInt(row.max_invocations) || BigInt(row.lease_ms)>BigInt(row.execution_ms)) return invalid();
    for (const key of ['first_claim_at','lifetime_deadline','execution_deadline','lease_until_at_proof','accepted_at','observed_at','recorded_at']) instant(row[key]);
    if (!(row.first_claim_at <= row.recorded_at && row.recorded_at < row.lease_until_at_proof && row.lease_until_at_proof <= row.execution_deadline && row.execution_deadline <= row.lifetime_deadline) || Date.parse(row.lifetime_deadline)-Date.parse(row.first_claim_at) !== Number(row.lifetime_ms)) return invalid();
    const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;
    if (['intent_id','source_entity_id','ordering_key','claim_id'].some(key=>!uuid.test(row[key])) || integer(row.source_revision,9223372036854775807n) !== row.source_revision) return invalid();
    if (!/^[0-9a-f]{64}$/u.test(row.binding_contract_sha256) || !/^[0-9a-f]{64}$/u.test(row.service_contract_sha256)) return invalid();
    return row;
  };
  const read = async (execute: ReturnType<typeof guarded>, id: string): Promise<any | null> => {
    if (!await exists(execute)) return null; // Consumer is strictly read-only on absence.
    await checkSchema(execute);
    const rows = await execute('delivery.acceptance.read',`SELECT ${selection} FROM "${table}" WHERE "intent_id" = $1`,[id]);
    if (rows.length > 1) return invalid();
    return rows.length === 0 ? null : decode(rows[0]);
  };
  const locked = async (execute: ReturnType<typeof guarded>, id: string): Promise<{intent:any;ledger:any} | null> => {
    const initial = await execute('delivery.acceptance.intent','SELECT *, CAST("key_sequence" AS TEXT) AS "key_sequence" FROM "__jadpo_deliveries_v1" WHERE "intent_id" = $1',[id]);
    if (initial.length > 1) return invalid(); if (initial.length === 0) return null;
    const keys = await execute('delivery.acceptance.key_lock','UPDATE "__jadpo_delivery_keys_v1" SET "next_sequence" = "next_sequence" WHERE "job" = $1 AND "ordering_key" = $2 RETURNING "ordering_key"',[initial[0].job,initial[0].ordering_key]);
    if (keys.length !== 1) return invalid();
    const intents = await execute('delivery.acceptance.intent','SELECT *, CAST("key_sequence" AS TEXT) AS "key_sequence" FROM "__jadpo_deliveries_v1" WHERE "intent_id" = $1',[id]);
    const ledgers = await execute('delivery.acceptance.ledger','SELECT *, CAST("generation" AS TEXT) AS "generation" FROM "__jadpo_delivery_claims_v1" WHERE "intent_id" = $1',[id]);
    if (intents.length !== 1 || ledgers.length !== 1) return invalid();
    return {intent:intents[0],ledger:ledgers[0]};
  };
  const matching = (proof: any, contract: DeliveryAcceptanceContract, current: {intent:any;ledger:any}, profile: Readonly<{executionMs:number;leaseMs:number}>): void => {
    const {intent,ledger} = current;
    if (intent.job !== contract.job || intent.source_entity !== contract.sourceEntity || intent.source_operation !== contract.sourceOperation || intent.payload_version !== contract.payloadVersion || intent.ordering_key !== intent.source_entity_id) return invalid();
    const pairs = {intent_id:intent.intent_id,payload_version:intent.payload_version,payload:intent.payload,source_entity:intent.source_entity,source_entity_id:intent.source_entity_id,source_revision:intent.source_revision,ordering_key:intent.ordering_key,key_sequence:integer(intent.key_sequence,9223372036854775807n),claim_id:ledger.claim_id,generation:integer(ledger.generation,9223372036854775807n),invocation:integer(ledger.invocations),max_invocations:integer(ledger.max_invocations),lifetime_ms:integer(ledger.lifetime_ms),execution_ms:integer(ledger.execution_ms),lease_ms:integer(ledger.lease_ms),first_claim_at:ledger.first_claim_at,lifetime_deadline:ledger.lifetime_deadline,execution_deadline:ledger.execution_deadline,binding_identity:contract.binding,binding_contract_sha256:contract.digest,service_contract_version:contract.serviceVersion,service_contract_sha256:contract.serviceDigest,service_operation:contract.operation};
    if (proof.max_invocations !== '3' || proof.lifetime_ms !== '3600000' || Object.entries(pairs).some(([key,value])=>proof[key]!==value) || !(ledger.possible_dispatch===1 || ledger.possible_dispatch==='1') || proof.lease_until_at_proof>ledger.lease_until || Number(proof.execution_ms)!==profile.executionMs || Number(proof.lease_ms)!==profile.leaseMs) return invalid();
  };
  return Object.freeze({
    async record(evidence: unknown, contract: DeliveryAcceptanceContract): Promise<void> {
      requireTransaction(); const guard = () => { readCurrentDeliveryAcceptedEvidence(evidence); }; const execute = guarded(guard);
      const accepted = readCurrentDeliveryAcceptedEvidence(evidence), invocation = accepted.invocation, handle = invocation.prepared.handle;
      const current = await locked(execute,handle.intentId); if (current === null || current.intent.state !== 'running') return invalid();
      const {intent,ledger} = current;
      if (intent.job !== contract.job || intent.source_entity !== contract.sourceEntity || intent.source_operation !== contract.sourceOperation || intent.payload_version !== contract.payloadVersion || intent.source_entity_id !== invocation.prepared.sourceEntityId || intent.source_revision !== invocation.prepared.sourceRevision || intent.ordering_key !== intent.source_entity_id) return invalid();
      if (ledger.claim_id !== handle.claimId || integer(ledger.generation,9223372036854775807n) !== handle.generation || integer(ledger.invocations) !== String(handle.invocation) || !(ledger.possible_dispatch===1 || ledger.possible_dispatch==='1') || invocation.binding !== contract.binding || intent.payload !== accepted.payload) return invalid();
      if (integer(ledger.max_invocations) !== '3' || integer(ledger.lifetime_ms) !== '3600000' || integer(ledger.execution_ms) !== String(invocation.profile.executionMs) || integer(ledger.lease_ms) !== String(invocation.profile.leaseMs) || ledger.first_claim_at !== handle.firstClaimAt || ledger.lifetime_deadline !== handle.lifetimeDeadline || ledger.execution_deadline !== handle.executionDeadline) return invalid();
      if (!await exists(execute)) await execute('delivery.acceptance.schema',schema);
      await checkSchema(execute);
      const nowRows = await execute('delivery.acceptance.time',`SELECT ${clockSql} AS now`); if (nowRows.length !== 1) return invalid();
      const now = instant(nowRows[0].now);
      if (!(ledger.first_claim_at <= now && now < ledger.lease_until && now < ledger.execution_deadline && now < ledger.lifetime_deadline) || performance.now() >= invocation.deadlineAt) return invalid();
      const data:any = {schema_version:1,intent_id:intent.intent_id,binding_identity:contract.binding,binding_contract_sha256:contract.digest,service_contract_version:contract.serviceVersion,service_contract_sha256:contract.serviceDigest,service_operation:contract.operation,payload_version:intent.payload_version,payload:intent.payload,source_entity:intent.source_entity,source_entity_id:intent.source_entity_id,source_revision:intent.source_revision,ordering_key:intent.ordering_key,key_sequence:integer(intent.key_sequence,9223372036854775807n),claim_id:ledger.claim_id,generation:integer(ledger.generation,9223372036854775807n),invocation:ledger.invocations,max_invocations:ledger.max_invocations,lifetime_ms:ledger.lifetime_ms,execution_ms:ledger.execution_ms,lease_ms:ledger.lease_ms,first_claim_at:ledger.first_claim_at,lifetime_deadline:ledger.lifetime_deadline,execution_deadline:ledger.execution_deadline,lease_until_at_proof:ledger.lease_until,accepted_at:instant(accepted.outcome.receipt.accepted_at),observed_at:instant(accepted.outcome.observedAt),recorded_at:now};
      const previous = await read(execute,handle.intentId);
      if (previous !== null) { if (columns.some(key=>key!=='recorded_at' && key!=='lease_until_at_proof' && String(previous[key])!==String(data[key]))) return invalid(); }
      else await execute('delivery.acceptance.insert',`INSERT INTO "${table}" (${columns.map(key=>`"${key}"`).join(', ')}) VALUES (${columns.map((_,index)=>'$'+(index+1)).join(', ')})`,columns.map(key=>data[key]));
      const proof = await read(execute,handle.intentId); if (proof === null) return invalid(); matching(proof,contract,current,invocation.profile);
      const final = await execute('delivery.acceptance.time',`SELECT ${clockSql} AS now`); if (final.length!==1 || !(instant(final[0].now)>=now && final[0].now<ledger.lease_until && final[0].now<ledger.execution_deadline && final[0].now<ledger.lifetime_deadline) || performance.now()>=invocation.deadlineAt) return invalid();
      guard();
    },
    async resolve(id: string, contract: DeliveryAcceptanceContract, grant: unknown, profile: Readonly<{executionMs:number;leaseMs:number}>): Promise<any | null> {
      requireTransaction(); const guard = () => readCurrentDeliveryReceiptReconciliationGrant(grant); const execute = guarded(guard);
      const proof = await read(execute,id); if (proof === null) return null;
      const current = await locked(execute,id); if (current === null) return invalid(); matching(proof,contract,current,profile);
      if (current.intent.state === 'succeeded') {
        const receipts = await execute('delivery.acceptance.receipt','SELECT * FROM "__jadpo_delivery_mail_receipts_v1" WHERE "intent_id" = $1',[id]);
        if (receipts.length!==1 || receipts[0].claim_id!==proof.claim_id || String(receipts[0].generation)!==proof.generation || receipts[0].payload_version!==proof.payload_version || receipts[0].accepted_at!==proof.accepted_at || receipts[0].observed_at!==proof.observed_at) return invalid();
        return Object.freeze({proof,alreadySucceeded:true});
      }
      if (!['running','outcome_unknown'].includes(current.intent.state)) return invalid();
      await execute('delivery.acceptance.receipt.schema',`CREATE TABLE IF NOT EXISTS "__jadpo_delivery_mail_receipts_v1" ("intent_id" TEXT PRIMARY KEY NOT NULL REFERENCES "__jadpo_deliveries_v1" ("intent_id"), "claim_id" TEXT NOT NULL, "generation" BIGINT NOT NULL, "payload_version" TEXT NOT NULL, "accepted_at" TEXT NOT NULL, "observed_at" TEXT NOT NULL)`);
      // Terminal evidence resolution has no live lease predicate and creates no
      // claim, attempt, renewal or authority to invoke a provider again.
      const updated = await execute('delivery.acceptance.terminal',`UPDATE "__jadpo_deliveries_v1" SET "state" = 'succeeded' WHERE "intent_id" = $1 AND "state" IN ('running','outcome_unknown') RETURNING "intent_id"`,[id]);
      if (updated.length !== 1) return invalid();
      await execute('delivery.acceptance.receipt.insert','INSERT INTO "__jadpo_delivery_mail_receipts_v1" ("intent_id","claim_id","generation","payload_version","accepted_at","observed_at") VALUES ($1,$2,$3,$4,$5,$6)',[id,proof.claim_id,proof.generation,proof.payload_version,proof.accepted_at,proof.observed_at]);
      guard(); return Object.freeze({proof,alreadySucceeded:false});
    },
  });
}
