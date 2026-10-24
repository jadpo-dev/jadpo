"""Normalize explicit shared-test PostgreSQL DSNs at the harness boundary.

Accept libpq URI or key=value syntax. Preserve quoted/backslash-escaped values
and common connection parameters; only replace the selected database. A Unix
socket directory becomes a percent-encoded `host` query value, never TCP or a
substitute server. Production connection validation remains separate.
"""
from __future__ import annotations

import re
from urllib.parse import parse_qsl, quote, unquote, urlencode, urlsplit, urlunsplit


_INVALID = 'Invalid shared PostgreSQL connection string'


def connection_parameters(value: str) -> dict[str, str]:
    if not isinstance(value, str) or '\0' in value:
        raise ValueError(_INVALID)
    if value.startswith(('postgres://', 'postgresql://')):
        try:
            uri = urlsplit(value)
            if uri.fragment:
                raise ValueError(_INVALID)
            result = {}
            if uri.hostname is not None:
                result['host'] = unquote(uri.hostname)
            if uri.port is not None:
                result['port'] = str(uri.port)
            if uri.username is not None:
                result['user'] = unquote(uri.username)
            if uri.password is not None:
                result['password'] = unquote(uri.password)
            if uri.path and uri.path != '/':
                result['dbname'] = unquote(uri.path[1:])
            # libpq's URI reader percent-decodes; '+' is a literal plus, not
            # HTML form-space. Later values override earlier values as in libpq.
            result.update(parse_qsl(uri.query.replace('+', '%2B'), keep_blank_values=True))
            return result
        except ValueError:
            raise ValueError(_INVALID) from None
    # libpq keywords are not shell syntax: backslash also escapes inside
    # single quotes, and unquoted backslash can escape whitespace.
    result = {}
    at = 0
    size = len(value)
    while at < size:
        while at < size and value[at].isspace():
            at += 1
        if at == size:
            break
        match = re.match(r'[A-Za-z_][A-Za-z_0-9]*', value[at:])
        if match is None:
            raise ValueError(_INVALID)
        key = match[0]
        at += len(key)
        while at < size and value[at].isspace():
            at += 1
        if at == size or value[at] != '=':
            raise ValueError(_INVALID)
        at += 1
        while at < size and value[at].isspace():
            at += 1
        quoted = at < size and value[at] == "'"
        if quoted:
            at += 1
        text = []
        closed = not quoted
        while at < size:
            character = value[at]
            if character == '\\':
                at += 1
                if at == size:
                    raise ValueError(_INVALID)
                text.append(value[at])
                at += 1
            elif quoted and character == "'":
                at += 1
                closed = True
                break
            elif not quoted and character.isspace():
                break
            else:
                text.append(character)
                at += 1
        if not closed or (at < size and not value[at].isspace()):
            raise ValueError(_INVALID)
        result[key] = ''.join(text)
    if not result:
        raise ValueError(_INVALID)
    return result


def postgres_uri(value: str, database: str | None = None) -> str:
    parameters = connection_parameters(value)
    selected = parameters.pop('dbname', '') if database is None else database
    parameters.pop('dbname', None)
    host = parameters.pop('host', '')
    port = parameters.pop('port', '')
    user = parameters.pop('user', None)
    password = parameters.pop('password', None)
    if not host or (port and (not port.isascii() or not port.isdigit() or not 1 <= int(port) <= 65535)):
        raise ValueError(_INVALID)
    if host.startswith('/'):
        parameters['host'] = host
        authority = 'localhost'
    else:
        authority = '[' + host + ']' if ':' in host else host
    if port:
        authority += ':' + port
    if user is not None or password is not None:
        credentials = quote(user or '', safe='')
        if password is not None:
            credentials += ':' + quote(password, safe='')
        authority = credentials + '@' + authority
    return urlunsplit(('postgresql', authority, '/' + quote(selected, safe=''),
                       urlencode(parameters, quote_via=quote, safe=''), ''))
