import { setTimeout as pause } from "node:timers/promises";
import type { z } from "zod";
import type { Egress } from "./egress.ts";
import type { RequestScope } from "./runtime.ts";
import { uniqueJson } from "./strict-json.ts";
import { MailPayload, MailReceipt } from "./schemas.ts";
export const reminderMail = Object.freeze({
  url: "https://mail.example.invalid/v1/reminders",
  attempts: 3,
  withinMs: 30000,
  windowMs: 86400000,
  safeWindowMs: 82800000,
});
export class ProviderFailure extends Error {
  readonly permanent: boolean;
  constructor(permanent = false) {
    super(permanent ? "Provider refused." : "Provider unavailable.");
    this.permanent = permanent;
  }
}
type Wait = (milliseconds: number, scope: RequestScope) => Promise<void>;
const wait: Wait = async (milliseconds, scope) => {
  await pause(milliseconds, undefined, { signal: scope.signal });
};
export class MailProvider {
  readonly egress: Egress;
  readonly secret: string;
  readonly random: () => number;
  readonly wait: Wait;
  constructor(
    egress: Egress,
    secret: string,
    random: () => number = Math.random,
    delay: Wait = wait,
  ) {
    this.egress = egress;
    this.secret = secret;
    this.random = random;
    this.wait = delay;
  }
  async send(
    input: z.infer<typeof MailPayload>,
    key: string,
    scope: RequestScope,
    guard: () => Promise<void>,
  ) {
    const payload = MailPayload.parse(input);
    // Plain Int has an unbounded declared range in SendReminder, so Jadpo's
    // provider codec writes it as a decimal JSON string, even for small values.
    const body = JSON.stringify(payload);
    const started = performance.now();
    for (let attempt = 0; attempt < reminderMail.attempts; attempt++) {
      scope.check();
      await guard();
      scope.check();
      let retryAfter: number | undefined;
      try {
        const answer = await this.egress.send(
          {
            url: reminderMail.url,
            method: "POST",
            headers: {
              "content-type": "application/json",
              authorization: "Bearer " + this.secret,
              "idempotency-key": key,
            },
            body,
          },
          scope,
        );
        if (answer.status >= 200 && answer.status < 300) {
          try {
            return MailReceipt.parse(uniqueJson(answer.body, 524288));
          } catch {
            throw new ProviderFailure();
          }
        }
        if (![408, 409, 429].includes(answer.status) && answer.status < 500)
          throw new ProviderFailure(true);
        const seconds = answer.headers?.["retry-after"];
        if (seconds && /^\d+$/.test(seconds))
          retryAfter = Number(seconds) * 1000;
        throw new ProviderFailure();
      } catch (error) {
        scope.check();
        if (error instanceof ProviderFailure && error.permanent) throw error;
        if (attempt + 1 === reminderMail.attempts) throw new ProviderFailure();
        const milliseconds =
          retryAfter ?? Math.floor(this.random() * 200 * 2 ** attempt);
        if (
          milliseconds >= scope.remaining() ||
          performance.now() - started + milliseconds >= reminderMail.withinMs
        )
          throw new ProviderFailure();
        await this.wait(milliseconds, scope);
      }
    }
    throw new ProviderFailure();
  }
}
