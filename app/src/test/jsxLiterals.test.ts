// Rule 5: UI strings live only in the dictionaries. Every .tsx file is parsed
// with the TypeScript compiler; JSX text with letters and string literals
// with letters in user-facing attributes fail the test.

import * as ts from "typescript";
import { describe, expect, it } from "vitest";

const sources = import.meta.glob<string>(["../**/*.tsx", "!../**/*.test.tsx"], {
  query: "?raw",
  import: "default",
  eager: true,
});

const TEXT_ATTRIBUTES = new Set(["title", "aria-label", "aria-description", "placeholder", "alt", "label"]);
const LETTER = /\p{L}/u;

function literalsIn(file: string, text: string): string[] {
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const problems: string[] = [];
  const where = (node: ts.Node) => {
    const { line } = source.getLineAndCharacterOfPosition(node.getStart(source));
    return `${file}:${line + 1}`;
  };
  const visit = (node: ts.Node) => {
    if (ts.isJsxText(node) && LETTER.test(node.text)) {
      problems.push(`${where(node)} JSX text ${JSON.stringify(node.text.trim())}`);
    }
    if (ts.isJsxAttribute(node)) {
      const name = node.name.getText(source);
      const init = node.initializer;
      if (TEXT_ATTRIBUTES.has(name) && init !== undefined) {
        const literal =
          ts.isStringLiteral(init)
            ? init
            : ts.isJsxExpression(init) && init.expression !== undefined && ts.isStringLiteralLike(init.expression)
              ? init.expression
              : null;
        if (literal !== null && LETTER.test(literal.text)) {
          problems.push(`${where(node)} ${name}=${JSON.stringify(literal.text)}`);
        }
      }
    }
    if (
      ts.isJsxExpression(node) &&
      node.expression !== undefined &&
      ts.isStringLiteralLike(node.expression) &&
      ts.isJsxElement(node.parent) &&
      LETTER.test(node.expression.text)
    ) {
      problems.push(`${where(node)} JSX string ${JSON.stringify(node.expression.text)}`);
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return problems;
}

describe("JSX literals", () => {
  it("scans the app's components", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(20);
  });

  it("puts every UI string in the dictionaries", () => {
    const problems = Object.entries(sources).flatMap(([file, text]) => literalsIn(file, text));
    expect(problems).toEqual([]);
  });

  it("catches a literal", () => {
    expect(literalsIn("x.tsx", 'const a = <p title="Hello">Write now</p>;')).toHaveLength(2);
    expect(literalsIn("x.tsx", "const a = <p>{dict.x} · {n}/{m} →</p>;")).toEqual([]);
  });
});
