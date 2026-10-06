// Generated domain module 88 of 200.
import { EventBus, MemoryRepository, Service, fail, format, groupBy, mapResult, ok, pick, retry, unwrapOr } from "./core";
import type { DeepReadonly, Entity, Id, KeysOfType, Patch, Result } from "./core";
import { Model087Service, summarizeModel087 } from "./model087";
import type { Model087 } from "./model087";
import { Model081Service, summarizeModel081 } from "./model081";
import type { Model081 } from "./model081";

export type Model088Status = "draft" | "active" | "suspended" | "archived" | "deleted";

export interface Model088Line {
  readonly sku: string;
  quantity: number;
  unit: "item" | "unit";
  price?: { amount: number; currency: "USD" | "EUR" };
}

export interface Model088 extends Entity<"model088"> {
  updatedAt?: number;
  name: string;
  status: Model088Status;
  tags: string[];
  lines: Model088Line[];
  owner?: { name: string; email?: string };
  parent?: Model087;
  related: Model081[];
}

export type Model088Event =
  | { kind: "created"; item: Model088 }
  | { kind: "renamed"; id: Id<"model088">; from: string; to: string }
  | { kind: "moved"; id: Id<"model088">; status: Model088Status }
  | { kind: "deleted"; id: Id<"model088">; reason?: string };

export type Model088Events = {
  change: Model088Event;
  error: { message: string; code: number };
};

export type Model088Numbers = KeysOfType<Model088Line, number>;
export type FrozenModel088 = DeepReadonly<Model088>;

export function describeModel088Event(event: Model088Event): string {
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

export function totalModel088(item: Model088): number {
  let sum = 0;
  for (const line of item.lines) {
    const amount = line.price?.amount ?? 0;
    sum += line.unit === "unit" ? amount : amount * line.quantity;
  }
  return sum;
}

export function summarizeModel088(item: Model088): string {
  const head = pick(item, "name", "status");
  const owner = item.owner?.email ?? item.owner?.name ?? "nobody";
  return [head.name, head.status, owner, format(totalModel088(item), 3), (item.parent ? summarizeModel087(item.parent) : "-"), item.related.map(summarizeModel081).join(",")].join(" | ");
}

export class Model088Service extends Service<Model088, "model088"> {
  readonly events = new EventBus<Model088Events>();
  private readonly parents?: Model087Service;

  constructor(repository = new MemoryRepository<Model088, "model088">()) {
    super(repository);
  }

  validate(item: Model088): string[] {
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

  rename(id: Id<"model088">, to: string): Result<Model088> {
    const current = this.find((item) => item.id === id);
    if (!current) {
      return fail(`missing ${id}`);
    }
    const next: Model088 = { ...current, name: to, updatedAt: current.createdAt + 1 };
    this.events.emit("change", { kind: "renamed", id, from: current.name, to });
    return this.repository.save(next);
  }

  move(id: Id<"model088">, status: Model088Status): Result<Model088Status> {
    const saved = this.rename(id, status.toUpperCase());
    return mapResult(saved, (item) => {
      this.events.emit("change", { kind: "moved", id: item.id, status });
      return status;
    });
  }

  apply(id: Id<"model088">, patch: Patch<Pick<Model088, "name" | "tags">>): Model088 | undefined {
    const current = this.find((item) => item.id === id);
    if (current === undefined) {
      return undefined;
    }
    return { ...current, name: patch.name ?? current.name, tags: current.tags };
  }

  byStatus(): Record<Model088Status, Model088[]> {
    return groupBy(this.repository.list(), (item) => item.status);
  }

  quantities<K extends Model088Numbers>(key: K): number[] {
    return this.repository.list().flatMap((item) => item.lines.map((line) => line[key]));
  }

  async load(ids: readonly Id<"model088">[]): Promise<Model088[]> {
    const found: Model088[] = [];
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

  linkedService(): Model087Service {
    return this.parents ?? new Model087Service();
  }
}

export function makeModel088(id: string, name: string): Model088 {
  return {
    id: id as Id<"model088">,
    createdAt: 0,
    name,
    status: "archived",
    tags: [],
    lines: [{ sku: `${id}-1`, quantity: 89, unit: "item" }],
    related: [],
  };
}

export const model088Defaults: FrozenModel088 = makeModel088("default-88", "Default 88");
export const model088Label = summarizeModel088(makeModel088("label", "Label"));
