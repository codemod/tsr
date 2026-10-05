// Generated domain module 48 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model047Service, summarizeModel047 } from "./model047";
import type { Model047 } from "./model047";
import { Model041Service, summarizeModel041 } from "./model041";
import type { Model041 } from "./model041";

export type Model048Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model048Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model048 extends Entity<"model048"> {
  updatedAt?: number;
  name: string;
  status: Model048Status;
  tags: string[];
  lines: Model048Line[];
  owner?: { name: string; email?: string };
  parent?: Model047;
  related: Model041[];
}

export type Model048Event =
  | { kind: "created"; item: Model048 }
  | { kind: "renamed"; id: Id<"model048">; from: string; to: string }
  | { kind: "moved"; id: Id<"model048">; status: Model048Status }
  | { kind: "deleted"; id: Id<"model048">; reason?: string };

export type Model048Events = {
  change: Model048Event;
  error: { message: string; code: number };
};

export type Model048Numbers = KeysOfType<Model048Line, number>;
export type FrozenModel048 = DeepReadonly<Model048>;

export function describeModel048Event(event: Model048Event): string {
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

export function totalModel048(item: Model048): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel048(item: Model048): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel048(item), 3), (item.parent ? summarizeModel047(item.parent) : "-"), item.related.map(summarizeModel041).join(",")].join(" | ");
}

export class Model048Service extends Service<Model048, "model048"> {
  readonly events = new EventBus<Model048Events>();
  private readonly parents?: Model047Service;

  constructor(repository = new MemoryRepository<Model048, "model048">()) {
    super(repository);
  }

  validate(item: Model048): string[] {
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

  rename(id: Id<"model048">, to: string): Result<Model048> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model048 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model048">, status: Model048Status): Result<Model048Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model048">, patch: Patch<Pick<Model048, "name" | "tags">>): Model048 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model048Status, Model048[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model048Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model048">[]): Promise<Model048[]> {
    const found: Model048[] = [];
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

  linkedService(): Model047Service {
    return this.parents ?? new Model047Service();
  }
}

export function makeModel048(id: string, name: string): Model048 {
  return {
    id: id as Id<"model048">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 49, unit: "item" }],
    related: [],
  };
}

export const model048Defaults: FrozenModel048 = makeModel048("default-48", "Default 48");
export const model048Label = summarizeModel048(makeModel048("label", "Label"));
