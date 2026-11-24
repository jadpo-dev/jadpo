// Every route: its method and path, what it reads, its success status, and
// the action it runs. src/app.ts serves this table; src/inventory.ts lists it.
import type { z } from "zod";
import type { Principal } from "./auth.ts";
import type { Desk } from "./desk.ts";
import {
  AddTicketTag,
  Assignment,
  CreateCustomer,
  CreateTicket,
  ListTickets,
  MemberChange,
  NameTag,
  NewMacro,
  NewMember,
  NewWorkspace,
  PatchCustomer,
  PatchMacro,
  PatchTicket,
  WorkspaceRename,
  WriteMessage,
} from "./schemas.ts";

type Params = Readonly<Record<string, string>>;
export type Route = Readonly<{
  method: "GET" | "POST" | "PATCH" | "DELETE";
  /** With `{name}` placeholders, each a record's id. */
  path: string;
  /** A JSON body, or (for a list) the query string, read by this schema. */
  body?: z.ZodType;
  query?: z.ZodType;
  status: 200 | 201 | 204;
  run: (
    desk: Desk,
    user: Principal,
    params: Params,
    input: never,
  ) => Promise<unknown>;
}>;

function route<S extends z.ZodType | undefined>(
  method: Route["method"],
  path: string,
  options: { body?: S; query?: S; status?: Route["status"] },
  run: (
    desk: Desk,
    user: Principal,
    params: Params,
    input: S extends z.ZodType ? z.output<S> : undefined,
  ) => Promise<unknown>,
): Route {
  return {
    method,
    path,
    ...(options.body ? { body: options.body } : {}),
    ...(options.query ? { query: options.query } : {}),
    status: options.status ?? (method === "DELETE" ? 204 : 200),
    run,
  };
}
const p = (params: Params, name: string) => params[name] ?? "";

