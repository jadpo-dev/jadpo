---
title: Project layout and modules, design notes
area: 1
---

# Project layout and modules: design notes

Why [Project layout and modules](../01-project-layout-and-modules.md) is the way it is: the decisions behind it, how it fits the rest of the language and what it carried to other areas. An app author reads the area page, not this one.

## Decisions

| Topic | Decision |
|---|---|
| Shape | Same at every size: fixed root, all code in one level of domains, the same fixed folders in every domain, no subfolders |
| `domains/` folder (question 1) | Domains live in `domains/`, so every root entry is fixed by the language, projects differ only inside it, and nobody has to tell a domain from a reserved folder |
| Call naming (question 2) | The dot names the owner of what you call, so every call says where its function lives and libraries read exactly like the standard library; your own domain's actions and queries are called bare (area 3) |
| New domains (question 3) | Creating or renaming a domain appears in the approval review like a policy change, because it changes who owns what; otherwise an agent that hits a cross-domain error might add a domain or reshuffle code rather than ask |
| Policy protection (question 4) | Through the approval check, which fingerprints policy, so any policy change needs human approval wherever it lives, not by file path |
| Deployables (question 5) | An API, a worker or a back office are declarations in one project, not separate projects; each selects its routes, schedules and subscribers and is compiled from what it reaches |
| Large entities (question 6) | No companion files; the formatter keeps a fixed section order. Revisit with measured sizes from the order and payment app |
| Reserved words (question 7) | Control-flow and declaration words are fully reserved, with a short explicit list of words that can be used as names; the lists are under [Naming](../01-project-layout-and-modules.md#naming) |
| Documentation comments (question 8) | `///` comments attach to the next declaration and feed generated documentation, the library catalogue and OpenAPI descriptions |
| Formatting (question 9) | The formatter decides blank lines (one between declarations) and section order within a file, so equivalent code always looks identical |
| Root `api/` and `acceptances/` folders (question 10) | Both are in the fixed root: `api/` keeps each API's record and `acceptances/` a person's acceptance of a breaking change (areas 7 and 18), beside `packages/` and `jadpo.lock` (area 11) |
| Declinable errors (question 11) | No warning severity: every finding is an error, and the judgement calls are declinable with a `decline <rule>: "<reason>"` line directly above the declaration, the reason required; see [Declining a judgement call](../01-project-layout-and-modules.md#declining-a-judgement-call). One generic form rather than a per-rule option, which would need a new name per rule, fits only declarations with a settings block and can't be listed or cleared by one command |
| Folder names | Each folder is named for what its files contain, usually the declaration keyword in plural; `failures/` holds `type X = Failure` declarations (area 4) |
| Keywords match folders | `schedule` (folder `schedules/`) and `library` (`libraries/`); external services are provider packages (area 11) |
| Triggers | Routes, schedules and subscribers each call actions; durable workflows are subscribers |
| Root files | `app.jadpo`, `config.jadpo`, `authentication.jadpo`, `policy.jadpo` and `flags.jadpo`; no `env.jadpo` |
| Monitoring files | Each domain may hold one `monitoring.jadpo` directly in its folder. The place is built; what it declares, area 16's thresholds, is not built yet |
| Compiler data | A visible `database/` folder for the identity registry, migrations and seeds, not a hidden folder |
| Tests | Beside their subject; cross-domain scenarios in root `tests/`; no folder per test kind |
| Domain naming | The folder name is the domain name; business-area names; junk-drawer names rejected |
| Cross-domain writes | Public actions, sharing the caller's transaction; events for reactions that need not be all-or-nothing |
| Promotion | Required, with a one-step fix; demotion never forced |
| Helpers | Descriptions written once as `///`; catalogue, callers and impact computed; duplicates flagged by `jadpo check` |
| Unnamed singletons | Declarations that exist once per project carry no name: `application { }`, `config { }`, `principal { }`, `locales { }` |
| Query and action placement | Every query in its domain's `queries/` folder and every action in `actions/`, one per file, called by name; entity files hold the data definition and the generated operations (area 5) |
| Names folding together | A type may fold to the name of the callable declared in the same file (an input named after its action); everywhere else `naming.folded_collision` applies |
| Emails, translations, prompts and documents folders | Each domain may have `emails/` (area 16), `translations/` (area 10), `prompts/` (area 21) and `documents/` (area 16); the root `translations/` holds only the app's wording for Jadpo's and library packages' text, grouped by owner |
| Public fixtures | A fixture may be `public`, offered to other domains' tests, fixtures and seeds and to root `tests/` scenarios, which call it through its domain; only tests, fixtures and seeds ever call a fixture (area 13) |
| Unused `public` | `public` on a declaration nothing outside its domain uses is `visibility.public_unused`; a package's public members are exempt (area 11) |
| Reachable from public | A type or failure reachable from a public declaration's signature must itself be public, in domains and packages alike |
| Imports | Every name not declared in a file is imported with `use <folder> { Names }`, own domain included; built-ins never are; `jadpo fmt` writes and sorts them; see [Imports](../01-project-layout-and-modules.md#imports) |
| A file per shared name | A declaration any other file uses has a file of its own named after it; supporting declarations used only by their file (and its test) stay in it |
| A file holds only what its primary declaration uses | Everything beside the primary declaration is used by it, or is `layout.unused_declaration`; see [Files and names](../01-project-layout-and-modules.md#files-and-names) |
| Definition before use | Every declaration comes before the first one that uses it, so the primary declaration comes last; see [Files and names](../01-project-layout-and-modules.md#files-and-names) |
| Names per domain | A private function, type or constant needs a name unique only within its domain, so agents in parallel workspaces don't clash on private helper names at merge; a domain is not a namespace. See [Files and names](../01-project-layout-and-modules.md#files-and-names) |
| Constants in the layout | A `constant` is placed and shared as a helper function is (area 3) |
| Packages in the layout | The root `packages/` holds only installed packages, checked against `jadpo.lock`; the app's own integrations live in `shared/providers/` in the package format; `use packages/…` always names installed code (area 11) |
| Records | Committed generated files are readable `.jadpo_record` files beside what they describe, each starting with the line that says what wrote it; a hand edit is an error; `jadpo.lock` keeps its name (area 18) |
