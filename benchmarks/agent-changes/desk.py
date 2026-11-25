"""The helpdesk's fixtures: two workspaces, a member of each role, customers,
tickets with replies, notes and tags, and a bearer token for each person.
Every hidden test of the helpdesk set starts from this state, through the
shared contract controller, against the Jadpo app and its TypeScript twin
alike. Nothing here is shown to an agent.
"""
from __future__ import annotations

import json
import sys
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'contract'))
from contract import MILLIS, Contract, require  # noqa: E402

NOW = '2026-01-15T12:00:00Z'
EARLIER = '2026-01-14T09:00:00Z'
LATER = '2026-01-16T09:00:00Z'


def ident(group, number):
    return f'00000000-0000-4000-8000-{group:04d}{number:08d}'


# People. Each signs in by bearer token as their own subject.
ANN, BEN, CAT, DAN, EVE, FAY = (ident(1, n) for n in range(1, 7))
PEOPLE = {'ann': ANN, 'ben': BEN, 'cat': CAT, 'dan': DAN, 'eve': EVE, 'fay': FAY}
EMAILS = {who: f'{who}@staff.example.test' for who in PEOPLE}

# Workspaces: Acme is the one the tests work in; Globex is the outsider's.
ACME, GLOBEX = ident(2, 1), ident(2, 2)

# Memberships: Ann owns Acme, Ben is its admin, Cat an agent, Dan a viewer.
# Eve owns Globex and Fay is its agent; neither belongs to Acme.
M_ANN, M_BEN, M_CAT, M_DAN, M_EVE, M_FAY = (ident(3, n) for n in range(1, 7))
MEMBERS = [
    (M_ANN, ACME, ANN, 'owner', 'Ann Owner'),
    (M_BEN, ACME, BEN, 'admin', 'Ben Admin'),
    (M_CAT, ACME, CAT, 'agent', 'Cat Agent'),
    (M_DAN, ACME, DAN, 'viewer', 'Dan Viewer'),
    (M_EVE, GLOBEX, EVE, 'owner', 'Eve Owner'),
    (M_FAY, GLOBEX, FAY, 'agent', 'Fay Agent'),
]

# Customers, with their personal data.
CORA, COLIN, GAIL = ident(4, 1), ident(4, 2), ident(4, 3)
CUSTOMERS = [
    (CORA, ACME, 'Cora Customer', 'cora@customer.example.test', '+44 20 7946 0001', EARLIER),
    (COLIN, ACME, 'Colin Client', 'colin@customer.example.test', None, EARLIER),
    (GAIL, GLOBEX, 'Gail Globex', 'gail@globex-customer.example.test', '+44 20 7946 0003', EARLIER),
]

# Tickets. T_BIN is in Acme's bin; G_OPEN is Globex's.
T_OPEN, T_PENDING, T_SOLVED, T_BIN, G_OPEN = (ident(5, n) for n in range(1, 6))
TICKETS = [
    # id, workspace, customer, assignee, subject, status, priority, created, respond_by, first_response, solved, deleted
    (T_OPEN, ACME, CORA, M_CAT, 'Cannot sign in', 'open', 'high', EARLIER, '2026-01-15T09:00:00Z', None, None, None),
    (T_PENDING, ACME, COLIN, M_CAT, 'Invoice is wrong', 'pending', 'normal', EARLIER, '2026-01-15T09:00:00Z', EARLIER, None, None),
    (T_SOLVED, ACME, CORA, M_BEN, 'Change of address', 'solved', 'low', EARLIER, '2026-01-15T09:00:00Z', EARLIER, NOW, None),
    (T_BIN, ACME, COLIN, None, 'Spam ticket', 'open', 'normal', EARLIER, '2026-01-15T09:00:00Z', None, None, NOW),
    (G_OPEN, GLOBEX, GAIL, M_FAY, 'Globex outage', 'open', 'urgent', EARLIER, '2026-01-15T09:00:00Z', None, None, None),
]

