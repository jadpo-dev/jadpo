import { describe, expect, test } from "bun:test";
import {createHash} from "node:crypto";
import {readFileSync} from "node:fs";

import {
  temporal,
  temporalProvenance,
} from "../../examples/temporal/build/target/app.ts";

const runtime = temporal as any;

describe("compiler-owned temporal runtime", () => {
  test("canonicalises explicit offsets and rejects non-canonical instants", () => {
    expect(
      runtime.in_zone("2026-01-15T13:00:00+01:00", "Europe/London"),
    ).toEqual({ instant: "2026-01-15T12:00:00.000Z", zone: "Europe/London" });

    for (const invalid of [
      "2026-01-15T12:00:00",
      "2026-01-15T12:00:00.0001Z",
      "2026-02-30T12:00:00Z",
      "2026-01-15T12:00:00-00:00",
    ]) {
      expect(() => runtime.in_zone(invalid, "Europe/London")).toThrow();
    }
    expect(() => runtime.in_zone("2026-01-15T12:00:00Z", "GMT")).toThrow();
    expect(
      runtime.add_elapsed("2026-01-15T12:00:00Z", "-PT1H"),
    ).toBe("2026-01-15T11:00:00.000Z");
  });

  test("resolves London overlap and gap policies explicitly", () => {
    const earlier = runtime.resolve({
      date: "2026-10-25",
      at: "01:30",
      zone: "Europe/London",
      overlap: "earlier",
      gap: "reject",
    });
    const later = runtime.resolve({
      date: "2026-10-25",
      at: "01:30",
      zone: "Europe/London",
      overlap: "later",
      gap: "reject",
    });
    expect(earlier.instant).toBe("2026-10-25T00:30:00.000Z");
    expect(later.instant).toBe("2026-10-25T01:30:00.000Z");
    expect(runtime.between(earlier, later)).toBe("PT1H");
    expect(() =>
      runtime.resolve({
        date: "2026-10-25",
        at: "01:30",
        zone: "Europe/London",
        overlap: "reject",
        gap: "reject",
      }),
    ).toThrow();

    const backward = runtime.resolve({
      date: "2026-03-29",
      at: "01:30",
      zone: "Europe/London",
      overlap: "reject",
      gap: "shift_backward",
    });
    const forward = runtime.resolve({
      date: "2026-03-29",
      at: "01:30",
      zone: "Europe/London",
      overlap: "reject",
      gap: "shift_forward",
    });
    expect(backward.instant).toBe("2026-03-29T00:30:00.000Z");
    expect(forward.instant).toBe("2026-03-29T01:30:00.000Z");
    expect(() =>
      runtime.resolve({
        date: "2026-03-29",
        at: "01:30",
        zone: "Europe/London",
        overlap: "reject",
        gap: "reject",
      }),
    ).toThrow();
  });

  test("resolves non-European daylight-saving transitions", () => {
    const earlier = runtime.resolve({
      date: "2026-11-01",
      at: "01:30",
      zone: "America/New_York",
      overlap: "earlier",
      gap: "reject",
    });
    const later = runtime.resolve({
      date: "2026-11-01",
      at: "01:30",
      zone: "America/New_York",
      overlap: "later",
      gap: "reject",
    });
    expect(earlier.instant).toBe("2026-11-01T05:30:00.000Z");
    expect(later.instant).toBe("2026-11-01T06:30:00.000Z");

    const forward = runtime.resolve({
      date: "2026-03-08",
      at: "02:30",
      zone: "America/New_York",
      overlap: "reject",
      gap: "shift_forward",
    });
    expect(forward.instant).toBe("2026-03-08T07:30:00.000Z");
  });

  test("local-day bounds retain 23, 24 and 25 hour days", () => {
    for (const [date, duration] of [
      ["2026-03-29", "PT23H"],
      ["2026-06-01", "PT24H"],
      ["2026-10-25", "PT25H"],
    ]) {
      const bounds = runtime.day_bounds(date, "Europe/London");
      expect(runtime.between(bounds.start, bounds.end)).toBe(duration);
    }
  });

  test("calendar and elapsed arithmetic remain distinct", () => {
    expect(runtime.add_days("0001-01-01", { days: 1 })).toBe("0001-01-02");
    expect(
      runtime.add_months("2025-01-31", {
        months: 1,
        invalid_day: "last_valid_day",
      }),
    ).toBe("2025-02-28");
    expect(() =>
      runtime.add_months("2025-01-31", { months: 1, invalid_day: "reject" }),
    ).toThrow();

    const start = runtime.in_zone("2026-03-28T12:00:00Z", "Europe/London");
    const localDay = runtime.add_local_days(start, {
      days: 1,
      overlap: "reject",
      gap: "reject",
    });
    const elapsedDay = runtime.add_elapsed(start, "PT24H");
    expect(runtime.between(start, localDay)).toBe("PT23H");
    expect(runtime.between(start, elapsedDay)).toBe("PT24H");
  });

  test("friendly formatting uses its explicit reference instant", () => {
    const relative = "2026-09-27T13:30:00Z";
    const past = runtime.in_zone("2026-09-27T13:28:59Z", "Europe/London");
    const future = runtime.in_zone("2026-09-27T13:31:01Z", "Europe/London");
    expect(
      runtime.format_friendly(past, {
        relative_to: relative,
        locale: "en-GB",
        profile: "conversational",
      }),
    ).toBe("1 minute ago");
    expect(
      runtime.format_friendly(future, {
        relative_to: relative,
        locale: "en-GB",
        profile: "conversational",
      }),
    ).toBe("in 2 minutes");
    expect(() =>
      runtime.format_friendly(future, {
        relative_to: relative,
        locale: "de-DE",
        profile: "conversational",
      }),
    ).toThrow();
  });

  test("rejects invalid ambiguity and formatting policy values at runtime", () => {
    expect(() =>
      runtime.resolve({
        date: "2026-10-25",
        at: "01:30",
        zone: "Europe/London",
        overlap: "silently_choose",
        gap: "reject",
      }),
    ).toThrow();
    const time = runtime.in_zone("2026-09-27T13:30:00Z", "Europe/London");
    expect(() =>
      runtime.format(time, { locale: "en-GB", style: "custom_pattern" }),
    ).toThrow();
    expect(() =>
      runtime.week_bounds("2026-09-27", "Europe/London", {
        starts_on: "weekend",
      }),
    ).toThrow();
    expect(() =>
      runtime.format(time, {
        locale: "en-GB",
        components: { day: "ordinal" },
      }),
    ).toThrow();
    expect(() =>
      runtime.format(time, { locale: "en-GB", components: {} }),
    ).toThrow();
  });

  test("calendar extraction is independent of method binding", () => {
    const value = runtime.in_zone(
      "2026-09-27T23:30:00Z",
      "Europe/London",
    );
    const weekday = runtime.weekday;
    expect(weekday(value)).toBe("monday");
    expect(runtime.calendar_date(value)).toBe("2026-09-28");
  });

  test("reports the generated temporal data provenance", () => {
    expect(temporalProvenance.tzdb).toBe("IANA 2026c");
    expect(temporalProvenance.tzdbScope).toBe("compiler_zone_enumeration_only");
    expect(temporalProvenance.zoneEnumVersion).toBe(temporalProvenance.tzdb);
    expect(temporalProvenance.zoneEnumSource).toBe("jadpo/data/iana-zones-2026c.txt");
    const zoneBytes = readFileSync(
      new URL("../../jadpo/data/iana-zones-2026c.txt", import.meta.url),
    );
    expect(temporalProvenance.zoneEnumByteDigest).toBe("sha256:" + createHash("sha256").update(zoneBytes).digest("hex"));
    expect(temporalProvenance.zoneEnumCount).toBe(new Set(zoneBytes.toString().trim().split("\n")).size);
    expect(temporalProvenance.runtimeRuleProvider).toBe("ECMAScript Intl.DateTimeFormat");
    expect(temporalProvenance.runtimeRuleDataVersionSource).toBe("process.versions.tz");
    const reported =
      typeof process.versions.tz === "string" && process.versions.tz.trim()
        ? process.versions.tz
        : "unreported";
    expect(temporalProvenance.runtimeRuleDataVersion).toBe(reported);
    expect(temporalProvenance.runtimeRuleDataByteDigest).toBeNull();
    expect(Object.isFrozen(temporalProvenance)).toBe(true);
    expect(temporalProvenance.defaultLocale).toBe("en-GB");
    expect(temporalProvenance.runtimeIcu).not.toBe("unreported");
    expect(temporalProvenance.runtimeEngine).toMatch(/^Bun /);
  });
});
