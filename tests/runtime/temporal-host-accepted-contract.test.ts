import {beforeAll, expect, test} from "bun:test";
import {writeFileSync} from "node:fs";
import {observeHost} from "./fixtures/temporal-host-observer";
let observation: Awaited<ReturnType<typeof observeHost>>;
beforeAll(async()=>{observation=await observeHost();if(Bun.env.JADPO_TEMPORAL_HOST_RECEIPT)writeFileSync(Bun.env.JADPO_TEMPORAL_HOST_RECEIPT,JSON.stringify(observation,null,2)+"\n");});
// Accepted TIME-D32 expectations retain original baseline reds in the evidence.
// This is retained separately from wiring conformance; no invented translation.
test("French future under-minute must not state one full minute",()=>{
 const actual=observation.frenchSubminute;
 expect(Date.parse(actual.value)-Date.parse(actual.reference)).toBe(30000);
 const oneFullMinute=new Intl.RelativeTimeFormat("fr-FR",{numeric:"always"}).format(1,"minute");
 expect(actual.output).not.toBe(oneFullMinute);
});
test("friendly absolute fallback must carry the accepted short zone name",()=>{
 const row=observation.days.find(r=>r.label==="plus7" && r.locale==="en-GB")!;
 const name=(width:"short"|"long")=>new Intl.DateTimeFormat("en-GB",{timeZone:row.zone,timeZoneName:width}).formatToParts(new Date(row.value)).find(p=>p.type==="timeZoneName")!.value;
 const shortName=name("short"),longName=name("long");
 expect(shortName).not.toBe(longName);
 expect(row.output).toContain(shortName);
 expect(row.output).not.toContain(longName);
});

for(const [zone_name, nativeName] of [["long_offset","longOffset"],["short_offset","shortOffset"],["long_generic","longGeneric"],["short_generic","shortGeneric"]]) {
 test(`accepted zone_name ${zone_name} must reach valid native Intl option`,()=>{
  const rows=observation.zoneNames.filter(r=>r.components.zone_name===zone_name);
  expect(rows.map(r=>r.zone)).toEqual(["Europe/London","America/New_York"]);
  for(const row of rows) {
   const expected=new Intl.DateTimeFormat(row.locale,{timeZone:row.zone,hour:"2-digit",minute:"2-digit",timeZoneName:nativeName as Intl.DateTimeFormatOptions["timeZoneName"]}).format(new Date(row.instant));
   expect(row.rejected).toBe(false);
   expect(row.output).toBe(expected);
  }
 });
}

test("French future under-minute approved wording preserves adjacent branches",()=>{
 for(const row of observation.frenchBoundaries) {
  expect(Date.parse(row.value)-Date.parse(row.reference)).toBe(row.difference);
  if(row.difference>0 && row.difference<60000) expect(row.output).toBe("dans moins d’une minute");
  else if(row.difference<=0 && Math.abs(row.difference)<60000) expect(row.output).toBe(new Intl.RelativeTimeFormat("fr-FR",{numeric:"auto"}).format(0,"second"));
  else if(Math.abs(row.difference)<3600000) expect(row.output).toBe(new Intl.RelativeTimeFormat("fr-FR",{numeric:"always"}).format(row.difference<0?-Math.floor(-row.difference/60000):Math.ceil(row.difference/60000),"minute"));
  else {const today=new Intl.RelativeTimeFormat("fr-FR",{numeric:"auto"}).format(0,"day");expect(row.output).toBe(new Intl.DateTimeFormat("fr-FR",{timeZone:"Europe/London",weekday:"long",hour:"numeric",minute:"2-digit"}).formatToParts(new Date(row.value)).map(p=>p.type==="weekday"?today:p.value).join(""));}
 }
});
