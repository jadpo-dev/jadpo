#!/usr/bin/env python3
"""The reference's manifest, docs/reference.toml (docs/documentation.md, "The
reference's manifest"): how the reference is grouped for a reader. A topic's
text stays on its area page.

A node is a group, a page or a section, each holding the next. It gives a
`title`, and any of:

- `id`: its landing page. A topic of that ID is placed by it; no page of that
  ID means a landing page to be made, which needs a `summary`.
- `topics`, `details`: topics in reading order, usage then exact rules.
- `built`: its "What is built" topics, which stay out of the sidebar.
- `types`: a group of the compiler's standard types, listed after its topics.
- `commands`: true on the one node that lists every command.
- `status`: the "What is built" rows it reports, as `<area file>: <the start of
  the row's first cell>`, and `missing`, what a partly built one lacks.
- `href`: only for a page the site writes itself.

`[topic_status]` gives a topic the rows of its own. Nothing here parses a
page: Bundle reads the pages and hands this module its index entries, each with
`id`, `kind`, `title` and `summary`.

The manifest gives index.json each placed page's `group`, `page` and `status`,
and the bundle's nav.json.
"""
from __future__ import annotations

import re
import tomllib
from pathlib import Path

import agent_guidance

MANIFEST = Path('docs/reference.toml')
AREA_PAGES = 'docs/language'
AREA_TABLE = '## What is built'
KEBAB = re.compile(r'^[a-z0-9]+(?:-[a-z0-9]+)*$')
# The pages a node may name by `href`: the site writes them, so they are in no
# bundle (docs/documentation.md, "What the site serves").
SITE_OWN = {'/docs/errors'}
# The one page a node may open that isn't a topic.
TYPES_INDEX = 'types'
# A status as its row's first words give it.
BUILT, PARTLY, NOT_BUILT = 'built', 'partly built', 'not built'
STATUS_WORDS = (('Partly built', PARTLY), ('Not built', NOT_BUILT), ('Built', BUILT))
BUILT_TITLE = 'What is built'
NODE_KEYS = {'id', 'title', 'summary', 'href', 'status', 'missing', 'topics', 'details', 'built', 'types', 'commands'}
TOP_KEYS = {'group', 'topic_status'}


def status_rows(root):
    """Each area page's "What is built" rows, `{area file: [(first cell,
    status cell)]}`, as the tables give them."""
    rows = {}
    for path in sorted((root / AREA_PAGES).glob('[0-9][0-9]-*.md')):
        table = agent_guidance.section(path.read_text(), AREA_TABLE)
        if table is not None:
            rows[path.stem] = [(agent_guidance.plain(row[0]), row[-1]) for row in agent_guidance.first_table(table)]
    return rows


def load(root):
    """The manifest under `root`, or None when it has none."""
    path = root / MANIFEST
    if not path.exists():
        return None
    try:
        data = tomllib.loads(path.read_text())
    except tomllib.TOMLDecodeError as error:
        return Reference({}, {}, error=str(error))
    return Reference(data, status_rows(root))


def status_of(cell):
    """A status cell's first words as a status, or None."""
    return next((status for words, status in STATUS_WORDS if cell.startswith(words)), None)


def combined(statuses):
    """What several rows say together: one status if they agree, else partly
    built."""
    found = {status for status in statuses if status}
    if not found:
        return None
    return found.pop() if len(found) == 1 else PARTLY


def children_of(node):
    return list(node.get('page', [])) + list(node.get('section', []))


def is_landing(page):
    """Whether a page may be a node's own: a topic, or the types' index."""
    return page is not None and (page['kind'] == 'topic' or page['id'] == TYPES_INDEX)


def landing_of(node, pages):
    """The page a node's `id` names, if it may be the node's own."""
    page = pages.get(node['id']) if node.get('id') else None
    return page if is_landing(page) else None


