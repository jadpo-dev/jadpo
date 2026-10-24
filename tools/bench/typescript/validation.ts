import { z } from '@hono/zod-openapi';

/** Unicode scalar bounds, rather than JavaScript's UTF-16 unit length. */
export function scalarText(min: number, max: number) {
  return z.string().refine(value => {
    const units = value.length;
    if (units < min || units > max * 2 || !value.isWellFormed()) return false;
    // Every field here has min 0 or 1. For well-formed text within the
    // unit bound, the scalar bound follows without walking code points.
    if (units <= max && min <= 1) return true;
    let count = 0;
    for (let at = 0; at < units; at++) {
      if (++count > max) return false;
      const unit = value.charCodeAt(at);
      if (unit >= 0xd800 && unit <= 0xdbff) at++;
    }
    return count >= min;
  }, { message: `Expected ${min} to ${max} Unicode scalar values` })
    .openapi({ type: 'string', minLength: min, maxLength: max });
}
