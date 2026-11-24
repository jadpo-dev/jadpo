"""The helpdesk's twins answer alike: every route, as each role and as an
outsider, from the same fixtures, with the answers compared once ids the
app generates, request ids and the clock's own times are replaced by
placeholders. `python3 desk_parity.py record --language jadpo` writes what
one app answers; `compare` reports where two recordings differ.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import desk  # noqa: E402
from desk import (ACME, CORA, GLOBEX, M_CAT, M_DAN, M_EVE, MACRO_THANKS, N_GLOBEX, N_OPEN, R_GLOBEX, R_OPEN,  # noqa: E402
                  T_BIN, T_OPEN, T_PENDING, T_SOLVED, TAG_BILLING, TAG_VIP, G_OPEN, GAIL, M_BEN, COLIN, BEN, CAT, DAN)

A, G = f'/workspaces/{ACME}', f'/workspaces/{GLOBEX}'
MISSING = '00000000-0000-4000-8000-000099999999'

# Each step: who, method, path, body. Steps run in order from one reset, so
# later steps see what earlier ones wrote.
STEPS = [
    ('ann', 'GET', '/workspaces', None), ('eve', 'GET', '/workspaces', None), ('nobody', 'GET', '/workspaces', None),
    ('ann', 'GET', A, None), ('eve', 'GET', A, None), ('ann', 'GET', '/workspaces/not-a-uuid', None),
    ('ann', 'GET', '/me', None), ('dan', 'GET', '/me', None),
    ('ben', 'PATCH', A, {'name': 'Acme Help'}), ('ann', 'PATCH', A, {'name': 'Acme Help'}), ('ann', 'PATCH', A, {'name': ''}),
    ('ann', 'POST', '/workspaces', {'name': 'Initech', 'display_name': 'Ann Again'}),
    ('ann', 'POST', '/workspaces', {'name': 'Initech'}),
    ('dan', 'GET', A + '/members', None), ('eve', 'GET', A + '/members', None),
    ('ben', 'POST', A + '/members', {'user_id': desk.FAY, 'role': 'agent', 'display_name': 'Fay Visiting'}),
    ('ann', 'POST', A + '/members', {'user_id': desk.FAY, 'role': 'agent', 'display_name': 'Fay Visiting'}),
    ('ann', 'POST', A + '/members', {'user_id': desk.FAY, 'role': 'agent', 'display_name': 'Fay Again'}),
    ('ann', 'POST', A + '/members', {'user_id': MISSING, 'role': 'agent', 'display_name': 'Nobody'}),
    ('ann', 'POST', A + '/members', {'user_id': desk.BEN, 'role': 'boss', 'display_name': 'Ben'}),
    ('ann', 'PATCH', f'/members/{M_DAN}', {'role': 'agent'}), ('ben', 'PATCH', f'/members/{M_DAN}', {'role': 'viewer'}),
    ('ann', 'PATCH', f'/members/{M_DAN}', {'role': 'viewer', 'display_name': 'Dan V'}),
    ('ann', 'PATCH', f'/members/{desk.M_ANN}', {'role': 'agent'}), ('ann', 'DELETE', f'/members/{desk.M_ANN}', None),
    ('eve', 'DELETE', f'/members/{M_DAN}', None), ('ann', 'PATCH', f'/members/{M_EVE}', {'role': 'agent'}),
    ('cat', 'GET', A + '/customers', None), ('dan', 'GET', A + '/customers', None), ('eve', 'GET', A + '/customers', None),
    ('cat', 'POST', A + '/customers', {'name': 'Cleo New', 'email': 'cleo@customer.example.test'}),
    ('cat', 'POST', A + '/customers', {'name': 'Cleo Dup', 'email': 'cleo@customer.example.test', 'phone': '+44 1234'}),
    ('cat', 'POST', A + '/customers', {'name': 'Bad', 'email': 'not-an-email'}),
    ('dan', 'POST', A + '/customers', {'name': 'Dan Made', 'email': 'dan-made@customer.example.test'}),
    ('eve', 'POST', A + '/customers', {'name': 'Eve Made', 'email': 'eve-made@customer.example.test'}),
    ('cat', 'GET', f'/customers/{CORA}', None), ('dan', 'GET', f'/customers/{CORA}', None), ('eve', 'GET', f'/customers/{CORA}', None),
    ('cat', 'PATCH', f'/customers/{CORA}', {'phone': None}), ('cat', 'PATCH', f'/customers/{CORA}', {'email': 'colin@customer.example.test'}),
    ('cat', 'PATCH', f'/customers/{CORA}', {}), ('cat', 'GET', f'/customers/{CORA}/tickets', None),
    ('dan', 'GET', f'/customers/{CORA}/tickets', None), ('eve', 'GET', f'/customers/{CORA}/tickets', None),
    ('cat', 'DELETE', f'/customers/{GAIL}', None), ('cat', 'DELETE', f'/customers/{COLIN}', None),
    ('ben', 'DELETE', f'/customers/{COLIN}', None), ('ben', 'GET', f'/tickets/{T_PENDING}', None),
    ('dan', 'GET', A + '/tickets', None), ('cat', 'GET', A + '/tickets?status=open', None),
    ('cat', 'GET', A + f'/tickets?assignee_id={M_CAT}&page_size=1', None), ('eve', 'GET', A + '/tickets', None),
    ('cat', 'GET', A + '/tickets?status=closed', None), ('cat', 'GET', A + '/tickets?page_size=0', None),
    ('cat', 'POST', A + '/tickets', {'customer_id': CORA, 'subject': 'Printer on fire'}),
    ('cat', 'POST', A + '/tickets', {'customer_id': GAIL, 'subject': 'Wrong workspace'}),
    ('cat', 'POST', A + '/tickets', {'customer_id': MISSING, 'subject': 'Nobody'}),
    ('dan', 'POST', A + '/tickets', {'customer_id': CORA, 'subject': 'Viewer ticket'}),
    ('cat', 'POST', A + '/tickets', {'customer_id': CORA, 'subject': 'x' * 201}),
    ('dan', 'GET', f'/tickets/{T_OPEN}', None), ('eve', 'GET', f'/tickets/{T_OPEN}', None), ('ann', 'GET', f'/tickets/{T_BIN}', None),
    ('cat', 'PATCH', f'/tickets/{T_OPEN}', {'priority': 'urgent'}), ('dan', 'PATCH', f'/tickets/{T_OPEN}', {'priority': 'low'}),
    ('cat', 'PATCH', f'/tickets/{T_OPEN}', {'status': 'solved'}),
    ('cat', 'POST', f'/tickets/{T_OPEN}/assign', {'assignee_id': M_BEN}),
    ('cat', 'POST', f'/tickets/{T_OPEN}/assign', {'assignee_id': M_DAN}),
    ('cat', 'POST', f'/tickets/{T_OPEN}/assign', {'assignee_id': M_EVE}),
    ('cat', 'POST', f'/tickets/{T_OPEN}/assign', {'assignee_id': None}),
    ('cat', 'POST', f'/tickets/{T_OPEN}/solve', None), ('cat', 'POST', f'/tickets/{T_OPEN}/solve', None),
    ('cat', 'POST', f'/tickets/{T_OPEN}/reopen', None), ('dan', 'POST', f'/tickets/{T_OPEN}/solve', None),
    ('cat', 'GET', f'/tickets/{T_OPEN}/customer', None), ('dan', 'GET', f'/tickets/{T_OPEN}/customer', None),
    ('dan', 'GET', f'/tickets/{T_OPEN}/replies', None), ('eve', 'GET', f'/tickets/{T_OPEN}/replies', None),
    ('ann', 'GET', f'/tickets/{T_BIN}/replies', None),
    ('cat', 'POST', f'/tickets/{T_OPEN}/replies', {'body': 'Fixed it for you.'}),
    ('cat', 'GET', f'/tickets/{T_OPEN}', None), ('dan', 'POST', f'/tickets/{T_OPEN}/replies', {'body': 'Viewer reply'}),
    ('cat', 'PATCH', f'/replies/{R_OPEN}', {'body': 'Edited'}), ('dan', 'PATCH', f'/replies/{R_OPEN}', {'body': 'Viewer edit'}),
    ('eve', 'PATCH', f'/replies/{R_OPEN}', {'body': 'Outsider edit'}), ('cat', 'DELETE', f'/replies/{R_GLOBEX}', None),
    ('cat', 'GET', f'/tickets/{T_OPEN}/notes', None), ('dan', 'GET', f'/tickets/{T_OPEN}/notes', None),
    ('eve', 'GET', f'/tickets/{T_OPEN}/notes', None), ('cat', 'POST', f'/tickets/{T_OPEN}/notes', {'body': 'Second note'}),
    ('dan', 'POST', f'/tickets/{T_OPEN}/notes', {'body': 'Viewer note'}), ('ben', 'PATCH', f'/notes/{N_OPEN}', {'body': 'Edited note'}),
    ('dan', 'PATCH', f'/notes/{N_OPEN}', {'body': 'Viewer edit'}), ('cat', 'DELETE', f'/notes/{N_GLOBEX}', None),
    ('dan', 'DELETE', f'/notes/{N_OPEN}', None), ('cat', 'DELETE', f'/notes/{N_OPEN}', None),
    ('dan', 'GET', A + '/tags', None), ('eve', 'GET', A + '/tags', None), ('cat', 'POST', A + '/tags', {'name': 'urgent'}),
    ('ben', 'POST', A + '/tags', {'name': 'urgent'}), ('ben', 'POST', A + '/tags', {'name': 'urgent'}),
    ('ben', 'PATCH', f'/tags/{TAG_BILLING}', {'name': 'invoices'}), ('ben', 'PATCH', f'/tags/{TAG_VIP}', {'name': 'mine'}),
    ('dan', 'GET', f'/tickets/{T_PENDING}/tags', None), ('cat', 'POST', f'/tickets/{T_OPEN}/tags', {'tag_id': TAG_BILLING}),
    ('cat', 'POST', f'/tickets/{T_OPEN}/tags', {'tag_id': TAG_BILLING}), ('cat', 'POST', f'/tickets/{T_OPEN}/tags', {'tag_id': TAG_VIP}),
    ('dan', 'POST', f'/tickets/{T_SOLVED}/tags', {'tag_id': TAG_BILLING}),
    ('cat', 'DELETE', f'/tickets/{T_OPEN}/tags/{TAG_BILLING}', None), ('cat', 'DELETE', f'/tickets/{T_OPEN}/tags/{TAG_BILLING}', None),
    ('ben', 'DELETE', f'/tags/{TAG_BILLING}', None), ('cat', 'GET', f'/tickets/{T_PENDING}/tags', None),
    ('cat', 'GET', A + '/macros', None), ('dan', 'GET', A + '/macros', None), ('cat', 'POST', A + '/macros', {'title': 'Hi', 'body': 'Hello'}),
    ('ben', 'POST', A + '/macros', {'title': 'Hi', 'body': 'Hello'}), ('cat', 'GET', f'/macros/{MACRO_THANKS}', None),
    ('dan', 'GET', f'/macros/{MACRO_THANKS}', None), ('ben', 'PATCH', f'/macros/{MACRO_THANKS}', {'body': 'Thanks again.'}),
    ('cat', 'PATCH', f'/macros/{MACRO_THANKS}', {'body': 'Agent edit'}), ('cat', 'DELETE', f'/macros/{MACRO_THANKS}', None),
    ('ben', 'DELETE', f'/macros/{MACRO_THANKS}', None), ('ben', 'GET', f'/macros/{MACRO_THANKS}', None),
    ('cat', 'DELETE', f'/tickets/{T_SOLVED}', None), ('ben', 'DELETE', f'/tickets/{T_SOLVED}', None),
    ('ben', 'DELETE', f'/tickets/{T_SOLVED}', None), ('ben', 'GET', A + '/tickets', None),
    ('eve', 'DELETE', f'/tickets/{T_PENDING}', None), ('cat', 'GET', f'/tickets/{G_OPEN}', None),
    ('ann', 'DELETE', f'/members/{M_CAT}', None), ('ann', 'GET', f'/tickets/{T_PENDING}', None),
    ('cat', 'GET', A + '/tickets', None),
]


def placeholders(text, known):
    """Ids the fixtures don't hold and the clock's own reading, replaced by
    stable names in order of first appearance; request ids dropped."""
    names = {}

    def swap(match):
        value = match.group(0)
        if value in known:
            return value
        return names.setdefault(value, f'<id{len(names) + 1}>')
    text = re.sub(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', swap, text)
    return text


def normalise(response, known):
    body = response['json']
    if isinstance(body, dict) and isinstance(body.get('error'), dict):
        body = {'error': {k: v for k, v in body['error'].items() if k in {'code', 'details'}}}
    if isinstance(body, dict) and 'next' in body and body['next']:
        body = {**body, 'next': '<cursor>'}
    return {'status': response['status'], 'body': body}


def record(controller):
    known = {getattr(desk, name) for name in dir(desk) if isinstance(getattr(desk, name), str)
             and re.fullmatch(r'[0-9a-f-]{36}', getattr(desk, name))} | {MISSING}
    controller.reset()
    out = []
    for who, method, path, body in STEPS:
        response = controller.request(method, path, auth=who, body=body)
        out.append({'step': f'{who} {method} {path}', 'request': body, **normalise(response, known)})
    text = placeholders(json.dumps(out, indent=1, sort_keys=True), known)
    return json.loads(text)


def compare(left, right):
    differences = []
    for a, b in zip(left, right):
        if a != b:
            differences.append({'step': a['step'], 'jadpo': {k: a[k] for k in ('status', 'body')},
                                'typescript': {k: b[k] for k in ('status', 'body')}})
    if len(left) != len(right):
        differences.append({'step': 'count', 'jadpo': len(left), 'typescript': len(right)})
    return differences


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('compare').add_argument('files', nargs=2)
    args = parser.parse_args()
    if args.command == 'compare':
        left, right = (json.loads(Path(f).read_text()) for f in args.files)
        differences = compare(left, right)
        print(json.dumps(differences, indent=1))
        sys.exit(1 if differences else 0)


if __name__ == '__main__':
    main()
