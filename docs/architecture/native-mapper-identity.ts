// Public semantics controls; internal identity/work controls live in the Go test.
export interface Pair<A, B> { first: A; second: B }
declare function pair<A, B>(a: A, b: B): Pair<A, B>;
const ordered = pair<string, number>("s", 1);
const reordered = pair<number, string>(1, "s");
export const orderedFirst: string = ordered.first;
export const reorderedFirst: number = reordered.first;

interface Box<T> { value: T }
interface Nested<T> { box: Box<T> }
declare const nested: Nested<string>;
export const composed: string = nested.box.value;

namespace Left { export class Token { private brand!: void; value = "left"; } }
namespace Right { export class Token { private brand!: void; value = "right"; } }
declare const left: Left.Token;
declare const right: Right.Token;
export const sameNameLeft: Left.Token = left;
export const sameNameRight: Right.Token = right;

interface Shadow<T> { outer: T; inner<T>(x: T): T }
declare const shadow: Shadow<string>;
export const shadowOuter: string = shadow.outer;
export const shadowInner: number = shadow.inner(1);

declare function infer<T>(input: { produce: () => T; consume: (x: T) => void }): T;
export const inferredString: string = infer({ produce: () => "s", consume: x => { const y: string = x; } });
export const inferredNumber: number = infer({ produce: () => 1, consume: x => { const y: number = x; } });

class Chain { next(): this { return this; } }
class Derived extends Chain { own = 1; }
export const receiver: number = new Derived().next().own;

type Deep<T> = { [K in keyof T]: T[K] extends object ? Deep<T[K]> : T[K] };
interface Link<T> { value: T; next: Link<T> }
declare const deepString: Deep<Link<string>>;
declare const deepNumber: Deep<Link<number>>;
export const recursiveMappedString: string = deepString.next.next.value;
export const recursiveMappedNumber: number = deepNumber.next.next.value;

type Flatten<T> = T extends readonly (infer U)[] ? Flatten<U> : T;
declare function flatten<T>(x: T): Flatten<T>;
export const flattened: string = flatten([["s"]] as const);

// NEGATIVE CONTROLS: the harness appends explicit assignments using these values.
