//! Const initializer values compared with pinned tsgo declaration output.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn template_folding_follows_const_symbols_and_initializers() {
    let source = r#"// @strict: true
// @target: es2020
const a = "1";
const b = a + " 2";
export const chained = `${b} 3`;
export const chainedConst = `${b} 3` as const;
const arithmetic = (1 + 2) * 3;
export const arithmeticTemplate = `value:${arithmetic}`;
const annotated: string = "fixed";
export const annotatedTemplate = `${annotated}`;
let mutable = "fixed";
export const mutableTemplate = `${mutable}`;
const literalAnnotated: "fixed" = "fixed";
export const literalAnnotatedTemplate = `${literalAnnotated}`;
namespace Values { export const prefix = "a" + "b"; }
export const namespaceTemplate = `${Values.prefix}c`;
export function shadow() { const b = "inner" + " value"; return `${b}!`; }
export function deferred() { return `${later}`; }
const later = "late" + " value";
const object = {field:"value"};
export const propertyTemplate = `${object.field}`;
"#;
    let case = TestCase::parse("probe/constant-variables", "constant-variables.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "chained : \"1 2 3\"",
        "chainedConst : \"1 2 3\"",
        "arithmeticTemplate : \"value:9\"",
        "annotatedTemplate : string",
        "mutableTemplate : string",
        "literalAnnotatedTemplate : string",
        "namespaceTemplate : \"abc\"",
        "shadow : () => string",
        "deferred : () => string",
        "propertyTemplate : string",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn template_special_numbers_require_global_symbol_identity() {
    // Pinned tsgo 5b1047d TestLocal/templateSpecialNumbersProbe.types.
    // The shadowed cases distinguish symbol identity from spelling/type.
    let source = r#"// @strict: true
// @target: es2020
export const special = `${Infinity}:${-Infinity}:${NaN}:${Infinity - Infinity}:${NaN | 5}`;
const infinite = 1 / 0;
export const indirect = `${infinite}:${infinite - infinite}`;
function localConstants() {
    const Infinity = 13 * 2;
    const NaN = "finite" + " text";
    const localSpecial = `${Infinity}:${NaN}`;
}
function annotations() {
    const Infinity: number = 26;
    const NaN: "text" = "text";
    const annotatedSpecial = `${Infinity}:${NaN}`;
}
function mutable() {
    let Infinity = 26;
    const mutableSpecial = `${Infinity}`;
}
function ordering() {
    const beforeSpecial = `${Infinity}`;
    const Infinity = 13 * 2;
    const afterSpecial = `${Infinity}`;
}
function cycles() {
    const Infinity = NaN + 1;
    const NaN = Infinity + 2;
    const cyclicSpecial = `${Infinity}:${NaN}`;
}
namespace Numbers { export const Infinity = 5 + 4; export const NaN = "not" + " special"; }
export const qualifiedSpecial = `${Numbers.Infinity}:${Numbers.NaN}`;
const object = {Infinity: 9, NaN: "not special"};
export const objectSpecial = `${object.Infinity}:${object.NaN}`;
export const assertedSpecial = `${Infinity as number}`;
declare function tag(parts: TemplateStringsArray, value: number): string;
export const taggedSpecial = tag`${Infinity}`;
"#;
    let case = TestCase::parse("probe/special-numbers", "special-numbers.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "special : \"Infinity:-Infinity:NaN:NaN:5\"",
        "indirect : \"Infinity:NaN\"",
        "localSpecial : \"26:finite text\"",
        "annotatedSpecial : string",
        "mutableSpecial : string",
        "beforeSpecial : string",
        "afterSpecial : \"26\"",
        "cyclicSpecial : string",
        "qualifiedSpecial : \"9:not special\"",
        "objectSpecial : string",
        "assertedSpecial : string",
        "taggedSpecial : string",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn template_import_aliases_evaluate_terminal_const_initializers() {
    // Acyclic values pinned by tsgo 5b1047d templateImportedConstantsProbe.types.
    // Imported special spellings are ordinary values, not global numbers.
    // Written annotations/mutability must still decline. The cross-file cycle
    // additionally pins the existing active-declaration guard: pinned native
    // overflows its evaluator stack on that control, so it is not a parity pin.
    let source = r#"// @strict: true
// @target: es2020
// @module: commonjs
// @filename: values.ts
export const Infinity = 7 + 2;
export const NaN = "not" + " a number";
export const text = "im" + "ported";
export const annotated: string = "hidden";
export const literalAnnotated: "hidden" = "hidden";
export let mutable = "hidden";
export const asserted = "hidden" as const;
// @filename: barrel.ts
export { Infinity as finite, NaN as message, text } from "./values";
// @filename: a.ts
import { second } from "./b";
export const first = second + "a";
// @filename: b.ts
import { first } from "./a";
export const second = first + "b";
// @filename: main.ts
import { Infinity, NaN, annotated, literalAnnotated, mutable, asserted } from "./values";
import { finite as renamed, message, text } from "./barrel";
import * as values from "./values";
import * as barrel from "./barrel";
import { first } from "./a";
export const importedSpecial = `${Infinity}:${NaN}`;
export const renamedSpecial = `${renamed}:${message}`;
export const importedText = `${text}!`;
export const namespaceSpecial = `${values.Infinity}:${values.NaN}`;
export const namespaceReexport = `${barrel.finite}:${barrel.message}`;
export const importedAnnotation = `${annotated}`;
export const importedLiteralAnnotation = `${literalAnnotated}`;
export const importedMutable = `${mutable}`;
export const importedAssertion = `${asserted}`;
export const cycleTemplate = `${first}`;
"#;
    let case = TestCase::parse("probe/imported-constants", "imported-constants.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "importedSpecial : \"9:not a number\"",
        "renamedSpecial : \"9:not a number\"",
        "importedText : \"imported!\"",
        "namespaceSpecial : \"9:not a number\"",
        "namespaceReexport : \"9:not a number\"",
        "importedAnnotation : string",
        "importedLiteralAnnotation : string",
        "importedMutable : string",
        "importedAssertion : string",
        "cycleTemplate : string", // Guard pin; native stack overflow, not a type oracle.
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn template_values_decline_forward_cycles_annotations_mutable_and_tagged_sources() {
    let source = r#"// @strict: true
// @target: es2020
export const before = `${after}`;
const after = "late" + " value";
const self = `${self}`;
export const selfTemplate = `${self}`;
const first = second + "a";
const second = first + "b";
export const cycleTemplate = `${first}`;
let literalMutable: "fixed" = "fixed";
export const mutableLiteralTemplate = `${literalMutable}`;
declare function tag(parts:TemplateStringsArray,value:string):string;
const text = "a" + "b";
export const tagged = tag`${text}`;
let t1 = "foo" as const;
let t2 = "bar" as const;
export const nestedConst = `${`(${t1})`}-${`(${t2})`}` as const;
"#;
    let case = TestCase::parse("probe/constant-guards", "constant-guards.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "before : string",
        "selfTemplate : string",
        "cycleTemplate : string",
        "mutableLiteralTemplate : string",
        "tagged : string",
        "nestedConst : \"(foo)-(bar)\"",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
