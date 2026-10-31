// Formats a file with Prettier off the UI thread. Measured with Prettier
// 3.9.10's standalone build on one Bun 1.4.4 executable's modules: 8 ms for
// the median (5 KB), 0.24 s at the 99th percentile (216 KB), 5.4 s for the
// largest (5.1 MB). The UI would freeze for all of that on the main thread.

import * as prettier from "prettier/standalone";
import * as babel from "prettier/plugins/babel";
import * as estree from "prettier/plugins/estree";
import * as typescript from "prettier/plugins/typescript";
import * as postcss from "prettier/plugins/postcss";
import * as html from "prettier/plugins/html";
import * as markdown from "prettier/plugins/markdown";

export interface FormatRequest {
  id: number;
  text: string;
  parser: string;
}

export type FormatResponse =
  | { id: number; ok: true; text: string }
  | { id: number; ok: false; error: string };

const plugins = [babel, estree, typescript, postcss, html, markdown];

self.onmessage = async (event: MessageEvent<FormatRequest>) => {
  const { id, text, parser } = event.data;
  let response: FormatResponse;
  try {
    response = { id, ok: true, text: await prettier.format(text, { parser, plugins }) };
  } catch (error) {
    response = { id, ok: false, error: String(error) };
  }
  self.postMessage(response);
};
