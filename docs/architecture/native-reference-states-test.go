// Copy into internal/checker/reference_states_test.go in an isolated checkout
// of native 5b1047d. Private API controls do not establish CLI work equivalence.
package checker

import (
	"testing"

	"github.com/microsoft/typescript-go/internal/ast"
)

func referenceStateChecker() *Checker {
	c := &Checker{}
	c.couldContainTypeVariables = c.couldContainTypeVariablesWorker
	c.stringType = c.newIntrinsicType(TypeFlagsString, "string")
	c.numberType = c.newIntrinsicType(TypeFlagsNumber, "number")
	c.unknownType = c.newIntrinsicType(TypeFlagsUnknown, "unknown")
	c.errorType = c.newIntrinsicType(TypeFlagsAny, "error")
	c.noConstraintType = c.newIntrinsicType(TypeFlagsUnknown, "noConstraint")
	c.circularConstraintType = c.newIntrinsicType(TypeFlagsUnknown, "circularConstraint")
	return c
}

func referenceStateTarget(c *Checker, name string) *Type {
	t := c.newObjectType(ObjectFlagsInterface|ObjectFlagsReference, &ast.Symbol{Name: name})
	d := t.AsInterfaceType()
	d.target = t
	d.instantiations = make(map[CacheHashKey]*Type)
	return t
}

func referenceStateParameter(c *Checker) *Type {
	p := c.newTypeParameter(&ast.Symbol{Name: "T", Flags: ast.SymbolFlagsTypeParameter})
	p.AsTypeParameter().constraint = c.noConstraintType
	p.AsTypeParameter().resolvedBaseConstraint = c.noConstraintType
	return p
}

func TestReferenceStateHandlePrecedesMembers(t *testing.T) {
	c := referenceStateChecker()
	target := referenceStateTarget(c, "Box")
	args := []*Type{c.stringType}
	before := c.TypeCount
	ref := c.createTypeReference(target, args)
	if c.TypeCount != before+1 || target.AsInterfaceType().instantiations[getTypeListKey(args)] != ref {
		t.Fatal("first reference request must create and publish one handle")
	}
	if ref.objectFlags&ObjectFlagsMembersResolved != 0 || ref.AsStructuredType().members != nil {
		t.Fatal("handle publication must not be classified as resolved members")
	}
	if c.createTypeReference(target, args) != ref || c.TypeCount != before+1 {
		t.Fatal("repeat request reuses the still-lazy handle without another factory")
	}
	// Directly exercise the real publication primitive; this does not assert
	// that a natural member query on this minimally initialized checker ran.
	c.setStructuredTypeMembers(ref, ast.SymbolTable{}, nil, nil, nil)
	if ref.objectFlags&ObjectFlagsMembersResolved == 0 || c.createTypeReference(target, args) != ref {
		t.Fatal("member publication changes state without changing reference identity")
	}
}

func TestReferenceStateTargetArgumentsAndCheckerOwnership(t *testing.T) {
	c, other := referenceStateChecker(), referenceStateChecker()
	target, foreign := referenceStateTarget(c, "Box"), referenceStateTarget(other, "Box")
	ref, foreignRef := c.createTypeReference(target, []*Type{c.stringType}), other.createTypeReference(foreign, []*Type{other.stringType})
	if ref == foreignRef || ref.id != foreignRef.id {
		t.Fatal("equal private numeric IDs must not become cross-checker handles")
	}
	secondTarget := referenceStateTarget(c, "Box")
	if c.createTypeReference(secondTarget, []*Type{c.stringType}) == ref {
		t.Fatal("same printed target and arguments do not merge different target owners")
	}
	ordered := c.createTypeReference(target, []*Type{c.stringType, c.numberType})
	if c.createTypeReference(target, []*Type{c.numberType, c.stringType}) == ordered {
		t.Fatal("argument order identifies different reference handles")
	}
}

func TestReferenceStateEmptyAndErrorArguments(t *testing.T) {
	c := referenceStateChecker()
	target := referenceStateTarget(c, "Box")
	empty := c.createTypeReference(target, nil)
	if c.createTypeReference(target, []*Type{}) != empty {
		t.Fatal("empty argument representations must reuse the same handle")
	}
	args := []*Type{c.errorType}
	ref := c.createTypeReference(target, args)
	if ref == c.errorType || ref == empty || ref.AsTypeReference().resolvedTypeArguments[0] != c.errorType {
		t.Fatal("a lazy reference with an error argument differs from a returned error value")
	}
	if c.createTypeReference(target, args) != ref || ref.objectFlags&ObjectFlagsMembersResolved != 0 {
		t.Fatal("error argument must not imply either another factory or completed members")
	}
}

func TestReferenceStateRefusalDoesNotPublishReference(t *testing.T) {
	c := referenceStateChecker()
	target := referenceStateTarget(c, "Empty")
	c.emptyGenericType = target
	before := c.TypeCount
	for range 2 {
		if c.tryCreateTypeReference(target, []*Type{c.stringType}) != c.unknownType {
			t.Fatal("empty-generic argument refusal must return unknown")
		}
	}
	if len(target.AsInterfaceType().instantiations) != 0 || c.TypeCount != before {
		t.Fatal("refused requests must not allocate or publish reference handles")
	}
	ref := c.tryCreateTypeReference(target, nil)
	if ref == c.unknownType || c.TypeCount != before+1 || c.tryCreateTypeReference(target, nil) != ref {
		t.Fatal("a later valid empty request must publish and reuse its own handle")
	}
}