class Reference:
    def __init__(self, data, rows, error=None):
        self.data, self.rows, self.error = data, rows, error
        self.topic_status = dict(data.get('topic_status', {}))

    def walk(self):
        """Every node, parents first, with its trail of titles."""
        def under(node, trail):
            trail = trail + (node.get('title') or node.get('id') or '(no title)',)
            yield node, trail
            for child in children_of(node):
                yield from under(child, trail)
        for group in self.data.get('group', []):
            yield from under(group, ())

    def row(self, spec):
        """A status row by its `<area file>: <start of first cell>`, as
        (the status, a problem): one of them is None, or both when the row
        has no reading."""
        area, separator, feature = spec.partition(': ')
        if not separator:
            return None, f'status row "{spec}" needs "<area file>: <the start of its first cell>"'
        if area not in self.rows:
            return None, f'status row "{spec}": {AREA_PAGES}/{area}.md has no "{AREA_TABLE[3:]}" table'
        rows = self.rows[area]
        matches = [r for r in rows if r[0] == feature] or [r for r in rows if r[0].startswith(feature)]
        if not matches:
            return None, f'status row "{spec}": {AREA_PAGES}/{area}.md has no row starting that way'
        if len(matches) > 1:
            return None, f'status row "{spec}": more than one row in {AREA_PAGES}/{area}.md starts that way'
        status = status_of(matches[0][1])
        if status is None:
            return None, (f'status row "{spec}": its status says "{matches[0][1][:40]}", which doesn\'t start '
                          f'with Built, Partly built or Not built')
        return status, None

    def status(self, specs):
        return combined(self.row(spec)[0] for spec in specs)

    def type_ids(self, node, type_groups):
        """The pages of a node's compiler group."""
        return dict(type_groups or []).get(node.get('types'), [])

    def listed(self, node, pages, type_groups):
        """A node's `topics`, then what it generates, as the IDs with pages."""
        ids = list(node.get('topics', [])) + list(self.type_ids(node, type_groups))
        if node.get('commands'):
            ids += [page['id'] for page in pages.values() if page['kind'] == 'command']
        return [page_id for page_id in ids if page_id in pages]

    def entry_status(self, page_id, node_status, gone, stands_for):
        """A page's status: its own rows; else nothing on a page not built is
        built; else the page's own topic stands for the page. Anything else on
        a partly built page says nothing, since it may be wholly built."""
        own = self.topic_status.get(page_id)
        if own:
            return self.status(own)
        if gone:
            return NOT_BUILT
        return node_status if page_id == stands_for else None

    def stands_for(self, node, listed, pages):
        """The topic a node's status is the status of."""
        if landing_of(node, pages):
            return node['id']
        return listed[0] if listed else None

    def places(self, pages, type_groups):
        """`{page id: {'group', 'page', 'status'}}`, for index.json."""
        places = {}

        def visit(node, trail, gone):
            status = self.status(node.get('status', []))
            gone = gone or status == NOT_BUILT
            listed = self.listed(node, pages, type_groups)
            stands_for = self.stands_for(node, listed, pages)
            ids = listed + [i for i in node.get('details', []) if i in pages]
            if landing_of(node, pages):
                ids.insert(0, node['id'])
            where = {'group': trail[0], 'page': trail[1] if len(trail) > 1 else None}
            for page_id in ids:
                places.setdefault(page_id, where | {'status': self.entry_status(page_id, status, gone, stands_for)})
            for page_id in node.get('built', []):
                if page_id in pages:
                    places.setdefault(page_id, where | {'status': None})
            for child in children_of(node):
                visit(child, trail + (child.get('title'),), gone)
        for group in self.data.get('group', []):
            visit(group, (group.get('title'),), False)
        return places

    def nav(self, pages, type_groups, path_of, version):
        """The tree nav.json holds, every address worked out, so the site reads
        neither this manifest nor the tables."""
        def entry(page_id, status):
            page = pages[page_id]
            return {'id': page_id, 'kind': page['kind'], 'title': page['title'], 'summary': page['summary'],
                    'href': path_of(page['kind'], page_id), 'status': status}

        def tree(node, gone):
            status = self.status(node.get('status', []))
            gone = gone or status == NOT_BUILT
            listed = self.listed(node, pages, type_groups)
            stands_for = self.stands_for(node, listed, pages)
            topics = [entry(i, self.entry_status(i, status, gone, stands_for)) for i in listed]
            details = [entry(i, self.entry_status(i, status, gone, stands_for))
                       for i in node.get('details', []) if i in pages]
            built = [entry(i, None) for i in node.get('built', []) if i in pages]
            children = [tree(child, gone) for child in children_of(node)]
            own = landing_of(node, pages)
            first = next((n for n in topics + details + children), None)
            href = node.get('href') or (path_of(own['kind'], own['id']) if own else first['href'] if first else None)
            summary = node.get('summary') or (own['summary'] if own else first['summary'] if first else None)
            return {'id': node.get('id'), 'title': node.get('title'), 'summary': summary, 'href': href,
                    'status': status, 'missing': node.get('missing') if status == PARTLY else None,
                    'topics': topics, 'details': details, 'built': built, 'children': children}

        return {'version': version, 'groups': [tree(group, False) for group in self.data.get('group', [])]}

    def problems(self, pages, aliases, type_groups):
        """What's wrong with the manifest against the bundle's pages: a topic
        in no place or two, an ID or a status row that's gone, a partly built
        page not saying what is missing. `aliases` maps a renamed topic's old
        IDs to its new one; `type_groups` is the compiler's, or None when
        there is no compiler data to compare."""
        if self.error:
            return [f'{MANIFEST}: {self.error}']
        found, places, ids, groups, commands = [], {}, {}, {}, []

        def at(where, message):
            found.append(f'{MANIFEST}, {where}: {message}')

        def place(where, page_id, built=False):
            page_id, named = aliases.get(page_id, page_id), page_id
            page = pages.get(page_id)
            if named != page_id:
                at(where, f'"{named}" is now "{page_id}"')
            elif page is None or page['kind'] != 'topic':
                what = 'no page has it' if page is None else f'it is a page of kind {page["kind"]}'
                at(where, f'"{named}" isn\'t a topic: {what}')
                return
            places.setdefault(page_id, []).append(where)
            if page is not None and page['title'].startswith(BUILT_TITLE) and not built:
                at(where, f'"{page_id}" is a "{BUILT_TITLE}" topic, which goes under `built`, out of the sidebar')
            if page is not None and built and not page['title'].startswith(BUILT_TITLE):
                at(where, f'"{page_id}" isn\'t a "{BUILT_TITLE}" topic, so it doesn\'t go under `built`')

        for key in sorted(set(self.data) - TOP_KEYS):
            found.append(f'{MANIFEST}: "{key}" is a key nothing reads')
        for node, trail in self.walk():
            where = ' › '.join(trail)
            for key in sorted(set(node) - NODE_KEYS - {'page' if len(trail) == 1 else 'section'}):
                at(where, f'"{key}" is a key nothing reads')
            if not node.get('title'):
                at(where, 'needs a title')
            node_id = node.get('id')
            if node_id is not None:
                ids.setdefault(node_id, []).append(where)
                if not KEBAB.match(node_id):
                    at(where, f'its id "{node_id}" isn\'t kebab case')
                page = pages.get(node_id)
                if page is None:
                    if not node.get('summary'):
                        at(where, f'no page is "{node_id}", so a landing page will be made, which needs a `summary`')
                elif not is_landing(page):
                    at(where, f'its id "{node_id}" is a page of kind {page["kind"]}, not a topic to open')
                elif page['kind'] == 'topic':
                    place(where, node_id)
            href = node.get('href')
            if href is not None and href not in SITE_OWN:
                at(where, f'`href` is only for a page the site writes itself, and {href} isn\'t one')
            for page_id in list(node.get('topics', [])) + list(node.get('details', [])):
                place(where, page_id)
            for page_id in node.get('built', []):
                place(where, page_id, built=True)
            if node.get('types') is not None:
                groups.setdefault(node['types'], []).append(where)
            if node.get('commands'):
                commands.append(where)
            problems = [self.row(spec)[1] for spec in node.get('status', [])]
            for problem in filter(None, problems):
                at(where, problem)
            if self.status(node.get('status', [])) == PARTLY and not node.get('missing'):
                at(where, 'it is partly built, so it needs `missing`, a short phrase saying what is')
            holds = (landing_of(node, pages) or children_of(node) or node.get('types') or node.get('commands')
                     or node.get('topics') or node.get('details') or node.get('built'))
            if not holds:
                at(where, 'it places nothing')
        for node_id, wheres in sorted(ids.items()):
            if len(wheres) > 1:
                found.append(f'{MANIFEST}: the id "{node_id}" is on {" and ".join(wheres)}')
        for page_id, wheres in sorted(places.items()):
            if len(wheres) > 1:
                found.append(f'{MANIFEST}: topic "{page_id}" is in more than one place: {" and ".join(wheres)}')
        for page_id, page in pages.items():
            if page_id and page['kind'] == 'topic' and page_id not in places:
                found.append(f'{MANIFEST}: topic "{page_id}" has no place: add it to a page\'s `topics` or `details`')
        for page_id, specs in self.topic_status.items():
            if page_id not in places:
                found.append(f'{MANIFEST}, [topic_status]: "{page_id}" is no topic placed here')
            for problem in filter(None, (self.row(spec)[1] for spec in specs)):
                found.append(f'{MANIFEST}, [topic_status], "{page_id}": {problem}')
        if type_groups is not None:
            known = [name for name, _ in type_groups]
            for name in known:
                if name not in groups:
                    found.append(f'{MANIFEST}: the compiler\'s type group "{name}" has no place: give a section '
                                 f'`types = "{name}"`')
            for name, wheres in sorted(groups.items()):
                if name not in known:
                    found.append(f'{MANIFEST}, {wheres[0]}: the compiler has no type group "{name}"')
                if len(wheres) > 1:
                    found.append(f'{MANIFEST}: the type group "{name}" is on {" and ".join(wheres)}')
        if not commands and any(page['kind'] == 'command' for page in pages.values()):
            found.append(f'{MANIFEST}: the commands have no place: give one page `commands = true`')
        if len(commands) > 1:
            found.append(f'{MANIFEST}: `commands = true` is on {" and ".join(commands)}, and belongs on one')
        return found
