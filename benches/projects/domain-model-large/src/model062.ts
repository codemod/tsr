// Generated domain module 62 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model061Service, summarizeModel061 } from "./model061";
import type { Model061 } from "./model061";
import { Model055Service, summarizeModel055 } from "./model055";
import type { Model055 } from "./model055";

export type Model062Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model062Line {
  readonly sku: string;
  quantity: number;
  unit: "s" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model062 extends Entity<"model062"> {
  updatedAt?: number;
  name: string;
  status: Model062Status;
  tags: string[];
  lines: Model062Line[];
  owner?: { name: string; email?: string };
  parent?: Model061;
  related: Model055[];
}

export type Model062Event =
  | { kind: "created"; item: Model062 }
  | { kind: "renamed"; id: Id<"model062">; from: string; to: string }
  | { kind: "moved"; id: Id<"model062">; status: Model062Status }
  | { kind: "deleted"; id: Id<"model062">; reason?: string };

export type Model062Events = {
  change: Model062Event;
  error: { message: string; code: number };
};

export type Model062Numbers = KeysOfType<Model062Line, number>;
export type FrozenModel062 = DeepReadonly<Model062>;

export function describeModel062Event(event: Model062Event): string {
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

export function totalModel062(item: Model062): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel062(item: Model062): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel062(item), 3), (item.parent ? summarizeModel061(item.parent) : "-"), item.related.map(summarizeModel055).join(",")].join(" | ");
}

export class Model062Service extends Service<Model062, "model062"> {
  readonly events = new EventBus<Model062Events>();
  private readonly parents?: Model061Service;

  constructor(repository = new MemoryRepository<Model062, "model062">()) {
    super(repository);
  }

  validate(item: Model062): string[] {
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

  rename(id: Id<"model062">, to: string): Result<Model062> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model062 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model062">, status: Model062Status): Result<Model062Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model062">, patch: Patch<Pick<Model062, "name" | "tags">>): Model062 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model062Status, Model062[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model062Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model062">[]): Promise<Model062[]> {
    const found: Model062[] = [];
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

  linkedService(): Model061Service {
    return this.parents ?? new Model061Service();
  }
}

export function makeModel062(id: string, name: string): Model062 {
  return {
    id: id as Id<"model062">,
    createdAt: 0,
    name,
    status: "suspended",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 63, unit: "s" }],
    related: [],
  };
}

export const model062Defaults: FrozenModel062 = makeModel062("default-62", "Default 62");
export const model062Label = summarizeModel062(makeModel062("label", "Label"));
