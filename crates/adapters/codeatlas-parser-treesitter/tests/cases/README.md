# Parser cases (ENG-02)

Each source file here is parsed by `TreeSitterParser`, and `tests/cases.rs` checks the result against `<file>.expected.json` exactly.

## Conventions

- A file whose name starts with `clean-` must parse with no syntax problems. Every other file must have at least one.
- The extension selects the language as in the scanner: `.ts`, `.mts` and `.cts` are TypeScript, `.tsx` is TSX, `.js`, `.mjs` and `.cjs` are JavaScript, `.jsx` is JSX.
- `<file>.expected.json` has `problems` (sorted by position) and `truncated`. Each problem is `{"kind": "unexpected", "range": …}` or `{"kind": "missing", "expected": "<token or node kind>", "range": …}`.
- `range` is `startLine:startCharacter-endLine:endCharacter`, zero-based, with characters counted in UTF-16 code units, as in the contract. Positions do not count a leading byte order mark.
- Files are kept byte for byte (`.gitattributes` sets `-text` here), because some cases use CRLF line endings or a byte order mark on purpose. Prettier ignores this folder.

## What the cases cover

| Case | Checks |
|---|---|
| `clean-typescript.ts` | Enums, generics with defaults, decorators, abstract classes, private `#` fields, overloads, `satisfies`, type-only imports, optional chaining |
| `clean-tsx.tsx` | Generic arrow `<T,>` in TSX, fragments, spread props, nested JSX |
| `clean-jsx-in-js.js`, `clean-component.jsx` | JSX in `.js` (accepted, as bundlers do) and in `.jsx` |
| `clean-commonjs.cjs`, `clean-modern.mjs` | CommonJS; top-level `await`, static blocks, private static fields, regular expressions, template literals |
| `clean-empty.ts`, `clean-bom-crlf.ts` | Empty file; byte order mark with CRLF line endings |
| `missing-brace.ts`, `missing-paren.tsx` | Missing tokens, reported as an empty range where the token was expected |
| `stray-token.ts`, `unterminated-string.js` | Unexpected text, with the valid code around it still parsed |
| `jsx-in-ts.ts` / `clean-jsx-in-tsx.tsx` | The same text: JSX is an error in `.ts` and fine in `.tsx` |
| `types-in-js.js` | Type annotations are an error in JavaScript |
| `utf16-columns.ts` | Columns in UTF-16 units after `é`, `世` and `😀` (26, where bytes would give 31 and code points 25) |
| `bom-error.ts` | Columns after a byte order mark start at 0 |

## Independent check

`npm run check:parser-cases` in `packages/typescript-analyzer` (CI job `typescript-analyzer`) asks the TypeScript compiler whether each case has syntax errors, and fails if it disagrees with the expectation. The two parsers recover differently, so only that yes/no answer is compared, not positions.

## Adding or changing a case

1. Add the source file, named `clean-…` if it must parse cleanly.
2. Run `CODEATLAS_BLESS=1 cargo test -p codeatlas-parser-treesitter --test cases` to write its expectation.
3. Read the new `.expected.json` and check every range against the source by hand, then run the tests and `npm run check:parser-cases` without the variable.
