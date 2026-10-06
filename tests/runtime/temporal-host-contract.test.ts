import {beforeAll, describe, expect, test} from "bun:test";
import {writeFileSync} from "node:fs";
import {observeHost} from "./fixtures/temporal-host-observer";
let observation: Awaited<ReturnType<typeof observeHost>>;
beforeAll(async () => {
  observation = await observeHost();
  if (Bun.env.JADPO_TEMPORAL_HOST_RECEIPT) writeFileSync(Bun.env.JADPO_TEMPORAL_HOST_RECEIPT, JSON.stringify(observation,null,2)+"\n");
});
// Independent test mappings from accepted closed formats. These expected calls
// qualify option wiring/provider wording, never independent Intl rule bytes.
function styleOptions(style: string): Intl.DateTimeFormatOptions {
  const widths = ["full", "long", "medium", "short"] as const;
  for (const width of widths) {
    if (style === `date_${width}`) return {dateStyle: width};
    if (style === `time_${width}`) return {timeStyle: width};
    if (style === `date_time_${width}`) return {dateStyle: width, timeStyle: width};
  }
  throw new Error("Unrecognized fixture style");
}
function parts(instant: string, zone: string) {
  return Object.fromEntries(new Intl.DateTimeFormat("en-GB", {timeZone:zone,year:"numeric",month:"2-digit",day:"2-digit",hour:"2-digit",minute:"2-digit",second:"2-digit",hourCycle:"h23"}).formatToParts(new Date(instant)).map(p=>[p.type,p.value]));
}
function componentExpected(row: {locale: string; instant: string; zone: string; components: Record<string,string>}) {
  const options: Intl.DateTimeFormatOptions = {timeZone: row.zone};
  for (const [key,value] of Object.entries(row.components)) {
    const intlKey = key === "hour_cycle" ? "hourCycle" : key === "zone_name" ? "timeZoneName" : key;
    (options as any)[intlKey] = value === "two_digit" ? "2-digit" : ({long_offset:"longOffset",short_offset:"shortOffset",long_generic:"longGeneric",short_generic:"shortGeneric"} as Record<string,string>)[value] ?? value;
  }
  return new Intl.DateTimeFormat(row.locale, options).format(new Date(row.instant));
}
function dayProviderExpected(row: typeof observation.days[number]) {
  const formatter = new Intl.DateTimeFormat(row.locale,{timeZone:row.zone,weekday:"long",hour:"numeric",minute:"2-digit"});
  if (Math.abs(row.expectedLocalDays)<=1) {
    const word = new Intl.RelativeTimeFormat(row.locale,{numeric:"auto"}).format(row.expectedLocalDays,"day");
    return formatter.formatToParts(new Date(row.value)).map(p=>p.type==="weekday"?word:p.value).join("");
  }
  if (Math.abs(row.expectedLocalDays)<=6) return formatter.format(new Date(row.value));
  // Long-style wiring observation; short-zone semantic requirement is tested
  // independently in the retained accepted-contract diagnostic suite.
  return new Intl.DateTimeFormat(row.locale,{timeZone:row.zone,dateStyle:"long",timeStyle:"long"}).format(new Date(row.value));
}
describe("generated temporal direct HOST finite contract", () => {
 test("all36 named formats wire explicit declared locale and zone",()=>{
  expect(observation.named.length).toBe(36);
  for (const row of [...observation.named,...observation.otherZone]) {
   expect(row.output).toBe(new Intl.DateTimeFormat(row.locale,{timeZone:row.zone,...styleOptions(row.style)}).format(new Date(row.instant)));
  }
  expect(parts(observation.named[0].instant,"Europe/London")).toMatchObject({year:"2026",month:"09",day:"27",hour:"14",minute:"30"});
  expect(parts(observation.named[0].instant,"America/New_York")).toMatchObject({year:"2026",month:"09",day:"27",hour:"09",minute:"30"});
  expect(observation.otherZone[0].output).toContain("09:30");
  expect(observation.civil).toEqual([{zone:"Europe/London",year:2026,month:9,day:27,hour:14,minute:30,second:0,calendarDate:"2026-09-27"},{zone:"America/New_York",year:2026,month:9,day:27,hour:9,minute:30,second:0,calendarDate:"2026-09-27"}]);
 });
 test("CalendarDate and closed component axes use explicit options",()=>{
  for(const row of observation.calendars) expect(row.output).toBe(new Intl.DateTimeFormat(row.locale,{timeZone:"UTC",dateStyle:"full"}).format(new Date(`${row.date}T00:00:00.000Z`)));
  expect(observation.structured.length).toBe(12);expect(observation.zoneNames.length).toBe(12);
  for(const row of observation.structured) expect(row.output).toBe(componentExpected(row));
  for(const row of observation.zoneNames.filter(row=>["short","long"].includes(row.components.zone_name))) { expect(row.rejected).toBe(false);expect(row.output).toBe(componentExpected(row)); }
  for(const row of observation.invalidOptions) expect(row.rejected).toBe(true);
 });
 test("friendly whole-minute rounding and exact elapsed thresholds",()=>{
  for(const row of observation.thresholds) {
   expect(Date.parse(row.value)-Date.parse(row.reference)).toBe(row.difference);
   if(Math.abs(row.difference)<60000) expect(row.output).toBe(row.difference<=0?"just now":"in under a minute");
   else if(Math.abs(row.difference)<3600000) {
    const magnitude=row.difference<0?Math.floor(-row.difference/60000):Math.ceil(row.difference/60000);
    expect(row.output).toBe(row.difference<0?`${magnitude} ${magnitude===1?"minute":"minutes"} ago`:`in ${magnitude} ${magnitude===1?"minute":"minutes"}`);
   } else expect(row.output).toBe(new Intl.DateTimeFormat("en-GB",{timeZone:row.zone,weekday:"long",hour:"numeric",minute:"2-digit"}).formatToParts(new Date(row.value)).map(p=>p.type==="weekday"?"today":p.value).join(""));
  }
 });
 test("friendly civil days and DST boundaries use the supplied reference",()=>{
  for(const row of observation.days) {
   const v=parts(row.value,row.zone),r=parts(row.reference,row.zone);
   const delta=(Date.UTC(+v.year,+v.month-1,+v.day)-Date.UTC(+r.year,+r.month-1,+r.day))/86400000;
   expect(delta).toBe(row.expectedLocalDays);
   expect(row.output).toBe(dayProviderExpected(row));
  }
  const spring=observation.days.find(r=>r.label==="spring-civil-next" && r.locale==="en-GB")!;
  const autumn=observation.days.find(r=>r.label==="autumn-civil-same" && r.locale==="en-GB")!;
  expect(Date.parse(spring.value)-Date.parse(spring.reference)).toBe(23*3600000);expect(spring.expectedLocalDays).toBe(1);
  expect(Date.parse(autumn.value)-Date.parse(autumn.reference)).toBe(23.5*3600000);expect(autumn.expectedLocalDays).toBe(0);
  expect(observation.references.map(r=>r.output)).toEqual(["in 2 minutes","1 minute ago","in 2 minutes"]);
 });
 test("NewYork all policies and both zones preserve gap seconds and milliseconds",()=>{
  expect(observation.policies.map(r=>r.rejected)).toEqual([true,false,false,true,false,false,false,false]);
  expect(observation.policies.filter(r=>!r.rejected).map(r=>(r as any).output.instant)).toEqual(["2026-11-01T05:30:00.000Z","2026-11-01T06:30:00.000Z","2026-03-08T06:30:00.000Z","2026-03-08T07:30:00.000Z","2026-03-08T06:15:12.345Z","2026-03-08T07:15:12.345Z"]);
  expect(observation.londonPositions.map(r=>r.output.instant)).toEqual(["2026-03-29T00:15:12.345Z","2026-03-29T01:15:12.345Z"]);
 });
 test("actual effective child defaults are observed and explicit outputs remain equal",async()=>{
  const reports=[];
  for(const env of [{TZ:"UTC",LANG:"en_US.UTF-8",LC_ALL:"en_US.UTF-8"},{TZ:"Pacific/Honolulu",LANG:"fr_FR.UTF-8",LC_ALL:"fr_FR.UTF-8"}]) {
   const child=Bun.spawn([process.execPath,new URL("./fixtures/temporal-host-observer.ts",import.meta.url).pathname],{env:{...Bun.env,...env,JADPO_TEMPORAL_HOST_RECEIPT:""},stdout:"pipe",stderr:"pipe"});
   const [stdout,stderr,exit]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);
   expect(exit).toBe(0);expect(stderr).toBe("");
   const report=JSON.parse(stdout);reports.push({...report,childExit:exit});
   expect(report.appSHA256).toBe(observation.appSHA256);
   for(const key of ["named","otherZone","calendars","structured","zoneNames","thresholds","days","references","policies","londonPositions","civil"]) expect(report[key]).toEqual((observation as any)[key]);
  }
  expect(reports.map(r=>r.environment.effectiveTimeZone)).toEqual(["UTC","Pacific/Honolulu"]);
  // Merely changing LANG is not evidence that Intl default locale changed.
  const localeVariation=reports[0].environment.effectiveLocale!==reports[1].environment.effectiveLocale?"observed": "inconclusive_no_effective_locale_variation";
  if(Bun.env.JADPO_TEMPORAL_HOST_ENV_RECEIPT) writeFileSync(Bun.env.JADPO_TEMPORAL_HOST_ENV_RECEIPT,JSON.stringify({localeVariation,children:reports},null,2)+"\n");
 },20000);
});
