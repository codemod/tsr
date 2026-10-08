#!/usr/bin/env python3
"""r5-typetriage: classify every non-RIGHT checker_types line by root cause.

Inputs (all produced by the box protocol at the integration head):
  typesx.tsv  TSR_VERDICT_EXPR=1 verdictdump (key, verdict, want, got, expr)
  anylost.tsv target/any_lost_lines.tsv from `TSR_ANY_DUMP=1 any_audit`
              (plain cases only: the minting rule of each printed `any`)
  ctlist.tsv  `casequery -- checker_types --list` (plain per-case verdicts)

Outputs:
  lines.tsv   key, verdict, cause, want, got, expr
  cases.tsv   case, failing lines, causes (sorted, `;`-joined), sole cause,
              dominant cause (most lines)
  stdout      the cause table (lines, cases touched, cases solely blocked)

Every rule below is a string comparison on (want, got) plus, for `any`
lines, the producer the any_audit probe names. The rule ORDER is the
classification: the first rule that fires owns the line. Stdlib only.
"""
import collections
import os
import re
import sys

BASE = os.environ.get("TRIAGE_BASE", "/tmp/box/base")
REF = "vendor/typescript-go/testdata/baselines/reference/submodule"

# ---------------------------------------------------------------- helpers

LIT_STR = re.compile(r'"(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)*\'|`(?:[^`\\]|\\.)*`')
LIT_NUM = re.compile(r'(?<![\w.$])-?\d+(?:\.\d+)?(?:e[+-]?\d+)?n?(?![\w$])')
WORD = re.compile(r"[A-Za-z_$][\w$]*")


def widen(t):
    t = LIT_STR.sub("string", t)
    t = LIT_NUM.sub("number", t)
    t = re.sub(r"\b(true|false)\b", "boolean", t)
    t = t.replace("unique symbol", "symbol")
    return t


def split_top(t, sep):
    """Split on `sep` (a single char) at nesting depth 0."""
    out, depth, cur, i = [], 0, [], 0
    in_str = None
    while i < len(t):
        c = t[i]
        if in_str:
            cur.append(c)
            if c == "\\" and i + 1 < len(t):
                cur.append(t[i + 1])
                i += 2
                continue
            if c == in_str:
                in_str = None
        elif c in "\"'`":
            in_str = c
            cur.append(c)
        elif c in "([{<":
            depth += 1
            cur.append(c)
        elif c in ")]}>":
            # `=>` is not a closer
            if c == ">" and i > 0 and t[i - 1] == "=":
                cur.append(c)
            else:
                depth -= 1
                cur.append(c)
        elif c == sep and depth == 0:
            out.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
        i += 1
    out.append("".join(cur).strip())
    return out


def members(t):
    return split_top(t, "|")


def norm_union(t):
    # recursively sort union members inside every bracket level, cheaply:
    # only the top level is sorted, which is where the printer orders.
    return "|".join(sorted(m for m in members(t)))


def strip_parens(t):
    return t.replace("(", "").replace(")", "")


def strip_qual(t):
    # `a.b.C` -> `C`; keeps `import("x").C` apart by first dropping import().
    t = re.sub(r'import\("[^"]*"\)\.?', "", t)
    return re.sub(r"\b(?:[A-Za-z_$][\w$]*\.)+(?=[A-Za-z_$])", "", t)


def tokens(t):
    return set(WORD.findall(t))


IDENT_REF = re.compile(r"^(?:typeof )?[A-Za-z_$][\w$.]*(?:<.*>)?(?:\[\])?$")


def is_ref(t):
    """A bare (possibly generic) reference: `Foo`, `a.Foo<T>`, `typeof x`."""
    return bool(IDENT_REF.match(t)) and not t.startswith("{")


def t_eq(a, b):
    return a == b


def has_word(t, w):
    return re.search(r"(?<![\w$.])" + re.escape(w) + r"(?![\w$])", t) is not None


# ------------------------------------------------------------ rule table

