/** Joins the class names that are actually on, dropping the conditions that are off. */
export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}
