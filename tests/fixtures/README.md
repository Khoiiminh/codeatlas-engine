# Source fixtures

Deterministic source repositories that analysers are tested against (FND-04). Every folder here except `src`, `tests` and `target` is a fixture, and the checks below run on all of them automatically; adding a project needs no code change (see "Adding a fixture"). Schema examples and the snapshot-port behaviour scenarios live in [`contracts/fixtures`](../../contracts/fixtures/README.md).

| Path | What it is |
|---|---|
| `synchrostream/repo/` | A small synthetic repository shaped like SynchroStream: a NestJS-style backend (TypeScript), a Next.js-style web app (TSX, plus one `.jsx` and one `.js` file) and an SFU (JavaScript: ESM `.js`/`.mjs`, CommonJS `.cjs`, a JSON import). |
| `synchrostream/expected.json` | The golden answer: every file, symbol, relationship and unresolved reference a complete analysis must report, with 1-based line numbers. |
| `monorepo-minimal/` | The smallest monorepo: two workspace packages, each with its own `tsconfig.json`, linked by a path alias, with an `.mts` file. It keeps the multi-project path of the checks tested; ENG-01 grows it into a full monorepo fixture. |
| `expected.schema.json` | JSON Schema for every `expected.json`. |
| `src/`, `tests/` | The `codeatlas-fixture-tests` crate (test tooling, not a product crate). |

The fixture is **synthetic**. It copies no code from the real SynchroStream repositories: it only mirrors their shape, so it stays small, stable and free of credentials. It is not runnable; never install dependencies in it.

## What keeps the golden file honest

| Check | Where | Catches |
|---|---|---|
| Every entry points at real source text; the file inventory is exact; the fixture set as a whole covers the FND-04 exit criteria; every fixture folder has `expected.json` | `cargo test -p codeatlas-fixture-tests` (`tests/source_fixtures.rs`, CI job `rust`) | Wrong line, missing or extra file, a covered case removed |
| The pinned TypeScript compiler agrees: inventory, exact diagnostics, import resolution, the declaration every call, reference and heritage edge reaches, the enclosing symbol, and that the symbol list is exhaustive | `npm run check:fixtures` in `packages/typescript-analyzer` (CI job `typescript-analyzer`) | Wrong call target, wrong caller, missing or extra symbol, wrong symbol kind |

Both checks fail on planted mistakes; they were tried when FND-04 was written (§5.1 of the Traceable Implementation Plan).

## Conventions in `expected.json`

These are the rules future analysers (ENG-01 to ENG-05) must follow to match the golden file. They are written here once; change them only together with the fixture, and increase `revision`.

**Ids.** `file:<path>`, `symbol:<path>#<qualifiedName>` and `external:<specifier>`, with `<path>` relative to the repository root and `/` separators. A member's qualified name is `Owner.member`.

**Lines.** 1-based, as an editor shows them. The canonical contract uses zero-based positions, so convert by subtracting 1. `text` is source text that appears on that line.

**Files.** Every `.ts`, `.mts`, `.cts`, `.tsx`, `.js`, `.mjs`, `.cjs` and `.jsx` file, except declaration files (`.d.ts`, `.d.mts`, `.d.cts`) and anything under `ignoredPaths` (in SynchroStream, `node_modules/` and the build output `backend/dist/`). An ignored folder must exist and contain source-like files, so scanners are really tested against it (ENG-01). Other files, such as `defaults.json`, are not source files. `.mts` and `.cts` count as `typescript`, `.mjs` and `.cjs` as `javascript`.

**Symbols** (exhaustive):
- module-scope declarations: functions, classes, interfaces, type aliases, enums, variables (`const`/`let`/`var`), and `export * as Name` namespaces;
- class members: methods, properties, accessors (as `property`) and constructor parameter properties (as `property`);
- interface members: method signatures (as `method`) and property signatures (as `property`).

