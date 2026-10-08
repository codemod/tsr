#!/usr/bin/env python3
"""r5-typetriage: turn classify.py's lines.tsv/cases.tsv into the cause table.

Each cause slug maps to (owning file/subsystem, claim). The claim names the
Beads issue and lane that already owns the work, or UNCLAIMED. Run after
classify.py with the same TRIAGE_OUT directory:

    python3 report.py > table.md
"""
import collections
import os
import re
import sys

OUT = os.environ.get("TRIAGE_OUT", "/tmp/box/base")

# slug -> (owner file / subsystem, claim)
OWNERS = {
    "gap": ("errorType producers across the checker (the port answers `error`)", ".944/.1038 r5-errorsplit4 (in_progress)"),
    "unaligned-lines": ("`types_producer` walker / case loading (lines not aligned, so not in the dump)", "r5-harness (tsr-2zk.37/.46/.1017/.1041 lane)"),
    "any:contextual-parameter": ("`contextual.rs` contextual parameter typing (getContextuallyTypedParameterType)", "UNCLAIMED (tsr-2zk.14 has no lane)"),
    "any:uncontextual-parameter": ("`symbols.rs`/`contextual.rs` isContextSensitive decision for the container", "UNCLAIMED"),
    "any:property-lookup-error": ("`members.rs` getPropertyOfType on the receiver", "main tsr-2zk.4"),
    "any:any-receiver-propagated": ("downstream of an implicit-any parameter (see any:contextual-parameter)", "UNCLAIMED (follows contextual)"),
    "any:any-initialiser-propagated": ("downstream of an `any` initialiser", "follows the initialiser's producer"),
    "any:call-error": ("`calls.rs` resolveCall / chooseOverload answered error", "main tsr-2zk.9"),
    "any:function-expression-error": ("`contextual.rs`/`function_types.rs` checkFunctionExpressionOrObjectLiteralMethod answered error", "UNCLAIMED (tsr-2zk.14 has no lane)"),
    "any:type-reference-error": ("`declared.rs` getTypeFromTypeNode answered error", "r5-declared (.979/.1010/.1034)"),
    "any:function-declaration-error": ("type of a function/method declaration answered error (`signatures.rs`/`symbols.rs`; any_audit's label says shorthand ambient module, the witnesses are ordinary functions)", "UNCLAIMED"),
    "any:alias-target": ("alias resolution (`resolution.rs`/module exports)", "main tsr-2zk.6; r5-modexports .991/.992"),
    "any:variable-initialiser-error": ("variable whose initialiser answered error (any_audit's label says reportCircularityError; witnesses are ordinary initialisers, so the producer is the initialiser's)", "follows the initialiser's producer"),
    "any:literal-expression-error": ("`objects.rs`/`array_literals.rs` checkObjectLiteral/checkArrayLiteral answered error", "UNCLAIMED"),
    "any:binding-element": ("`destructure.rs`/`binding_patterns.rs` getTypeForBindingElement", "UNCLAIMED (tsr-2zk.10 has no lane)"),
    "any:jsx": ("`jsx_*.rs`", "UNCLAIMED"),
    "any:binary-operand": ("`binary.rs` operand answered error", "UNCLAIMED"),
    "any:unannotated-declaration": ("`symbols.rs` unannotated declaration widening", "main tsr-2zk.11"),
    "any:other": ("mixed", "-"),
    "any:configured": ("configured cases (any_audit skips them): arbitraryModuleNamespaceIdentifiers_module, nodeModulesResolveJsonModule, nodeModulesJson dominate", "r5-modexports .991/.992 for the top three; rest mixed"),
    "import-qualifier-missing": ("symbol-chain printer: `import(\"…\").X` for a symbol not accessible at the print site", "main tsr-2zk.39 (ADR-0044)"),
    "import-qualifier-extra": ("symbol-chain printer: accessible symbol printed through `import(\"…\")`", "main tsr-2zk.39"),
    "import-specifier-differs": ("module specifier generation (`module_specifiers.rs`)", "r5-modules .989/.999"),
    "qualified-name-choice": ("symbol-chain printer (getAccessibleSymbolChain)", "main tsr-2zk.39"),
    "typeof-name-choice": ("symbol-chain printer for `typeof` targets (alias site naming)", "main tsr-2zk.39"),
    "different-name": ("mixed naming: alias re-naming, enum, this, symbol chain", "partly .39 / r5-declared"),
    "alias-name-native-reassigned": ("`declared.rs` alias attachment: native names an instantiated alias by the declaring alias (`type Baz = Omit<…>` prints `Baz`)", "r5-declared"),
    "alias-name-port-reassigned": ("`declared.rs` alias attachment (inverse)", "r5-declared"),
    "alias-port-kept": ("alias attachment / reduction: port keeps an alias name native resolves through", "r5-declared; main tsr-2zk.16.2"),
    "alias-native-kept": ("alias attachment / union origin: native keeps a name the port expands", "r5-declared; main tsr-2zk.16.2"),
    "type-argument-differs": ("instantiation / inference picks different type arguments", "main tsr-2zk.9"),
    "literal-widened": ("literal widening / const contexts / reverse-mapped inference", "main tsr-2zk.11 (widening), tsr-2zk.9 (inference)"),
    "literal-not-widened": ("literal widening", "main tsr-2zk.11"),
    "literal-mixed": ("mixed literal text: union literal order, template text", "-"),
    "string-escape-printing": ("printer escaping (escapeString/escapeNonAsciiString) and name-as-written; scanner for octal/surrogates", "UNCLAIMED (printing.rs part: r5-typetriage)"),
    "undefined-optionality": ("flow narrowing / optional-property `undefined`", "UNCLAIMED (flow.rs is main's)"),
    "optional-param-undefined-not-printed": ("signature printing of an instantiated optional parameter (`?: X | undefined`)", "UNCLAIMED"),
    "optional-property-undefined-printed": ("node reuse of optional property annotations", "UNCLAIMED (node_reuse.rs)"),
    "signature-differs": ("mixed signature shape (return-type inference, parameter printing)", "-"),
    "signature-type-params-dropped": ("signature printing drops the type-parameter list of an instantiated/aliased generic signature", "UNCLAIMED"),
    "duplicate-type-parameters": ("`declared.rs` type-parameter gathering keeps duplicate declarations (native appendIfUnique)", "r5-declared"),
    "binding-pattern-parameter-type": ("binding-pattern implied type for parameters (tsr-2zk.16.47)", "UNCLAIMED (.16.47 has no lane)"),
    "type-param-rename": ("printer typeParameterToName shadow renaming (`T_1`)", "UNCLAIMED (printing.rs: r5-typetriage)"),
    "type-param-constraint": ("`declared.rs` circular type-parameter constraints / constraint printing", "r5-declared"),
    "mapped-type-unresolved": ("`mapped.rs` resolveMappedTypeMembers / print-from-parts", "r5-mapped4 (.1033 family)"),
    "mapped-type-resolved-where-native-deferred": ("`mapped.rs` generic mapped type kept deferred natively", "r5-mapped4"),
    "conditional-type-unresolved": ("`declared.rs` conditional producers", "r5-declared (.1034)"),
    "conditional-type-resolved-where-native-deferred": ("`declared.rs` conditional producers", "r5-declared (.1034)"),
    "module-object-default-shape": ("synthetic default / module object (`{ default: … }`)", "r5-modexports .992"),
    "noinfer-not-substituted": ("NoInfer substitution", "UNCLAIMED"),
    "this-type": ("`this` typing in object literals / methods", "UNCLAIMED"),
    "enum-member-vs-enum": ("enum literal vs enum printing / widening", "UNCLAIMED"),
    "array-sugar": ("printer `Array<T>` vs `T[]` (node reuse of written form)", "UNCLAIMED (node_reuse.rs)"),
    "binding-pattern-print": ("printer of binding-pattern parameter names", "UNCLAIMED"),
    "rest-tuple-params-not-expanded": ("signature printing of rest tuple parameters", "UNCLAIMED"),
    "tuple-vs-array": ("tuple from array binding pattern / array literal context", "claude-cloud-r5-tuples (.16.79 etc., Beads in_progress)"),
    "unknown-where-native-typed": ("inference: no candidates → unknown", "main tsr-2zk.9"),
    "typed-where-native-unknown": ("inference / declared unknown", "main tsr-2zk.9"),
    "never-where-native-typed": ("narrowing / reduction to never", "UNCLAIMED"),
    "typed-where-native-any": ("errorType print (native prints any for errorType)", ".944 r5-errorsplit4"),
    "partial-any": ("an `any` nested in a structured answer (inner producer)", "-"),
    "union-order": ("union member ordering (type id order)", "UNCLAIMED"),
    "union-members-differ": ("union construction / narrowing", "-"),
    "union-missing-members": ("union construction / narrowing", "-"),
    "union-extra-members": ("union construction / subtype reduction", "-"),
    "object-members-differ": ("object member tables (late-bound, quoted, accessor)", "-"),
    "readonly-modifier": ("readonly modifier printing", "-"),
    "parenthesization": ("printer parenthesization", "UNCLAIMED"),
    "spacing": ("printer spacing", "UNCLAIMED"),
    "method-vs-property-form": ("printer method vs property signature form", "UNCLAIMED"),
    "typedarray-buffer-arg": ("TypedArray default type argument", "-"),
    "truncation": ("printer truncation", "-"),
    "other": ("unclassified", "-"),
}


