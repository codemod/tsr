//! `dts_reachable_target`: how large a target a checker-free `.d.ts` emitter has.
//!
//! # Why this is measured before the emitter exists
//!
//! Phase 3.5 ships declaration emit without a checker. The expensive part of that
//! is a printer — upstream's is 6,280 lines and nothing like it exists here — and
//! [ADR-0021](../../../docs/adr/0021-isolated-declarations-is-not-a-port.md)
//! committed in advance to a falsifier rather than to the work:
//!
//! > If `bd tsr-49v.3` finds that fewer than a few hundred of the 1,414
//! > `@declaration` cases are annotated enough for a checker-free emitter, then the
//! > artifact this phase ships is not usable on real TypeScript, and the phase
//! > should be cut rather than built.
//!
//! So this suite reports the **size of the reachable target**, not a pass rate.
//! "Passed" here means *a checker-free emitter could be held to this case*, and
//! nothing whatsoever about whether we emit anything for it yet. It is the same
//! shape as [`crate::suites::ParserReachable`].
//!
//! # What makes a case reachable
//!
//! Both halves must hold:
//!
//! 1. **Upstream emitted declaration output for it.** Read from the `.js`
//!    baseline's embedded `//// [x.d.ts]` sections — see [`crate::js_baseline`] for
//!    why there is no `.d.ts` baseline to read instead. This is evidence rather
//!    than a directive: a case emits `.d.ts` because of `@declaration`, or
//!    `@composite`, or a `tsconfig` unit, and the baseline records what actually
//!    happened.
//! 2. **No declaration in it needs inference**, per [`tsr_dts`]. A case that draws
//!    a `TS9xxx` is by definition one a checker-free emitter cannot do.
//!
//! Half 2 is only worth trusting because the analysis converged with **zero false
//! positives** across its own 15-case oracle. A false positive here would mark a
//! reachable case unreachable and shrink this number — failing in the direction
//! that quietly cancels work.
//!
//! # This is an upper bound, and the gap is not small
//!
//! A reachable case is one where nothing needs *inferring*. It is not a case we
//! could byte-match today, because emitting the right text also needs visibility
//! decisions upstream makes through `EmitResolver.IsDeclarationVisible`.
//! `compiler/declareFileExportAssignmentWithVarFromVariableStatement` annotates
//! everything — so it counts as reachable here — and its `.d.ts` baseline still
//! drops `var x = 10` entirely, because `x` is not visible from the `export = m2`.
//!
//! Read this number as "the population worth aiming at", never as "cases we would
//! pass".

use tsr_parser::{ParsedFile, ScriptKind};

use crate::{
    CaseEntry,
    js_baseline::JsBaseline,
    suite::{Outcome, Suite},
};

/// The `dts_reachable_target` suite.
pub struct DtsReachableTarget;

impl Suite for DtsReachableTarget {
    fn name(&self) -> &'static str {
        "dts_reachable_target"
    }

    fn describes(&self) -> &'static str {
        "upstream emitted .d.ts for this case and no declaration in it needs inference, \
         so a checker-free emitter could be aimed at it — an UPPER BOUND on what is \
         achievable, not a pass rate, and not a claim that we emit anything"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if !case.has_any_baseline() {
            return Outcome::Skipped { reason: "upstream recorded no output for this case".into() };
        }

        let path = case.baseline_path("js");
        let Ok(text) = std::fs::read_to_string(&path) else {
            // No emit baseline at all. This is not "unreachable" — it is no
            // evidence either way, and folding it into the denominator as a
            // failure would make the target look smaller than it is.
            return Outcome::Skipped { reason: "upstream recorded no .js emit baseline".into() };
        };

        let baseline = JsBaseline::parse(&text);
        let Ok(parsed_case) = case.load() else {
            return Outcome::Failed { reason: "case did not load".into() };
        };
        // A `.d.ts` **section** is not a `.d.ts` **output**: the baseline echoes
        // every input unit first, so a case with a `foo.d.ts` input carries a
        // section upstream never emitted. This test originally read the echo as
        // evidence of declaration output — see the correction in
        // `docs/architecture/isolated-declarations.md`. The discriminator is exact:
        // a declaration section is output iff no input unit has that name.
        let inputs: std::collections::HashSet<&str> =
            parsed_case.files.iter().map(|file| file.name.as_str()).collect();
        let has_output = baseline
            .sections
            .iter()
            .any(|section| section.is_declaration() && !inputs.contains(section.name.as_str()));
        if !has_output {
            // Excluded from the denominator rather than counted as unreachable.
            // A case that never emits declarations is not a case a declaration
            // emitter failed at — it is not in the population at all, and leaving
            // it in would bury the ratio that matters under 7,456 cases that were
            // never asking the question. The skip count is reported, so the larger
            // denominator remains visible.
            return Outcome::Skipped {
                reason: "the emit baseline contains no .d.ts section".into(),
            };
        }

        let Ok(parsed_case) = case.load() else {
            return Outcome::Failed { reason: "case did not load".into() };
        };

        let mut codes = Vec::new();
        for unit in &parsed_case.files {
            // The dialect decides what the text means: a leading `<` opens JSX in
            // `.tsx` and a type assertion in `.ts`. Parsing every unit as
            // TypeScript would report phantom diagnostics on 799 `.tsx` units and
            // deflate this number.
            let kind = ScriptKind::from_file_name(&unit.name);
            if kind == ScriptKind::Json {
                continue;
            }
            let parsed = ParsedFile::parse_with_script_kind(unit.content.clone(), kind);
            let reported = parsed.with_ast(|file| tsr_dts::analyze(file, parsed.nodes()));
            codes.extend(reported.into_iter().map(|d| d.message.code()));
        }

        if codes.is_empty() {
            return Outcome::Passed;
        }
        codes.sort_unstable();
        codes.dedup();
        let listed: Vec<String> = codes.iter().take(4).map(|code| format!("TS{code}")).collect();
        Outcome::Failed { reason: format!("needs inference: {}", listed.join(" ")) }
    }
}
