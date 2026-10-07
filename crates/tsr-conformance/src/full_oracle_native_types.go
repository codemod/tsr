package tsbaseline

import (
	"github.com/microsoft/typescript-go/internal/compiler"
	"github.com/microsoft/typescript-go/internal/testutil/harnessutil"
	"strings"
)

// FullOracleTypes uses the pinned native selector, independently of TSR's walker.
func FullOracleTypes(program compiler.ProgramLike, files []*harnessutil.TestFile, hadErrors bool) [][3]string {
	walker := newTypeWriterWalker(program, hadErrors)
	var rows [][3]string
	for _, file := range files {
		for _, result := range walker.getTypes(file.UnitName) {
			rows = append(rows, [3]string{file.UnitName, lineDelimiter.ReplaceAllString(result.sourceText, ""), strings.ReplaceAll(result.typ, "\r\n", "\n")})
		}
	}
	return rows
}
