// Each module carries a unique marker string so an extractor's output can be
// scored by grep, independent of how the tool names or splits files.
import { greet } from "./greet.ts";
import notes from "./notes.txt" with { type: "text" };

const MARKER_ENTRY = "BUNMATRIX_MARKER_ENTRY_7f3a";
console.log(MARKER_ENTRY, greet("matrix"), notes.length);
