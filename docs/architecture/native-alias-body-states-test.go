// Copy alongside native-reference-states-test.go into internal/checker in an
// isolated checkout of 5b1047d. Seeded private API controls are not CLI parity.
package checker

import (
	"testing"

	"github.com/microsoft/typescript-go/internal/ast"
)

func aliasBodyStateAlias(c *Checker, parameters []*Type, body *Type) *ast.Symbol {
	symbol := &ast.Symbol{Name: "Same", Flags: ast.SymbolFlagsTypeAlias}
	links := c.typeAliasLinks.Get(symbol)
	links.declaredType, links.typeParameters = body, parameters
	links.instantiations = make(map[CacheHashKey]*Type)
	return symbol
}

func TestAliasBodyStateDepthErrorPersistsInAliasTable(t *testing.T) {
	c := referenceStateChecker()
	p := referenceStateParameter(c)
	symbol := aliasBodyStateAlias(c, []*Type{p}, p)
	args := []*Type{c.numberType}
	c.instantiationDepth = 100
	if c.getTypeAliasInstantiation(symbol, args, nil) != c.errorType || c.TotalInstantiationCount != 0 {
		t.Fatal("depth refusal must precede worker execution")
	}
	if c.typeAliasLinks.Get(symbol).instantiations[getTypeAliasInstantiationKey(args, nil)] != c.errorType {
		t.Fatal("alias table must publish the returned depth error")
	}
	diagnostics := c.diagnostics.GetGlobalDiagnostics()
	if len(diagnostics) != 1 || diagnostics[0].Code() != 2589 {
		t.Fatal("initial refusal must retain native TS2589")
	}
	c.instantiationDepth = 0
	if c.getTypeAliasInstantiation(symbol, args, nil) != c.errorType || c.TotalInstantiationCount != 0 || len(c.diagnostics.GetGlobalDiagnostics()) != 1 {
		t.Fatal("same tuple reuses published error even after depth recovers")
	}
	if c.getTypeAliasInstantiation(symbol, []*Type{c.stringType}, nil) != c.stringType || c.TotalInstantiationCount != 1 {
		t.Fatal("a different tuple must still run its own worker")
	}
}

func TestAliasBodyStateSymbolAndOrderedArgumentsRemainDistinct(t *testing.T) {
	c := referenceStateChecker()
	a, b := referenceStateParameter(c), referenceStateParameter(c)
	first := aliasBodyStateAlias(c, []*Type{a, b}, a)
	second := aliasBodyStateAlias(c, []*Type{a, b}, b)
	args := []*Type{c.numberType, c.stringType}
	if c.getTypeAliasInstantiation(first, args, nil) != c.numberType || c.getTypeAliasInstantiation(second, args, nil) != c.stringType {
		t.Fatal("same printed alias and tuple must retain distinct symbol owners")
	}
	if c.getTypeAliasInstantiation(first, []*Type{c.stringType, c.numberType}, nil) != c.stringType || c.TotalInstantiationCount != 3 {
		t.Fatal("reordering arguments must select a distinct computation")
	}
	if c.getTypeAliasInstantiation(first, args, nil) != c.numberType || c.TotalInstantiationCount != 3 {
		t.Fatal("repeat original tuple must reuse its own published result")
	}
}
