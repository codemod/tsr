interface ClassDecoratorContext<T> { receiver: T }
interface ClassMethodDecoratorContext<T, V> { receiver: T; value: V }
interface ClassGetterDecoratorContext<T, V> { receiver: T; value: V }
interface ClassSetterDecoratorContext<T, V> { receiver: T; value: V }
interface ClassAccessorDecoratorContext<T, V> { receiver: T; value: V }
interface ClassFieldDecoratorContext<T, V> { receiver: T; value: V }
interface ClassAccessorDecoratorTarget<T, V> { get(this: T): V; set(this: T, value: V): void }
interface ClassAccessorDecoratorResult<T, V> { init?(this: T, value: V): V }
interface TypedPropertyDescriptor<V> { value?: V }
declare const token: unique symbol;
@((classTarget, classContext) => {})
class Vessel {
    constructor(first: string, @((ctorTarget, ctorKey, ctorIndex) => {}) second: number) {}
    @((staticTarget, staticContext) => {})
    static shared(input: string): boolean { return true; }
    @((instanceTarget, instanceContext) => {})
    @((secondTarget, secondContext) => {})
    shared(input: number): string { return "out"; }
    @((privateTarget, privateContext) => {})
    static #secret(input: boolean): number { return 7; }
    @((getterTarget, getterContext) => {})
    get reading(): number { return 1; }
    @((setterTarget, setterContext) => {})
    set reading(value: number) {}
    @((accessorTarget, accessorContext) => {})
    accessor entry: string = "entry";
    @((fieldTarget, fieldContext) => {})
    slot: boolean = false;
    @((numericTarget, numericContext) => {})
    17: string = "numeric";
    @((computedTarget, computedContext) => {})
    ["computed"]: number = 19;
    @((symbolTarget, symbolContext) => {})
    [token]: boolean = false;
    indexed(this: Vessel, first: string, @((paramTarget, paramKey, paramIndex) => {}) second: number) {}
}
type Wrapped<U> = { retained: U };
class Parcel<T> {
    @((genericTarget, genericContext) => {})
    item!: Wrapped<T>;
}
@((oneTarget) => {})
class Single {}
