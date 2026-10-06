// Separate native host process, using actual generated persistence unchanged.
import { pathToFileURL } from "node:url";
const { persistence, configureLifecycleRetention } = await import(pathToFileURL(Bun.env.JADPO_MAINTENANCE_MODULE!).href);
configureLifecycleRetention({ soft_delete_retention: "30d" });
try {
  const result = await persistence.withOperationTime(Bun.env.JADPO_MAINTENANCE_AT!).transaction(async (transaction: any) => {
    const occurrence = await transaction.scheduled_retention_purge_Todo(() => {});
    if (Bun.env.JADPO_MAINTENANCE_HOLD === "1") {
      console.log("HELD");
      await new Promise(() => {});
    }
    return occurrence;
  });
  console.log(JSON.stringify({ outcome: result?.outcome ?? "skipped", count: result?.result?.committedCount ?? 0 }));
} catch {
  console.log(JSON.stringify({ outcome: "refused" }));
  process.exitCode = 1;
}
