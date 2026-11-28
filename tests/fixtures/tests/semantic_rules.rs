//! ENG-04 rules that the FND-04 fixtures do not contain, end to end (scan, parse, resolve,
//! link). Each case is a small repository; its relationships are listed exhaustively, as
//! `kind from -> to @line`, with `symbol:` dropped from ids. The conventions are in
//! `tests/fixtures/README.md` and `packages/typescript-analyzer/src/analysis/references.ts`.

mod semantic_support;

use std::fs;

use codeatlas_core::RelationTarget;
use semantic_support::{analyse, analyzer};

/// Every relationship of a repository made of `files`, sorted; `None` if the analyzer cannot
/// run here (outside CI).
fn relationships_of(files: &[(&str, &str)]) -> Option<Vec<String>> {
    let resolver = analyzer()?;
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }
    let report = analyse(dir.path(), &resolver);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let short = |id: String| id.trim_start_matches("symbol:").to_string();
    let mut out: Vec<String> = report
        .relationships
        .iter()
        .map(|r| {
            let line = r.location.range.start().line + 1;
            let target = match &r.target {
                RelationTarget::Resolved(to) => short(to.id()),
                RelationTarget::Unresolved { specifier, reason } => {
                    format!("? {specifier} ({reason})")
                }
            };
            let type_only = if r.type_only { " (type only)" } else { "" };
            format!(
                "{} {} -> {target} @{line}{type_only}",
                r.kind,
                short(r.from.id())
            )
        })
        .collect();
    out.sort();
    Some(out)
}

fn assert_relationships(files: &[(&str, &str)], expected: &[&str]) {
    let Some(got) = relationships_of(files) else {
        return;
    };
    let mut want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn member_reads_and_export_lists_are_not_references() {
    assert_relationships(
        &[(
            "a.ts",
            r#"export const LIMIT = 3;
export class Box {
    size = 1;
    static make() { return new Box(); }
}
export function useBox(b: Box) {
    return b.size + LIMIT + Box.make().size;
}
export default useBox;
export { LIMIT as MAX };
"#,
        )],
        &[
            "calls a.ts#Box.make -> a.ts#Box @4",
            "references a.ts#useBox -> a.ts#LIMIT @7",
            "calls a.ts#useBox -> a.ts#Box.make @7",
        ],
    );
}

#[test]
fn qualified_names_reach_namespace_members_and_enum_reads_reach_the_enum() {
    assert_relationships(
        &[
            (
                "shapes.ts",
                r#"export namespace Geometry {
    export function area(r: number) { return r * r; }
    export const PI = 3.14;
}
export enum Color { Red, Green }
"#,
            ),
            (
                "use.ts",
                r#"import * as shapes from "./shapes";
import { Geometry, Color } from "./shapes";
export function paint() {
    shapes.Geometry.area(2);
    const c = Color.Red;
    return Geometry.PI + c;
}
"#,
            ),
        ],
        &[
            "imports file:use.ts -> file:shapes.ts @1",
            "imports file:use.ts -> file:shapes.ts @2",
            "calls use.ts#paint -> shapes.ts#Geometry.area @4",
            "references use.ts#paint -> shapes.ts#Color @5",
            "references use.ts#paint -> shapes.ts#Geometry.PI @6",
        ],
    );
}

#[test]
fn every_kind_of_module_target() {
    assert_relationships(
        &[
            ("types.ts", "export type T = number;\n"),
            ("side.ts", "console.log(1);\n"),
            ("data.json", "{ \"a\": 1 }\n"),
            ("legacy.js", "export function helper() {}\n"),
            ("legacy.d.ts", "export declare function helper(): void;\n"),
            ("utils.ts", "export const u = 1;\n"),
            ("lazy.ts", "export const l = 1;\n"),
            (
                "a.ts",
                r#"import type { T } from "./types";
import "./side";
import data from "./data.json";
import { x } from "./missing";
import React, { useState } from "react";
import fs from "node:fs";
import { y } from "@/missing-alias";
import { helper } from "./legacy";
export * as utils from "./utils";
export const lazy = () => import("./lazy");
export const page = (n: string) => import(`./pages/${n}`);
export type U = typeof import("./types");
x(); helper(); React.createElement("div"); fs.readFileSync(data); useState(0);
"#,
            ),
        ],
        &[
            "imports file:a.ts -> file:types.ts @1 (type only)",
            "imports file:a.ts -> file:side.ts @2",
            "imports file:a.ts -> ? ./data.json (unsupported) @3",
            "imports file:a.ts -> ? ./missing (not_found) @4",
            "imports file:a.ts -> external:react @5",
            "imports file:a.ts -> external:node:fs @6",
            "imports file:a.ts -> ? @/missing-alias (not_found) @7",
            // The declaration file stands for its JavaScript source. A call reaches the
            // declaration file, which is not scanned, so `helper()` is not a relationship.
            "imports file:a.ts -> file:legacy.js @8",
            "exports file:a.ts -> file:utils.ts @9",
            "imports file:a.ts -> file:lazy.ts @10",
            "imports file:a.ts -> ? `./pages/${n}` (dynamic) @11",
            "imports file:a.ts -> file:types.ts @12 (type only)",
            // A name imported from a missing module is not found; names from packages and
            // JSON modules are not project symbols.
            "calls file:a.ts -> ? x (not_found) @13",
        ],
    );
}

