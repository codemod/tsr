//! A narrowed union receiver's TS2339 is certified when its flow walk met no
//! step that may have answered wider than native (r7-flow §49): `instanceof`
//! narrowing an `any` to `C1 | C2` reports, and a type-predicate narrowing
//! this port cannot decide (a memberless mapped predicate) stays silent,
//! where native narrows to `ArrayLike<any>` and reports nothing; a
//! statement call whose no-effects answer is completed (`u.m()`) does not
//! uncertify the receiver. Expected
//! positions are pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb on the
//! same source.

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn narrowed_union_receiver_reports_only_when_certified() {
    let source = r"interface C1 { foo: string; bar1: number }
interface C2 { foo: string; bar2: number }
interface CConstructor { new (): C1; new (x: number): C2 }
declare var C: CConstructor;
declare var obj: any;
if (obj instanceof C) {
    obj.bar1;
}
declare function hasOwnProperty<P extends PropertyKey>(target: {}, property: P): target is { readonly [K in P]: unknown };
declare const arrayLikeOrIterable: ArrayLike<any> | Iterable<any>;
if (hasOwnProperty(arrayLikeOrIterable, 'length')) {
    arrayLikeOrIterable.length;
}
interface I1 { a: string; m(): void }
interface I2 { b: string; m(): void }
declare var u: I1 | I2;
u.m();
u.a;
";
    let mut case = TestCase::parse("probe/union-receiver-certificate", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    let reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    assert_eq!(reported, [(7, 9, 2339), (18, 3, 2339)]);
}

#[test]
fn getter_receiver_alias_certifies_names_without_resolving_its_return() {
    let source = r#"export var basePrototype = {
  get primaryPath() {
    var _this = this;
    return _this.collection.schema.primaryPath;
  }
};
export var present = {
  collection: { schema: { primaryPath: "ok" } },
  get primaryPath() {
    var _this = this;
    return _this.collection.schema.primaryPath;
  }
};
export var typed = {
  label: "known",
  get primaryPath() {
    const alias = this;
    return alias.collection;
  },
  get count(): number { return 3; }
};
"#;
    let mut case = TestCase::parse("probe/getter-receiver-certificate", "probe.ts", source);
    for (name, value) in
        [("target", "es2015"), ("module", "commonjs"), ("strict", "true"), ("declaration", "true")]
    {
        case.options.insert(name.into(), value.into());
    }
    let reported: Vec<_> = diagnostics_suite::rendered_for(&case)
        .into_iter()
        .map(|((_, line, column, code), message)| (line, column, code, message))
        .collect();
    assert_eq!(
        reported,
        [
            (
                4,
                18,
                2339,
                "Property 'collection' does not exist on type '{ readonly primaryPath: any; }'.".into()
            ),
            (
                18,
                18,
                2339,
                "Property 'collection' does not exist on type '{ label: string; readonly primaryPath: any; readonly count: number; }'.".into()
            )
        ]
    );
}

#[test]
fn commonjs_export_receivers_certify_only_the_resolved_values_members() {
    for (name, source, missing, conflict_line) in [
        (
            "forward",
            "exports.other = 1;\nmodule.exports = function value() {};\n",
            Some((1, 9)),
            Some(2),
        ),
        (
            "reverse",
            "module.exports = function value() {};\nexports.other = 1;\n",
            Some((2, 9)),
            Some(1),
        ),
        (
            "dotted",
            "module.exports = function value() {};\nmodule.exports.other = 1;\n",
            Some((2, 16)),
            Some(1),
        ),
        (
            "present",
            "const value = { other: 1 };\nmodule.exports = value;\nexports.other = 2;\n",
            None,
            Some(2),
        ),
        (
            "shadowed",
            "const exports = { other: 1 };\nmodule.exports = function value() {};\nexports.other = 2;\n",
            None,
            Some(2),
        ),
        (
            "expando",
            "function value() {}\nvalue.other = 1;\nmodule.exports = value;\nexports.other = 2;\n",
            None,
            Some(3),
        ),
        ("ordinary", "function value() {}\nvalue.other = 1;\nvalue.other = 2;\n", None, None),
        (
            "global",
            "module.exports = function value() {};\nexports.apply;\nexports.toString;\n",
            None,
            None,
        ),
    ] {
        let mut case =
            TestCase::parse(&format!("probe/commonjs-receiver-{name}"), "probe.js", source);
        for (option, value) in [
            ("allowjs", "true"),
            ("checkjs", "true"),
            ("strict", "true"),
            ("noemit", "true"),
            ("target", "es2022"),
            ("module", "commonjs"),
        ] {
            case.options.insert(option.into(), value.into());
        }
        // Check the complete native output: the resolved value's missing
        // member and the module's export-table conflict are independent.
        let mut reported: Vec<_> = diagnostics_suite::rendered_for(&case)
            .into_iter()
            .map(|((_, line, column, code), message)| (line, column, code, message))
            .collect();
        let mut expected: Vec<_> = missing
            .into_iter()
            .map(|(line, column)| {
                (line, column, 2339, "Property 'other' does not exist on type '() => void'.".into())
            })
            .collect();
        if let Some(line) = conflict_line {
            expected.push((
                line,
                1,
                2309,
                "An export assignment cannot be used in a module with other exported elements."
                    .into(),
            ));
        }
        reported.sort_unstable();
        expected.sort_unstable();
        assert_eq!(reported, expected, "{name}");
    }
}
