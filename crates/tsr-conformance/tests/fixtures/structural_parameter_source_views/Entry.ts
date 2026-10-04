// @target: es2015
// @strict: true
type FormalInputs<F extends (...args: any[]) => any> = F extends (...args: infer P) => any ? P : never;
type ActualResult<F> = F extends (...args: any[]) => infer R ? R : never;
function foo(arg: FormalInputs<typeof bar>[0]) { return arg; }
function bar(arg: string, count: number) { return foo(arg); }
const fooResult = foo('cold');
const barResult = bar('warm', 23);
const fooAgain = foo('again');
const barAgain = bar('last', 5);
function make() { return { value: 23 }; }
declare const returned: ActualResult<typeof make>;
const result = make();
const returnedValue = returned.value;
const resultValue = result.value;