def classify(want, got, expr, ctx):
    """Return the cause slug for one non-RIGHT line."""
    if got == "error":
        return "gap:" + expr_shape(expr)
    if "... " in want and ("more ..." in want or "..." in want and len(want) > 300):
        return "truncation"
    if want.endswith("...") or " more ...;" in want or "more ...>" in want:
        return "truncation"
    if got == "any":
        prod = ctx.get("any_producer")
        if prod is None:
            return "any:configured:" + expr_shape(expr)
        return any_family(prod)
    if want == "any":
        return "typed-where-native-any"
    # string-literal escaping in the printer
    if "\\u" in want and "\\u" not in got:
        return "string-escape-printing"
    # signature shape families
    if re.match(r"^<([\w$]+)(, \1)+>", got) or re.search(r"<([\w$]+), \1>\(", got):
        return "duplicate-type-parameters"
    sig_tp = re.compile(r"^(new )?<[^()]*>\(")
    if sig_tp.match(want) and not sig_tp.match(got) and got.startswith(("(", "new (")):
        return "signature-type-params-dropped"
    if re.search(r"\(\.\.\.\[", got) or (re.search(r"\]\??: any\)", got) and re.search(r"\]\??: \[", want)):
        return "binding-pattern-parameter-type"
    # module / import printing
    if ("import(" in want) != ("import(" in got) or (
        "import(" in want and re.findall(r'import\("[^"]*"\)', want) != re.findall(r'import\("[^"]*"\)', got)
    ):
        iw = re.findall(r'import\("([^"]*)"\)', want)
        ig = re.findall(r'import\("([^"]*)"\)', got)
        if strip_qual(want) == strip_qual(got) or (iw and ig):
            if iw and ig and len(iw) == len(ig):
                return "import-specifier-differs"
            if len(iw) > len(ig):
                return "import-qualifier-missing"
            return "import-qualifier-extra"
    # structural families visible in the printed text
    if re.search(r"\[\w+ in ", got) and not re.search(r"\[\w+ in ", want):
        return "mapped-type-unresolved"
    if re.search(r"\[\w+ in ", want) and not re.search(r"\[\w+ in ", got):
        return "mapped-type-resolved-where-native-deferred"
    if want.startswith("{ default: ") and not got.startswith("{ default: "):
        return "module-object-default-shape"
    if got.startswith("{ default: ") and not want.startswith("{ default: "):
        return "module-object-default-shape"
    if "NoInfer<" in got and "NoInfer<" not in want:
        return "noinfer-not-substituted"
    if re.search(r"\bextends\b.*\?", got) and not re.search(r"\bextends\b.*\?", want) and not got.startswith("<"):
        return "conditional-type-unresolved"
    if re.search(r"\bextends\b.*\?", want) and not re.search(r"\bextends\b.*\?", got) and not want.startswith("<"):
        return "conditional-type-resolved-where-native-deferred"
    if has_word(want, "this") != has_word(got, "this"):
        return "this-type"
    # ordering / layout
    if norm_union(want) == norm_union(got):
        return "union-order"
    if strip_parens(want) == strip_parens(got):
        return "parenthesization"
    tok = re.compile(r'"(?:[^"\\]|\\.)*"|[\w$]+|\S')
    if len(want) == len(got) and sorted(tok.findall(want)) == sorted(tok.findall(got)) and "|" in want:
        return "union-order"
    if want.replace(" ", "") == got.replace(" ", ""):
        return "spacing"
    # qualified-name / symbol-chain choice
    if strip_qual(want) == strip_qual(got):
        return "qualified-name-choice"
    # TypedArray<ArrayBuffer> default type arguments
    if re.sub(r"<ArrayBufferLike>|<ArrayBuffer>", "", got) == re.sub(r"<ArrayBufferLike>|<ArrayBuffer>", "", want):
        return "typedarray-buffer-arg"
    # type-parameter renaming (T_1)
    if re.sub(r"_\d+\b", "", want) == re.sub(r"_\d+\b", "", got):
        return "type-param-rename"
    # enum literal vs enum
    if is_ref(want) and is_ref(got) and (re.sub(r"\.[\w$]+$", "", got) == want or re.sub(r"\.[\w$]+$", "", want) == got):
        return "enum-member-vs-enum"
    # Array<T> vs T[]
    def arr(t):
        prev = None
        while prev != t:
            prev = t
            t = re.sub(r"\bArray<([\w$.]+)>", r"\1[]", t)
            t = re.sub(r"\bReadonlyArray<([\w$.]+)>", r"readonly \1[]", t)
        return t
    if arr(want) == arr(got):
        return "array-sugar"
    # binding-pattern / parameter-list printing
    if re.sub(r",\s*}", " }", want) == re.sub(r",\s*}", " }", got):
        return "binding-pattern-print"
    if re.search(r"\(\.\.\.[\w$]+: \[", got) and not re.search(r"\(\.\.\.[\w$]+: \[", want):
        return "rest-tuple-params-not-expanded"
    # type-parameter constraints
    def no_constraints(t):
        return re.sub(r"\s+extends\s+[^,<>]+(?=[,>])", "", t)
    if t_eq(no_constraints(want), no_constraints(got)):
        return "type-param-constraint"
    # literal widening / missing widening
    if widen(want) == widen(got):
        if widen(got) == got and widen(want) != want:
            return "literal-widened"  # native kept a literal, port widened
        if widen(want) == want and widen(got) != got:
            return "literal-not-widened"  # port kept a literal native widened
        return "literal-mixed"
    # `| undefined` / optionality
    def no_undef(t):
        return re.sub(r"\s*\|\s*undefined\b|\bundefined\s*\|\s*", "", t).replace("?:", ":")
    if no_undef(want) == no_undef(got) or strip_parens(no_undef(want)) == strip_parens(no_undef(got)):
        if re.search(r"\?: \(*[^;]*\| undefined", want) and "=>" in want and not re.search(r"\?: \(*[^;]*\| undefined", got):
            return "optional-param-undefined-not-printed"
        if "?:" in got and "| undefined" in got and "| undefined" not in want:
            return "optional-property-undefined-printed"
        return "undefined-optionality"
    def no_null(t):
        return re.sub(r"\s*\|\s*null\b|\bnull\s*\|\s*", "", t)
    if no_null(no_undef(want)) == no_null(no_undef(got)):
        return "undefined-optionality"
    if strip_parens(no_undef(want)) == strip_parens(no_undef(got)):
        return "undefined-optionality"
    # method vs property signature form
    if re.sub(r"(\w+)\((.*?)\): ", r"\1: (\2) => ", want) == got or re.sub(
        r"(\w+)\((.*?)\): ", r"\1: (\2) => ", got) == want:
        return "method-vs-property-form"
    # readonly
    if want.replace("readonly ", "") == got.replace("readonly ", ""):
        return "readonly-modifier"
    # `any` inside an otherwise-structured answer
    if has_word(got, "any") and not has_word(want, "any"):
        return "partial-any"
    if has_word(got, "unknown") and not has_word(want, "unknown"):
        return "unknown-where-native-typed"
    if has_word(want, "unknown") and not has_word(got, "unknown"):
        return "typed-where-native-unknown"
    if has_word(got, "never") and not has_word(want, "never"):
        return "never-where-native-typed"
    # tuple vs array
    if ("[" in want and "]" in want and not want.endswith("[]")) and got.endswith("[]"):
        if want.startswith("[") or want.startswith("readonly ["):
            return "tuple-vs-array"
    # alias kept vs structure printed
    if is_ref(want) and not is_ref(got):
        return "alias-native-kept"  # native printed a name, port printed structure
    if is_ref(got) and not is_ref(want):
        return "alias-port-kept"  # port printed a name, native printed structure
    if is_ref(want) and is_ref(got):
        if want.startswith("typeof ") and got.startswith("typeof "):
            return "typeof-name-choice"
        if "<" not in want and "<" in got:
            return "alias-name-native-reassigned"  # native prints the declaring alias
        if "<" in want and "<" not in got:
            return "alias-name-port-reassigned"
        if want.split("<")[0] != got.split("<")[0]:
            return "different-name"
        return "type-argument-differs"
    # union arity differs (narrowing / union members)
    mw, mg = members(want), members(got)
    if len(mw) > 1 or len(mg) > 1:
        sw, sg = set(mw), set(mg)
        if sg < sw:
            return "union-missing-members"
        if sw < sg:
            return "union-extra-members"
        return "union-members-differ"
    if want.startswith("(") or "=>" in want:
        return "signature-differs"
    if want.startswith("{") and got.startswith("{"):
        return "object-members-differ"
    return "other"


