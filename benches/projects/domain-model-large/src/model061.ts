// Generated domain module 61 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model060Service, summarizeModel060 } from "./model060";
import type { Model060 } from "./model060";
import { Model054Service, summarizeModel054 } from "./model054";
import type { Model054 } from "./model054";

export type Model061Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model061Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model061 extends Entity<"model061"> {
  updatedAt?: number;
  name: string;
  status: Model061Status;
  tags: string[];
  lines: Model061Line[];
  owner?: { name: string; email?: string };
  parent?: Model060;
  related: Model054[];
}

export type Model061Event =
  | { kind: "created"; item: Model061 }
  | { kind: "renamed"; id: Id<"model061">; from: string; to: string }
  | { kind: "moved"; id: Id<"model061">; status: Model061Status }
  | { kind: "deleted"; id: Id<"model061">; reason?: string };

export type Model061Events = {
  change: Model061Event;
  error: { message: string; code: number };
};

export type Model061Numbers = KeysOfType<Model061Line, number>;
export type FrozenModel061 = DeepReadonly<Model061>;

export function describeModel061Event(event: Model061Event): string {
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

export function totalModel061(item: Model061): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel061(item: Model061): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel061(item), 3), (item.parent ? summarizeModel060(item.parent) : "-"), item.related.map(summarizeModel054).join(",")].join(" | ");
}

export class Model061Service extends Service<Model061, "model061"> {
  readonly events = new EventBus<Model061Events>();
  private readonly parents?: Model060Service;

  constructor(repository = new MemoryRepository<Model061, "model061">()) {
    super(repository);
  }

  validate(item: Model061): string[] {
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

  rename(id: Id<"model061">, to: string): Result<Model061> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model061 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model061">, status: Model061Status): Result<Model061Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model061">, patch: Patch<Pick<Model061, "name" | "tags">>): Model061 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model061Status, Model061[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model061Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model061">[]): Promise<Model061[]> {
    const found: Model061[] = [];
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

  linkedService(): Model060Service {
    return this.parents ?? new Model060Service();
  }
}

export function makeModel061(id: string, name: string): Model061 {
  return {
    id: id as Id<"model061">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 62, unit: "m" }],
    related: [],
  };
}

export const model061Defaults: FrozenModel061 = makeModel061("default-61", "Default 61");
export const model061Label = summarizeModel061(makeModel061("label", "Label"));
