// Monaco served from the app bundle. `@monaco-editor/react` loads Monaco from
// a CDN unless given an instance, and the app must work offline and must not
// fetch code at run time.

import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import TypeScriptWorker from "monaco-editor/language/typescript/ts.worker?worker";
import JsonWorker from "monaco-editor/language/json/json.worker?worker";
import CssWorker from "monaco-editor/language/css/css.worker?worker";
import HtmlWorker from "monaco-editor/language/html/html.worker?worker";

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    switch (label) {
      case "typescript":
      case "javascript":
        return new TypeScriptWorker();
      case "json":
        return new JsonWorker();
      case "css":
      case "scss":
      case "less":
        return new CssWorker();
      case "html":
        return new HtmlWorker();
      default:
        return new EditorWorker();
    }
  },
};

// Extracted code imports files the language service cannot see, and a
// minified bundle trips every rule, so diagnostics would be all noise.
// Highlighting, folding and in-file navigation still work.
const noDiagnostics = { noSemanticValidation: true, noSyntaxValidation: true };
monaco.typescript.typescriptDefaults.setDiagnosticsOptions(noDiagnostics);
monaco.typescript.javascriptDefaults.setDiagnosticsOptions(noDiagnostics);

loader.config({ monaco });