def owner(slug):
    s = slug[3:] if slug.startswith("js:") else slug
    if slug.startswith("js:"):
        return ("JavaScript file: JSDoc / JS checking", "r5-jsdoc3 (.1046), main tsr-2zk.5")
    if s.startswith("gap:"):
        return OWNERS["gap"]
    if s.startswith("any:configured"):
        return OWNERS["any:configured"]
    return OWNERS.get(s, ("?", "?"))


def main():
    lines = collections.defaultdict(list)
    for row in open(os.path.join(OUT, "lines.tsv"), encoding="utf-8"):
        p = row.rstrip("\n").split("\t")
        lines[p[2]].append(p)
    touched = collections.Counter()
    sole = collections.Counter()
    sole_cases = collections.defaultdict(list)
    for row in open(os.path.join(OUT, "cases.tsv"), encoding="utf-8"):
        case, n, causes, s, _dominant = row.rstrip("\n").split("\t")
        for c in causes.split(";"):
            touched[c] += 1
        if s:
            sole[s] += 1
            sole_cases[s].append(case)
    causes = sorted(set(lines) | set(touched), key=lambda c: (-sole[c], -len(lines[c]), c))
    print("| cause | owner | claim | lines | cases touched | cases solely blocked | witness |")
    print("|---|---|---|---:|---:|---:|---|")
    for c in causes:
        own, claim = (x.replace("|", "\\|") for x in owner(c))
        w = ""
        if lines[c]:
            # prefer a witness from a solely-blocked case
            pick = None
            sc = set(sole_cases[c])
            for p in lines[c]:
                if p[0].rsplit(":", 2)[0] in sc:
                    pick = p
                    break
            pick = pick or lines[c][0]
            want = pick[3][:70].replace("|", "\\|")
            got = pick[4][:70].replace("|", "\\|")
            w = f"`{pick[0]}` want `{want}` got `{got}`"
        elif sole_cases[c]:
            w = ", ".join(f"`{x}`" for x in sole_cases[c][:3])
        print(f"| `{c}` | {own} | {claim} | {len(lines[c])} | {touched[c]} | {sole[c]} | {w} |")


