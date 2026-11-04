#!/usr/bin/env python3
"""What the bundle writes from the reference's manifest (docs/documentation.md,
"The reference's manifest"): each node's landing page and contents, and the
status line at the top of a page not wholly built.

Everything here works from the tree `reference_manifest.Reference.nav` gives,
so a table can't drift from the sidebar, and nothing is written into a topic's
source.
"""
from __future__ import annotations

import os
import re

from reference_manifest import NOT_BUILT, PARTLY, SITE_OWN, TYPES_INDEX

STATUS_LINE = re.compile(r'^\*\*(Partly built|Not built)\.\*\* ')
STATUS_WORDS = {PARTLY: 'Partly built', NOT_BUILT: 'Not built'}
# A row's status where it stands beside a name, or in a column of its own.
MARKS = {PARTLY: '▲ partly', NOT_BUILT: '○ not built'}
COLUMN = {'built': '✓ built', **MARKS}
DETAILS_LINE = 'The exact rules, for when you hit an edge case.'
ALL_OF_IT = 'All of it'


def own_page(node, pages):
    """The page a node opens: its landing, or the first topic that stands for
    it. A page the site writes itself is none of the bundle's."""
    if node['href'] in SITE_OWN:
        return None
    if node['id']:
        return node['id'] if node['id'] in pages else None
    return node['topics'][0]['id'] if node['topics'] else None


def walk(tree):
    def under(node):
        yield node
        for child in node['children']:
            yield from under(child)
    for group in tree['groups']:
        yield from under(group)


def cell(text):
    """Text for a table's cell. A summary cut short can leave a code span open,
    which would swallow the cell, so it stops before that span, and a sentence
    that led into a list ends here."""
    text = (text or '').replace('|', '\\|').replace('\n', ' ').strip()
    if text.count('`') % 2:
        text = text[:text.rfind('`')].rstrip(' ,;:') + '…'
    return text[:-1] + '.' if text.endswith(':') else text


def capital(text):
    return text[0].upper() + text[1:] if text and text[0].isalpha() else text


