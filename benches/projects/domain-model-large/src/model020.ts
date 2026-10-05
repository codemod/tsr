// Generated domain module 20 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model019Service, summarizeModel019 } from "./model019";
import type { Model019 } from "./model019";
import { Model013Service, summarizeModel013 } from "./model013";
import type { Model013 } from "./model013";

export type Model020Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model020Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model020 extends Entity<"model020"> {
  updatedAt?: number;
  name: string;
  status: Model020Status;
  tags: string[];
  lines: Model020Line[];
  owner?: { name: string; email?: string };
  parent?: Model019;
  related: Model013[];
}

export type Model020Event =
  | { kind: "created"; item: Model020 }
  | { kind: "renamed"; id: Id<"model020">; from: string; to: string }
  | { kind: "moved"; id: Id<"model020">; status: Model020Status }
  | { kind: "deleted"; id: Id<"model020">; reason?: string };

export type Model020Events = {
  change: Model020Event;
  error: { message: string; code: number };
};

export type Model020Numbers = KeysOfType<Model020Line, number>;
export type FrozenModel020 = DeepReadonly<Model020>;

export function describeModel020Event(event: Model020Event): string {
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

export function totalModel020(item: Model020): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel020(item: Model020): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel020(item), 3), (item.parent ? summarizeModel019(item.parent) : "-"), item.related.map(summarizeModel013).join(",")].join(" | ");
}

export class Model020Service extends Service<Model020, "model020"> {
  readonly events = new EventBus<Model020Events>();
  private readonly parents?: Model019Service;

  constructor(repository = new MemoryRepository<Model020, "model020">()) {
    super(repository);
  }

  validate(item: Model020): string[] {
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

  rename(id: Id<"model020">, to: string): Result<Model020> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model020 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model020">, status: Model020Status): Result<Model020Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model020">, patch: Patch<Pick<Model020, "name" | "tags">>): Model020 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model020Status, Model020[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model020Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model020">[]): Promise<Model020[]> {
    const found: Model020[] = [];
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

  linkedService(): Model019Service {
    return this.parents ?? new Model019Service();
  }
}

export function makeModel020(id: string, name: string): Model020 {
  return {
    id: id as Id<"model020">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 21, unit: "kg" }],
    related: [],
  };
}

export const model020Defaults: FrozenModel020 = makeModel020("default-20", "Default 20");
export const model020Label = summarizeModel020(makeModel020("label", "Label"));
