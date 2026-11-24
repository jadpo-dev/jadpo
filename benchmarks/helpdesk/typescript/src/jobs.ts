// Background work. Each schedule runs on a grid of times counted from the
// first tick that sees it: its first run is due one interval after that
// tick; a tick at or past the due time runs it once (missed runs don't pile
// up) and moves the due time to the next grid time after the tick.
import type { Desk } from "./desk.ts";
import type { Logger } from "./errors.ts";

export type Schedule = Readonly<{
  name: string;
  everyMs: number;
  run: (desk: Desk) => Promise<number>;
}>;

export const schedules: readonly Schedule[] = [
  {
    // Flags open tickets past their first-response target.
    name: "first_response_breaches",
    everyMs: 5 * 60000,
    run: (desk) => desk.flagBreaches(),
  },
  {
    // Deletes tickets in the bin longer than DELETED_TICKET_RETENTION.
    name: "Ticket.purge",
    everyMs: 3600000,
    run: (desk) => desk.purgeBinned(),
  },
];

export class Jobs {
  readonly desk: Desk;
  readonly clock: () => number;
  readonly logger: Logger;
  #due = new Map<string, number>();
  #running = new Set<string>();
  constructor(desk: Desk, clock: () => number, logger: Logger) {
    this.desk = desk;
    this.clock = clock;
    this.logger = logger;
  }

  /** Forgets every schedule's grid, as a fresh deployment would. */
  reset(): void {
    this.#due.clear();
  }

  /** Runs the schedules that are due, side by side. */
  async tick(): Promise<Record<string, number>> {
    const now = this.clock();
    const due: Schedule[] = [];
    for (const schedule of schedules) {
      const next = this.#due.get(schedule.name);
      if (next === undefined) {
        this.#due.set(schedule.name, now + schedule.everyMs);
        continue;
      }
      if (now < next) continue;
      const missed = Math.floor((now - next) / schedule.everyMs) + 1;
      this.#due.set(schedule.name, next + missed * schedule.everyMs);
      if (!this.#running.has(schedule.name)) due.push(schedule);
    }
    const ran: Record<string, number> = {};
    try {
      ran.ended_sessions = await this.desk.forgetEndedSessions();
    } catch {
      this.logger({
        event: "job_failed",
        request_id: crypto.randomUUID(),
        code: "unavailable",
      });
    }
    await Promise.all(
      due.map(async (schedule) => {
        this.#running.add(schedule.name);
        try {
          ran[schedule.name] = await schedule.run(this.desk);
        } catch {
          this.logger({
            event: "job_failed",
            request_id: crypto.randomUUID(),
            code: "unavailable",
          });
        } finally {
          this.#running.delete(schedule.name);
        }
      }),
    );
    return ran;
  }
}
