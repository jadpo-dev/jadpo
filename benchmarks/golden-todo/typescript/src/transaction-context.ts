import { AsyncLocalStorage } from "node:async_hooks";
import type { Database } from "./db/store.ts";
import type { RequestScope } from "./runtime.ts";
import { AppError } from "./errors.ts";
export type TransactionFrame = {
  owner: object;
  database: Database;
  scope: RequestScope;
};
const transactions = new AsyncLocalStorage<TransactionFrame>();
export const currentTransaction = () => transactions.getStore();
export async function inTransaction<T>(
  frame: TransactionFrame,
  body: () => Promise<T>,
): Promise<T> {
  return await transactions.run(frame, body);
}
export function assertOutsideTransaction(): void {
  if (currentTransaction()) throw new AppError("internal_fault");
}
