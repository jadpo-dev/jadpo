import { cpSync, mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

// Preserve the unchanged application's repository-relative pinned service import
// in an isolated tree. The compiler discovers import roots through this marker.
export function copyGoldenFixture(project: string): void {
  const source = resolve("examples/golden-todo-migration");
  cpSync(source, project, { recursive: true, filter: path => path !== join(source, "build") });
  writeFileSync(join(project, ".git"), "isolated golden fixture import root\n");
  const assurance = join(project, "tests/assurance");
  mkdirSync(assurance, { recursive: true });
  cpSync(resolve("tests/assurance/service-reference-mail-v0.1.json"), join(assurance, "service-reference-mail-v0.1.json"));
}