# What only some may see: internal notes, and text that must never leave its workspace.
NOTE_CANARY = 'note-canary-4b1d'
GLOBEX_CANARY = 'globex-canary-9e2a'
R_OPEN, R_PENDING, R_GLOBEX = (ident(6, n) for n in range(1, 4))
N_OPEN, N_GLOBEX = ident(7, 1), ident(7, 2)
TAG_BILLING, TAG_VIP = ident(8, 1), ident(8, 2)
MACRO_THANKS = ident(9, 1)


def literal(value):
    return 'NULL' if value is None else "'" + str(value).replace("'", "''") + "'"


# A required column the agent's change added, with no database default, is
# one these fixtures can't name: they hold the rows the app had before the
# change, and so can the inserts of a change's own tests. When an insert is
# refused for it (23502), the row takes the type's neutral value, as a
# migration's backfill would give it, and the app's schema is left as the
# agent wrote it.
REQUIRED_WITHOUT_DEFAULT = (
    "SELECT table_name, column_name, CASE WHEN data_type = 'boolean' THEN 'false' "
    "WHEN data_type IN ('text', 'character varying') THEN '''''' ELSE '0' END "
    "FROM information_schema.columns WHERE table_schema = 'public' AND is_nullable = 'NO' "
    "AND column_default IS NULL AND is_identity = 'NO' AND is_generated = 'NEVER' "
    "AND data_type IN ('boolean', 'smallint', 'integer', 'bigint', 'numeric', 'real', 'double precision', "
    "'text', 'character varying') ORDER BY table_name, column_name")