#[test]
fn commonjs_exports_reach_their_functions() {
    assert_relationships(
        &[
            (
                "lib.cjs",
                r#"function parse(t) { return t; }
function format(v) { return String(v); }
exports.parse = parse;
module.exports.format = format;
"#,
            ),
            (
                "single.cjs",
                "function start() {}\nmodule.exports = start;\n",
            ),
            (
                "use.mjs",
                r#"import { parse, format } from "./lib.cjs";
import start from "./single.cjs";
parse("x");
format(1);
start();
"#,
            ),
        ],
        &[
            "imports file:use.mjs -> file:lib.cjs @1",
            "imports file:use.mjs -> file:single.cjs @2",
            "calls file:use.mjs -> lib.cjs#parse @3",
            "calls file:use.mjs -> lib.cjs#format @4",
            "calls file:use.mjs -> single.cjs#start @5",
        ],
    );
}

#[test]
fn export_star_ambiguity_needs_two_different_declarations() {
    assert_relationships(
        &[
            ("a.ts", "export function same() {}\n"),
            (
                "b.ts",
                "export function same() {}\nexport function other() {}\n",
            ),
            ("c.ts", "export { same } from \"./a\";\n"),
            (
                "both.ts",
                "export * from \"./a\";\nexport * from \"./b\";\n",
            ),
            (
                "diamond.ts",
                "export * from \"./a\";\nexport * from \"./c\";\n",
            ),
            (
                "local.ts",
                "export * from \"./a\";\nexport * from \"./b\";\nexport function same() {}\n",
            ),
            (
                "use.ts",
                r#"import { same as s1, other } from "./both";
import { same as s2 } from "./diamond";
import { same as s3 } from "./local";
import * as all from "./both";
s1(); other(); s2(); s3(); all.same(); all.other();
"#,
            ),
        ],
        &[
            "exports file:both.ts -> file:a.ts @1",
            "exports file:both.ts -> file:b.ts @2",
            "exports file:c.ts -> file:a.ts @1",
            "exports file:diamond.ts -> file:a.ts @1",
            "exports file:diamond.ts -> file:c.ts @2",
            "exports file:local.ts -> file:a.ts @1",
            "exports file:local.ts -> file:b.ts @2",
            "imports file:use.ts -> file:both.ts @1",
            "imports file:use.ts -> file:diamond.ts @2",
            "imports file:use.ts -> file:local.ts @3",
            "imports file:use.ts -> file:both.ts @4",
            "calls file:use.ts -> ? s1 (ambiguous) @5",
            "calls file:use.ts -> b.ts#other @5",
            // Both `export *` sources lead to the same declaration: not ambiguous.
            "calls file:use.ts -> a.ts#same @5",
            // A module's own export wins over `export *`.
            "calls file:use.ts -> local.ts#same @5",
            // The same rule through a namespace import.
            "calls file:use.ts -> ? all.same (ambiguous) @5",
            "calls file:use.ts -> b.ts#other @5",
        ],
    );
}

#[test]
fn decorators_mixins_and_super_members() {
    assert_relationships(
        &[
            (
                "base.ts",
                r#"export class Base { greet() { return "hi"; } }
export interface Named { name: string }
export function mixin<T>(B: T): T { return B; }
export function track(target: unknown, context?: unknown) {}
export const TOKEN = "token";
"#,
            ),
            (
                "impl.ts",
                r#"import { Base, type Named, mixin, track, TOKEN } from "./base";
@track
export class Impl extends Base implements Named {
    name = TOKEN;
    hello() { return super.greet(); }
}
export class Mixed extends mixin(Base) {}
"#,
            ),
        ],
        &[
            "imports file:impl.ts -> file:base.ts @1",
            "calls impl.ts#Impl -> base.ts#track @2",
            "extends impl.ts#Impl -> base.ts#Base @3",
            "implements impl.ts#Impl -> base.ts#Named @3",
            "references impl.ts#Impl.name -> base.ts#TOKEN @4",
            "calls impl.ts#Impl.hello -> base.ts#Base.greet @5",
            // A computed base class is not a heritage edge; the call and its argument are.
            "calls impl.ts#Mixed -> base.ts#mixin @7",
            "references impl.ts#Mixed -> base.ts#Base @7",
        ],
    );
}

#[test]
fn shorthand_properties_instantiations_and_undeclared_names() {
    assert_relationships(
        &[
            (
                "a.ts",
                "export function handler() {}\nexport function make<T>(): T[] { return []; }\n",
            ),
            (
                "b.ts",
                r#"import { handler, make } from "./a";
export const routes = { handler };
export const factory = make<string>;
export function run() {
    missingFn();
    return missingValue;
}
export function go() { return routes.handler(); }
"#,
            ),
        ],
        &[
            "imports file:b.ts -> file:a.ts @1",
            "references b.ts#routes -> a.ts#handler @2",
            "references b.ts#factory -> a.ts#make @3",
            "calls b.ts#run -> ? missingFn (not_found) @5",
            "references b.ts#run -> ? missingValue (not_found) @6",
            // A call through a shorthand member reaches the function it names.
            "calls b.ts#go -> a.ts#handler @8",
        ],
    );
}
