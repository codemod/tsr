// Installed only through a `go test -overlay` as
// internal/testutil/tsbaseline/full_oracle.go; the pinned checkout is never modified.
package tsbaseline

import "github.com/microsoft/typescript-go/internal/compiler"

// FullOracleRow is one iterateBaseline input: a section header (Line < 0) or one
// typeWriterResult. The rendered `.types` text is a function of these rows and the
// unit contents, so equal rows are equal baselines.
type FullOracleRow struct {
	File string
	Line int
	Text string
	Type string
}

// FullOracleTypes mirrors DoTypeAndSymbolBaseline's type half: newTypeWriterWalker
// over the compiled program, then iterateBaseline's per-file getTypes walk in
// allFiles order, with its `>text : type` spelling (lineDelimiter-stripped text).
func FullOracleTypes(program compiler.ProgramLike, files []string, hadErrorBaseline bool) []FullOracleRow {
	walker := newTypeWriterWalker(program, hadErrorBaseline)
	var rows []FullOracleRow
	for _, file := range files {
		rows = append(rows, FullOracleRow{File: file, Line: -1})
		for _, result := range walker.getTypes(file) {
			rows = append(rows, FullOracleRow{
				File: file,
				Line: result.line,
				Text: lineDelimiter.ReplaceAllString(result.sourceText, ""),
				Type: result.typ,
			})
		}
	}
	return rows
}

// FullOraclePath is the printed file identity, removeTestPathPrefixes.
func FullOraclePath(name string) string { return removeTestPathPrefixes(name, false) }
