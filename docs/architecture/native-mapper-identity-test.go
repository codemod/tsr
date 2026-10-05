// Copy into internal/checker/mapper_identity_test.go in an isolated archive of
// native 5b1047d. These tests characterize private methods, not CLI equivalence.
package checker

import (
	"testing"

	"github.com/microsoft/typescript-go/internal/ast"
)

func identityChecker() *Checker {
	c := &Checker{}
	c.couldContainTypeVariables = c.couldContainTypeVariablesWorker
	c.stringType = c.newIntrinsicType(TypeFlagsString, "string")
	c.numberType = c.newIntrinsicType(TypeFlagsNumber, "number")
	c.unknownType = c.newIntrinsicType(TypeFlagsUnknown, "unknown")
	c.noConstraintType = c.newIntrinsicType(TypeFlagsUnknown, "noConstraint")
	c.circularConstraintType = c.newIntrinsicType(TypeFlagsUnknown, "circularConstraint")
	return c
}

func identityParameter(c *Checker, name string) *Type {
	t := c.newTypeParameter(&ast.Symbol{Name: name, Flags: ast.SymbolFlagsTypeParameter})
	t.AsTypeParameter().constraint = c.noConstraintType
	t.AsTypeParameter().resolvedBaseConstraint = c.noConstraintType
	return t
}

func TestMapperIdentityOrderedPairs(t *testing.T) {
	c := identityChecker()
	a, b := identityParameter(c, "T"), identityParameter(c, "U")
	m := newTypeMapper([]*Type{a, b}, []*Type{c.stringType, c.numberType})
	copy := newTypeMapper([]*Type{a, b}, []*Type{c.stringType, c.numberType})
	reordered := newTypeMapper([]*Type{b, a}, []*Type{c.numberType, c.stringType})
	swapped := newTypeMapper([]*Type{a, b}, []*Type{c.numberType, c.stringType})
	if m == copy || m.Map(a) != copy.Map(a) || m.Map(b) != reordered.Map(b) || swapped.Map(a) == m.Map(a) {
		t.Fatal("mapper object identity differs from ordered substitution answers")
	}
	if getTypeListKey([]*Type{a, b}) == getTypeListKey([]*Type{b, a}) {
		t.Fatal("type-list keys must preserve order")
	}
	c.pushActiveMapper(m)
	if c.findActiveMapper(m) != 0 || c.findActiveMapper(copy) != -1 {
		t.Fatal("active mapper reuse requires the same mapper pointer")
	}
	c.popActiveMapper()
	duplicate := newArrayTypeMapper([]*Type{a, a}, []*Type{c.stringType, c.numberType})
	if duplicate.Map(a) != c.stringType {
		t.Fatal("array lookup takes first pointer match")
	}
	images := []*Type{c.stringType, c.numberType}
	borrowed := newArrayTypeMapper([]*Type{a, b}, images)
	images[0] = c.numberType
	if borrowed.Map(a) != c.numberType {
		t.Fatal("array factory retains its caller-owned image slice")
	}
}

func TestMapperIdentityShadowedAndCheckerLocal(t *testing.T) {
	c, other := identityChecker(), identityChecker()
	a, shadow := identityParameter(c, "T"), identityParameter(c, "T")
	foreign := identityParameter(other, "T")
	m := newSimpleTypeMapper(a, c.stringType)
	if a.symbol.Name != shadow.symbol.Name || m.Map(shadow) != shadow || m.Map(foreign) != foreign {
		t.Fatal("equal source names cannot replace pointer identity")
	}
	if a.id != foreign.id || getTypeListKey([]*Type{a}) != getTypeListKey([]*Type{foreign}) || a == foreign {
		t.Fatal("local numeric type IDs and their hashes can coincide across private checkers")
	}
}

func TestMapperIdentityCompositionAndReceiver(t *testing.T) {
	c := identityChecker()
	a, b := identityParameter(c, "T"), identityParameter(c, "U")
	m1, m2 := newSimpleTypeMapper(a, b), newSimpleTypeMapper(b, c.stringType)
	if mergeTypeMappers(m1, m2).Map(a) != c.stringType || mergeTypeMappers(m2, m1).Map(a) != b {
		t.Fatal("merged composition order is semantic")
	}
	if prependTypeMapping(a, c.numberType, m1).Map(a) != c.numberType || appendTypeMapping(m1, a, c.numberType).Map(a) != b {
		t.Fatal("prepend and append have different lookup precedence")
	}
	target := c.newObjectType(ObjectFlagsInterface|ObjectFlagsReference, &ast.Symbol{Name: "Box"})
	target.AsInterfaceType().instantiations = make(map[CacheHashKey]*Type)
	target.AsInterfaceType().target = target
	target.AsInterfaceType().allTypeParameters = []*Type{b}
	image := c.createTypeReference(target, []*Type{b})
	if c.createTypeReference(target, []*Type{b}) != image || c.createTypeReference(target, []*Type{a}) == image {
		t.Fatal("reference cache belongs to its target and ordered effective arguments")
	}
	outer := newSimpleTypeMapper(a, image)
	merged := mergeTypeMappers(outer, m2).Map(a)
	composed := c.combineTypeMappers(outer, m2).Map(a)
	if merged != image || composed == image || composed.AsTypeReference().resolvedTypeArguments[0] != c.stringType {
		t.Fatal("composite instantiates inside the first image; merged only applies two direct maps")
	}
	this := identityParameter(c, "this")
	this.AsTypeParameter().isThisType = true
	receiver := newSimpleTypeMapper(this, c.numberType)
	if !receiver.MapsThisOnly() || m1.MapsThisOnly() || receiver.Map(a) != a || receiver.Map(this) != c.numberType {
		t.Fatal("receiver substitution has its own source identity")
	}
}

