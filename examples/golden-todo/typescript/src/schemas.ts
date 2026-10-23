import { z } from "@hono/zod-openapi";

// Jadpo Text bounds count Unicode scalars, and never admit unpaired surrogates.
export const Title = z
  .string()
  .transform((value) => value.normalize("NFC"))
  .refine(
    (value) =>
      value.isWellFormed() &&
      [...value].length >= 1 &&
      [...value].length <= 200,
  )
  .openapi({ type: "string", minLength: 1, maxLength: 200 });
export const Id = z
  .uuid()
  .transform((value) => value.toLowerCase())
  .openapi({ type: "string", format: "uuid" });
export const IdempotencyKey = z
  .string()
  .regex(/^[\x20-\x7e]{1,255}$/)
  .openapi({ minLength: 1, maxLength: 255 });
export const Instant = z.iso.datetime({ offset: true });
export const Status = z.enum(["open", "done"]);
export const CreateTodo = z
  .strictObject({ title: Title, due_at: Instant.nullable().default(null) })
  .openapi("CreateTodo");
export const PatchTodo = z
  .strictObject({
    title: Title.optional(),
    status: Status.optional(),
    due_at: Instant.nullable().optional(),
  })
  .openapi("PatchTodo");
export const ListTodos = z
  .strictObject({
    status: Status.optional(),
    due_before: Instant.optional(),
    page_size: z
      .string()
      .regex(/^[0-9]+$/)
      .transform(Number)
      .pipe(z.number().int().min(1).max(100))
      .default(25),
    after: z.string().min(1).max(2048).optional(),
  })
  .openapi("ListTodos");
export const TodoView = z
  .strictObject({
    id: Id,
    title: Title,
    status: Status,
    due_at: Instant.nullable(),
    created_at: Instant,
    updated_at: Instant,
  })
  .openapi("TodoView");
export const TodoPage = z
  .strictObject({
    items: z.array(TodoView).max(100),
    next: z.string().nullable(),
  })
  .openapi("TodoPage");
export const UserWithTodos = z
  .strictObject({ id: Id, todos: z.array(TodoView).max(100) })
  .openapi("UserWithTodos");
export const ExchangedToken = z
  .strictObject({ token: z.string(), expires_at: Instant })
  .openapi("ExchangedToken");
export const Failure = z
  .strictObject({
    error: z.strictObject({
      code: z.string(),
      message: z.string(),
      request_id: z.string(),
    }),
  })
  .openapi("Failure");
export const Cursor = z.strictObject({ created_at: Instant, id: Id });
export const MailPayload = z.strictObject({
  to: z.email(),
  title: Title,
  todo_id: Id,
  revision: z.number().int().positive(),
});
export const MailReceipt = z.strictObject({ accepted_at: Instant });
export const QueuedEvent = z.strictObject({
  todo_id: Id,
  revision: z.number().int().positive(),
});
export type PublicTodo = z.infer<typeof TodoView>;
export type TodoChanges = z.infer<typeof PatchTodo>;
export type Principal =
  | Readonly<{ kind: "user"; id: string; subject: string }>
  | Readonly<{ kind: "service"; id: string; subject: string }>;
