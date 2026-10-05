// Generated domain module 126 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model125Service, summarizeModel125 } from "./model125";
import type { Model125 } from "./model125";
import { Model119Service, summarizeModel119 } from "./model119";
import type { Model119 } from "./model119";

export type Model126Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model126Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model126 extends Entity<"model126"> {
  updatedAt?: number;
  name: string;
  status: Model126Status;
  tags: string[];
  lines: Model126Line[];
  owner?: { name: string; email?: string };
  parent?: Model125;
  related: Model119[];
}

export type Model126Event =
  | { kind: "created"; item: Model126 }
  | { kind: "renamed"; id: Id<"model126">; from: string; to: string }
  | { kind: "moved"; id: Id<"model126">; status: Model126Status }
  | { kind: "deleted"; id: Id<"model126">; reason?: string };

export type Model126Events = {
  change: Model126Event;
  error: { message: string; code: number };
};

export type Model126Numbers = KeysOfType<Model126Line, number>;
export type FrozenModel126 = DeepReadonly<Model126>;

export function describeModel126Event(event: Model126Event): string {
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

export function totalModel126(item: Model126): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel126(item: Model126): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel126(item), 3), (item.parent ? summarizeModel125(item.parent) : "-"), item.related.map(summarizeModel119).join(",")].join(" | ");
}

export class Model126Service extends Service<Model126, "model126"> {
  readonly events = new EventBus<Model126Events>();
  private readonly parents?: Model125Service;

  constructor(repository = new MemoryRepository<Model126, "model126">()) {
    super(repository);
  }

  validate(item: Model126): string[] {
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

  rename(id: Id<"model126">, to: string): Result<Model126> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model126 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model126">, status: Model126Status): Result<Model126Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model126">, patch: Patch<Pick<Model126, "name" | "tags">>): Model126 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model126Status, Model126[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model126Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model126">[]): Promise<Model126[]> {
    const found: Model126[] = [];
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

  linkedService(): Model125Service {
    return this.parents ?? new Model125Service();
  }
}

export function makeModel126(id: string, name: string): Model126 {
  return {
    id: id as Id<"model126">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 127, unit: "m" }],
    related: [],
  };
}

export const model126Defaults: FrozenModel126 = makeModel126("default-126", "Default 126");
export const model126Label = summarizeModel126(makeModel126("label", "Label"));
