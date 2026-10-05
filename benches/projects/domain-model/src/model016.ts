// Generated domain module 16 of 40.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model015Service, summarizeModel015 } from "./model015";
import type { Model015 } from "./model015";
import { Model009Service, summarizeModel009 } from "./model009";
import type { Model009 } from "./model009";

export type Model016Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model016Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model016 extends Entity<"model016"> {
  updatedAt?: number;
  name: string;
  status: Model016Status;
  tags: string[];
  lines: Model016Line[];
  owner?: { name: string; email?: string };
  parent?: Model015;
  related: Model009[];
}

export type Model016Event =
  | { kind: "created"; item: Model016 }
  | { kind: "renamed"; id: Id<"model016">; from: string; to: string }
  | { kind: "moved"; id: Id<"model016">; status: Model016Status }
  | { kind: "deleted"; id: Id<"model016">; reason?: string };

export type Model016Events = {
  change: Model016Event;
  error: { message: string; code: number };
};

export type Model016Numbers = KeysOfType<Model016Line, number>;
export type FrozenModel016 = DeepReadonly<Model016>;

export function describeModel016Event(event: Model016Event): string {
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

export function totalModel016(item: Model016): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel016(item: Model016): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel016(item), 3), (item.parent ? summarizeModel015(item.parent) : "-"), item.related.map(summarizeModel009).join(",")].join(" | ");
}

export class Model016Service extends Service<Model016, "model016"> {
  readonly events = new EventBus<Model016Events>();
  private readonly parents?: Model015Service;

  constructor(repository = new MemoryRepository<Model016, "model016">()) {
    super(repository);
  }

  validate(item: Model016): string[] {
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

  rename(id: Id<"model016">, to: string): Result<Model016> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model016 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model016">, status: Model016Status): Result<Model016Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model016">, patch: Patch<Pick<Model016, "name" | "tags">>): Model016 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model016Status, Model016[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model016Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model016">[]): Promise<Model016[]> {
    const found: Model016[] = [];
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

  linkedService(): Model015Service {
    return this.parents ?? new Model015Service();
  }
}

export function makeModel016(id: string, name: string): Model016 {
  return {
    id: id as Id<"model016">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 17, unit: "m" }],
    related: [],
  };
}

export const model016Defaults: FrozenModel016 = makeModel016("default-16", "Default 16");
export const model016Label = summarizeModel016(makeModel016("label", "Label"));