func TestReferenceStateAliasCanPublishError(t *testing.T) {
	c := referenceStateChecker()
	p := referenceStateParameter(c)
	symbol := &ast.Symbol{Name: "Identity", Flags: ast.SymbolFlagsTypeAlias}
	links := c.typeAliasLinks.Get(symbol)
	links.declaredType, links.typeParameters = p, []*Type{p}
	links.instantiations = make(map[CacheHashKey]*Type)
	args := []*Type{c.errorType}
	key := getTypeAliasInstantiationKey(args, nil)
	before := c.TotalInstantiationCount
	if c.getTypeAliasInstantiation(symbol, args, nil) != c.errorType || links.instantiations[key] != c.errorType {
		t.Fatal("returned alias error is a real persistent cache value")
	}
	if c.TotalInstantiationCount != before+1 || c.getTypeAliasInstantiation(symbol, args, nil) != c.errorType || c.TotalInstantiationCount != before+1 {
		t.Fatal("repeat published error must be distinguished from another worker execution")
	}
	target := referenceStateTarget(c, "Box")
	ref := c.createTypeReference(target, args)
	if ref == c.errorType || links.instantiations[key] != c.errorType || c.createTypeReference(target, args) != ref {
		t.Fatal("same argument shape must not merge alias-error and target-reference domains")
	}
	if c.getTypeAliasInstantiation(symbol, []*Type{c.numberType}, nil) != c.numberType || c.TotalInstantiationCount != before+2 {
		t.Fatal("error for one argument tuple must not poison a different tuple")
	}
}

func TestReferenceStateActiveErrorEndsAtPop(t *testing.T) {
	c := referenceStateChecker()
	p := referenceStateParameter(c)
	answer, calls := c.errorType, 0
	m := newFunctionTypeMapper(func(*Type) *Type { calls++; return answer })
	c.pushActiveMapper(m)
	if c.instantiateType(p, m) != c.errorType || c.instantiateType(p, m) != c.errorType || calls != 1 || c.TotalInstantiationCount != 1 {
		t.Fatal("returned error may be reused inside its active mapper frame")
	}
	answer = c.numberType
	if c.instantiateType(p, m) != c.errorType || calls != 1 {
		t.Fatal("published active error remains distinct from a new callback answer")
	}
	c.popActiveMapper()
	if c.instantiateType(p, m) != c.numberType || calls != 2 || c.TotalInstantiationCount != 2 {
		t.Fatal("error cache must not escape its active frame into a later request")
	}
	if len(c.activeMappers) != 0 || len(c.activeTypeMappersCaches) != 0 {
		t.Fatal("top-level request must restore the active stack")
	}
}

func TestReferenceStateDepthRefusalPrecedesCachedAnswer(t *testing.T) {
	c := referenceStateChecker()
	p := referenceStateParameter(c)
	calls := 0
	m := newFunctionTypeMapper(func(*Type) *Type { calls++; return c.numberType })
	c.pushActiveMapper(m)
	if c.instantiateType(p, m) != c.numberType {
		t.Fatal("initial nested worker must publish its returned answer")
	}
	c.instantiationDepth = 100
	if c.instantiateType(p, m) != c.errorType || calls != 1 || c.TotalInstantiationCount != 1 {
		t.Fatal("depth refusal must occur before a completed active-cache lookup")
	}
	diagnostics := c.diagnostics.GetGlobalDiagnostics()
	if len(diagnostics) != 1 || diagnostics[0].Code() != 2589 || diagnostics[0].File() != nil || diagnostics[0].Pos() != 0 || diagnostics[0].End() != 0 || len(diagnostics[0].MessageArgs()) != 0 || len(diagnostics[0].MessageChain()) != 0 {
		t.Fatal("synthetic depth refusal must retain its expected global diagnostic shape")
	}
	c.instantiationDepth = 0
	if c.instantiateType(p, m) != c.numberType || calls != 1 || c.TotalInstantiationCount != 1 {
		t.Fatal("refusal must not replace the previously returned active answer")
	}
	c.popActiveMapper()
}

func TestReferenceStateRecursiveReturnBeforeOuterCompletion(t *testing.T) {
	c := referenceStateChecker()
	p := referenceStateParameter(c)
	inside, calls := false, 0
	var mapper *TypeMapper
	mapper = newFunctionTypeMapper(func(*Type) *Type {
		calls++
		if inside {
			return c.numberType
		}
		inside = true
		if c.instantiateType(p, mapper) != c.numberType || c.instantiateType(p, mapper) != c.numberType {
			t.Fatal("recursive reentry must reuse only the returned inner answer")
		}
		inside = false
		return c.stringType
	})
	for round := 1; round <= 2; round++ {
		if c.instantiateType(p, mapper) != c.stringType || calls != round*2 || c.TotalInstantiationCount != uint32(round*2) {
			t.Fatal("an active outer request is not itself a published answer")
		}
		if len(c.activeMappers) != 0 || len(c.activeTypeMappersCaches) != 0 {
			t.Fatal("nested returned answers must not survive top-level pop")
		}
	}
}

func TestReferenceStateComposedMapperRetainsTargetCacheBoundary(t *testing.T) {
	c := referenceStateChecker()
	a, b := referenceStateParameter(c), referenceStateParameter(c)
	target := referenceStateTarget(c, "Box")
	target.AsInterfaceType().allTypeParameters = []*Type{b}
	image := c.createTypeReference(target, []*Type{b})
	outer, inner := newSimpleTypeMapper(a, image), newSimpleTypeMapper(b, c.stringType)
	merged := mergeTypeMappers(outer, inner).Map(a)
	composed := c.combineTypeMappers(outer, inner).Map(a)
	if merged != image || composed == image || composed != c.createTypeReference(target, []*Type{c.stringType}) {
		t.Fatal("recursive composition must reach the concrete target-owned reference")
	}
	if composed.objectFlags&ObjectFlagsMembersResolved != 0 {
		t.Fatal("composed cached reference still does not prove completed members")
	}
}
