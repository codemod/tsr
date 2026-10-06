package checker

// Private test adapter in an isolated pinned archive. It deliberately executes
// native queries; these observations do not establish ordinary CLI work counts.
import (
	"fmt"
	"github.com/microsoft/typescript-go/internal/ast"
)

type NativeSharedTypeView struct {
	ID          uint32      `json:"id"`
	Flags       TypeFlags   `json:"flags"`
	ObjectFlags ObjectFlags `json:"object_flags"`
	Alias       string      `json:"alias"`
	Name        string      `json:"name"`
	This        bool        `json:"this"`
}

func (c *Checker) NativeSharedType(t *Type) NativeSharedTypeView {
	v := NativeSharedTypeView{}
	if t == nil {
		return v
	}
	v.ID = uint32(t.id)
	v.Flags = t.flags
	v.ObjectFlags = t.objectFlags
	if t.alias != nil && t.alias.symbol != nil {
		v.Alias = t.alias.symbol.Name
	}
	if t.symbol != nil {
		v.Name = t.symbol.Name
	}
	if t.flags&TypeFlagsTypeParameter != 0 {
		v.This = t.AsTypeParameter().isThisType
	}
	return v
}

type NativeSharedMemberView struct {
	Symbol           string               `json:"symbol"`
	Target           string               `json:"target"`
	LinkTarget       string               `json:"link_target"`
	Mapper           string               `json:"mapper"`
	Containing       uint32               `json:"containing"`
	Parent           string               `json:"parent"`
	ParentParameters int                  `json:"parent_parameters"`
	Optional         bool                 `json:"optional"`
	Readonly         bool                 `json:"readonly"`
	Accessibility    ast.ModifierFlags    `json:"accessibility"`
	Flags            ast.SymbolFlags      `json:"flags"`
	CheckFlags       ast.CheckFlags       `json:"check_flags"`
	Read             NativeSharedTypeView `json:"read"`
	Write            NativeSharedTypeView `json:"write"`
}

func (c *Checker) nativeSharedMember(p *ast.Symbol) NativeSharedMemberView {
	v := NativeSharedMemberView{}
	if p == nil {
		return v
	}
	v.Symbol = fmt.Sprintf("%p", p)
	v.Target = fmt.Sprintf("%p", c.getTargetSymbol(p))
	v.Flags = p.Flags
	v.CheckFlags = p.CheckFlags
	v.Optional = p.Flags&ast.SymbolFlagsOptional != 0
	v.Readonly = c.isReadonlySymbol(p)
	v.Accessibility = getDeclarationModifierFlagsFromSymbol(p) & ast.ModifierFlagsNonPublicAccessibilityModifier
	if p.Parent != nil {
		v.Parent = p.Parent.Name
		v.ParentParameters = len(c.getLocalTypeParametersOfClassOrInterfaceOrTypeAlias(p.Parent))
	}
	v.Read = c.NativeSharedType(c.getNonMissingTypeOfSymbol(p))
	v.Write = c.NativeSharedType(c.getWriteTypeOfSymbol(p))
	if l := c.valueSymbolLinks.TryGet(p); l != nil {
		v.LinkTarget = fmt.Sprintf("%p", l.target)
		v.Mapper = fmt.Sprintf("%p", l.mapper)
		if l.containingType != nil {
			v.Containing = uint32(l.containingType.id)
		}
	}
	return v
}

type NativeSharedPropertyView struct {
	Checker             uint32                   `json:"checker"`
	Input               NativeSharedTypeView     `json:"input"`
	Container           NativeSharedTypeView     `json:"container"`
	CacheBefore         bool                     `json:"cache_before"`
	Kind                string                   `json:"kind"`
	Parts               []NativeSharedMemberView `json:"parts"`
	Result              NativeSharedMemberView   `json:"result"`
	SameTarget          bool                     `json:"same_target"`
	SameRead            bool                     `json:"same_read"`
	SameWrite           bool                     `json:"same_write"`
	Comparison          Ternary                  `json:"comparison"`
	OptionalMismatch    Ternary                  `json:"optional_mismatch"`
	ReadonlyMismatch    Ternary                  `json:"readonly_mismatch"`
	FirstMapperRetained bool                     `json:"first_mapper_retained"`
	FirstWriteRetained  bool                     `json:"first_write_retained"`
}