Not symbols: constructors, enum members, locals, parameters, members of object literals and of anonymous type literals, and declarations inside `node_modules`. `exported` means exported from the module (ESM `export`, or CommonJS `module.exports`). Members are never module exports.

**Edges** (exhaustive):
- `imports`: from the importing file to the module it names, including `import type` (marked `typeOnly`) and CommonJS `require("…")`. The target is the module actually named (a barrel `index.ts` stays the target), or `external:<specifier>` for packages and `node:` built-ins. The implicit JSX runtime import is not recorded.
- `exports`: re-exports only (`export … from`), from the barrel to the re-exported file.
- `calls`: a call or `new` whose callee resolves to a project symbol, from the innermost enclosing symbol (or the file for top-level statements). `new X()` targets class `X`; `super(…)` targets the base class. Calls on built-ins, external packages, locals, parameters and object-literal members are not recorded. Calls through an interface target the interface method, not an implementation.
- `references`: a project symbol used as a value without being called (passed as an argument, listed in decorator metadata, read as `Enum.Member`), and JSX elements (`<Component />`, `<Ns.Component />`). The target is the final declaration, after following import aliases, barrels and namespace re-exports. Type annotations are not references.
- `extends`, `implements`: between project symbols only (extending a built-in such as `Error` is not recorded).
- Decorators belong to the declaration they decorate, so a reference inside `@Inject(TOKEN)` on a constructor parameter property comes from that property.

**Unresolved** (exhaustive), with the contract's `UnresolvedReason`:
- `not_found`: a missing module, an undeclared global, or a call through an import whose module is missing;
- `ambiguous`: a name exported by two `export *` sources (TypeScript reports TS2308 and silently picks one);
- `dynamic`: an import or `require` whose specifier is computed at run time; `specifier` is the expression text;
- `unsupported`: an import of a non-source module (here, a JSON module).

**TypeScript diagnostics.** Exactly the diagnostics TypeScript 6.0.3 reports for the fixture's TypeScript projects (by default `<root>/tsconfig.json`; see `typescriptProjects` below); each corresponds to a deliberate unresolved reference.

## Adding a fixture

Add a fixture for each project shape the analysers must handle (a monorepo, a plain JavaScript service, a framework with unusual conventions). Small, focused fixtures are better than one large one: coverage is required of the set as a whole, not of each fixture.

1. Create `tests/fixtures/<name>/repo/` with the project's source. Write it yourself or reduce a public example; never copy private code or anything with credentials. Use LF line endings and 4-space indentation.
2. Make it type-check with TypeScript 6.0.3:
   - a single project: a `repo/tsconfig.json`;
   - several projects (a monorepo): list each `tsconfig.json` in `typescriptProjects`, relative to the fixture folder (see `monorepo-minimal`);
   - a plain JavaScript project with no tsconfig: put a checker-only `checker.tsconfig.json` next to `expected.json`, with `allowJs` and `"include": ["repo"]`, and list it in `typescriptProjects`. That keeps the analysed repository unchanged.
3. Write `tests/fixtures/<name>/expected.json` following the conventions above, with `"fixture": "<name>"` and `"revision": 1`.
4. If the project keeps dependency or build folders (`node_modules/`, `dist/`), list them in `ignoredPaths` and add `!/tests/fixtures/<name>/repo/<folder>/` lines to the root `.gitignore`, so the stand-ins are committed.
5. Run both checks below. Both discover the new fixture automatically.

## Changing a fixture

1. Edit the source under `<name>/repo/`, keeping LF line endings (`.gitattributes` enforces them; Prettier ignores this folder because the line numbers are golden).
2. Update `<name>/expected.json` and increase `revision`.
3. Run `cargo test -p codeatlas-fixture-tests` and `npm run check:fixtures` in `packages/typescript-analyzer`. Both must pass.
4. If you changed SynchroStream lines that a seed snapshot in `contracts/fixtures/ports/seeds` was built from, update the seed too: seed positions and content hashes come from that fixture.