func TestMapperIdentityDeferredAnswersAreLive(t *testing.T) {
	c := identityChecker()
	a := identityParameter(c, "T")
	answer, calls := c.stringType, 0
	m := newDeferredTypeMapper([]*Type{a}, []func() *Type{func() *Type { calls++; return answer }})
	if m.Map(a) != c.stringType || calls != 1 {
		t.Fatal("deferred mapping must execute its image callback")
	}
	answer = c.numberType
	if m.Map(a) != c.numberType || calls != 2 || m.Map(c.unknownType) != c.unknownType || calls != 2 {
		t.Fatal("a mapper pointer alone does not freeze deferred answers")
	}
}

func TestMapperIdentityActiveReuseAndPublication(t *testing.T) {
	c := identityChecker()
	a, b := identityParameter(c, "T"), identityParameter(c, "U")
	var m *TypeMapper
	calls := 0
	m = newFunctionTypeMapper(func(x *Type) *Type {
		calls++
		if x == a {
			first, second := c.instantiateType(b, m), c.instantiateType(b, m)
			if first != c.stringType || second != first {
				t.Fatal("completed inner answer changed")
			}
		}
		return c.stringType
	})
	for round := 1; round <= 2; round++ {
		if c.instantiateType(a, m) != c.stringType || calls != round*2 || c.TotalInstantiationCount != uint32(round*2) {
			t.Fatal("only completed nested work under the active mapper is reused", round, calls, c.TotalInstantiationCount)
		}
		if len(c.activeMappers) != 0 || len(c.activeTypeMappersCaches) != 0 {
			t.Fatal("active lifetime escaped return")
		}
	}
	t.Logf("completed reuse: requests=6 worker_executions=%d mapper_calls=%d active_after=0", c.TotalInstantiationCount, calls)
	calls = 0
	inside := false
	m = newFunctionTypeMapper(func(x *Type) *Type {
		calls++
		if !inside {
			inside = true
			if c.instantiateType(x, m) != c.numberType || c.instantiateType(x, m) != c.numberType {
				t.Fatal("recursive return changed")
			}
		}
		return c.numberType
	})
	before := c.TotalInstantiationCount
	if c.instantiateType(a, m) != c.numberType || calls != 2 || c.TotalInstantiationCount-before != 2 {
		t.Fatal("active entry is not a published answer; a returned inner answer is")
	}
	t.Logf("active publication: requests=3 worker_executions=%d mapper_calls=%d", c.TotalInstantiationCount-before, calls)
}

func TestMapperIdentityInferenceFixingAndInvalidation(t *testing.T) {
	c := identityChecker()
	a := identityParameter(c, "T")
	info := &InferenceInfo{typeParameter: a, candidates: []*Type{c.stringType}}
	n := c.newInferenceContextWorker([]*InferenceInfo{info}, nil, InferenceFlagsNone, nil)
	c.pushActiveMapper(n.nonFixingMapper)
	key := getTypeListKey([]*Type{a})
	c.activeTypeMappersCaches[0][key] = c.unknownType
	if n.nonFixingMapper.Map(a) != c.stringType || info.isFixed || len(c.activeTypeMappersCaches[0]) != 0 {
		t.Fatal("new inference answers invalidate active instantiation answers without fixing")
	}
	info.candidates = []*Type{c.numberType}
	clearCachedInferences(n.inferences)
	if n.nonFixingMapper.Map(a) != c.numberType || info.isFixed {
		t.Fatal("same non-fixing mapper has a new answer")
	}
	if n.mapper.Map(a) != c.numberType || !info.isFixed {
		t.Fatal("fixing read commits the parameter")
	}
	clearCachedInferences(n.inferences)
	if info.inferredType != c.numberType || n.mapper.Map(a) != c.numberType {
		t.Fatal("fixed inference survives clear")
	}
	c.popActiveMapper()
	other := c.newInferenceContextWorker([]*InferenceInfo{{typeParameter: a, candidates: []*Type{c.stringType}}}, nil, InferenceFlagsNone, nil)
	if other.mapper == n.mapper || other.nonFixingMapper.Map(a) != c.stringType {
		t.Fatal("context identity is not parameter identity")
	}
}

func TestMapperIdentityInstantiationKeyContext(t *testing.T) {
	c := identityChecker()
	a := identityParameter(c, "T")
	alias := &TypeAlias{symbol: &ast.Symbol{Name: "Alias"}, typeArguments: []*Type{a}}
	otherAlias := &TypeAlias{symbol: &ast.Symbol{Name: "Alias"}, typeArguments: []*Type{a}}
	changedArguments := &TypeAlias{symbol: alias.symbol, typeArguments: []*Type{c.numberType}}
	base := getTypeInstantiationKey([]*Type{c.stringType}, alias, false)
	if base != getTypeInstantiationKey([]*Type{c.stringType}, alias, false) ||
		base == getTypeInstantiationKey([]*Type{c.stringType}, otherAlias, false) ||
		base == getTypeInstantiationKey([]*Type{c.stringType}, changedArguments, false) ||
		base == getTypeInstantiationKey([]*Type{c.stringType}, alias, true) ||
		base == getTypeInstantiationKey([]*Type{c.numberType}, alias, false) ||
		getConditionalTypeKey([]*Type{a}, alias, false) == getConditionalTypeKey([]*Type{a}, alias, true) {
		t.Fatal("alias symbol/arguments and signature/constraint modes distinguish completed keys")
	}
}