func (c *Checker) NativeSharedProperty(t *Type, name string) NativeSharedPropertyView {
	input := c.NativeSharedType(t)
	// Match getPropertyOfTypeEx: suppliers belong to the reduced apparent
	// intersection, whose this arguments carry the original receiver.
	t = c.getReducedApparentType(t)
	v := NativeSharedPropertyView{Checker: c.id, Input: input, Container: c.NativeSharedType(t)}
	if t.flags&TypeFlagsIntersection == 0 {
		v.Kind = "not-intersection"
		return v
	}
	cache := t.AsUnionOrIntersectionType().propertyCacheWithoutFunctionPropertyAugment
	v.CacheBefore = cache != nil && cache[name] != nil
	// Execute the actual publication operation before inspecting its suppliers.
	result := c.getPropertyOfUnionOrIntersectionType(t, name, true)
	props := []*ast.Symbol{}
	for _, part := range t.Types() {
		prop := c.getPropertyOfTypeEx(c.getApparentType(part), name, true, false)
		if prop != nil {
			props = append(props, prop)
			v.Parts = append(v.Parts, c.nativeSharedMember(prop))
		}
	}
	v.Result = c.nativeSharedMember(result)
	v.Kind = "missing"
	if result != nil {
		v.Kind = "synthesized"
		if len(props) > 0 {
			if result == props[0] {
				v.Kind = "single"
			}
			if l := c.valueSymbolLinks.TryGet(result); l != nil && l.target == props[0] && l.containingType == t {
				v.Kind = "merged-instantiation-clone"
			}
			l := c.valueSymbolLinks.TryGet(result)
			first := c.valueSymbolLinks.TryGet(props[0])
			v.FirstMapperRetained = l != nil && first != nil && l.mapper == first.mapper
			v.FirstWriteRetained = c.getWriteTypeOfSymbol(result) == c.getWriteTypeOfSymbol(props[0])
		}
	}
	if len(props) == 2 {
		v.SameTarget = c.getTargetSymbol(props[0]) == c.getTargetSymbol(props[1])
		v.SameRead = c.getNonMissingTypeOfSymbol(props[0]) == c.getNonMissingTypeOfSymbol(props[1])
		v.SameWrite = c.getWriteTypeOfSymbol(props[0]) == c.getWriteTypeOfSymbol(props[1])
		v.Comparison = c.compareProperties(props[0], props[1], compareTypesEqual)
		// Deliberately constructed metadata negatives, separate from natural states.
		// Copies retain the same declaration, root and read type; never mutate a
		// source declaration or the naturally published symbol.
		original := props[0]
		if !c.isReadonlySymbol(original) && original.Flags&ast.SymbolFlagsOptional == 0 {
			copyWith := func(optional, readonly bool) *ast.Symbol {
				clone := c.createSymbolWithType(original, c.getTypeOfSymbol(original))
				clone.CheckFlags |= ast.CheckFlagsInstantiated
				c.valueSymbolLinks.Get(clone).target = c.getTargetSymbol(original)
				if optional {
					clone.Flags |= ast.SymbolFlagsOptional
				}
				if readonly {
					clone.CheckFlags |= ast.CheckFlagsReadonly
				}
				return clone
			}
			v.OptionalMismatch = c.compareProperties(original, copyWith(true, false), compareTypesEqual)
			v.ReadonlyMismatch = c.compareProperties(original, copyWith(false, true), compareTypesEqual)
		}
	}
	return v
}
