// Generated domain module 117 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model116Service, summarizeModel116 } from "./model116";
import type { Model116 } from "./model116";
import { Model110Service, summarizeModel110 } from "./model110";
import type { Model110 } from "./model110";

export type Model117Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model117Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model117 extends Entity<"model117"> {
  updatedAt?: number;
  name: string;
  status: Model117Status;
  tags: string[];
  lines: Model117Line[];
  owner?: { name: string; email?: string };
  parent?: Model116;
  related: Model110[];
}

export type Model117Event =
  | { kind: "created"; item: Model117 }
  | { kind: "renamed"; id: Id<"model117">; from: string; to: string }
  | { kind: "moved"; id: Id<"model117">; status: Model117Status }
  | { kind: "deleted"; id: Id<"model117">; reason?: string };

export type Model117Events = {
  change: Model117Event;
  error: { message: string; code: number };
};

export type Model117Numbers = KeysOfType<Model117Line, number>;
export type FrozenModel117 = DeepReadonly<Model117>;

export function describeModel117Event(event: Model117Event): string {
  switch (event.kind) {
    case "created":
      return `created ${event.item.name}`;
    case "renamed":
      return `renamed ${event.from} -> ${event.to}`;
    case "moved":
      return `moved to ${event.status}`;
    case "deleted":
      return event.reason ? `deleted: ${event.reason}` : "deleted";
    default: {
      const unreachable: never = event;
      return unreachable;
    }
  }
}

export function totalModel117(item: Model117): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel117(item: Model117): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel117(item), 3), (item.parent ? summarizeModel116(item.parent) : "-"), item.related.map(summarizeModel110).join(",")].join(" | ");
}

export class Model117Service extends Service<Model117, "model117"> {
  readonly events = new EventBus<Model117Events>();
  private readonly parents?: Model116Service;

  constructor(repository = new MemoryRepository<Model117, "model117">()) {
    super(repository);
  }

  validate(item: Model117): string[] {
    const problems: string[] = [];
    if (item.name.trim().length === 0) {
      problems.push("name is required");
    }
    for (const [index, line] of item.lines.entries()) {
      if (line.quantity <= 0) {
        problems.push(`line ${index} has no quantity`);
      }
    }
    return problems;
  }

  rename(id: Id<"model117">, to: string): Result<Model117> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model117 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model117">, status: Model117Status): Result<Model117Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model117">, patch: Patch<Pick<Model117, "name" | "tags">>): Model117 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model117Status, Model117[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model117Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model117">[]): Promise<Model117[]> {
    const found: Model117[] = [];
    for (const id of ids) {
      const item = await retry(3, async () => this.find((candidate) => candidate.id === id));
      if (item) {
        found.push(item);
      }
    }
    return found;
  }

  parentNames(): string[] {
    return this.repository
      .list((item) => item.parent !== undefined)
      .map((item) => unwrapOr(ok(item.parent?.name ?? ""), ""));
  }

  linkedService(): Model116Service {
    return this.parents ?? new Model116Service();
  }
}

export function makeModel117(id: string, name: string): Model117 {
  return {
    id: id as Id<"model117">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 118, unit: "s" }],
    related: [],
  };
}

export const model117Defaults: FrozenModel117 = makeModel117("default-117", "Default 117");
export const model117Label = summarizeModel117(makeModel117("label", "Label"));