def expr_shape(e):
    e = e.strip()
    if re.match(r"^(async\s+)?(\([^)]*\)|[\w$]+)\s*(:[^=]*)?=>", e) or e.startswith("function") or e.startswith("async function"):
        return "function-expression"
    if e.startswith("class"):
        return "class-expression"
    if e.startswith("new "):
        return "new"
    if re.match(r"^[\w$.]+(<.*>)?\(", e) or e.endswith(")") and "(" in e and not e.startswith("("):
        return "call"
    if re.match(r"^[\w$]+$", e):
        return "identifier"
    if re.match(r"^[\w$.]+$", e) or re.match(r"^[\w$.]+\[", e):
        return "property-access"
    if e.startswith("{"):
        return "object-literal"
    if e.startswith("["):
        return "array-literal"
    if e.startswith("<") or e.startswith("<"):
        return "jsx-or-assertion"
    return "other-expression"


ANY_FAMILIES = [
    ("container CONTEXTUALISABLE", "any:contextual-parameter"),
    ("container cannot be contextually typed", "any:uncontextual-parameter"),
    ("member name of an access that ANSWERED ERROR", "any:property-lookup-error"),
    ("property access: the property's own type is `any` <- ERROR", "any:property-lookup-error"),
    ("element access: the indexed type is `any` <- ERROR", "any:property-lookup-error"),
    ("shorthand ambient module", "any:function-declaration-error"),
    ("alias target is `any`", "any:alias-target"),
    ("self-referential initialiser", "any:variable-initialiser-error"),
    ("annotation denotes `any`", "any:type-reference-error"),
    ("declared type of a type symbol is `any`", "any:type-reference-error"),
    ("ExpressionWithTypeArguments", "any:type-reference-error"),
    ("CallExpression", "any:call-error"),
    ("NewExpression", "any:call-error"),
    ("TaggedTemplateExpression", "any:call-error"),
    ("SuperKeyword", "any:call-error"),
    ("ArrowFunction", "any:function-expression-error"),
    ("FunctionExpression", "any:function-expression-error"),
    ("ObjectLiteralExpression", "any:literal-expression-error"),
    ("ArrayLiteralExpression", "any:literal-expression-error"),
    ("PropertyAssignment", "any:literal-expression-error"),
    ("BindingElement", "any:binding-element"),
    ("JsxAttribute", "any:jsx"),
    ("Jsx", "any:jsx"),
    ("binary ", "any:binary-operand"),
    ("on an `any` receiver", "any:any-receiver-propagated"),
    ("initialiser is `any`", "any:any-initialiser-propagated"),
    ("PropertyDeclaration with no annotation", "any:unannotated-declaration"),
    ("VariableDeclaration with no annotation", "any:unannotated-declaration"),
]


