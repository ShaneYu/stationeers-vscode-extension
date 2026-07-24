/**
 * Finds an IC10 line comment while ignoring hash characters inside quoted
 * HASH/STR literals. This mirrors the language server's comment parsing.
 */
export function findIc10CommentStart(line: string): number | undefined {
  let quoted = false;
  let escaped = false;

  for (let index = 0; index < line.length; index += 1) {
    const character = line[index];

    if (escaped) {
      escaped = false;
      continue;
    }

    if (character === "\\" && quoted) {
      escaped = true;
    } else if (character === "\"") {
      quoted = !quoted;
    } else if (character === "#" && !quoted) {
      return index;
    }
  }

  return undefined;
}

/**
 * Includes whitespace immediately before a comment in the removal range so
 * stripping comments does not leave trailing spaces behind.
 */
export function findIc10CommentRemovalStart(
  line: string,
): number | undefined {
  const commentStart = findIc10CommentStart(line);
  if (commentStart === undefined) {
    return undefined;
  }

  let removalStart = commentStart;
  while (
    removalStart > 0 &&
    (line[removalStart - 1] === " " || line[removalStart - 1] === "\t")
  ) {
    removalStart -= 1;
  }
  return removalStart;
}

interface Ic10Line {
  text: string;
  ending: string;
}

interface Token {
  text: string;
  start: number;
  end: number;
}

interface RelativeBranch {
  lineNumber: number;
  offset: number | undefined;
  offsetToken: Token;
}

export interface RemoveIc10CommentsResult {
  text: string;
  removedCommentLines: number;
  adjustedRelativeBranches: number;
  removedRedundantBranches: number;
  unadjustedRelativeBranches: number;
}

const relativeBranchOperandCounts: Readonly<Record<string, number>> = {
  brap: 4,
  brapz: 3,
  brdns: 2,
  brdse: 2,
  breq: 3,
  breqz: 2,
  brge: 3,
  brgez: 2,
  brgt: 3,
  brgtz: 2,
  brle: 3,
  brlez: 2,
  brlt: 3,
  brltz: 2,
  brna: 4,
  brnan: 2,
  brnaz: 3,
  brne: 3,
  brnez: 2,
  jr: 1,
};

const decimalNumberPattern =
  /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/;
const hexadecimalNumberPattern = /^\$[0-9A-Fa-f](?:_?[0-9A-Fa-f])*$/;
const binaryNumberPattern = /^%[01](?:_?[01])*$/;

/**
 * Removes IC10 comments and preserves the destinations of relative branches
 * with literal integer offsets when entire comment-only lines disappear.
 */
export function removeIc10Comments(source: string): RemoveIc10CommentsResult {
  const lines = splitLines(source);
  const deletedLines = new Set<number>();
  const branches: RelativeBranch[] = [];

  for (let lineNumber = 0; lineNumber < lines.length; lineNumber += 1) {
    const line = lines[lineNumber];
    if (!line) {
      continue;
    }

    const commentStart = findIc10CommentStart(line.text);
    if (
      commentStart !== undefined &&
      line.text.slice(0, commentStart).trim().length === 0
    ) {
      deletedLines.add(lineNumber);
      continue;
    }

    const branch = parseRelativeBranch(line.text, lineNumber);
    if (branch) {
      branches.push(branch);
    }
  }

  const removedCommentLines = deletedLines.size;

  // Removing a now-redundant branch is another line deletion, which may make
  // an earlier or later branch redundant too. Iterate until that settles.
  let foundRedundantBranch: boolean;
  do {
    foundRedundantBranch = false;
    for (const branch of branches) {
      const offset = branch.offset;
      if (offset === undefined || deletedLines.has(branch.lineNumber)) {
        continue;
      }

      const adjustedOffset = adjustRelativeOffset(
        branch,
        offset,
        deletedLines,
      );
      if (offset !== 0 && adjustedOffset === 0) {
        deletedLines.add(branch.lineNumber);
        foundRedundantBranch = true;
      }
    }
  } while (foundRedundantBranch);

  let adjustedRelativeBranches = 0;
  let text = "";

  for (let lineNumber = 0; lineNumber < lines.length; lineNumber += 1) {
    const line = lines[lineNumber];
    if (!line || deletedLines.has(lineNumber)) {
      continue;
    }

    const removalStart = findIc10CommentRemovalStart(line.text);
    let transformedLine =
      removalStart === undefined
        ? line.text
        : line.text.slice(0, removalStart);

    const branch = branches.find(
      (candidate) => candidate.lineNumber === lineNumber,
    );
    const offset = branch?.offset;
    if (branch && offset !== undefined) {
      const adjustedOffset = adjustRelativeOffset(
        branch,
        offset,
        deletedLines,
      );
      if (adjustedOffset !== offset) {
        transformedLine =
          transformedLine.slice(0, branch.offsetToken.start) +
          String(adjustedOffset) +
          transformedLine.slice(branch.offsetToken.end);
        adjustedRelativeBranches += 1;
      }
    }

    text += transformedLine + line.ending;
  }

  return {
    text,
    removedCommentLines,
    adjustedRelativeBranches,
    removedRedundantBranches: deletedLines.size - removedCommentLines,
    unadjustedRelativeBranches: branches.filter(
      (branch) => branch.offset === undefined,
    ).length,
  };
}

