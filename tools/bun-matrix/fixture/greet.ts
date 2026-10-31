export const MARKER_GREET = "BUNMATRIX_MARKER_GREET_c41e";

export function greet(name: string): string {
  return `${MARKER_GREET} hello ${name}`;
}
