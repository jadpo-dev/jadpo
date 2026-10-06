// Local native provenance probe; application import uses ephemeral SQLite only.
// No business handler or worker invocation.
import { dlopen, ptr } from "bun:ffi";
import { pathToFileURL } from "node:url";
const [application] = Bun.argv.slice(2);
if (!application || process.platform !== "darwin") throw new Error("This provenance probe requires the selected macOS native ICU boundary");
delete Bun.env.DATABASE_URL;
Bun.env.SQLITE_PATH = ":memory:";
const { driverInitializationCount } = await import("./golden_sql_observer.ts");
const { temporalProvenance } = await import(pathToFileURL(application).href);
const libraryPath = "/usr/lib/libicucore.A.dylib";
const status = new Int32Array(1);
const version = new Uint8Array(4);
const library = dlopen(libraryPath, {
  ucal_getTZDataVersion: { args: ["ptr"], returns: "cstring" },
  u_getVersion: { args: ["ptr"], returns: "void" },
});
try {
  const timezoneDataVersion = String(library.symbols.ucal_getTZDataVersion(ptr(status)));
  library.symbols.u_getVersion(ptr(version));
  if (status[0] !== 0 || !/^\d{4}[a-z]+$/u.test(timezoneDataVersion)) throw new Error("Native ICU timezone version probe failed");
  console.log(JSON.stringify({
    executable: process.execPath, platform: process.platform, architecture: process.arch,
    engine: `Bun ${Bun.version}`, reportedIcu: process.versions.icu ?? "unreported",
    reportedTimezoneDataVersion: typeof process.versions.tz === "string" && process.versions.tz.trim() ? process.versions.tz : "unreported",
    nativeIcu: { libraryPath, releaseVersion: Array.from(version), timezoneDataVersion, status: status[0] },
    generatedTemporalProvenance: temporalProvenance,
    generatedImportControl: { databaseMode: "isolated ephemeral SQLite memory", observedDriverInitializations: driverInitializationCount() },
    canonicalZoneControl: new Intl.DateTimeFormat("en-GB", { timeZone: "America/Coyhaique" }).resolvedOptions().timeZone,
  }));
} finally { library.close(); }
