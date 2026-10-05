// Generated domain module 66 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model065Service, summarizeModel065 } from "./model065";
import type { Model065 } from "./model065";
import { Model059Service, summarizeModel059 } from "./model059";
import type { Model059 } from "./model059";

export type Model066Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model066Line {
  readonly sku: string;
  quantity: number;
  unit: "m" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model066 extends Entity<"model066"> {
  updatedAt?: number;
  name: string;
  status: Model066Status;
  tags: string[];
  lines: Model066Line[];
  owner?: { name: string; email?: string };
  parent?: Model065;
  related: Model059[];
}

export type Model066Event =
  | { kind: "created"; item: Model066 }
  | { kind: "renamed"; id: Id<"model066">; from: string; to: string }
  | { kind: "moved"; id: Id<"model066">; status: Model066Status }
  | { kind: "deleted"; id: Id<"model066">; reason?: string };

export type Model066Events = {
  change: Model066Event;
  error: { message: string; code: number };
};

export type Model066Numbers = KeysOfType<Model066Line, number>;
export type FrozenModel066 = DeepReadonly<Model066>;

export function describeModel066Event(event: Model066Event): string {
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

export function totalModel066(item: Model066): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel066(item: Model066): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel066(item), 3), (item.parent ? summarizeModel065(item.parent) : "-"), item.related.map(summarizeModel059).join(",")].join(" | ");
}

export class Model066Service extends Service<Model066, "model066"> {
  readonly events = new EventBus<Model066Events>();
  private readonly parents?: Model065Service;

  constructor(repository = new MemoryRepository<Model066, "model066">()) {
    super(repository);
  }

  validate(item: Model066): string[] {
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

  rename(id: Id<"model066">, to: string): Result<Model066> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model066 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model066">, status: Model066Status): Result<Model066Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model066">, patch: Patch<Pick<Model066, "name" | "tags">>): Model066 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model066Status, Model066[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model066Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model066">[]): Promise<Model066[]> {
    const found: Model066[] = [];
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

  linkedService(): Model065Service {
    return this.parents ?? new Model065Service();
  }
}

export function makeModel066(id: string, name: string): Model066 {
  return {
    id: id as Id<"model066">,
    createdAt: 0,
    name,
    status: "active",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 67, unit: "m" }],
    related: [],
  };
}

export const model066Defaults: FrozenModel066 = makeModel066("default-66", "Default 66");
export const model066Label = summarizeModel066(makeModel066("label", "Label"));