if __name__ == "__main__" and "--claims" not in sys.argv:
    main()


def lane(claim):
    for needle, key in [
        ("r5-jsdoc3", "r5-jsdoc3 / main .5 (JS files)"), ("errorsplit4", "r5-errorsplit4 (.944/.1038)"),
        ("UNCLAIMED", "UNCLAIMED"), ("tsr-2zk.39", "main .39 symbol-chain printer"),
        ("r5-modexports", "r5-modexports remainder"), ("r5-harness", "r5-harness"),
        ("r5-declared", "r5-declared"), ("r5-modules", "r5-modules"), ("r5-mapped4", "r5-mapped4"),
        ("r5-tuples", "r5-tuples (Beads)"), ("tsr-2zk.11", "main .11"), ("tsr-2zk.9", "main .9"),
        ("tsr-2zk.4", "main .4"), ("tsr-2zk.6", "main .6"),
    ]:
        if needle in claim:
            return key
    return "mixed / follows another producer"


def by_claim():
    """Second table: cases whose every cause routes to one claim."""
    touched = collections.Counter()
    sole = collections.Counter()
    nlines = collections.Counter()
    for row in open(os.path.join(OUT, "lines.tsv"), encoding="utf-8"):
        p = row.rstrip("\n").split("\t")
        nlines[lane(owner(p[2])[1])] += 1
    for row in open(os.path.join(OUT, "cases.tsv"), encoding="utf-8"):
        case, n, causes, s, _dominant = row.rstrip("\n").split("\t")
        claims = {lane(owner(c)[1]) for c in causes.split(";")}
        for c in claims:
            touched[c] += 1
        if len(claims) == 1:
            sole[next(iter(claims))] += 1
    print()
    print("| lane | lines | cases touched | cases solely blocked |")
    print("|---|---:|---:|---:|")
    for c in sorted(touched, key=lambda c: (-sole[c], -nlines[c])):
        print(f"| {c} | {nlines[c]} | {touched[c]} | {sole[c]} |")


if __name__ == "__main__" and "--claims" in sys.argv:
    by_claim()
