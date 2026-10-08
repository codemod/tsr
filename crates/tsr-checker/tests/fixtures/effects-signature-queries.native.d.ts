export declare function ordinaryRepeat(a: unknown, b: unknown): {
    left: unknown;
    right: unknown;
};
export declare function assertion(value: unknown): {
    left: string;
    right: string;
};
export declare function predicateBranch(value: unknown): {
    left: string;
    right: string;
};
export declare function annotatedNever(value: string | number): string | number;
export declare function inferredNever(value: string | number): string | number;
export declare function dottedAssertion(value: unknown): string;
export declare function dottedOrdinary(value: unknown): unknown;
export declare function optionalOrdinary(value: unknown): unknown;
export declare function genericOrdinary(value: unknown): unknown;
export declare function genericAssertion(value: unknown): string;
export declare function loop(value: unknown, repeat: boolean): unknown;
export declare function contextual(value: unknown): unknown;
export declare function latePredicate(value: unknown): string;
export declare function recursive(value: unknown): boolean;
