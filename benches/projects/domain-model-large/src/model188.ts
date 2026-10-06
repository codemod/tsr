// Generated domain module 188 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model187Service, summarizeModel187 } from "./model187";
import type { Model187 } from "./model187";
import { Model181Service, summarizeModel181 } from "./model181";
import type { Model181 } from "./model181";

export type Model188Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model188Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model188 extends Entity<"model188"> {
  updatedAt?: number;
  name: string;
  status: Model188Status;
  tags: string[];
  lines: Model188Line[];
  owner?: { name: string; email?: string };
  parent?: Model187;
  related: Model181[];
}

export type Model188Event =
  | { kind: "created"; item: Model188 }
  | { kind: "renamed"; id: Id<"model188">; from: string; to: string }
  | { kind: "moved"; id: Id<"model188">; status: Model188Status }
  | { kind: "deleted"; id: Id<"model188">; reason?: string };

export type Model188Events = {
  change: Model188Event;
  error: { message: string; code: number };
};

export type Model188Numbers = KeysOfType<Model188Line, number>;
export type FrozenModel188 = DeepReadonly<Model188>;

export function describeModel188Event(event: Model188Event): string {
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

export function totalModel188(item: Model188): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel188(item: Model188): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel188(item), 3), (item.parent ? summarizeModel187(item.parent) : "-"), item.related.map(summarizeModel181).join(",")].join(" | ");
}

export class Model188Service extends Service<Model188, "model188"> {
  readonly events = new EventBus<Model188Events>();
  private readonly parents?: Model187Service;

  constructor(repository = new MemoryRepository<Model188, "model188">()) {
    super(repository);
  }

  validate(item: Model188): string[] {
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

  rename(id: Id<"model188">, to: string): Result<Model188> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model188 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model188">, status: Model188Status): Result<Model188Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model188">, patch: Patch<Pick<Model188, "name" | "tags">>): Model188 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model188Status, Model188[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model188Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model188">[]): Promise<Model188[]> {
    const found: Model188[] = [];
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

  linkedService(): Model187Service {
    return this.parents ?? new Model187Service();
  }
}

export function makeModel188(id: string, name: string): Model188 {
  return {
    id: id as Id<"model188">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 189, unit: "item" }],
    related: [],
  };
}

export const model188Defaults: FrozenModel188 = makeModel188("default-188", "Default 188");
export const model188Label = summarizeModel188(makeModel188("label", "Label"));
