/**
 * sky-la design system (direction A, "Tahoe"; spec in docs/design/DESIGN.md §6).
 * Tokens and primitives land in WP-08.
 */

/** Joins class names, skipping falsy entries. */
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
