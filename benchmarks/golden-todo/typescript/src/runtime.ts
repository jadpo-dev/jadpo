import { AppError } from "./errors.ts";
export class Admission {
  #active = 0;
  readonly limit: number;
  constructor(limit = 128) {
    this.limit = limit;
  }
  get active() {
    return this.#active;
  }
  enter(): () => void {
    if (this.#active >= this.limit) throw new AppError("unavailable");
    this.#active++;
    let released = false;
    return () => {
      if (!released) {
        released = true;
        this.#active--;
      }
    };
  }
}
export class RequestScope {
  readonly signal: AbortSignal;
  readonly deadline: number;
  #controller = new AbortController();
  #timer: ReturnType<typeof setTimeout>;
  #parent: AbortSignal;
  #cancel: () => void;
  #expired = false;
  constructor(parent: AbortSignal, timeoutMs = 10000) {
    this.#parent = parent;
    this.deadline = performance.now() + timeoutMs;
    this.signal = this.#controller.signal;
    this.#cancel = () => {
      this.#controller.abort();
    };
    parent.addEventListener("abort", this.#cancel, { once: true });
    if (parent.aborted) this.#cancel();
    this.#timer = setTimeout(() => {
      this.#expired = true;
      this.#controller.abort();
    }, timeoutMs);
    this.#timer.unref();
  }
  check(): void {
    if (this.#expired || performance.now() >= this.deadline)
      throw new AppError("deadline_exceeded");
    if (this.signal.aborted) throw new AppError("unavailable");
  }
  remaining(): number {
    this.check();
    return Math.max(1, Math.floor(this.deadline - performance.now()));
  }
  close(): void {
    clearTimeout(this.#timer);
    this.#parent.removeEventListener("abort", this.#cancel);
  }
}
export type Clock = () => number;
