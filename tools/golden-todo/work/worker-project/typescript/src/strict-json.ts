// Validate JSON's object member uniqueness before JSON.parse can erase evidence.
// Bounds are intentionally small at the credential boundary.
export function uniqueJson(text: string, maximum = 16384): unknown {
  if (text.length > maximum) throw Error("JSON limit");
  let at = 0;
  const space = () => {
    while (/[\x20\t\r\n]/.test(text[at] ?? "!")) at++;
  };
  const string = (): string => {
    if (text[at++] !== '"') throw Error("JSON string");
    const start = at - 1;
    while (at < text.length) {
      const char = text[at++];
      if (char === "\\") {
        at++;
        continue;
      }
      if (char === '"') {
        const parsed = JSON.parse(text.slice(start, at)) as string;
        if (!parsed.isWellFormed()) throw Error("Unpaired JSON surrogate");
        return parsed;
      }
    }
    throw Error("JSON string");
  };
  const value = (depth: number): void => {
    if (depth > 128) throw Error("JSON depth");
    space();
    const char = text[at];
    if (char === '"') {
      string();
      return;
    }
    if (char === "{") {
      at++;
      space();
      const members = new Set<string>();
      if (text[at] === "}") {
        at++;
        return;
      }
      while (true) {
        space();
        const key = string();
        if (members.has(key)) throw Error("Duplicate JSON member");
        members.add(key);
        space();
        if (text[at++] !== ":") throw Error("JSON colon");
        value(depth + 1);
        space();
        const end = text[at++];
        if (end === "}") return;
        if (end !== ",") throw Error("JSON comma");
      }
    }
    if (char === "[") {
      at++;
      space();
      if (text[at] === "]") {
        at++;
        return;
      }
      while (true) {
        value(depth + 1);
        space();
        const end = text[at++];
        if (end === "]") return;
        if (end !== ",") throw Error("JSON comma");
      }
    }
    const primitive =
      /^(?:true|false|null|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?)/.exec(
        text.slice(at),
      );
    if (!primitive) throw Error("JSON value");
    at += primitive[0].length;
  };
  value(0);
  space();
  if (at !== text.length) throw Error("JSON trailing");
  return JSON.parse(text) as unknown;
}