function splitLines(source: string): Ic10Line[] {
  const lines: Ic10Line[] = [];
  let lineStart = 0;

  for (let index = 0; index < source.length; index += 1) {
    const character = source[index];
    if (character !== "\r" && character !== "\n") {
      continue;
    }

    const endingStart = index;
    if (character === "\r" && source[index + 1] === "\n") {
      index += 1;
    }
    lines.push({
      text: source.slice(lineStart, endingStart),
      ending: source.slice(endingStart, index + 1),
    });
    lineStart = index + 1;
  }

  lines.push({ text: source.slice(lineStart), ending: "" });
  return lines;
}

function parseRelativeBranch(
  line: string,
  lineNumber: number,
): RelativeBranch | undefined {
  const commentStart = findIc10CommentStart(line);
  const code = line.slice(0, commentStart ?? line.length);
  const tokens = tokenize(code);
  const mnemonic = tokens[0]?.text.toLowerCase();
  if (!mnemonic) {
    return undefined;
  }

  const operandCount = relativeBranchOperandCounts[mnemonic];
  if (operandCount === undefined || tokens.length !== operandCount + 1) {
    return undefined;
  }

  const offsetToken = tokens[operandCount];
  if (!offsetToken) {
    return undefined;
  }

  return {
    lineNumber,
    offset: parseIntegerLiteral(offsetToken.text),
    offsetToken,
  };
}

function tokenize(code: string): Token[] {
  const tokens: Token[] = [];
  let tokenStart: number | undefined;
  let quoted = false;
  let escaped = false;

  for (let index = 0; index < code.length; index += 1) {
    const character = code[index];
    if (character?.trim().length === 0 && !quoted) {
      if (tokenStart !== undefined) {
        tokens.push({
          text: code.slice(tokenStart, index),
          start: tokenStart,
          end: index,
        });
        tokenStart = undefined;
      }
      continue;
    }

    tokenStart ??= index;
    if (escaped) {
      escaped = false;
    } else if (character === "\\" && quoted) {
      escaped = true;
    } else if (character === "\"") {
      quoted = !quoted;
    }
  }

  if (tokenStart !== undefined) {
    tokens.push({
      text: code.slice(tokenStart),
      start: tokenStart,
      end: code.length,
    });
  }
  return tokens;
}

function parseIntegerLiteral(token: string): number | undefined {
  let value: number;

  if (decimalNumberPattern.test(token)) {
    value = Number(token);
  } else if (hexadecimalNumberPattern.test(token)) {
    value = Number.parseInt(token.slice(1).replaceAll("_", ""), 16);
  } else if (binaryNumberPattern.test(token)) {
    value = Number.parseInt(token.slice(1).replaceAll("_", ""), 2);
  } else {
    return undefined;
  }

  return Number.isSafeInteger(value) ? value : undefined;
}

function adjustRelativeOffset(
  branch: RelativeBranch,
  offset: number,
  deletedLines: ReadonlySet<number>,
): number {
  const originalTarget = branch.lineNumber + 1 + offset;
  const adjustedSource =
    branch.lineNumber - countDeletedLinesBefore(deletedLines, branch.lineNumber);
  const adjustedTarget =
    originalTarget - countDeletedLinesBefore(deletedLines, originalTarget);
  return adjustedTarget - adjustedSource - 1;
}

function countDeletedLinesBefore(
  deletedLines: ReadonlySet<number>,
  lineNumber: number,
): number {
  let count = 0;
  for (const deletedLine of deletedLines) {
    if (deletedLine < lineNumber) {
      count += 1;
    }
  }
  return count;
}
