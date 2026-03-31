import fs from "node:fs";
import path from "node:path";

const ROOTS = [
  "dashboard/client/src",
  "dashboard/client/e2e",
  "dashboard/client/vite.config.ts",
  "dashboard/client/vitest.config.ts",
  "dashboard/client/playwright.config.ts",
];

const COMMAND = process.argv[2] ?? "check";

main();

function main() {
  const files = collectFiles(ROOTS);
  if (COMMAND === "fix") {
    const updated = files.reduce((count, file) => count + Number(fixFile(file)), 0);
    console.log(`ts comment audit fix: updated_files=${updated}`);
    return;
  }

  const summary = new Map();
  for (const file of files) {
    const violations = analyzeFile(file);
    if (violations > 0) {
      summary.set(file, violations);
    }
  }

  printSummary(summary);
  if (COMMAND === "check" && summary.size > 0) {
    process.exitCode = 1;
  }
}

function collectFiles(roots) {
  const files = [];
  for (const root of roots) {
    if (!fs.existsSync(root)) {
      continue;
    }
    const stat = fs.statSync(root);
    if (stat.isDirectory()) {
      walk(root, files);
      continue;
    }
    if (isTargetFile(root)) {
      files.push(root);
    }
  }
  return files.sort();
}

function walk(root, files) {
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    if (entry.name === "node_modules") {
      continue;
    }
    const fullPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      walk(fullPath, files);
      continue;
    }
    if (isTargetFile(fullPath)) {
      files.push(fullPath);
    }
  }
}

function isTargetFile(file) {
  return [".ts", ".tsx"].includes(path.extname(file));
}

function analyzeFile(file) {
  const source = fs.readFileSync(file, "utf8");
  return commentRanges(source).length;
}

function fixFile(file) {
  const source = fs.readFileSync(file, "utf8");
  const ranges = commentRanges(source);
  if (ranges.length === 0) {
    return false;
  }

  let output = "";
  let cursor = 0;
  for (const [start, end] of ranges) {
    output += source.slice(cursor, start);
    cursor = end;
  }
  output += source.slice(cursor);
  output = output.replace(/^\s*\{\}\s*(?:\r?\n)?/gmu, "");
  output = collapseBlankLines(output);

  if (output === source) {
    return false;
  }

  fs.writeFileSync(file, output);
  return true;
}

function commentRanges(source) {
  const ranges = [];
  const stack = [{ mode: "code", braces: 0 }];
  let index = 0;

  while (index < source.length) {
    const state = stack[stack.length - 1];
    const current = source[index];
    const next = source[index + 1];

    if (state.mode === "single") {
      index = advanceQuotedString(source, index, "'");
      stack.pop();
      continue;
    }

    if (state.mode === "double") {
      index = advanceQuotedString(source, index, '"');
      stack.pop();
      continue;
    }

    if (state.mode === "template") {
      if (current === "\\") {
        index += 2;
        continue;
      }
      if (current === "`") {
        stack.pop();
        index += 1;
        continue;
      }
      if (current === "$" && next === "{") {
        stack.push({ mode: "template_expr", braces: 0 });
        index += 2;
        continue;
      }
      index += 1;
      continue;
    }

    if (state.mode === "template_expr") {
      if (current === "{") {
        state.braces += 1;
        index += 1;
        continue;
      }
      if (current === "}") {
        if (state.braces === 0) {
          stack.pop();
        } else {
          state.braces -= 1;
        }
        index += 1;
        continue;
      }
    }

    if (current === "'" && canStartString(source, index)) {
      stack.push({ mode: "single", braces: 0 });
      index += 1;
      continue;
    }

    if (current === '"' && canStartString(source, index)) {
      stack.push({ mode: "double", braces: 0 });
      index += 1;
      continue;
    }

    if (current === "`") {
      stack.push({ mode: "template", braces: 0 });
      index += 1;
      continue;
    }

    if (current === "/" && next === "/") {
      const end = source.indexOf("\n", index);
      const commentEnd = end === -1 ? source.length : end;
      const text = source.slice(index, commentEnd);
      if (!isAllowedDirective(text)) {
        ranges.push([index, commentEnd]);
      }
      index = commentEnd;
      continue;
    }

    if (current === "/" && next === "*") {
      const end = source.indexOf("*/", index + 2);
      const commentEnd = end === -1 ? source.length : end + 2;
      const text = source.slice(index, commentEnd);
      if (!isAllowedDirective(text)) {
        ranges.push([index, commentEnd]);
      }
      index = commentEnd;
      continue;
    }

    index += 1;
  }

  return ranges;
}

function advanceQuotedString(source, index, quote) {
  let cursor = index;
  while (cursor < source.length) {
    if (source[cursor] === "\\") {
      cursor += 2;
      continue;
    }
    if (source[cursor] === quote) {
      return cursor + 1;
    }
    cursor += 1;
  }
  return cursor;
}

function canStartString(source, index) {
  let cursor = index - 1;
  while (cursor >= 0 && /\s/u.test(source[cursor])) {
    cursor -= 1;
  }
  if (cursor < 0) {
    return true;
  }
  return source[cursor] !== "\\";
}

function isAllowedDirective(text) {
  const trimmed = text.trimStart();
  return (
    trimmed.startsWith("// eslint-") ||
    trimmed.startsWith("/* eslint-") ||
    trimmed.startsWith("// @ts-") ||
    trimmed.startsWith("/* @ts-")
  );
}

function collapseBlankLines(source) {
  const hadTrailingNewline = source.endsWith("\n");
  const lines = source.split("\n");
  const output = [];
  let blankRun = 0;

  for (const line of lines) {
    const trimmed = line.replace(/[ \t]+$/u, "");
    if (trimmed.length === 0) {
      blankRun += 1;
      if (blankRun === 1) {
        output.push("");
      }
      continue;
    }
    blankRun = 0;
    output.push(trimmed);
  }

  let result = output.join("\n");
  if (hadTrailingNewline) {
    result += "\n";
  }
  return result;
}

function printSummary(summary) {
  console.log("ts comment audit:");
  for (const [file, comments] of summary.entries()) {
    console.log(`  ${file}: disallowed_comments=${comments}`);
  }
  const total = [...summary.values()].reduce((sum, value) => sum + value, 0);
  console.log(`ts comment audit summary: files_with_backlog=${summary.size}, disallowed_comments=${total}`);
}
