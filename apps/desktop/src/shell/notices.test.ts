// @vitest-environment node
import { describe, expect, it } from "vitest";
import { parseNotices } from "./notices";

const SAMPLE = `# Third-party notices

## Rust crates

- MIT License: 2 crate(s)

### MIT License

Used by:
- alpha 1.2.3 (https://example.com/alpha)
- alpha 1.2.3 (https://example.com/alpha)
- beta 0.1.0 (https://example.com/beta)

\`\`\`text
MIT text for the group
\`\`\`

### Apache License 2.0

Used by:
- gamma 2.0.0 (https://example.com/gamma)

\`\`\`text
Apache text
\`\`\`

## Frontend packages

### react 19.1.0

License: MIT · https://react.dev

\`\`\`text
React MIT text
\`\`\`

### nofile 1.0.0

License: ISC · https://example.com/nofile

No license file is shipped in the package; see the ISC license text.
`;

describe("parseNotices", () => {
  const packages = parseNotices(SAMPLE);

  it("reads every Rust crate from its Used by lines with the group license", () => {
    expect(packages.filter((item) => item.source === "rust")).toEqual([
      {
        name: "alpha",
        version: "1.2.3",
        license: "MIT License",
        url: "https://example.com/alpha",
        text: "MIT text for the group",
        source: "rust",
      },
      {
        name: "beta",
        version: "0.1.0",
        license: "MIT License",
        url: "https://example.com/beta",
        text: "MIT text for the group",
        source: "rust",
      },
      {
        name: "gamma",
        version: "2.0.0",
        license: "Apache License 2.0",
        url: "https://example.com/gamma",
        text: "Apache text",
        source: "rust",
      },
    ]);
  });

  it("reads frontend packages with their own version, license and text", () => {
    expect(packages.filter((item) => item.source === "frontend")).toEqual([
      {
        name: "react",
        version: "19.1.0",
        license: "MIT",
        url: "https://react.dev",
        text: "React MIT text",
        source: "frontend",
      },
      {
        name: "nofile",
        version: "1.0.0",
        license: "ISC",
        url: "https://example.com/nofile",
        text: "",
        source: "frontend",
      },
    ]);
  });

  it("merges an entry that is listed twice with the same license text", () => {
    expect(packages.filter((item) => item.name === "alpha")).toHaveLength(1);
  });

  it("keeps entries that share a license id but ship different texts", () => {
    const source = `## Rust crates

### MIT License

Used by:
- alpha 1.2.3 (https://example.com/alpha)

\`\`\`text
First notice for the crate
\`\`\`

### MIT License

Used by:
- alpha 1.2.3 (https://example.com/alpha)

\`\`\`text
A second notice for the same crate
\`\`\`
`;
    expect(parseNotices(source).filter((item) => item.name === "alpha")).toHaveLength(2);
  });

  it("gives nothing for text without sections", () => {
    expect(parseNotices("# nothing here")).toEqual([]);
  });
});
