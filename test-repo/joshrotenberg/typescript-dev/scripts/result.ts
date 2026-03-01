/**
 * A Result type for TypeScript -- return errors as values instead of throwing.
 *
 * Usage:
 *   function parseConfig(raw: string): Result<Config, ParseError> {
 *     try {
 *       return ok(JSON.parse(raw));
 *     } catch (e) {
 *       return err(new ParseError("Invalid JSON", { cause: e }));
 *     }
 *   }
 *
 *   const result = parseConfig(input);
 *   if (result.ok) {
 *     console.log(result.value);
 *   } else {
 *     console.error(result.error);
 *   }
 */

export type Result<T, E = Error> =
  | { ok: true; value: T }
  | { ok: false; error: E };

export function ok<T>(value: T): Result<T, never> {
  return { ok: true, value };
}

export function err<E>(error: E): Result<never, E> {
  return { ok: false, error };
}

export function unwrap<T, E>(result: Result<T, E>): T {
  if (result.ok) return result.value;
  throw result.error;
}
