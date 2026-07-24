const assert = require("node:assert/strict");
const { describe, it } = require("node:test");
const {
  findIc10CommentRemovalStart,
  findIc10CommentStart,
  removeIc10Comments,
} = require("./comments.ts") as typeof import("./comments");

describe("IC10 comment parsing", () => {
  it("finds full-line and inline comments", () => {
    assert.equal(findIc10CommentStart("# comment"), 0);
    assert.equal(findIc10CommentStart("move r0 1 # comment"), 10);
    assert.equal(findIc10CommentStart("move r0 1#comment"), 9);
  });

  it("ignores hash characters in quoted literals", () => {
    const line = String.raw`move r0 HASH("A#B") # comment`;
    assert.equal(findIc10CommentStart(line), 20);
  });

  it("handles escaped quotes in quoted literals", () => {
    const line = String.raw`move r0 HASH("A\"#B") # comment`;
    assert.equal(findIc10CommentStart(line), 22);
  });

  it("returns undefined when a line has no comment", () => {
    assert.equal(findIc10CommentStart("move r0 1"), undefined);
  });

  it("includes preceding whitespace in the removal range", () => {
    assert.equal(findIc10CommentRemovalStart("move r0 1  # comment"), 9);
    assert.equal(findIc10CommentRemovalStart("  # comment"), 0);
  });
});

describe("IC10 comment removal", () => {
  it("removes comment-only lines and preserves existing blank lines", () => {
    const source = [
      "move r0 1",
      "# comment 1",
      "  # comment 2",
      "",
      "j end # inline comment",
      "",
      "end:",
    ].join("\n");

    assert.equal(
      removeIc10Comments(source).text,
      ["move r0 1", "", "j end", "", "end:"].join("\n"),
    );
  });

  it("preserves line endings while deleting full comment lines", () => {
    const source = "move r0 1\r\n# comment\r\nj end\r\n";
    assert.equal(removeIc10Comments(source).text, "move r0 1\r\nj end\r\n");
  });

  it("updates positive and negative literal relative offsets", () => {
    const source = [
      "brnez r0 4",
      "move r1 1",
      "# forward comment",
      "move r2 2",
      "move r3 3",
      "brnez r0 -4",
      "# unrelated trailing comment",
    ].join("\n");

    assert.equal(
      removeIc10Comments(source).text,
      [
        "brnez r0 3",
        "move r1 1",
        "move r2 2",
        "move r3 3",
        "brnez r0 -3",
        "",
      ].join("\n"),
    );
  });

  it("removes the example branch when its corrected offset is zero", () => {
    const source = [
      "move r0 1",
      "breq r0 1 2",
      "# comment 1",
      "# comment 2",
      "j end",
      "",
      "",
      "end:",
    ].join("\n");

    const result = removeIc10Comments(source);
    assert.equal(
      result.text,
      ["move r0 1", "j end", "", "", "end:"].join("\n"),
    );
    assert.equal(result.removedCommentLines, 2);
    assert.equal(result.removedRedundantBranches, 1);
  });

  it("removes newly redundant branches until offsets settle", () => {
    const source = [
      "brnez r0 3",
      "# comment before",
      "breq r1 1 1",
      "# comment after",
      "move r2 2",
    ].join("\n");

    const result = removeIc10Comments(source);
    assert.equal(result.text, "move r2 2");
    assert.equal(result.removedRedundantBranches, 2);
  });

  it("handles every relative control instruction", () => {
    const relativeBranches = [
      "brap r0 1 0.1 1",
      "brapz r0 0.1 1",
      "brdns d0 1",
      "brdse d0 1",
      "breq r0 1 1",
      "breqz r0 1",
      "brge r0 1 1",
      "brgez r0 1",
      "brgt r0 1 1",
      "brgtz r0 1",
      "brle r0 1 1",
      "brlez r0 1",
      "brlt r0 1 1",
      "brltz r0 1",
      "brna r0 1 0.1 1",
      "brnan r0 1",
      "brnaz r0 0.1 1",
      "brne r0 1 1",
      "brnez r0 1",
      "jr 1",
    ];

    for (const branch of relativeBranches) {
      const result = removeIc10Comments(
        [branch, "# comment", "move r0 1"].join("\n"),
      );
      assert.equal(result.text, "move r0 1", branch);
      assert.equal(result.removedRedundantBranches, 1, branch);
    }
  });

  it("adjusts integer-valued decimal, hexadecimal, and binary offsets", () => {
    const sources = ["breq r0 1 2.0", "breq r0 1 $2", "breq r0 1 %10"];

    for (const branch of sources) {
      const result = removeIc10Comments(
        [branch, "# comment 1", "# comment 2", "move r0 1"].join("\n"),
      );
      assert.equal(result.text, "move r0 1", branch);
    }
  });

  it("warns callers about relative offsets that cannot be inferred", () => {
    const source = [
      "breq r0 1 r2",
      "brnez r0 OFFSET",
      "# comment",
      "move r1 1",
    ].join("\n");

    const result = removeIc10Comments(source);
    assert.equal(result.unadjustedRelativeBranches, 2);
    assert.equal(
      result.text,
      ["breq r0 1 r2", "brnez r0 OFFSET", "move r1 1"].join("\n"),
    );
  });

  it("does not remove an existing zero-offset branch", () => {
    const source = ["breq r0 1 0", "# comment", "move r1 1"].join("\n");
    assert.equal(
      removeIc10Comments(source).text,
      ["breq r0 1 0", "move r1 1"].join("\n"),
    );
  });
});
