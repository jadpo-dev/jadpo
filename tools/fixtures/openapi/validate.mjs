// Checks each OpenAPI document named on the command line against the
// OpenAPI specification's own JSON Schema, for the version the document
// declares, with the validator pinned in package-lock.json
// (tools/checks/openapi_valid.py runs it). Prints one line a document and
// exits 1 if any isn't valid.
import { readFile } from "node:fs/promises";
import { Validator } from "@seriousme/openapi-schema-validator";

let failed = false;
for (const path of process.argv.slice(2)) {
  const document = JSON.parse(await readFile(path, "utf8"));
  const validator = new Validator();
  const result = await validator.validate(document);
  if (result.valid && validator.version === "3.1") {
    console.log(`valid OpenAPI ${document.openapi}: ${path}`);
  } else {
    failed = true;
    const why = result.valid
      ? `it declares OpenAPI ${document.openapi}, not 3.1`
      : JSON.stringify(result.errors, null, 2);
    console.log(`not valid OpenAPI 3.1: ${path}\n${why}`);
  }
}
process.exit(failed ? 1 : 0);
