import {createHash} from "node:crypto";
import {readFileSync, writeFileSync} from "node:fs";
import {pathToFileURL} from "node:url";

// Test-owned observation only. Import actual emitted code; no callbacks or authority.
export async function observeHost() {
  const appPath = Bun.env.JADPO_TEMPORAL_HOST_APP ??
    new URL("../../../examples/temporal/build/target/app.ts", import.meta.url).pathname;
  const {temporal, temporalProvenance} = await import(pathToFileURL(appPath).href);
  const t = temporal as any;
  const instant = "2026-09-27T13:30:00.000Z";
  const styles = ["date_full", "date_long", "date_medium", "date_short", "time_full", "time_long", "time_medium", "time_short", "date_time_full", "date_time_long", "date_time_medium", "date_time_short"];
  const locales = ["en-GB", "en-US", "fr-FR"];
  const named = locales.flatMap(locale => styles.map(style => ({
    instant, zone: "Europe/London", locale, style,
    output: t.format(t.in_zone(instant, "Europe/London"), {locale, style}),
  })));
  const otherZone = ["time_short", "date_time_long"].map(style => ({
    instant, zone: "America/New_York", locale: "en-GB", style,
    output: t.format(t.in_zone(instant, "America/New_York"), {locale: "en-GB", style}),
  }));
  const calendars = locales.map(locale => ({date: "2026-09-27", locale,
    output: t.format("2026-09-27", {locale, style: "date_full"}),
  }));
  const structured = locales.flatMap(locale => ["h11", "h12", "h23", "h24"].map(hour_cycle => {
    const components = {year: "numeric", month: "two_digit", day: "two_digit", hour: "two_digit", minute: "two_digit", second: "two_digit", hour_cycle};
    return {instant, zone: "Europe/London", locale, components,
      output: t.format(t.in_zone(instant, "Europe/London"), {locale, components})};
  }));
  const zoneNames = ["Europe/London", "America/New_York"].flatMap(zone => ["long", "short", "long_offset", "short_offset", "long_generic", "short_generic"].map(zone_name => {
    const components = {hour: "two_digit", minute: "two_digit", zone_name};
    try { return {instant, zone, locale: "en-GB", components, rejected: false,
      output: t.format(t.in_zone(instant, zone), {locale: "en-GB", components})}; }
    catch (error) { return {instant, zone, locale: "en-GB", components, rejected: true, errorName: (error as Error).name, errorMessage: (error as Error).message}; }
  }));
  const reference = "2026-09-27T13:30:00.000Z";
  const thresholds = [0, -59999, 59999, -60000, 60000, -60001, 60001, -3599999, 3599999, -3600000, 3600000].map(difference => {
    const value = new Date(Date.parse(reference) + difference).toISOString();
    return {reference, value, difference, zone: "Europe/London", locale: "en-GB",
      output: t.format_friendly(t.in_zone(value, "Europe/London"), {relative_to: reference, locale: "en-GB", profile: "conversational"})};
  });
  // Fixed UTC arithmetic derives concrete fixtures. Expected civil-day categories
  // live in the test, separate from product localParts and friendly branches.
  const dayFixtures = [
    ["same", "2026-09-27T08:00:00.000Z", "2026-09-27T13:30:00.000Z", 0],
    ["previous", "2026-09-26T13:30:00.000Z", reference, -1],
    ["next", "2026-09-28T13:30:00.000Z", reference, 1],
    ["minus2", "2026-09-25T13:30:00.000Z", reference, -2],
    ["plus2", "2026-09-29T13:30:00.000Z", reference, 2],
    ["minus6", "2026-09-21T13:30:00.000Z", reference, -6],
    ["plus6", "2026-10-03T13:30:00.000Z", reference, 6],
    ["minus7", "2026-09-20T13:30:00.000Z", reference, -7],
    ["plus7", "2026-10-04T13:30:00.000Z", reference, 7],
    ["midnight-next", "2026-09-27T23:30:00.000Z", "2026-09-27T21:30:00.000Z", 1],
    ["spring-civil-next", "2026-03-29T22:30:00.000Z", "2026-03-28T23:30:00.000Z", 1],
    ["autumn-civil-same", "2026-10-25T23:30:00.000Z", "2026-10-25T00:00:00.000Z", 0],
  ] as const;
  const days = locales.flatMap(locale => dayFixtures.map(([label, value, reference, expectedLocalDays]) => ({
    label, value, reference, expectedLocalDays, zone: "Europe/London", locale,
    output: t.format_friendly(t.in_zone(value, "Europe/London"), {relative_to: reference, locale, profile: "conversational"}),
  })));
  const explicitReferenceValue = t.in_zone("2026-09-27T13:31:01.000Z", "Europe/London");
  const references = [reference, "2026-09-27T13:32:02.000Z", reference].map(relative_to => ({relative_to,
    output: t.format_friendly(explicitReferenceValue, {relative_to, locale: "en-GB", profile: "conversational"})}));
  const policyFixtures = [
    {date: "2026-11-01", at: "01:30", overlap: "reject", gap: "reject"},
    {date: "2026-11-01", at: "01:30", overlap: "earlier", gap: "reject"},
    {date: "2026-11-01", at: "01:30", overlap: "later", gap: "reject"},
    {date: "2026-03-08", at: "02:30", overlap: "reject", gap: "reject"},
    {date: "2026-03-08", at: "02:30", overlap: "reject", gap: "shift_backward"},
    {date: "2026-03-08", at: "02:30", overlap: "reject", gap: "shift_forward"},
    {date: "2026-03-08", at: "02:15:12.345", overlap: "reject", gap: "shift_backward"},
    {date: "2026-03-08", at: "02:15:12.345", overlap: "reject", gap: "shift_forward"},
  ];
  const policies = policyFixtures.map(input => {
    try {return {input, output: t.resolve({...input, zone: "America/New_York"}), rejected: false};}
    catch (error) {return {input, rejected: true, errorName: (error as Error).name};}
  });
  const londonPositions = ["shift_backward", "shift_forward"].map(gap => ({gap,
    output: t.resolve({date: "2026-03-29", at: "01:15:12.345", zone: "Europe/London", overlap: "reject", gap})}));
  const invalidOptions = [
    {locale: "en-GB", style: "date_full", components: {day: "numeric"}},
    {locale: "en-GB"}, {locale: "en-GB", components: {}},
    {locale: "en-GB", components: {day: "ordinal"}},
    {locale: "en-GB", components: {unknown: "numeric"}},
  ].map(input => {
    try {return {input, output: t.format(t.in_zone(instant, "Europe/London"), input), rejected: false};}
    catch (error) {return {input, rejected: true, errorName: (error as Error).name};}
  });
  const frenchSubminute = {value: "2026-09-27T13:30:30.000Z", reference, locale: "fr-FR",
    output: t.format_friendly(t.in_zone("2026-09-27T13:30:30.000Z", "Europe/London"), {relative_to: reference, locale: "fr-FR", profile: "conversational"})};
  const defaultOptions = new Intl.DateTimeFormat().resolvedOptions();
  return {schemaVersion: 1, scope: "generated_direct_HOST_only", appPath,
    appSHA256: createHash("sha256").update(readFileSync(appPath)).digest("hex"),
    executable: process.execPath, executableSHA256: createHash("sha256").update(readFileSync(process.execPath)).digest("hex"),
    actualVersions: process.versions, provenance: temporalProvenance,
    environment: {TZ: Bun.env.TZ ?? null, LANG: Bun.env.LANG ?? null, LC_ALL: Bun.env.LC_ALL ?? null,
      effectiveTimeZone: defaultOptions.timeZone, effectiveLocale: defaultOptions.locale},
    wallObserved: new Date().toISOString(), named, otherZone, calendars, structured, zoneNames,
    civil: ["Europe/London", "America/New_York"].map(zone=>{const value=t.in_zone(instant,zone);return {zone,year:t.year(value),month:t.month(value),day:t.day(value),hour:t.hour(value),minute:t.minute(value),second:t.second(value),calendarDate:t.calendar_date(value)};}),
    frenchBoundaries: [1, 30000, 59999, 0, -1, -60000, 60000, 3599999, 3600000].map(difference=>({difference,reference,value:new Date(Date.parse(reference)+difference).toISOString(),output:t.format_friendly(t.in_zone(new Date(Date.parse(reference)+difference).toISOString(),"Europe/London"),{relative_to:reference,locale:"fr-FR",profile:"conversational"})})),
    thresholds, days, references, policies, londonPositions, invalidOptions, frenchSubminute};
}
if (import.meta.main) {
  const observation = await observeHost();
  if (Bun.env.JADPO_TEMPORAL_HOST_RECEIPT) writeFileSync(Bun.env.JADPO_TEMPORAL_HOST_RECEIPT, JSON.stringify(observation, null, 2)+"\n");
  console.log(JSON.stringify(observation));
}
