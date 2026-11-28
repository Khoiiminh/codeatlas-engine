# Symbol cases (ENG-03)

Each source file here is parsed by `TreeSitterParser`, and `tests/symbols.rs` checks the symbols it reports against `<file>.symbols.json` exactly. The FND-04 fixtures test the same extractor on a realistic repository (`cargo test -p codeatlas-fixture-tests --test symbols`); these cases cover the constructs the fixtures do not.

## What a symbol is

The rules in [`tests/fixtures/README.md`](../../../../../tests/fixtures/README.md) ("Symbols"), plus these for constructs the fixtures do not contain:

- **Namespaces.** `namespace N { … }` (or `module N`) is a `namespace` symbol, and its body is a scope like the module's: `N.f`, `N.C`, `N.C.m`. `namespace A.B { … }` declares `A` and `A.B`; the inner one's range starts at its own name. Namespace members are not module exports. `declare module "x" { … }` and `declare global { … }` describe other scopes and are skipped. Namespaces nested more than 32 levels deep are not extracted (`CA1003`).
- **One symbol per qualified name.** Declarations that share a qualified name in one file are one symbol with several declarations: overloads, a getter and its setter, an interface declared twice, a class or function and a namespace, a class and an interface, a `const` and a `type`. The kind is the most specific one among them (class, enum, function, method, variable, property, namespace, interface, type alias), and the symbol is located at the first declaration of that kind. A static and an instance member with one name are also one symbol.
- **Variables.** Every name a declaration binds, including each name in a destructuring pattern. A plain variable's range is its declarator (`a = 1`); a destructured name's range is its element of the pattern (`second: renamedSecond`, `...others`, `x2 = 2`).
- **Classes.** Methods, properties (including `declare` and `accessor` fields, `#private` members and abstract members), accessors as `property`, and constructor parameters with `public`, `private`, `protected`, `readonly` or `override` as `property`. Not symbols: constructors, index signatures, static blocks, and members named by a string, a number or a computed expression.
- **Interfaces.** Method signatures as `method`, property signatures and accessor signatures as `property`. Call, construct and index signatures are not symbols.
- **Anonymous default exports.** `export default function () {}` and `export default class {}` are named `default`; the name range is the `default` keyword.
- **Exported** means exported from the module: an `export` modifier, a local export list (`export { a, b as c }`, `export type { T }`), `export default name`, `export = name`, or in JavaScript, CommonJS `module.exports = name`, `module.exports = { a, b: c }`, `module.exports.x = name` and `exports.x = name`. Re-exports from other modules (`export { x } from "…"`) mark nothing here.
- **Ranges** cover the whole declaration with its decorators and modifiers (`export`, `declare`, `static`), and a member's trailing `;` or `,`. They do not include a leading comment.

## Conventions

- The extension selects the language as in the scanner.
- A file whose name starts with `broken-` has syntax errors on purpose; every other file must parse cleanly. Symbols in a broken file are whatever the parser recovered, and declarations inside a region it could not place are lost.
- `<file>.symbols.json` lists `symbols` in document order, each with `id`, `name`, `kind`, `parent` (for members), `exported`, and `declarations` in document order, each with its `kind`, `range` and `name` range; `truncated` is `null` unless a limit stopped extraction.
- Ranges are `startLine:startCharacter-endLine:endCharacter`, zero-based, with characters in UTF-16 code units, not counting a leading byte order mark.
- Files are kept byte for byte (`.gitattributes` sets `-text` here) and Prettier ignores this folder, because `crlf-bom.ts` uses CRLF line endings and a byte order mark on purpose.

## What the cases cover

| Case | Checks |
|---|---|
| `classes.ts` | Decorators on classes, members and parameters; static, `#private`, `readonly`, `declare`, `accessor` and optional fields; parameter properties (and a plain parameter that is not one); getter and setter merged; method overloads; skipped index signature, static block and string, number and computed names; abstract members; `override readonly` |
| `interfaces.ts` | `;`, `,` and no separator; optional methods; accessor signatures; skipped call, construct and index signatures and quoted keys; interface merging; class and interface merging in both orders (an interface declared first still gives a `class` located at the class) |
| `exports.ts` | Export lists with renames and `type`; re-exports that mark nothing; `export * as`; `export default name`; exported overloads; object and array destructuring with renames, holes, defaults, rest and nesting; a `const` and a `type` with one name |
| `export-equals.ts` | `export = name`; a class and a namespace merged |
| `default-function.ts`, `default-class.tsx` | Anonymous default exports, and the members of an anonymous class |
| `namespaces.ts` | Exported and nested namespaces, dotted names, `declare namespace`, a function merged with a namespace, skipped `declare module "x"` and `declare global` |
| `commonjs-object.cjs`, `commonjs-properties.js`, `commonjs-single.cjs` | The three CommonJS export forms; `require` results are variables, not exports |
| `components.jsx` | JavaScript classes with static, `#private` fields and methods, getters; generator and async functions; `var` declared twice |
| `ambient.ts` | `declare` functions, variables, classes, enums and abstract classes; `export declare`; `const enum` |
| `top-level-only.ts` | Locals, nested functions and classes, block-scoped `var`, class expressions, object literal members and IIFEs are not symbols |
| `unicode.ts` | Non-ASCII names, and columns in UTF-16 units after emoji on the same line |
| `crlf-bom.ts` | Byte order mark and CRLF line endings |
| `broken-recovery.ts`, `broken-member.ts` | What survives syntax errors around and inside a class |

The limits (32 namespace levels, `ParseLimits::max_symbols`), arbitrary input, deep nesting, minified files and id stability under edits are tested in `tests/symbol_robustness.rs`.

## Independent check

`npm run check:symbol-cases` in `packages/typescript-analyzer` (CI job `typescript-analyzer`) applies the same rules to the TypeScript compiler's syntax tree, asks its type checker what each module exports (`scripts/symbol-rules.ts`), and requires the result to equal every `.symbols.json` exactly: ids, kinds, parents, export flags, ranges and name ranges. The two implementations share no code. `broken-` cases are only checked for having syntax errors, because the two parsers recover differently. `npm run check:fixtures` uses the same rules on the FND-04 fixtures.

## Adding or changing a case

1. Add the source file, named `broken-…` only if it must have syntax errors.
2. Run `CODEATLAS_BLESS=1 cargo test -p codeatlas-parser-treesitter --test symbols` to write its expectation.
3. Read the new `.symbols.json` and check it against the source, then run the tests and `npm run check:symbol-cases` without the variable.