def any_family(reason):
    for needle, fam in ANY_FAMILIES:
        if needle in reason:
            return fam
    return "any:other"


# ------------------------------------------------------------------ main

def file_names(case):
    path = os.path.join(REF, case + ".types")
    names = []
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for line in f:
                if line.startswith("=== ") and line.rstrip().endswith(" ==="):
                    names.append(line.strip()[4:-4])
    except OSError:
        pass
    return names


def main():
    any_prod = {}
    path = os.path.join(BASE, "anylost.tsv")
    if os.path.exists(path):
        for line in open(path, encoding="utf-8"):
            parts = line.rstrip("\n").split("\t")
            if len(parts) >= 3:
                any_prod[parts[0]] = parts[2]
    rows = []
    names_cache = {}
    for line in open(os.path.join(BASE, "typesx.tsv"), encoding="utf-8"):
        parts = line.rstrip("\n").split("\t")
        if len(parts) < 4 or parts[1] == "RIGHT":
            continue
        key, verdict, want, got = parts[:4]
        expr = parts[4] if len(parts) > 4 else ""
        case, fidx, _pos = key.rsplit(":", 2)
        base = case.split("(")[0] if "(" in case else case
        if case not in names_cache:
            names_cache[case] = file_names(case)
        names = names_cache[case]
        fname = names[int(fidx)] if int(fidx) < len(names) else ""
        ctx = {"file": fname}
        if key in any_prod:
            ctx["any_producer"] = producer_slug(any_prod[key])
        cause = classify(want, got, expr, ctx)
        if fname.endswith((".js", ".jsx", ".cjs", ".mjs")) and cause not in ("gap-error",):
            cause = "js:" + cause
        rows.append((key, verdict, cause, want, got, expr, case))
    return rows


def producer_slug(reason):
    r = re.sub(r"\s*\([^)]*\.(rs|go):\d+[^)]*\)", "", reason)
    r = r.replace(" <- the branch answered ERROR; the producer prints `any` there", " <- ERROR")
    r = r.replace("DISAGREEMENT: ", "")
    return r.strip()


def report(rows):
    out = os.environ.get("TRIAGE_OUT", BASE)
    by_case = collections.defaultdict(list)
    with open(os.path.join(out, "lines.tsv"), "w", encoding="utf-8") as f:
        for key, verdict, cause, want, got, expr, case in rows:
            f.write("\t".join((key, verdict, cause, want, got, expr)) + "\n")
            by_case[case].append(cause)
    unaligned = set()
    p = os.path.join(BASE, "unaligned_cases.txt")
    if os.path.exists(p):
        unaligned = {l.strip() for l in open(p) if l.strip()}
    for c in unaligned:
        by_case[c].append("unaligned-lines")
    lines = collections.Counter(r[2] for r in rows)
    touched = collections.Counter()
    sole = collections.Counter()
    with open(os.path.join(out, "cases.tsv"), "w", encoding="utf-8") as f:
        for case in sorted(by_case):
            causes = sorted(set(by_case[case]))
            for c in causes:
                touched[c] += 1
            if len(causes) == 1:
                sole[causes[0]] += 1
            dominant = collections.Counter(by_case[case]).most_common(1)[0][0]
            f.write(f"{case}\t{len(by_case[case])}\t{';'.join(causes)}\t{causes[0] if len(causes) == 1 else ''}\t{dominant}\n")
    allc = set(lines) | set(touched)
    print(f"cases {len(by_case)}  lines {len(rows)}")
    for c in sorted(allc, key=lambda c: (-sole[c], -lines[c])):
        print(f"{sole[c]:5d} sole  {touched[c]:5d} cases  {lines[c]:5d} lines  {c}")


if __name__ == "__main__":
    report(main())