export const routes: readonly Route[] = [
  // Workspaces, and what belongs to each.
  route(
    "POST",
    "/workspaces",
    { body: NewWorkspace, status: 201 },
    (d, u, _, i) => d.foundWorkspace(u, i),
  ),
  route("GET", "/workspaces", {}, (d, u) => d.myWorkspaces(u)),
  route("GET", "/workspaces/{workspace_id}", {}, (d, u, a) =>
    d.workspace(u, p(a, "workspace_id")),
  ),
  route(
    "PATCH",
    "/workspaces/{workspace_id}",
    { body: WorkspaceRename },
    (d, u, a, i) => d.renameWorkspace(u, p(a, "workspace_id"), i),
  ),
  route("GET", "/workspaces/{workspace_id}/members", {}, (d, u, a) =>
    d.members(u, p(a, "workspace_id")),
  ),
  route(
    "POST",
    "/workspaces/{workspace_id}/members",
    { body: NewMember, status: 201 },
    (d, u, a, i) => d.addMember(u, p(a, "workspace_id"), i),
  ),
  route("GET", "/workspaces/{workspace_id}/customers", {}, (d, u, a) =>
    d.customers(u, p(a, "workspace_id")),
  ),
  route(
    "POST",
    "/workspaces/{workspace_id}/customers",
    { body: CreateCustomer, status: 201 },
    (d, u, a, i) => d.addCustomer(u, p(a, "workspace_id"), i),
  ),
  route(
    "GET",
    "/workspaces/{workspace_id}/tickets",
    { query: ListTickets },
    (d, u, a, i) => d.ticketsPage(u, p(a, "workspace_id"), i),
  ),
  route(
    "POST",
    "/workspaces/{workspace_id}/tickets",
    { body: CreateTicket, status: 201 },
    (d, u, a, i) => d.openTicket(u, p(a, "workspace_id"), i),
  ),
  route("GET", "/workspaces/{workspace_id}/tags", {}, (d, u, a) =>
    d.tags(u, p(a, "workspace_id")),
  ),
  route(
    "POST",
    "/workspaces/{workspace_id}/tags",
    { body: NameTag, status: 201 },
    (d, u, a, i) => d.createTag(u, p(a, "workspace_id"), i),
  ),
  route("GET", "/workspaces/{workspace_id}/macros", {}, (d, u, a) =>
    d.macros(u, p(a, "workspace_id")),
  ),
  route(
    "POST",
    "/workspaces/{workspace_id}/macros",
    { body: NewMacro, status: 201 },
    (d, u, a, i) => d.createMacro(u, p(a, "workspace_id"), i),
  ),
  // The signed-in person's own account.
  route("GET", "/me", {}, (d, u) => d.me(u)),
  // Members.
  route("PATCH", "/members/{member_id}", { body: MemberChange }, (d, u, a, i) =>
    d.changeMember(u, p(a, "member_id"), i),
  ),
  route("DELETE", "/members/{member_id}", {}, (d, u, a) =>
    d.removeMember(u, p(a, "member_id")),
  ),
  // Customers.
  route("GET", "/customers/{customer_id}", {}, (d, u, a) =>
    d.customer(u, p(a, "customer_id")),
  ),
  route(
    "PATCH",
    "/customers/{customer_id}",
    { body: PatchCustomer },
    (d, u, a, i) => d.editCustomer(u, p(a, "customer_id"), i),
  ),
  route("DELETE", "/customers/{customer_id}", {}, (d, u, a) =>
    d.deleteCustomer(u, p(a, "customer_id")),
  ),
  route("GET", "/customers/{customer_id}/tickets", {}, (d, u, a) =>
    d.customerTickets(u, p(a, "customer_id")),
  ),
  // Tickets.
  route("GET", "/tickets/{ticket_id}", {}, (d, u, a) =>
    d.ticket(u, p(a, "ticket_id")),
  ),
  route("PATCH", "/tickets/{ticket_id}", { body: PatchTicket }, (d, u, a, i) =>
    d.editTicket(u, p(a, "ticket_id"), i),
  ),
  route("DELETE", "/tickets/{ticket_id}", {}, (d, u, a) =>
    d.deleteTicket(u, p(a, "ticket_id")),
  ),
  route(
    "POST",
    "/tickets/{ticket_id}/assign",
    { body: Assignment },
    (d, u, a, i) => d.assignTicket(u, p(a, "ticket_id"), i),
  ),
  route("POST", "/tickets/{ticket_id}/solve", {}, (d, u, a) =>
    d.solveTicket(u, p(a, "ticket_id")),
  ),
  route("POST", "/tickets/{ticket_id}/reopen", {}, (d, u, a) =>
    d.reopenTicket(u, p(a, "ticket_id")),
  ),
  route("GET", "/tickets/{ticket_id}/customer", {}, (d, u, a) =>
    d.ticketCustomer(u, p(a, "ticket_id")),
  ),
  route("GET", "/tickets/{ticket_id}/replies", {}, (d, u, a) =>
    d.ticketReplies(u, p(a, "ticket_id")),
  ),
  route(
    "POST",
    "/tickets/{ticket_id}/replies",
    { body: WriteMessage, status: 201 },
    (d, u, a, i) => d.addReply(u, p(a, "ticket_id"), i),
  ),
  route("GET", "/tickets/{ticket_id}/notes", {}, (d, u, a) =>
    d.ticketNotes(u, p(a, "ticket_id")),
  ),
  route(
    "POST",
    "/tickets/{ticket_id}/notes",
    { body: WriteMessage, status: 201 },
    (d, u, a, i) => d.addNote(u, p(a, "ticket_id"), i),
  ),
  route("GET", "/tickets/{ticket_id}/tags", {}, (d, u, a) =>
    d.ticketTags(u, p(a, "ticket_id")),
  ),
  route(
    "POST",
    "/tickets/{ticket_id}/tags",
    { body: AddTicketTag, status: 201 },
    (d, u, a, i) => d.tagTicket(u, p(a, "ticket_id"), i),
  ),
  route("DELETE", "/tickets/{ticket_id}/tags/{tag_id}", {}, (d, u, a) =>
    d.untagTicket(u, p(a, "ticket_id"), p(a, "tag_id")),
  ),
  // Replies and notes, one at a time.
  route("PATCH", "/replies/{reply_id}", { body: WriteMessage }, (d, u, a, i) =>
    d.editReply(u, p(a, "reply_id"), i),
  ),
  route("DELETE", "/replies/{reply_id}", {}, (d, u, a) =>
    d.deleteReply(u, p(a, "reply_id")),
  ),
  route("PATCH", "/notes/{note_id}", { body: WriteMessage }, (d, u, a, i) =>
    d.editNote(u, p(a, "note_id"), i),
  ),
  route("DELETE", "/notes/{note_id}", {}, (d, u, a) =>
    d.deleteNote(u, p(a, "note_id")),
  ),
  // Tags, one at a time.
  route("PATCH", "/tags/{tag_id}", { body: NameTag }, (d, u, a, i) =>
    d.renameTag(u, p(a, "tag_id"), i),
  ),
  route("DELETE", "/tags/{tag_id}", {}, (d, u, a) =>
    d.deleteTag(u, p(a, "tag_id")),
  ),
  // Macros, one at a time.
  route("GET", "/macros/{macro_id}", {}, (d, u, a) =>
    d.macro(u, p(a, "macro_id")),
  ),
  route("PATCH", "/macros/{macro_id}", { body: PatchMacro }, (d, u, a, i) =>
    d.editMacro(u, p(a, "macro_id"), i),
  ),
  route("DELETE", "/macros/{macro_id}", {}, (d, u, a) =>
    d.deleteMacro(u, p(a, "macro_id")),
  ),
];