class Desk(Contract):
    """The contract controller with the helpdesk's fixtures."""

    def reset(self):
        self.call('outage', database=False, issuer=False)
        self.call('clock', at=MILLIS)
        self.call('worker_reset')
        self.sql("DO $$ DECLARE r record; BEGIN FOR r IN SELECT tablename FROM pg_tables WHERE schemaname='public' "
                 "AND tablename<>'jadpo_migrations' LOOP EXECUTE format('TRUNCATE TABLE %I CASCADE',r.tablename); END LOOP; END $$")
        self.seed()
        self.call('logs')
        self.history = []

    def sql(self, statement):
        try:
            return super().sql(statement)
        except AssertionError as refused:
            if '23502' not in str(refused) or not statement.lstrip().upper().startswith('INSERT'):
                raise
            added = super().sql(REQUIRED_WITHOUT_DEFAULT)
            for table, column, value in added:
                super().sql(f'ALTER TABLE "{table}" ALTER COLUMN "{column}" SET DEFAULT {value}')
            try:
                return super().sql(statement)
            finally:
                for table, column, _ in added:
                    super().sql(f'ALTER TABLE "{table}" ALTER COLUMN "{column}" DROP DEFAULT')

    def seed(self):
        self.sql('INSERT INTO "user"(id,authentication_subject,status,email,created_at) VALUES '
                 + ','.join(f"('{i}','{who}','active','{EMAILS[who]}','{EARLIER}')" for who, i in PEOPLE.items()))
        self.sql(f"INSERT INTO workspace(id,name,created_at,updated_at) VALUES ('{ACME}','Acme Support','{EARLIER}','{EARLIER}'),"
                 f"('{GLOBEX}','Globex {GLOBEX_CANARY}','{EARLIER}','{EARLIER}')")
        self.sql('INSERT INTO membership(id,workspace_id,user_id,role,display_name,created_at,updated_at) VALUES '
                 + ','.join(f"('{m}','{w}','{u}','{r}',{literal(n)},'{EARLIER}','{EARLIER}')" for m, w, u, r, n in MEMBERS))
        self.sql('INSERT INTO customer(id,workspace_id,name,email,phone,created_at,updated_at) VALUES '
                 + ','.join(f"('{i}','{w}',{literal(n)},{literal(e)},{literal(p)},'{c}','{c}')" for i, w, n, e, p, c in CUSTOMERS))
        for row in TICKETS:
            self.ticket(*row)
        self.message('reply', R_OPEN, ACME, T_OPEN, CAT, 'We are looking into it.')
        self.message('reply', R_PENDING, ACME, T_PENDING, CAT, 'Could you send the invoice number?')
        self.message('reply', R_GLOBEX, GLOBEX, G_OPEN, FAY, f'Globex reply {GLOBEX_CANARY}')
        self.message('note', N_OPEN, ACME, T_OPEN, CAT, f'Customer sounded upset {NOTE_CANARY}')
        self.message('note', N_GLOBEX, GLOBEX, G_OPEN, FAY, f'Globex note {GLOBEX_CANARY}')
        self.sql(f"INSERT INTO tag(id,workspace_id,name,created_at) VALUES ('{TAG_BILLING}','{ACME}','billing','{EARLIER}'),"
                 f"('{TAG_VIP}','{GLOBEX}','vip-{GLOBEX_CANARY}','{EARLIER}')")
        self.sql(f"INSERT INTO ticket_tag(id,workspace_id,ticket_id,tag_id,created_at) VALUES "
                 f"('{ident(10, 1)}','{ACME}','{T_PENDING}','{TAG_BILLING}','{EARLIER}'),"
                 f"('{ident(10, 2)}','{GLOBEX}','{G_OPEN}','{TAG_VIP}','{EARLIER}')")
        self.sql(f"INSERT INTO macro(id,workspace_id,title,body,created_at,updated_at) VALUES "
                 f"('{MACRO_THANKS}','{ACME}','Thanks','Thanks for getting in touch.','{EARLIER}','{EARLIER}')")
        self.auth = {}
        for who, identifier in PEOPLE.items():
            secret = uuid.uuid4().hex + uuid.uuid4().hex
            verifier = self.call('digest', text='jadpo_token_' + secret)['digest']
            row = str(uuid.uuid4())
            self.sql(f"INSERT INTO jadpo_sessions(id,principal_id,verifier,csrf_verifier,started_at,ends_at) VALUES "
                     f"('{row}','{identifier}','{verifier}',NULL,'2026-01-15T11:59:00Z','2026-02-14T11:59:00Z')")
            payload = {'enc': 1, 'm': 'api_token', 'r': uuid.UUID(row).hex, 'v': verifier,
                       'p': uuid.UUID(identifier).hex, 's': 't' + who, 'e': MILLIS + 240000}
            signed = self.call('sign', payload=json.dumps(payload, separators=(',', ':')))['signed']
            self.auth[who] = [['authorization', 'Bearer jadpo_token_' + secret + '.' + signed]]
        self.auth['nobody'] = []

    def ticket(self, identifier, workspace, customer, assignee, subject, status, priority, created, respond_by,
               first_response, solved, deleted, breached=None):
        self.sql('INSERT INTO ticket(id,workspace_id,customer_id,assignee_id,subject,status,priority,respond_by,'
                 'first_response_at,breached_at,solved_at,deleted_at,created_at,updated_at) VALUES ('
                 + ','.join(literal(v) for v in [identifier, workspace, customer, assignee, subject, status, priority,
                                                 respond_by, first_response, breached, solved, deleted, created, created])
                 + ')')

    def message(self, table, identifier, workspace, ticket, author, body, created=EARLIER):
        self.sql(f'INSERT INTO {table}(id,workspace_id,ticket_id,author_id,body,created_at,updated_at) VALUES '
                 f"('{identifier}','{workspace}','{ticket}',{literal(author)},{literal(body)},'{created}','{created}')")

    def row(self, table, identifier):
        rows = self.sql(f"SELECT row_to_json(t)::text FROM \"{table}\" t WHERE id='{identifier}'")
        return json.loads(rows[0][0]) if rows else None

    def rows(self, table, where='true'):
        return [json.loads(r[0]) for r in self.sql(f'SELECT row_to_json(t)::text FROM "{table}" t WHERE {where} ORDER BY id')]

    def request(self, method, path, auth='ann', body=None, raw=None):
        return super().request(method, path, auth=auth, body=body, raw=raw)

    def logged(self):
        return json.dumps(self.call('logs')['logs'])

    def breach_job(self):
        """One run of the first-response schedule, as the app's own runner
        would start it."""
        self.call('clock', at=MILLIS - 600000)
        self.call('tick')
        self.call('clock', at=MILLIS)
        return self.call('tick')


def body_text(response):
    return response.get('body') or ''


def require_hidden(response, *secrets):
    """None of these strings reach the response, whatever its status."""
    text = body_text(response)
    for secret in secrets:
        require(secret not in text, f'{secret!r} reached the response: {response["status"]} {text[:300]}')
