// @strict: true
type Inputs<F> = boolean;
type DefaultInputs<F = string> = boolean;
declare const missing: Inputs;
declare const excessive: Inputs<string, number>;
declare const valid: Inputs<string>;
declare const defaulted: DefaultInputs;
declare const defaultExcessive: DefaultInputs<string, number>;
