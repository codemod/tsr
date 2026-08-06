import { resolve } from 'node:path';

import { measure, repoState } from './measure.ts';

/**
 * Take one reading and print it. Useful on its own — it is the same
 * measurement the loop steers by, so a number quoted from here and a number
 * the loop acted on cannot disagree.
 */
const repo = resolve(process.env.TSR_REPO ?? resolve(import.meta.dirname, '../../..'));
const live = process.env.PARITY_SNAPSHOT !== '1';
const reading = await measure(repo, live);
const state = await repoState(repo);

console.log(`commit    ${reading.commit}   (${reading.source})`);
console.log(
  `gradient  ${reading.gradient.toFixed(2)}%   ${reading.matched.toLocaleString()} / ${reading.total.toLocaleString()} assertion lines`,
);
console.log(`cases     ${reading.cases.toLocaleString()} / ${reading.caseTotal.toLocaleString()}`);
console.log(`tree      ${state.clean ? 'clean' : 'DIRTY'}, ${state.pushed ? 'pushed' : 'UNPUSHED'}`);
