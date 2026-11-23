# Helpdesk

A support desk many companies share. Each company is a **workspace**. Its
members answer **tickets** from its **customers**, write **replies** the
customer sees and **notes** only staff see, label tickets with **tags**,
and keep saved answers as **macros**. A schedule flags tickets nobody
answered in time.

This is an ordinary Jadpo application with no compiler special case. Its
twin in TypeScript, in [`typescript/`](typescript/README.md), answers every
route the same way.

## Who may do what

Everyone signs in as a `User`. A person belongs to a workspace through a
membership, which holds one role:

| | owner | admin | agent | viewer |
|---|---|---|---|---|
| See the workspace, its members, tickets, replies and tags | ✓ | ✓ | ✓ | ✓ |
| See customers and internal notes | ✓ | ✓ | ✓ | |
| Open, change, assign, solve and reopen tickets; reply; note; tag | ✓ | ✓ | ✓ | |
| See and use macros | ✓ | ✓ | ✓ | |
| Add, change and delete customers | ✓ (delete: owner, admin) | ✓ | ✓ (not delete) | |
| Manage tags and macros; move tickets to the bin | ✓ | ✓ | | |
| Rename the workspace; add, change and remove members | ✓ | | | |

The rules that hold everywhere:

- **Workspaces are walled off.** Nothing of one workspace reaches a member
  of another. A record can only refer to records of its own workspace: a
  ticket's customer and assignee, a tag on a ticket.
- **Viewers never see customers** (their names, emails or phone numbers)
  **or internal notes.**
- **A member's email is theirs alone.** Colleagues see their display name
  only; `GET /me` shows a person their own email.
- **Tickets in the bin are gone** from every list, count, export and job,
  and are purged after 30 days (`deleted_ticket_retention`).
- **A ticket is solved through `/solve`**, which records `solved_at`;
  reopening clears it.
- **Only owners, admins and agents can be given tickets.** A viewer can't.
- **A workspace keeps at least one owner.**
- **The first reply** to an open ticket records `first_response_at` and
  sets it pending. A ticket still open and unanswered after its
  `respond_by` (24 hours, `first_response_target`) is flagged with
  `breached_at` by the `first_response_breaches` schedule.

## Routes

Every route needs a signed-in person. Lists come oldest first, except
tickets, which come newest first, a page at a time.

| Route | Does |
|---|---|
| `POST /workspaces`, `GET /workspaces`, `GET`/`PATCH /workspaces/{id}` | Start one (as its owner), list yours, read, rename |
| `GET /me` | Your own account |
| `GET`/`POST /workspaces/{id}/members`, `PATCH`/`DELETE /members/{id}` | Members |
| `GET`/`POST /workspaces/{id}/customers`, `GET`/`PATCH`/`DELETE /customers/{id}`, `GET /customers/{id}/tickets` | Customers |
| `GET`/`POST /workspaces/{id}/tickets`, `GET`/`PATCH`/`DELETE /tickets/{id}`, `POST /tickets/{id}/assign`, `/solve`, `/reopen`, `GET /tickets/{id}/customer` | Tickets |
| `GET`/`POST /tickets/{id}/replies`, `PATCH`/`DELETE /replies/{id}` | Replies |
| `GET`/`POST /tickets/{id}/notes`, `PATCH`/`DELETE /notes/{id}` | Notes |
| `GET`/`POST /workspaces/{id}/tags`, `PATCH`/`DELETE /tags/{id}` | Tags |
| `GET`/`POST /tickets/{id}/tags`, `DELETE /tickets/{id}/tags/{tag_id}` | A ticket's tags |
| `GET`/`POST /workspaces/{id}/macros`, `GET`/`PATCH`/`DELETE /macros/{id}` | Macros |

A record someone may not see answers `404`, as if it weren't there; a list
of another workspace's records is empty. A record someone may see but not
change answers `403 not_permitted`.
