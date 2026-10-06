// Generated domain module 96 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model095Service, summarizeModel095 } from "./model095";
import type { Model095 } from "./model095";
import { Model089Service, summarizeModel089 } from "./model089";
import type { Model089 } from "./model089";

export type Model096Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model096Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model096 extends Entity<"model096"> {
  updatedAt?: number;
  name: string;
  status: Model096Status;
  tags: string[];
  lines: Model096Line[];
  owner?: { name: string; email?: string };
  parent?: Model095;
  related: Model089[];
}

export type Model096Event =
  | { kind: "created"; item: Model096 }
  | { kind: "renamed"; id: Id<"model096">; from: string; to: string }
  | { kind: "moved"; id: Id<"model096">; status: Model096Status }
  | { kind: "deleted"; id: Id<"model096">; reason?: string };

export type Model096Events = {
  change: Model096Event;
  error: { message: string; code: number };
};

export type Model096Numbers = KeysOfType<Model096Line, number>;
export type FrozenModel096 = DeepReadonly<Model096>;

export function describeModel096Event(event: Model096Event): string {
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

export function totalModel096(item: Model096): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel096(item: Model096): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel096(item), 3), (item.parent ? summarizeModel095(item.parent) : "-"), item.related.map(summarizeModel089).join(",")].join(" | ");
}

export class Model096Service extends Service<Model096, "model096"> {
  readonly events = new EventBus<Model096Events>();
  private readonly parents?: Model095Service;

  constructor(repository = new MemoryRepository<Model096, "model096">()) {
    super(repository);
  }

  validate(item: Model096): string[] {
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

  rename(id: Id<"model096">, to: string): Result<Model096> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model096 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model096">, status: Model096Status): Result<Model096Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model096">, patch: Patch<Pick<Model096, "name" | "tags">>): Model096 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model096Status, Model096[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model096Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model096">[]): Promise<Model096[]> {
    const found: Model096[] = [];
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

  linkedService(): Model095Service {
    return this.parents ?? new Model095Service();
  }
}

export function makeModel096(id: string, name: string): Model096 {
  return {
    id: id as Id<"model096">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 97, unit: "m" }],
    related: [],
  };
}

export const model096Defaults: FrozenModel096 = makeModel096("default-96", "Default 96");
export const model096Label = summarizeModel096(makeModel096("label", "Label"));