class Writer:
    """One bundle's pages: `pages` is its `{id: bundle path}`, `unfinished`
    what `Reference.unfinished` gives, `built_of` an area file's "What is built"
    topic."""

    def __init__(self, pages, unfinished, built_of=None):
        self.pages, self.unfinished, self.built_of = pages, unfinished, built_of or {}

    def link(self, title, page_id, here, code=False):
        target = os.path.relpath(self.pages[page_id], os.path.dirname(here) or '.')
        return f'[{f"`{title}`" if code else cell(title)}]({target})'

    def status_of(self, node):
        own = own_page(node, self.pages)
        return node['status'] or (self.unfinished.get(own) or {}).get('status')

    def entry_row(self, entry, here):
        code = entry['kind'] != 'topic'
        status = entry['status'] if entry['status'] in MARKS else None
        name = self.link(entry['title'], entry['id'], here, code=code)
        return f'{name} {MARKS[status]}' if status else name, cell(entry['summary'])

    def node_row(self, node, here, also=False):
        own = own_page(node, self.pages)
        name = self.link(node['title'], own, here) if own else cell(node['title'])
        status = self.status_of(node)
        summary = cell(node['summary'])
        if also and node['topics'] and node['children']:
            links = [self.link(c['title'], own_page(c, self.pages), here)
                     for c in node['children'] if own_page(c, self.pages)]
            summary = f'{summary} Also: {" · ".join(links)}'
        return (f'{name} {MARKS[status]}' if status in MARKS else name), summary

    def column_row(self, node, here):
        own = own_page(node, self.pages)
        status = self.status_of(node)
        missing = capital(node['missing']) if node['missing'] else ALL_OF_IT if status == NOT_BUILT else ''
        name = self.link(node['title'], own, here) if own else cell(node['title'])
        return f'| {name} | {cell(node["summary"])} | {COLUMN.get(status, "–")} | {cell(missing)} |'

    def table(self, rows, head=('Page', 'What it covers')):
        if not rows:
            return []
        lines = [f'| {head[0]} | {head[1]} |', '|---|---|']
        lines += [f'| {name} | {summary} |' for name, summary in rows]
        return lines + ['']

    def rows_of(self, node, here, kind='topic'):
        own = own_page(node, self.pages)
        return [self.entry_row(e, here) for e in node['topics'] if e['kind'] == kind and e['id'] != own]

    def built_line(self, node, here, column):
        out = []
        for entry in node['built']:
            link = self.link('What is built', entry['id'], here)
            out += [f'Each row\'s status comes from {link}.' if column else f'{link} says which of this works today.', '']
        return out

    def contents(self, node, here, group=False):
        """What comes after a page's intro: a row for each thing it holds."""
        out = []
        column = node.get('status_column')
        topics = self.rows_of(node, here)
        types = self.rows_of(node, here, 'type')
        commands = self.rows_of(node, here, 'command')
        if node['id'] == TYPES_INDEX:
            out += ['## By group', '', '| Group | Types |', '|---|---|']
            out += [f'| {self.link(c["title"], own_page(c, self.pages), here)} | {len(c["topics"])} |'
                    for c in node['children']]
            out += ['', '## About standard types', '']
            out += self.table(topics)
            return out + self.built_line(node, here, False)
        if types:
            if topics:
                out += ['## Topics', ''] + self.table(topics)
            out += ['## Types', ''] + self.table(types, ('Type', 'What it is'))
            return out + self.built_line(node, here, False)
        if commands:
            return self.table(commands, ('Command', 'What it does')) + self.built_line(node, here, False)
        if any(self.rows_of(c, here, 'command') for c in node['children']):
            # The commands are looked up by name, so a heading holds its own.
            out += self.table(topics)
            for child in node['children']:
                out += [f'## {self.link(child["title"], own_page(child, self.pages), here)}', '', cell(child['summary']), '']
                out += self.table(self.rows_of(child, here, 'command'), ('Command', 'What it does'))
            return out + self.built_line(node, here, False)
        if column:
            out += self.table(topics)
            out += [f'| {column} | What it is | Status | What\'s missing |', '|---|---|---|---|']
            out += [self.column_row(c, here) for c in node['children']] + ['']
        else:
            out += self.table(topics + [self.node_row(c, here, also=group) for c in node['children']])
        return out + self.built_line(node, here, bool(column))

    def details(self, node, here):
        if not node['details']:
            return []
        return ['## Details', '', DETAILS_LINE, ''] + self.table([self.entry_row(e, here) for e in node['details']])

    def landing(self, node, group=False):
        """A landing page to be made, whole."""
        here = self.pages[node['id']]
        lines = [f'# {node["title"]}', '', node['summary'], '']
        lines += self.contents(node, here, group) + self.details(node, here)
        return '\n'.join(lines).rstrip('\n') + '\n'

    def with_contents(self, text, node, here, group=False):
        """A topic that opens a node, its contents after its intro and its
        Details last."""
        contents = '\n'.join(self.contents(node, here, group))
        details = '\n'.join(self.details(node, here))
        if contents:
            text = insert_after_intro(text, contents.rstrip('\n') + '\n', table_ends=node['id'] == TYPES_INDEX)
        if details:
            text = text.rstrip('\n') + '\n\n' + details.rstrip('\n') + '\n'
        return text

    def status_line(self, page_id, here):
        """The line at the top of a page not wholly built, or None."""
        info = self.unfinished.get(page_id)
        if not info:
            return None
        built = self.built_of.get(info['area'])
        see = f' See {self.link("What is built", built, here)}.' if built else ''
        words = f'**{STATUS_WORDS[info["status"]]}.** '
        if info['status'] == NOT_BUILT:
            return f"{words}This page is the design; Jadpo doesn't do this yet.{see}"
        if info['missing']:
            return f'{words}Missing: {info["missing"]}.{see}'
        return f"{words}Some of this isn't built yet.{see}"


def intro_end(lines, table_ends=False):
    """The line a page's intro stops at: its first `##`, or with `table_ends`
    its first table too."""
    fenced = False
    for i, line in enumerate(lines):
        if line.startswith('```'):
            fenced = not fenced
        elif not fenced and (line.startswith('## ') or (table_ends and line.startswith('|'))):
            return i
    return len(lines)


def insert_after_intro(text, block, table_ends=False):
    lines = text.split('\n')
    at = intro_end(lines, table_ends)
    before = '\n'.join(lines[:at]).rstrip('\n')
    after = '\n'.join(lines[at:]).lstrip('\n')
    if table_ends and after.startswith('|'):
        after = '## A to Z\n\n' + after
    return f'{before}\n\n{block}' + (f'\n{after}' if after else '')


def with_status_line(text, line):
    """A page with `line` straight under its title."""
    if text.startswith('# '):
        title, _, rest = text.partition('\n')
        return f'{title}\n\n{line}\n\n{rest.lstrip(chr(10))}'
    return f'{line}\n\n{text}'


def status_line_at_top(text):
    """The status words a page opens with, or None."""
    lines = [line for line in text.split('\n') if line.strip()]
    start = 1 if lines and lines[0].startswith('# ') else 0
    match = STATUS_LINE.match(lines[start]) if len(lines) > start else None
    return match.group(1) if match else None
