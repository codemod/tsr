// Generated domain module 70 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model069Service, summarizeModel069 } from "./model069";
import type { Model069 } from "./model069";
import { Model063Service, summarizeModel063 } from "./model063";
import type { Model063 } from "./model063";

export type Model070Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model070Line {
  readonly sku: string;
  quantity: number;
  unit: "kg" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model070 extends Entity<"model070"> {
  updatedAt?: number;
  name: string;
  status: Model070Status;
  tags: string[];
  lines: Model070Line[];
  owner?: { name: string; email?: string };
  parent?: Model069;
  related: Model063[];
}

export type Model070Event =
  | { kind: "created"; item: Model070 }
  | { kind: "renamed"; id: Id<"model070">; from: string; to: string }
  | { kind: "moved"; id: Id<"model070">; status: Model070Status }
  | { kind: "deleted"; id: Id<"model070">; reason?: string };

export type Model070Events = {
  change: Model070Event;
  error: { message: string; code: number };
};

export type Model070Numbers = KeysOfType<Model070Line, number>;
export type FrozenModel070 = DeepReadonly<Model070>;

export function describeModel070Event(event: Model070Event): string {
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

export function totalModel070(item: Model070): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel070(item: Model070): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel070(item), 3), (item.parent ? summarizeModel069(item.parent) : "-"), item.related.map(summarizeModel063).join(",")].join(" | ");
}

export class Model070Service extends Service<Model070, "model070"> {
  readonly events = new EventBus<Model070Events>();
  private readonly parents?: Model069Service;

  constructor(repository = new MemoryRepository<Model070, "model070">()) {
    super(repository);
  }

  validate(item: Model070): string[] {
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

  rename(id: Id<"model070">, to: string): Result<Model070> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model070 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model070">, status: Model070Status): Result<Model070Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model070">, patch: Patch<Pick<Model070, "name" | "tags">>): Model070 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model070Status, Model070[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model070Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model070">[]): Promise<Model070[]> {
    const found: Model070[] = [];
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

  linkedService(): Model069Service {
    return this.parents ?? new Model069Service();
  }
}

export function makeModel070(id: string, name: string): Model070 {
  return {
    id: id as Id<"model070">,
    createdAt: 0,
    name,
    status: "draft",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 71, unit: "kg" }],
    related: [],
  };
}

export const model070Defaults: FrozenModel070 = makeModel070("default-70", "Default 70");
export const model070Label = summarizeModel070(makeModel070("label", "Label"));
