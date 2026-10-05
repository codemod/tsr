// Supported public shadowing/captured-parameter control for mode partitioning.
export function outer<T>(value: T) {
    return function inner<T, U>(own: T, other: U) {
        return { outer: value, own, other };
    };
}
const first = outer("outer");
const result = first(1, true);
const accepted: { outer: string; own: number; other: boolean } = result;

export function callback<T>(value: T) {
    return function inner<U>(item: U) {
        return { value, item };
    };
}
const captured = callback("s")(2);
const acceptedCaptured: { value: string; item: number } = captured;

// The runner appends deliberate negative assignments to result and captured.
