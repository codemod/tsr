// Generated domain module 121 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model120Service, summarizeModel120 } from "./model120";
import type { Model120 } from "./model120";
import { Model114Service, summarizeModel114 } from "./model114";
import type { Model114 } from "./model114";

export type Model121Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model121Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model121 extends Entity<"model121"> {
  updatedAt?: number;
  name: string;
  status: Model121Status;
  tags: string[];
  lines: Model121Line[];
  owner?: { name: string; email?: string };
  parent?: Model120;
  related: Model114[];
}

export type Model121Event =
  | { kind: "created"; item: Model121 }
  | { kind: "renamed"; id: Id<"model121">; from: string; to: string }
  | { kind: "moved"; id: Id<"model121">; status: Model121Status }
  | { kind: "deleted"; id: Id<"model121">; reason?: string };

export type Model121Events = {
  change: Model121Event;
  error: { message: string; code: number };
};

export type Model121Numbers = KeysOfType<Model121Line, number>;
export type FrozenModel121 = DeepReadonly<Model121>;

export function describeModel121Event(event: Model121Event): string {
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

export function totalModel121(item: Model121): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel121(item: Model121): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel121(item), 3), (item.parent ? summarizeModel120(item.parent) : "-"), item.related.map(summarizeModel114).join(",")].join(" | ");
}

export class Model121Service extends Service<Model121, "model121"> {
  readonly events = new EventBus<Model121Events>();
  private readonly parents?: Model120Service;

  constructor(repository = new MemoryRepository<Model121, "model121">()) {
    super(repository);
  }

  validate(item: Model121): string[] {
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

  rename(id: Id<"model121">, to: string): Result<Model121> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model121 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model121">, status: Model121Status): Result<Model121Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model121">, patch: Patch<Pick<Model121, "name" | "tags">>): Model121 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model121Status, Model121[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model121Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model121">[]): Promise<Model121[]> {
    const found: Model121[] = [];
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

  linkedService(): Model120Service {
    return this.parents ?? new Model120Service();
  }
}

export function makeModel121(id: string, name: string): Model121 {
  return {
    id: id as Id<"model121">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 122, unit: "m" }],
    related: [],
  };
}

export const model121Defaults: FrozenModel121 = makeModel121("default-121", "Default 121");
export const model121Label = summarizeModel121(makeModel121("label", "Label"));
