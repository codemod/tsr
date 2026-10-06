// Generated domain module 151 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model150Service, summarizeModel150 } from "./model150";
import type { Model150 } from "./model150";
import { Model144Service, summarizeModel144 } from "./model144";
import type { Model144 } from "./model144";

export type Model151Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model151Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model151 extends Entity<"model151"> {
  updatedAt?: number;
  name: string;
  status: Model151Status;
  tags: string[];
  lines: Model151Line[];
  owner?: { name: string; email?: string };
  parent?: Model150;
  related: Model144[];
}

export type Model151Event =
  | { kind: "created"; item: Model151 }
  | { kind: "renamed"; id: Id<"model151">; from: string; to: string }
  | { kind: "moved"; id: Id<"model151">; status: Model151Status }
  | { kind: "deleted"; id: Id<"model151">; reason?: string };

export type Model151Events = {
  change: Model151Event;
  error: { message: string; code: number };
};

export type Model151Numbers = KeysOfType<Model151Line, number>;
export type FrozenModel151 = DeepReadonly<Model151>;

export function describeModel151Event(event: Model151Event): string {
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

export function totalModel151(item: Model151): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel151(item: Model151): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel151(item), 3), (item.parent ? summarizeModel150(item.parent) : "-"), item.related.map(summarizeModel144).join(",")].join(" | ");
}

export class Model151Service extends Service<Model151, "model151"> {
  readonly events = new EventBus<Model151Events>();
  private readonly parents?: Model150Service;

  constructor(repository = new MemoryRepository<Model151, "model151">()) {
    super(repository);
  }

  validate(item: Model151): string[] {
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

  rename(id: Id<"model151">, to: string): Result<Model151> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model151 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model151">, status: Model151Status): Result<Model151Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model151">, patch: Patch<Pick<Model151, "name" | "tags">>): Model151 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model151Status, Model151[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model151Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model151">[]): Promise<Model151[]> {
    const found: Model151[] = [];
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

  linkedService(): Model150Service {
    return this.parents ?? new Model150Service();
  }
}

export function makeModel151(id: string, name: string): Model151 {
  return {
    id: id as Id<"model151">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 152, unit: "m" }],
    related: [],
  };
}

export const model151Defaults: FrozenModel151 = makeModel151("default-151", "Default 151");
export const model151Label = summarizeModel151(makeModel151("label", "Label"));
