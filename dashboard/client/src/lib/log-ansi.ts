export interface AnsiToken {
  text: string;
  className: string;
}

interface AnsiState {
  fg: AnsiColor | null;
  bold: boolean;
  dim: boolean;
  italic: boolean;
}

type AnsiColor =
  | "black"
  | "red"
  | "green"
  | "yellow"
  | "blue"
  | "magenta"
  | "cyan"
  | "white"
  | "bright-black"
  | "bright-red"
  | "bright-green"
  | "bright-yellow"
  | "bright-blue"
  | "bright-magenta"
  | "bright-cyan"
  | "bright-white";

const STANDARD_COLORS: AnsiColor[] = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
];

const BRIGHT_COLORS: AnsiColor[] = [
  "bright-black",
  "bright-red",
  "bright-green",
  "bright-yellow",
  "bright-blue",
  "bright-magenta",
  "bright-cyan",
  "bright-white",
];

// eslint-disable-next-line no-control-regex
const SGR_PATTERN = /\x1b\[([0-9;]*)m/g;

function emptyState(): AnsiState {
  return { fg: null, bold: false, dim: false, italic: false };
}

function applyCodes(state: AnsiState, codes: number[]): AnsiState {
  let next: AnsiState = { ...state };
  for (const code of codes) {
    if (code === 0) {
      next = emptyState();
    } else if (code === 1) {
      next.bold = true;
    } else if (code === 2) {
      next.dim = true;
    } else if (code === 3) {
      next.italic = true;
    } else if (code === 22) {
      next.bold = false;
      next.dim = false;
    } else if (code === 23) {
      next.italic = false;
    } else if (code === 39) {
      next.fg = null;
    } else if (code >= 30 && code <= 37) {
      next.fg = STANDARD_COLORS[code - 30];
    } else if (code >= 90 && code <= 97) {
      next.fg = BRIGHT_COLORS[code - 90];
    }
  }
  return next;
}

function colorClass(color: AnsiColor | null): string | null {
  switch (color) {
    case "red":
    case "bright-red":
      return "text-accent-red";
    case "green":
    case "bright-green":
      return "text-accent-green";
    case "yellow":
    case "bright-yellow":
      return "text-accent-blue";
    case "blue":
    case "bright-blue":
      return "text-accent-blue";
    case "magenta":
    case "bright-magenta":
      return "text-muted";
    case "cyan":
    case "bright-cyan":
      return "text-muted";
    case "black":
    case "bright-black":
      return "text-muted";
    case "white":
    case "bright-white":
    case null:
      return null;
  }
}

function classNameFor(state: AnsiState): string {
  const classes: string[] = [];
  if (state.dim) {
    classes.push("text-muted");
  } else {
    const c = colorClass(state.fg);
    if (c) classes.push(c);
  }
  if (state.bold) classes.push("font-semibold");
  if (state.italic) classes.push("italic");
  return classes.join(" ");
}

export function stripAnsi(input: string): string {
  return parseAnsi(input)
    .map((token) => token.text)
    .join("");
}

export function parseAnsi(input: string): AnsiToken[] {
  const tokens: AnsiToken[] = [];
  let lastIndex = 0;
  let state = emptyState();

  SGR_PATTERN.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = SGR_PATTERN.exec(input)) !== null) {
    if (match.index > lastIndex) {
      const text = input.slice(lastIndex, match.index);
      if (text.length > 0) tokens.push({ text, className: classNameFor(state) });
    }
    const codes =
      match[1].length === 0
        ? [0]
        : match[1].split(";").map((c) => Number.parseInt(c, 10) || 0);
    state = applyCodes(state, codes);
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < input.length) {
    const text = input.slice(lastIndex);
    if (text.length > 0) tokens.push({ text, className: classNameFor(state) });
  }

  return tokens;
}
